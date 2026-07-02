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

/// per-file 读游标（Batch4-F14）。
///
/// - `consumed`：已消费（emit 过）的**完整行**字节数——下次增量读的起点。
///   torn tail（无尾 `\n` 的写中半行）不计入，留待补全后下次消费。
/// - `seen_len`：观察到的文件字节数高点（读前 len 快照与实际读到的字节取 max）。
///   截断判定必须用 `seen_len` 而非 `consumed`：torn tail 挂起时
///   `consumed < 真实 EOF`，若文件被非追加式重写成长度落在
///   `[consumed, seen_len)` 的新内容，用 `consumed` 判会漏检截断 →
///   从错位 offset 读出垃圾行且绕过 truncation warn（F14 审计发现）。
#[derive(Debug, Clone, Copy, Default)]
struct FileCursor {
    consumed: u64,
    seen_len: u64,
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
    let offsets: Arc<Mutex<HashMap<PathBuf, FileCursor>>> = Arc::new(Mutex::new(HashMap::new()));
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
    offsets: Arc<Mutex<HashMap<PathBuf, FileCursor>>>,
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
    offsets: &Arc<Mutex<HashMap<PathBuf, FileCursor>>>,
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
    let cursor = offsets.lock().get(&key).copied().unwrap_or_default();
    let truncated = len < cursor.seen_len;
    let start = if truncated { 0 } else { cursor.consumed };

    if start >= len {
        if truncated {
            // 截断到空（len==0——truncated 下能走到早退的唯一情形：len>0 时
            // start=0 < len 必走读循环）：本轮无可读，但必须立刻重置游标——
            // 否则文件重新长回 ≥ 旧 consumed 时截断漏检，从错位 offset 读垃圾行
            // （F14 审计发现的双端分歧：daemon 侧一直会重置）。len==0 无重读发生，
            // 沿旧惯例不在这里喊 warn；重新长出内容后的下一轮照常读。
            offsets.lock().insert(
                key,
                FileCursor {
                    consumed: 0,
                    seen_len: len,
                },
            );
        }
        return;
    }
    if truncated {
        // issue #25：截断重读 = 全文件换新 seq 重投（下面 seq 不重置的注释），即
        // at-least-once 投递的唯一已知本地触发点（doc/INVARIANTS.md § 25；
        // Batch4-F14 前还有第二个：len 快照 < 读到的真实 EOF 时 offset 回退重投，
        // 现 offset 改按实际消费推进后消除）。前端
        // 折叠层（#25）与渲染层（#26）均已按 uuid 幂等。必须留痕——曾因静默无日志
        // 导致误折叠根因定位极难。warn 放在空文件早退之后：len==0 时无重读发生不喊。
        tracing::warn!(
            "jsonl truncated (len {len} < seen_len {}), full re-read with new seqs: {path:?}",
            cursor.seen_len
        );
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }

    // P5.1：取当前文件 next_seq 作起点，逐行 ++ 写入 JsonlLine.seq。
    // 同一文件多次 process_file 跨调用累加（不重置）；保证 same-session 单调。
    // 文件截断时（len < cursor.seen_len）游标重置但 seq 不重置——前端 timeline
    // 已经填了旧 seq，新行用更大的 seq 仍正确排在后面。
    let mut next_seq = seqs.lock().get(&key).copied().unwrap_or(0);

    // 全量发：无论首次扫还是增量都不截断行数。v2.4.2 改成收集到 Vec 然后
    // 一次性 on_batch，让下游 lib.rs 按 batch 大小分流：
    //   - 小 batch（用户日常增量 1-N 行）→ 走 jsonl-line live emit
    //   - 大 batch（/resume 历史灌入 N 千行）→ 切块走 jsonl-batch
    // 而不是每行单独 emit 一次卡前端管线。
    //
    // Batch4-F14：只消费以 \n 结尾的**完整行**，offset 按实际消费字节推进。
    // 旧实现（reader.lines() + offset 推到读前 len 快照）有三个同源病：
    //   1. CLI 写大行时 debounce 恰落在写中途 → 无尾 \n 的半行被当完整行 emit
    //      （parse 失败丢弃）、offset 又越过它 → 该记录 live 视图**永久丢失**；
    //   2. len 是读前快照而 lines() 读到真实 EOF → 读循环期间文件增长时
    //      offset 回退 → 已发行换新 seq 重投；
    //   3. lines() 遇非法 UTF-8 行 Err → map_while 静默截断整个批次。
    // 现在 partial 留在文件里等下次事件补全；撕裂的多字节序列必然整体落在
    // partial 里，对完整行做 lossy 解码不再产生瞬态 U+FFFD。
    // 已接受取舍（同 daemon 侧 Parity 注释 / INVARIANTS §25）：写端写完整 JSON
    // 后、写 \n 前被 kill 且文件从此不变 → 该行 live 视图永不投递（历史 viewer
    // 的独立读取路径仍能读到）；实测 CLI 每条记录以 \n 收尾。
    let mut reader = BufReader::new(&mut file);
    let mut batch: Vec<JsonlLine> = Vec::new();
    let mut consumed: u64 = 0;
    let mut tail_bytes: u64 = 0; // torn tail / 中断残余：进 seen_len 不进 consumed
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break, // EOF，无残余
            Ok(_) if buf.last() != Some(&b'\n') => {
                // 尾部 partial：不消费不 emit，但计入 seen_len（截断判定用）
                tail_bytes = buf.len() as u64;
                break;
            }
            Ok(n) => consumed += n as u64,
            Err(e) => {
                // 已消费的完整行照常发；剩余字节下次从新 offset 续读——前提是
                // 还有下一次 FS 事件（文件从此不再变化则这些字节永不投递，与
                // torn tail 永不补全同属 INVARIANTS §25 登记的已接受取舍）
                tracing::warn!("read {} failed mid-file: {e}", path.display());
                tail_bytes = buf.len() as u64;
                break;
            }
        }
        let mut end = buf.len() - 1; // 剥 \n
        if end > 0 && buf[end - 1] == b'\r' {
            end -= 1; // 剥 \r，与旧 lines() 行为一致
        }
        let line = String::from_utf8_lossy(&buf[..end]).into_owned();
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
    offsets.lock().insert(
        key.clone(),
        FileCursor {
            consumed: start + consumed,
            // 高点取 max：读中增长时实际读到的字节可能超过读前 len 快照
            seen_len: len.max(start + consumed + tail_bytes),
        },
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 独立临时目录（惯例同 utils.rs 测试：temp_dir + pid + 用途标记）。
    fn temp_jsonl(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ccm-watcher-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test-session.jsonl")
    }

    /// 跑一次 process_file，收集这次 emit 的所有行。
    fn run_once(
        path: &Path,
        offsets: &Arc<Mutex<HashMap<PathBuf, FileCursor>>>,
        seqs: &Arc<Mutex<HashMap<PathBuf, u64>>>,
    ) -> Vec<JsonlLine> {
        let collected: Arc<Mutex<Vec<JsonlLine>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let on_batch: BatchHandler = Arc::new(move |batch| sink.lock().extend(batch));
        let active: ActiveFilter = Arc::new(|_| true);
        process_file(path, offsets, seqs, &on_batch, &active);
        let out = collected.lock().clone();
        out
    }

    #[test]
    fn torn_line_deferred_until_newline_arrives() {
        let path = temp_jsonl("torn");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        // 完整一行 + 无尾 \n 的半行
        std::fs::write(&path, b"{\"a\":1}\n{\"a\":2,\"tex").unwrap();
        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 1, "partial must not be emitted");
        assert_eq!(out[0].raw, r#"{"a":1}"#);
        assert_eq!(out[0].seq, 0);
        let key = path_key(&path);
        let cur = offsets.lock().get(&key).copied().unwrap();
        assert_eq!(
            cur.consumed, 8,
            "offset stops after the complete line, not at EOF"
        );
        assert_eq!(cur.seen_len, 19, "seen_len covers the torn tail");

        // 半行补全 + 再来一行完整的
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"t\":\"x\"}\n{\"a\":3}\n").unwrap();
        drop(f);
        let out2 = run_once(&path, &offsets, &seqs);
        assert_eq!(out2.len(), 2, "completed line emitted exactly once");
        assert_eq!(out2[0].raw, r#"{"a":2,"text":"x"}"#);
        assert_eq!(out2[0].seq, 1, "seq continuous across the deferral");
        assert_eq!(out2[1].raw, r#"{"a":3}"#);
        assert_eq!(out2[1].seq, 2);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn torn_multibyte_utf8_does_not_produce_replacement_char() {
        let path = temp_jsonl("mb");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        // "文" = E6 96 87；撕在第二个字节后
        let full = "{\"t\":\"文\"}\n".as_bytes();
        std::fs::write(&path, &full[..7]).unwrap(); // {"t":" + E6（多字节撕裂点）
        let out = run_once(&path, &offsets, &seqs);
        assert!(
            out.is_empty(),
            "torn multibyte tail must be deferred, not lossy-decoded"
        );

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(&full[7..]).unwrap();
        drop(f);
        let out2 = run_once(&path, &offsets, &seqs);
        assert_eq!(out2.len(), 1);
        assert_eq!(
            out2[0].raw, "{\"t\":\"文\"}",
            "no U+FFFD in the healed line"
        );

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn normal_multi_line_and_incremental_append_regression() {
        let path = temp_jsonl("normal");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        std::fs::write(&path, b"{\"n\":1}\n\n{\"n\":2}\n").unwrap();
        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 2, "blank line skipped");
        assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1]);

        // 无新字节 → 无重投
        let again = run_once(&path, &offsets, &seqs);
        assert!(again.is_empty(), "no re-emit on unchanged file");

        // 增量追加只发新行
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"{\"n\":3}\n").unwrap();
        drop(f);
        let out2 = run_once(&path, &offsets, &seqs);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].seq, 2);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// F14 审计修：torn tail 挂起时文件被非追加式重写、且新长度落在
    /// [consumed, 旧 EOF) 窗口内——seen_len 判定必须检出截断并全量重读，
    /// 不得从旧 consumed 错位读出垃圾行。
    #[test]
    fn rewrite_within_torn_window_detected_as_truncation() {
        let path = temp_jsonl("rewrite");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        // 19 字节：完整行(8) + torn tail(11)。consumed=8, seen_len=19。
        std::fs::write(&path, b"{\"a\":1}\n{\"a\":2,\"tor").unwrap();
        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 1);

        // 整体重写成 18 字节新内容：len(18) >= consumed(8) 但 < seen_len(19)
        std::fs::write(&path, b"{\"b\":111}\n{\"b\":2}\n").unwrap();
        let out2 = run_once(&path, &offsets, &seqs);
        assert_eq!(out2.len(), 2, "rewrite must be detected and re-read fully");
        assert_eq!(
            out2[0].raw, r#"{"b":111}"#,
            "no garbage line from a stale offset"
        );
        assert_eq!(out2[1].raw, r#"{"b":2}"#);
        assert_eq!(out2[0].seq, 1, "seq keeps climbing across truncation");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// F14 审计修：截断到空文件（早退路径）也必须重置游标，
    /// 文件重新长回超过旧 offset 时不得漏检、不得丢前缀。
    #[test]
    fn truncate_to_empty_then_regrow_reads_from_zero() {
        let path = temp_jsonl("regrow");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        std::fs::write(&path, b"{\"n\":1}\n{\"n\":2}\n").unwrap(); // 16 字节
        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 2);

        // 截断到 0（早退分支），再长出比旧 offset 更长的新内容
        std::fs::write(&path, b"").unwrap();
        let empty = run_once(&path, &offsets, &seqs);
        assert!(empty.is_empty());

        std::fs::write(&path, b"{\"m\":1}\n{\"m\":2}\n{\"m\":3}\n").unwrap(); // 24 > 16
        let out2 = run_once(&path, &offsets, &seqs);
        assert_eq!(out2.len(), 3, "must re-read from byte 0, no lost prefix");
        assert_eq!(out2[0].raw, r#"{"m":1}"#);
        assert_eq!(out2[0].seq, 2, "seq never resets");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// \r\n 行尾：raw 须与旧 BufRead::lines() 行为一致（剥 \n 及紧邻单个 \r），
    /// offset 按含 \r\n 的字节数推进。手写字节循环后这是新代码路径，必须钉住。
    #[test]
    fn crlf_line_endings_match_old_lines_behavior() {
        let path = temp_jsonl("crlf");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        std::fs::write(&path, b"{\"a\":1}\r\n{\"a\":2}\n").unwrap();
        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].raw, r#"{"a":1}"#, "\\r must be stripped");
        assert_eq!(out[1].raw, r#"{"a":2}"#);
        let key = path_key(&path);
        assert_eq!(offsets.lock().get(&key).unwrap().consumed, 17);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// 旧病 #3 回归测试：完整行内含非法 UTF-8 只 lossy 该行，不截断批次。
    #[test]
    fn invalid_utf8_in_complete_line_does_not_abort_batch() {
        let path = temp_jsonl("badutf8");
        let offsets = Arc::new(Mutex::new(HashMap::new()));
        let seqs = Arc::new(Mutex::new(HashMap::new()));

        let mut bytes = b"{\"a\":1}\n".to_vec();
        bytes.extend_from_slice(b"\xFF\xFEgarbage\n"); // 非法 UTF-8 完整行
        bytes.extend_from_slice(b"{\"a\":3}\n");
        std::fs::write(&path, &bytes).unwrap();

        let out = run_once(&path, &offsets, &seqs);
        assert_eq!(out.len(), 3, "batch must not be silently aborted");
        assert_eq!(out[2].raw, r#"{"a":3}"#, "lines after the bad one survive");
        assert!(
            out[1].raw.contains('\u{FFFD}'),
            "bad line lossy-decoded, still delivered"
        );

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
