use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct JsonlLine {
    pub session_id: String,
    pub path: PathBuf,
    pub raw: String,
}

/// 活跃过滤器：给定 session_id 返回是否应该 emit 这一行。
pub type ActiveFilter = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// 行回调：watcher 读到一条 jsonl 行后直接调用。
///
/// v2.4：替代 v2.3 的 mpsc 中间层。watcher 同步调 on_line 避免下游 async
/// task 异步消化导致 EventReplay 的 history snapshot 在 frontend-ready
/// 触发时不完整（首次启动乱序根因）。
///
/// 回调是 Send+Sync+'static 闭包，运行在 watcher 线程上，不能阻塞太久。
/// 业务侧（lib.rs）实现 parse + record，~几 μs 级别，足够。
pub type LineHandler = Arc<dyn Fn(JsonlLine) + Send + Sync + 'static>;

/// jsonl-watcher 的 handle：
/// - `force_rescan_tx`：主动重扫某个 session_id 对应的 jsonl 文件
/// - `initial_scan_done`：watcher 完成首次全量扫的信号；frontend-ready
///   listener 阻塞等待此 flag 才触发 replay，避免 snapshot 不完整。
///
/// `force_rescan_tx` 是为修 Bug 2-A 引入：当 session_map 后到（jsonl 行先于
/// PID.json 出现）时，watcher 的 process_file 因 active() 返 false 而 early
/// return，且不会自动重扫；外部（lib.rs）从 session-added 信号驱动这个通道
/// 触发一次重扫作为安全网。
pub struct WatcherHandle {
    pub force_rescan_tx: std::sync::mpsc::Sender<String>,
    pub initial_scan_done: Arc<AtomicBool>,
}

/// 递归监听 `root` 下所有 `*.jsonl` 文件。每读到一行（active 过滤通过）就
/// 同步调用 `on_line`。
///
/// **顺序保证**（v2.4 修首次启动乱序）：watcher 线程内先**同步**完成首次
/// 全量扫，扫完才设置 `initial_scan_done = true` 并进入 debouncer 监听阶段。
/// frontend-ready listener 等待这个 flag 才允许 replay 持锁 snapshot，
/// 保证 snapshot 时整个 history buffer 完整，不会有历史漏到 live emit 路径
/// 跟 chunked replay 错位。
///
/// 初始全量扫描也走 active 过滤，避免冷启动时回放死 session 的历史。
pub fn spawn_watcher(root: PathBuf, active: ActiveFilter, on_line: LineHandler) -> WatcherHandle {
    let (rescan_tx, rescan_rx) = std::sync::mpsc::channel::<String>();
    let offsets: Arc<Mutex<HashMap<PathBuf, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let initial_scan_done = Arc::new(AtomicBool::new(false));

    // spawn 失败不要 panic 整个 app（生产场景应该日志降级，让 UI 至少能开）
    let scan_flag = initial_scan_done.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || run_watcher(root, offsets, on_line, active, rescan_rx, scan_flag))
    {
        tracing::error!(
            "spawn jsonl-watcher thread failed: {e}; \
             monitor will start but won't show any session content"
        );
        // 线程没起来，永远不会设 flag；把它直接设 true 让 frontend-ready 不死等
        initial_scan_done.store(true, Ordering::Release);
    }

    WatcherHandle {
        force_rescan_tx: rescan_tx,
        initial_scan_done,
    }
}

fn run_watcher(
    root: PathBuf,
    offsets: Arc<Mutex<HashMap<PathBuf, u64>>>,
    on_line: LineHandler,
    active: ActiveFilter,
    rescan_rx: std::sync::mpsc::Receiver<String>,
    initial_scan_done: Arc<AtomicBool>,
) {
    if !root.exists() {
        tracing::warn!("watch root does not exist: {}", root.display());
        // root 不存在也算扫"完"了，让 frontend-ready 不死等
        initial_scan_done.store(true, Ordering::Release);
        return;
    }

    // 阶段 1：同步全量扫。watcher 线程独占，扫完才设 flag + 进监听阶段。
    let scan_started = std::time::Instant::now();
    let mut scanned_files = 0usize;
    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension().map_or(false, |e| e == "jsonl") && !is_subagent_path(p) {
            process_file(p, &offsets, &on_line, &active);
            scanned_files += 1;
        }
    }
    tracing::info!(
        "[perf] watcher initial scan done: {} files in {}ms",
        scanned_files,
        scan_started.elapsed().as_millis()
    );
    initial_scan_done.store(true, Ordering::Release);

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
                        process_file(&ev.path, &offsets, &on_line, &active);
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
                    process_file(p, &offsets, &on_line, &active);
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
    on_line: &LineHandler,
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

    // 全量发：无论首次扫还是增量都不截断行数。同步调用 on_line 让下游 record()
    // 把数据 push 进 history buffer。v2.4 起干掉 mpsc 中间层，避免异步 drain
    // 跟 frontend-ready 触发 replay snapshot 的竞态（首次启动乱序根因）。
    let reader = BufReader::new(&mut file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        on_line(JsonlLine {
            session_id: session_id.clone(),
            path: path.to_path_buf(),
            raw: line,
        });
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
