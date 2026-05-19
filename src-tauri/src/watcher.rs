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
        if p.is_file() && p.extension().map_or(false, |e| e == "jsonl") {
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
            if ev.path.extension().map_or(false, |e| e == "jsonl") {
                process_file(&ev.path, &offsets, &tx, &active);
            }
        }
    }
}

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

    let mut offsets_lock = offsets.lock();
    let last_offset = offsets_lock.get(path).copied().unwrap_or(0);
    let start = if len < last_offset { 0 } else { last_offset };

    if start >= len {
        return;
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }

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
    offsets_lock.insert(path.to_path_buf(), len);
}
