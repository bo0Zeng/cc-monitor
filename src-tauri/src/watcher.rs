//! `<claude_dir>/projects/` 的 JSONL 文件监听器。
//!
//! `spawn_watcher` 用 notify-debouncer-mini 监听目录；每次 `process_file` 从上次
//! 偏移**增量**读新行（不截断、记 per-file 偏移），剥 BOM 后交 parser，收集成
//! `Vec<JsonlLine>` **同步**调 `on_batch` 回调（无 mpsc / 无 async drain）。
//!
//! 关键：给每读出的一行分配 per-file 单调递增的 `seq: u64`（`seqs: HashMap<PathBuf,u64>`
//! 跨调用累加），透传到 `JsonlLinePayload.seq` —— 前端按 seq 排序，后端 emit 顺序不影响
//! 视觉（INVARIANT § 5 / § 9）。另维护 `initial_scan_done` 供启动重放等待全量扫完。

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
    /// P5.1：per-file 单调递增的行号。watcher 用 `seqs: HashMap<PathBuf, u64>` 维护
    /// 每文件 next_seq；process_file 顺序读，单调保证。前端 RecordTimeline 按 seq
    /// 排序到 DOM，emit 顺序不再影响视觉。同 session 内 seq 单调（同 session 通常
    /// 单文件）；跨 session 不可比（独立 timeline）；不跨进程持久。
    pub seq: u64,
    pub raw: String,
}

/// 活跃过滤器：给定 session_id 返回是否应该 emit 这一行。
pub type ActiveFilter = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// 批回调：watcher 读完一个 jsonl 文件后把这次读到的所有行作为一批传出。
///
/// v2.4.2 issue #2 修「/resume 历史灌入也走 live 一行一个 emit 导致卡顿」：
/// 之前 v2.4 是逐行 on_line(JsonlLine)，process_file 内的 N 行 → N 次 emit。
/// 用户 `claude --resume` 在新 sid 灌历史时单文件可能几千行 → 每行单独走渲染
/// 管线 → 卡 + 视觉上像新消息一条一条到来。
///
/// 改成 batch：lib.rs 拿到整批后根据大小分流：
/// - 小 batch（用户日常敲键 / 1-N 行）→ 仍走 jsonl-line live emit
/// - 大 batch（>= 阈值，如 /resume 历史灌入）→ 走 jsonl-batch chunked emit，
///   前端自动进入切块路径（lazy hljs / chunked prepend）
///
/// 回调 Send+Sync+'static，运行在 watcher 线程，不能阻塞太久。
pub type BatchHandler = Arc<dyn Fn(Vec<JsonlLine>) + Send + Sync + 'static>;

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

/// 递归监听 `root` 下所有 `*.jsonl` 文件。每次 process_file 读完一个文件后
/// 把这次读到的所有行作为一批同步调用 `on_batch`。
///
/// **顺序保证**（v2.4 修首次启动乱序）：watcher 线程内先**同步**完成首次
/// 全量扫，扫完才设置 `initial_scan_done = true` 并进入 debouncer 监听阶段。
/// frontend-ready listener 等待这个 flag 才允许 replay 持锁 snapshot，
/// 保证 snapshot 时整个 history buffer 完整，不会有历史漏到 live emit 路径
/// 跟 chunked replay 错位。
///
/// 初始全量扫描也走 active 过滤，避免冷启动时回放死 session 的历史。
pub fn spawn_watcher(root: PathBuf, active: ActiveFilter, on_batch: BatchHandler) -> WatcherHandle {
    let (rescan_tx, rescan_rx) = std::sync::mpsc::channel::<String>();
    let offsets: Arc<Mutex<HashMap<PathBuf, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    // P5.1：per-file next seq 计数器。跟 offsets 共生命周期；同一文件多次 process
    // 跨调用单调递增。
    let seqs: Arc<Mutex<HashMap<PathBuf, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let initial_scan_done = Arc::new(AtomicBool::new(false));

    // spawn 失败不要 panic 整个 app（生产场景应该日志降级，让 UI 至少能开）
    let scan_flag = initial_scan_done.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || run_watcher(root, offsets, seqs, on_batch, active, rescan_rx, scan_flag))
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
    seqs: Arc<Mutex<HashMap<PathBuf, u64>>>,
    on_batch: BatchHandler,
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
            process_file(p, &offsets, &seqs, &on_batch, &active);
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
                        process_file(&ev.path, &offsets, &seqs, &on_batch, &active);
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
                    process_file(p, &offsets, &seqs, &on_batch, &active);
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
    seqs: &Arc<Mutex<HashMap<PathBuf, u64>>>,
    on_batch: &BatchHandler,
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
    let truncated = len < last_offset;
    let start = if truncated { 0 } else { last_offset };

    if start >= len {
        return;
    }
    if truncated {
        // issue #25：截断重读 = 全文件换新 seq 重投（下面 seq 不重置的注释），即
        // at-least-once 投递的唯一已知本地触发点（doc/INVARIANTS.md § 25）。前端
        // 折叠层（#25）与渲染层（#26）均已按 uuid 幂等。必须留痕——曾因静默无日志
        // 导致误折叠根因定位极难。warn 放在空文件早退之后：len==0 时无重读发生不喊。
        tracing::warn!(
            "jsonl truncated (len {len} < offset {last_offset}), full re-read with new seqs: {path:?}"
        );
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }

    // P5.1：取当前文件 next_seq 作起点，逐行 ++ 写入 JsonlLine.seq。
    // 同一文件多次 process_file 跨调用累加（不重置）；保证 same-session 单调。
    // 文件截断时（len < last_offset）offset 重置但 seq 不重置——前端 timeline
    // 已经填了旧 seq，新行用更大的 seq 仍正确排在后面。
    let mut next_seq = seqs.lock().get(&key).copied().unwrap_or(0);

    // 全量发：无论首次扫还是增量都不截断行数。v2.4.2 改成收集到 Vec 然后
    // 一次性 on_batch，让下游 lib.rs 按 batch 大小分流：
    //   - 小 batch（用户日常增量 1-N 行）→ 走 jsonl-line live emit
    //   - 大 batch（/resume 历史灌入 N 千行）→ 切块走 jsonl-batch
    // 而不是每行单独 emit 一次卡前端管线。
    let reader = BufReader::new(&mut file);
    let mut batch: Vec<JsonlLine> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let seq = next_seq;
        next_seq += 1;
        batch.push(JsonlLine {
            session_id: session_id.clone(),
            path: path.to_path_buf(),
            seq,
            raw: line,
        });
    }
    if !batch.is_empty() {
        on_batch(batch);
    }
    offsets.lock().insert(key.clone(), len);
    seqs.lock().insert(key, next_seq);
}

#[cfg(windows)]
fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}
