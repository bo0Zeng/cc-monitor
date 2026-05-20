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

/// 递归监听 `root` 下所有 `*.jsonl` 文件。只 emit `active(session_id)` 为 true 的行。
/// 初始全量扫描也走过滤，避免冷启动时回放死 session 的历史。
pub fn spawn_watcher(root: PathBuf, active: ActiveFilter) -> mpsc::UnboundedReceiver<JsonlLine> {
    let (tx, rx) = mpsc::unbounded_channel::<JsonlLine>();
    let offsets: Arc<Mutex<HashMap<PathBuf, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || run_watcher(root, offsets, tx, active))
        .expect("spawn watcher thread");

    rx
}

fn run_watcher(
    root: PathBuf,
    offsets: Arc<Mutex<HashMap<PathBuf, u64>>>,
    tx: mpsc::UnboundedSender<JsonlLine>,
    active: ActiveFilter,
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

    while let Ok(evt) = notify_rx.recv() {
        let Ok(events) = evt else { continue };
        for ev in events {
            if ev.path.extension().map_or(false, |e| e == "jsonl") && !is_subagent_path(&ev.path) {
                process_file(&ev.path, &offsets, &tx, &active);
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

/// 首次扫一个 jsonl 文件时最多回放多少行（再多就丢最老的）。避免 monitor
/// 启动被几天前历史淹没 + 触发 event_replay cap 把展示 snapshot 卡在早期。
/// 取 1500：覆盖一次比较完整的会话 + 留出余量，超出部分用户基本不会回看。
const INITIAL_TAIL_LINES: usize = 1500;

fn process_file(
    path: &Path,
    offsets: &Arc<Mutex<HashMap<PathBuf, u64>>>,
    tx: &mpsc::UnboundedSender<JsonlLine>,
    active: &ActiveFilter,
) {
    let Some(session_id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
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
    let is_initial_scan = start == 0;

    if start >= len {
        return;
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }

    let reader = BufReader::new(&mut file);
    if is_initial_scan {
        // 首次扫：buffer 全部行后只发尾部 N 行
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let total = lines.len();
        let skip = total.saturating_sub(INITIAL_TAIL_LINES);
        if skip > 0 {
            tracing::info!(
                "watcher initial scan {}: keeping last {} of {} lines",
                session_id,
                INITIAL_TAIL_LINES,
                total
            );
        }
        for line in lines.into_iter().skip(skip) {
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
    } else {
        // 增量扫：notify 事件触发，按行流式发，不截断
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
