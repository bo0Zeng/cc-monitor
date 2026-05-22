use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct JsonlLine {
    pub session_id: String,
    pub path: PathBuf,
    pub raw: String,
}

/// 活跃过滤器：给定 session_id 返回是否应该 emit 这一行。
pub type ActiveFilter = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// jsonl-watcher 的 handle：
/// - `rx`：jsonl line 流（被动接收）
/// - `force_rescan_tx`：主动重扫某个 session_id 对应的 jsonl 文件
///
/// `force_rescan_tx` 是为修 Bug 2-A 引入：当 session_map 后到（jsonl 行先于
/// PID.json 出现）时，watcher 的 process_file 因 active() 返 false 而 early
/// return，且不会自动重扫；外部（lib.rs）从 session-added 信号驱动这个通道
/// 触发一次重扫作为安全网。
pub struct WatcherHandle {
    pub rx: mpsc::UnboundedReceiver<JsonlLine>,
    pub force_rescan_tx: std::sync::mpsc::Sender<String>,
}

/// 递归监听 `root` 下所有 `*.jsonl` 文件。只 emit `active(session_id)` 为 true 的行。
/// 初始全量扫描也走过滤，避免冷启动时回放死 session 的历史。
pub fn spawn_watcher(root: PathBuf, active: ActiveFilter) -> WatcherHandle {
    let (tx, rx) = mpsc::unbounded_channel::<JsonlLine>();
    let (rescan_tx, rescan_rx) = std::sync::mpsc::channel::<String>();
    let offsets: Arc<Mutex<HashMap<PathBuf, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    // spawn 失败不要 panic 整个 app（生产场景应该日志降级，让 UI 至少能开）
    if let Err(e) = std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || run_watcher(root, offsets, tx, active, rescan_rx))
    {
        tracing::error!(
            "spawn jsonl-watcher thread failed: {e}; \
             monitor will start but won't show any session content"
        );
    }

    WatcherHandle {
        rx,
        force_rescan_tx: rescan_tx,
    }
}

fn run_watcher(
    root: PathBuf,
    offsets: Arc<Mutex<HashMap<PathBuf, u64>>>,
    tx: mpsc::UnboundedSender<JsonlLine>,
    active: ActiveFilter,
    rescan_rx: std::sync::mpsc::Receiver<String>,
) {
    if !root.exists() {
        tracing::warn!("watch root does not exist: {}", root.display());
        return;
    }

    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension().map_or(false, |e| e == "jsonl") && !is_subagent_path(p) {
            process_file(p, &offsets, &tx, &active);
        }
    }

    let (notify_tx, notify_rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(Duration::from_millis(100), notify_tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("debouncer init failed: {e}");
            return;
        }
    };
    if let Err(e) = debouncer.watcher().watch(&root, RecursiveMode::Recursive) {
        tracing::error!("watch failed for {}: {e}", root.display());
        return;
    }

    // 主循环：用 recv_timeout 100ms 轮询 notify 事件，每轮 try_recv rescan 请求。
    // 100ms 轮询额外延迟是为兼容 rescan 通道；jsonl-line 已有 notify_debouncer 100ms
    // debounce，再加 100ms 总延迟 ~200ms，对流式渲染可接受。
    use std::sync::mpsc::RecvTimeoutError;
    loop {
        match notify_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(evt) => {
                let Ok(events) = evt else { continue };
                for ev in events {
                    if ev.path.extension().map_or(false, |e| e == "jsonl")
                        && !is_subagent_path(&ev.path)
                    {
                        process_file(&ev.path, &offsets, &tx, &active);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Drain 所有待办 rescan 请求（修 Bug 2-A）。新加入的 session 可能因为
        // jsonl 行先到的竞态没被 emit；这里强制重扫该 session 的所有 jsonl 文件
        // （offset 没更新过的就 process，更新过的就跳过——process_file 内部判断）
        while let Ok(sid) = rescan_rx.try_recv() {
            tracing::info!("forced jsonl rescan for session {sid}");
            for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
                let p = entry.path();
                if p.is_file()
                    && p.extension().map_or(false, |e| e == "jsonl")
                    && !is_subagent_path(p)
                    && p.file_stem().and_then(|s| s.to_str()) == Some(&sid)
                {
                    process_file(p, &offsets, &tx, &active);
                }
            }
        }
    }
}

/// subagent JSONL 不走主流：路径含 `/subagents/` 段。
/// 这些文件由前端 invoke `load_subagent` 命令在用户展开 Task 卡时按需加载。
fn is_subagent_path(p: &Path) -> bool {
    p.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("subagents"))
}

fn process_file(
    path: &Path,
    offsets: &Arc<Mutex<HashMap<PathBuf, u64>>>,
    tx: &mpsc::UnboundedSender<JsonlLine>,
    active: &ActiveFilter,
) {
    let Some(session_id) = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
    else {
        return;
    };

    if !active(&session_id) {
        tracing::trace!("watcher skip (inactive): {}", session_id);
        return;
    }
    tracing::debug!("watcher process: {} ({})", session_id, path.display());

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return,
    };

    // Windows 路径大小写不敏感，但 PathBuf::eq 是字节级；notify 偶发把同一
    // 文件以不同大小写回放（NTFS 路径规范化不一致）会重复 emit。统一以小写
    // 作 key，正常路径下 Linux/macOS 也无害（实际路径不变）。
    let key = path_key(path);

    // 锁内只读 + 写 offset，文件读循环走在锁外避免阻塞同 watcher 后续事件。
    let last_offset = offsets.lock().get(&key).copied().unwrap_or(0);
    let start = if len < last_offset { 0 } else { last_offset };

    if start >= len {
        return;
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }

    // 全量发：无论首次扫还是增量都不截断行数。event_replay 的"持锁严格按序
    // emit"保证前端按 jsonl 文件原始行顺序收到，不会因 snapshot/live emit 并
    // 发而错乱。代价是启动 IPC 流量大、watcher 阻塞秒级，可接受。
    let reader = BufReader::new(&mut file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        if tx
            .send(JsonlLine {
                session_id: session_id.clone(),
                path: path.to_path_buf(),
                raw: line,
            })
            .is_err()
        {
            return;
        }
    }
    offsets.lock().insert(key, len);
}

#[cfg(windows)]
fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}
