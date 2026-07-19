//! Phase-0 daemon watcher: tails `<claude_dir>/projects/**.jsonl` plus the
//! `<claude_dir>/sessions/<PID>.json` files and turns filesystem activity into
//! [`Frame`]s on a bounded channel.
//!
//! # Architecture (the §5.4 slow-consumer guard)
//!
//! This module is split into two halves joined by a **bounded** `mpsc` channel
//! (see [`spawn`]):
//!
//! - the **reader** ([`watch_loop`]) owns the `notify-debouncer-mini` watcher,
//!   does the incremental per-file offset reads, assigns `seq`s, and *sends*
//!   [`Frame`]s into the channel via [`FrameSink`]. It uses `try_send`, so a full
//!   channel drops the frame rather than ever blocking the notify callback — but
//!   it **counts** the dropped frames and emits a [`Frame::Overflow`] signal once
//!   the channel drains (#32), so the client can warn that live lines were lost.
//! - the **writer** ([`crate::main`]'s stdout task) drains the channel and
//!   writes one wire line per frame. A slow SSH pipe back-pressures the channel
//!   (the writer awaits on a full pipe), and the bound on the channel means
//!   that back-pressure stops at the channel — it never reaches the inotify
//!   reader, so the kernel inotify queue is the only thing that can overflow.
//!
//! The reader runs on a dedicated blocking thread (`notify-debouncer-mini` is a
//! synchronous, `std::sync::mpsc`-based API) and talks to the async writer
//! through `tokio::sync::mpsc`.
//!
//! # Parity with `../src-tauri/src/watcher.rs`
//!
//! The incremental read mirrors `process_file`: a per-file [`ReadCursor`],
//! read from `cursor.consumed` up to the **last `\n`** in the new region — a
//! torn tail without a trailing `\n` is deferred to the next event, never
//! emitted half-way (Batch4-F14). BOM strip via
//! `trim_start_matches('\u{feff}')`, skip blank lines, and `is_subagent_path`
//! excludes any path containing a `subagents` segment. Truncation is detected
//! against `cursor.seen_len` (the observed EOF high-water mark, which covers a
//! deferred torn tail); on truncation the cursor resets to byte 0 **but the
//! per-file seq keeps climbing** (the seq comes from [`SeqCounter`], which is
//! never reset) — see [`read_new_lines`].
//!
//! Known non-parity (accepted): on a mid-read I/O error the monitor keeps the
//! complete lines it already consumed and advances the cursor past them, while
//! this daemon reads via one `fs::read` snapshot and gives up the whole pass
//! (cursor untouched). Both are at-least-once-safe.

use crate::wire::{Frame, SeqCounter};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use walkdir::WalkDir;

/// Bounded channel capacity between the reader and the stdout writer.
///
/// Large enough to absorb a `/resume` history burst without dropping, small
/// enough that a wedged writer cannot grow memory without bound. A full channel
/// drops frames with a warning (Phase-0 gap, see module docs).
pub const CHANNEL_CAPACITY: usize = 10_000;

/// notify-debouncer-mini debounce window, matching `../src-tauri/src/watcher.rs`.
const DEBOUNCE_MS: u64 = 100;

/// Spawn the watcher reader on a dedicated blocking thread and return the
/// receiving half of the bounded frame channel for the stdout writer to drain.
///
/// `claude_dir` is the resolved `~/.claude` (or `$CLAUDE_CONFIG_DIR`). The
/// reader watches `<claude_dir>/projects/` recursively and
/// `<claude_dir>/sessions/`.
pub fn spawn(claude_dir: PathBuf, with_bg: bool, tail_only: bool) -> mpsc::Receiver<Frame> {
    let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    // notify-debouncer-mini is a synchronous std::sync::mpsc API; run it on a
    // blocking thread and hand frames to the async writer over tokio mpsc.
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || watch_loop(claude_dir, tx, with_bg, tail_only))
        .expect("spawn jsonl-watcher thread");
    rx
}

/// The reader half: initial walkdir scan, then the live debouncer loop.
///
/// Runs on its own OS thread. `tx` is the bounded sender; it is wrapped in a
/// [`FrameSink`] whose [`FrameSink::send`] never blocks the notify callback and
/// turns dropped frames into an [`Frame::Overflow`] signal (#32).
fn watch_loop(claude_dir: PathBuf, tx: mpsc::Sender<Frame>, with_bg: bool, tail_only: bool) {
    let projects = claude_dir.join("projects");
    let sessions = claude_dir.join("sessions");

    let mut state = ReaderState::new(projects.clone(), with_bg, tail_only);
    // All frames go out through a FrameSink: a bounded-channel sender that counts
    // frames dropped on a full channel and emits a single `Overflow` signal once
    // the channel drains enough to accept it (#32). Never blocks this reader.
    let mut sink = FrameSink::new(tx);

    // --- Phase 1: synchronous initial scan. ---
    // Mirror the LOCAL watcher's `active_filter` (`session_map.is_session_active`):
    // only stream sessions whose PID is alive (sessions/<PID>.json + /proc/<pid>).
    // We scan sessions/ FIRST to build the active set; process_session_added marks
    // the sid active and rescans its jsonl so an already-running session snapshots
    // on startup. We deliberately do NOT walk projects/ unconditionally — pulling
    // every historical jsonl as a Tab is the bug this fixes; browsing history is
    // the Ctrl+H history browser's job (Phase 1 for remote).
    if sessions.is_dir() {
        for entry in WalkDir::new(&sessions).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if is_session_json(p) {
                process_session_added(p, &mut state, &mut sink);
            }
        }
    }

    // --- Phase 2: live watch. ---
    let (notify_tx, notify_rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(Duration::from_millis(DEBOUNCE_MS), notify_tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("debouncer init failed: {e}");
            return;
        }
    };
    // Watch projects recursively; watch sessions (flat) for PID.json add/remove.
    if projects.is_dir() {
        if let Err(e) = debouncer
            .watcher()
            .watch(&projects, RecursiveMode::Recursive)
        {
            tracing::error!("watch failed for {}: {e}", projects.display());
        }
    } else {
        tracing::warn!("projects dir does not exist: {}", projects.display());
    }
    if sessions.is_dir() {
        if let Err(e) = debouncer
            .watcher()
            .watch(&sessions, RecursiveMode::NonRecursive)
        {
            tracing::error!("watch failed for {}: {e}", sessions.display());
        }
    } else {
        tracing::warn!("sessions dir does not exist: {}", sessions.display());
    }

    // Live loop with a 2s poll tick. The tick detects a session whose PID died
    // WITHOUT its sessions/<PID>.json being deleted (Claude Code can leave a
    // stale file when force-killed) — mirroring the local STILL_ACTIVE check —
    // and archives that Tab via SessionRemoved.
    loop {
        match notify_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(events)) => {
                for ev in events {
                    let p = ev.path.as_path();
                    if is_jsonl(p) && !is_subagent_path(p) {
                        // process_jsonl skips sids not in active_sids.
                        process_jsonl(p, &mut state, &mut sink);
                    } else if is_session_json(p) {
                        // notify coalesces to "something happened to this path";
                        // decide add vs remove by current existence on disk.
                        if p.exists() {
                            process_session_added(p, &mut state, &mut sink);
                        } else {
                            process_session_removed(p, &mut state, &mut sink);
                        }
                    }
                }
            }
            Ok(Err(errs)) => tracing::warn!("debouncer error: {errs:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Liveness poll: archive sessions whose PID is no longer alive OR whose
        // PID was reused by a different process (procStart mismatch, #34).
        // Batch6-F22-②：引用计数感知——同 sid 可能有多个 pidfile（resume 时原
        // 进程未死），任一 PID 死亡不再误杀整个 sid。
        let dead: Vec<PathBuf> = state
            .sessions
            .iter()
            .filter(|(_, e)| !session_alive(e.pid, e.start))
            .map(|(k, _)| k.clone())
            .collect();
        for k in dead {
            if let Some(e) = state.sessions.remove(&k) {
                retire_sid_if_unreferenced(&e.sid, &mut state, &mut sink);
            }
        }

        if sink.is_closed() {
            break;
        }
    }
}

/// Reader-side bookkeeping shared across `process_*` calls.
///
/// Not behind a lock: the reader is single-threaded (one OS thread), so all
/// access is serialized by construction.
struct ReaderState {
    /// `<claude_dir>/projects` — used to rescan a session's jsonl when it becomes
    /// active (so its existing lines stream, mirroring the local watcher's
    /// force-rescan on session-added).
    projects: PathBuf,
    /// Per-file consumed byte offset, keyed by [`path_key`]. Reset to 0 on
    /// truncation; the climbing seq lives separately in [`Self::seqs`] so a
    /// truncation never rolls the seq back.
    offsets: HashMap<PathBuf, ReadCursor>,
    /// Per-file monotonic seq source. `SeqCounter` only ever climbs for a given
    /// path (it is never reset), so truncation resetting `offsets` cannot pull
    /// the seq back — exactly the `watcher.rs:243-247` invariant.
    seqs: SeqCounter,
    /// PID-file path → [`SessionEntry`] for sessions currently considered ACTIVE
    /// (announced via `SessionAdded`). The pid + captured procStart let the
    /// liveness poll detect both a dead process AND a **reused PID** (#34); the
    /// cached sid lets a file-delete still emit the right `SessionRemoved`.
    sessions: HashMap<PathBuf, SessionEntry>,
    /// Fast membership for the active-session filter: sids currently streaming.
    /// Mirrors the local watcher's `active_filter` — only sessions whose PID is
    /// alive on this host stream; historical jsonl is NOT pulled (that is the
    /// Ctrl+H history browser's job).
    active_sids: HashSet<String>,
    /// Batch7-F24：`--with-bg` 时放行 kind:"bg" 会话（宣告+流行，帧带元信息）；
    /// 默认 false = Batch6-F21 行为（bg 不算会话）。
    with_bg: bool,
    /// Batch8-F25：`--tail-only` 时连接不重放历史——初扫/宣告只推进 cursor 与
    /// seq 计数器到当前完整行数 L（行号语义，之后新行 seq 从 L 起），零行帧；
    /// 历史由 monitor 经 `--read-session` 旁路快照拉取（0..L'-1 由 monitor 编号，
    /// 重叠区被 (sid,seq) 去重吸收）。默认 false = 全量重放（旧 monitor 兼容）。
    tail_only: bool,
}

impl ReaderState {
    fn new(projects: PathBuf, with_bg: bool, tail_only: bool) -> Self {
        ReaderState {
            projects,
            offsets: HashMap::new(),
            seqs: SeqCounter::new(),
            sessions: HashMap::new(),
            active_sids: HashSet::new(),
            with_bg,
            tail_only,
        }
    }
}

/// An ACTIVE session tracked by the reader, keyed in [`ReaderState::sessions`]
/// by its `sessions/<PID>.json` path.
///
/// `start` is the PID's procStart captured at session-add time (#34): on Linux
/// the `/proc/<pid>/stat` starttime (jiffies since boot). The liveness poll
/// compares the *current* procStart against this captured value so a PID that
/// the OS reused for an unrelated process is detected as dead (the original
/// session ended) rather than masquerading as still-live. `None` = procStart
/// unavailable (non-Linux smoke / read failure) → liveness degrades to plain
/// `/proc/<pid>` existence, matching the Phase-0 behaviour.
///
/// **Residual limitation (#34 §5, by design)**: `start` is captured at add-time
/// and never persisted. A daemon **restart** re-baselines `start` from the
/// *current* `/proc` on the next scan, so a PID that was reused *before* the
/// restart is indistinguishable from the original session. Probability is low
/// (restart ∧ PID-reuse ∧ reused-proc-still-alive) and this matches the local
/// watcher's identical non-persisted `proc_start`.
struct SessionEntry {
    pid: u32,
    sid: String,
    start: Option<u64>,
    /// Batch9-F27：pidfile 的官方 status（busy/idle/shell/waiting）与 waitingFor
    /// ——modify 事件 diff，变了发 session_status 帧（远端红绿灯）。
    status: Option<String>,
    waiting_for: Option<String>,
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
    /// daemon-01（gap#2）：本行末尾（含 `\n`）的累计**原始字节** offset，逐字节对齐 aterm `LineFramer`
    /// （计 `\r`、含 `\n`、残行不计）。**在原始字节上算**（非解码后串），故非法 UTF-8/CRLF 不错。
    pub byte_offset: u64,
}

/// Per-file read cursor, mirroring the monitor watcher's `FileCursor`
/// (Batch4-F14 audit fix).
///
/// - `consumed`: bytes of **complete lines** already emitted — the next
///   incremental read starts here. A deferred torn tail is not included.
/// - `seen_len`: high-water mark of the observed file length. Truncation must
///   be judged against this, not `consumed`: while a torn tail is pending,
///   `consumed < real EOF`, so a non-append rewrite whose new length lands in
///   `[consumed, seen_len)` would slip past a `len < consumed` check and read
///   garbage from a stale offset — silently, bypassing the truncation warn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadCursor {
    pub consumed: u64,
    pub seen_len: u64,
}

/// Pure bookkeeping core, factored out so it is unit-testable without a real
/// filesystem watcher.
///
/// Given the file's *full current bytes*, the prior [`ReadCursor`], the file's
/// `key` and the shared [`SeqCounter`], return the newly-appeared lines (with
/// seqs assigned) and the updated cursor. The seq for each kept line comes
/// from `seqs.next(key)`, so it is per-path monotonic and **never reset**.
///
/// Mirrors `../src-tauri/src/watcher.rs` `process_file`:
///
/// - read from `cursor.consumed`, but only consume **complete lines** — bytes
///   up to and including the last `\n` in the new region. A torn tail without
///   a trailing `\n` (the CLI caught mid-write) stays in the file: it is
///   neither emitted nor skipped over, and `consumed` stops right before it,
///   so the next event re-reads it once completed (Batch4-F14; the old
///   behaviour emitted the half line — the record was then lost for good after
///   the JSON parse failure — and a torn multibyte tail decayed into U+FFFD).
///   Accepted trade-off: a final line that is complete JSON but never gets its
///   `\n` (writer killed between the two writes) is never emitted if the file
///   never grows again — real jsonl ends with `\n` (8/8 sampled);
/// - **truncation**: judged against the high-water mark
///   (`len < cursor.seen_len`), so a rewrite landing inside a pending torn-tail
///   window `[consumed, seen_len)` is still caught → start over from byte 0;
/// - on truncation the byte cursor resets but the seq keeps climbing (it comes
///   from `SeqCounter`, which never resets), so a client that already placed
///   the old seqs still sorts the new lines after them;
/// - strip a leading UTF-8 BOM (`\u{feff}`) and skip blank lines;
/// - the returned `raw` is the original (untrimmed) line, exactly as
///   `watcher.rs` pushes `line` (not `trimmed`) into the batch.
pub fn read_new_lines(
    bytes: &[u8],
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
) -> (Vec<ReadLine>, ReadCursor) {
    let len = bytes.len() as u64;
    // Truncation guard against the high-water mark (see ReadCursor docs).
    let truncated = len < cursor.seen_len;
    let start = if truncated { 0 } else { cursor.consumed };
    if truncated && len > 0 {
        // Parity with the monitor's truncation warn (INVARIANTS §25: re-reads
        // hand out new seqs — must leave a trace; silence made an old
        // mis-folding bug near-impossible to diagnose). len == 0 re-reads
        // nothing, so stay quiet like the monitor.
        tracing::warn!(
            "jsonl truncated (len {len} < seen_len {}), full re-read with new seqs: {key}",
            cursor.seen_len
        );
    }

    let mut out = Vec::new();
    let mut consumed: u64 = 0;
    if start < len {
        let slice = &bytes[start as usize..];
        // Only the region ending at the last '\n' is complete; a torn tail
        // (mid-write, possibly mid-multibyte) is deferred to the next event.
        let complete_end = slice.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        consumed = complete_end as u64;
        // daemon-01（gap#2）：**在原始字节上逐行切**（非先解码整段再 `.lines()`）——因为 `byte_offset` 必须是
        // 累计原始字节（对齐 aterm `LineFramer`：计 `\r`、含 `\n`），而解码后串的字节位在非法 UTF-8（U+FFFD 替换
        // 3 字节换 1 字节）会漂。每行的原始内容单独 lossy 解码（残行已在 tail 外，故整行 multibyte 完整、安全）。
        let mut pos = 0usize; // 相对 slice 的原始字节游标
        while pos < complete_end {
            // 完整区内必有 '\n'（complete_end 到最后一个 '\n' 之后）。
            let nl = slice[pos..complete_end]
                .iter()
                .position(|&b| b == b'\n')
                .expect("complete region ends at a '\\n'");
            let content = &slice[pos..pos + nl]; // 行内容原始字节（不含 '\n'，可能尾随 '\r'）
            let line_end = pos + nl + 1; // 本行末尾（含 '\n'）在 slice 内的原始字节位
                                         // raw = 内容解码 + 剥尾随 '\r'（对齐 aterm：raw 无 CRLF/无尾 \n；但 offset **计** `\r`）。
            let text = String::from_utf8_lossy(content);
            let raw = text.strip_suffix('\r').unwrap_or(&text);
            let is_blank = raw.trim_start_matches('\u{feff}').trim().is_empty();
            if !is_blank {
                // Seq from the never-reset per-path counter. Blank lines do not
                // call `next`, so they do not consume a seq.
                let seq = seqs.next(key);
                out.push(ReadLine {
                    seq,
                    raw: raw.to_string(),
                    byte_offset: start + line_end as u64,
                });
            }
            pos = line_end;
        }
    }
    // `consumed` advances only past complete lines (from the possibly
    // truncation-reset `start`), never past a deferred torn tail; `seen_len`
    // records the full observed length so a later rewrite inside the torn-tail
    // window is still detected as truncation.
    (
        out,
        ReadCursor {
            consumed: start + consumed,
            seen_len: len,
        },
    )
}

/// Read a JSONL file incrementally and send a [`Frame::Line`] per new line.
fn process_jsonl(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let Some(session_id) = file_stem_str(path) else {
        return;
    };
    // Active-session filter (mirrors the local watcher's `active_filter`): only
    // stream sessions whose PID is alive. Historical jsonl is never pulled.
    if !state.active_sids.contains(&session_id) {
        return;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_cursor = state.offsets.get(&key).copied().unwrap_or_default();
    let (lines, new_cursor) = read_new_lines(&bytes, prev_cursor, &key_str, &mut state.seqs);
    state.offsets.insert(key, new_cursor);
    let path_str = path.to_string_lossy().into_owned();
    for line in lines {
        // daemon-09（phase②）：turn-end 边沿在 raw **之外**额外算——先解析（畸形→None、不影响 Line）。
        // 在 raw move 进 Line 帧前抽出（避免 clone raw）。§2.1 不变量并存：Line 逐行照发**每一条**。
        let turn_uuid: Option<String> = serde_json::from_str::<serde_json::Value>(&line.raw)
            .ok()
            .and_then(|v| crate::turn_detect::turn_end_uuid(&v).map(str::to_string));
        sink.send(Frame::Line {
            session_id: session_id.clone(),
            path: path_str.clone(),
            seq: line.seq,
            raw: line.raw,
            byte_offset: line.byte_offset, // daemon-01 gap#2：累计原始字节（对齐 aterm LineFramer）
        });
        // **先 Line 后 TurnEnd**：对齐 aterm β 的按行序处理——TurnEnd 结算时 currentOffset 已含本行。
        // 方案 C raw-per-record、daemon 不 dedup（aterm rolling-latest+debounce baselineByPath 塌合，
        // #daemon 2026-07-18 定）。TurnEnd 不带 byte_offset（只 Line 带）。
        if let Some(uuid) = turn_uuid {
            sink.send(Frame::TurnEnd {
                session_id: session_id.clone(),
                uuid,
            });
        }
    }
}

/// A `sessions/<PID>.json` appeared (or was already present): read it, extract
/// `sessionId`, cache PID→sid, emit [`Frame::SessionAdded`].
///
/// Idempotent: if we already cached the same sid for this path, skip the emit
/// so a debounced modify event does not re-announce an existing session.
fn process_session_added(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let key = path_key(path);
    // PID is the sessions/<PID>.json filename stem.
    let Some(pid) = file_stem_str(path).and_then(|s| s.parse::<u32>().ok()) else {
        return;
    };
    let Some(bytes) = std::fs::read(path).ok() else {
        return;
    };
    let Some(sid) = parse_session_id(&bytes) else {
        return;
    };
    // Only ACTIVE if the process is actually alive (mirrors local STILL_ACTIVE).
    // A stale pidfile for a dead process is NOT an active session.
    if !pid_alive(pid) {
        return;
    }
    // Batch6-F21: interactivity gate. CC 2.1.x 的 daemon 后台任务
    // (--fork-session --resume) **会**写 sessions/<PID>.json（kind:"bg" +
    // jobId）——"子会话不注册 pidfile"的旧假设已过期。bg 进程是自己 pidfile
    // 的真作者（F20 身份证据对它们正确地放行），但不是交互会话、不该成 tab。
    // 保守规则（与本地 session_map 一字一致）：kind 字段存在且非 "interactive"
    // 才排除；旧 CC 不写该字段 → 放行。
    if let Some(kind) = parse_kind(&bytes) {
        if kind != "interactive" && !state.with_bg {
            // 审计 S1：若该 key 此前以 interactive 身份被 track（原地翻 kind /
            // PID 复用写同路径），对称走退休路径——与 F22-① 一致，免掉 poll 的
            // 2s 窗口，并补齐"同进程翻 kind"这条本地有、远端缺的清理。
            if let Some(old) = state.sessions.remove(&key) {
                retire_sid_if_unreferenced(&old.sid, state, sink);
            }
            tracing::debug!(
                "sessions json skipped (kind={kind}): {} pid {pid} is a non-interactive claude (bg task)",
                path.display()
            );
            return;
        }
    }
    // Batch9-F27：帧元信息一次解析（status diff 与后面的宣告帧共用）
    let meta: Option<serde_json::Value> = serde_json::from_slice(&bytes).ok();
    let meta_str = |k: &str| {
        meta.as_ref()
            .and_then(|v| v.get(k))
            .and_then(|x| x.as_str())
            .map(str::to_string)
    };
    // Idempotent: a debounced modify of an already-tracked session re-announces
    // nothing —— Batch9-F27：但 status/waitingFor 变了要发 session_status 帧
    // （远端红绿灯的唯一数据源；CC 仅在状态转换时重写 pidfile，天然稀疏）。
    if state.sessions.get(&key).map(|e| e.sid.as_str()) == Some(sid.as_str()) {
        let new_status = meta_str("status");
        let new_waiting = meta_str("waitingFor");
        let entry = state.sessions.get_mut(&key).expect("just checked");
        if entry.status != new_status || entry.waiting_for != new_waiting {
            entry.status = new_status.clone();
            entry.waiting_for = new_waiting.clone();
            sink.send(Frame::SessionStatus {
                sid: sid.clone(),
                status: new_status,
                waiting_for: new_waiting,
            });
        }
        return;
    }
    // Batch6-F22-①：同 pidfile 原地换 sid（/clear 等重写 sessionId）——旧 sid
    // 必须走 removed 路径：旧实现 insert 直接覆盖 entry，旧 sid 既不清
    // active_sids 也永不发 SessionRemoved、还被挤出活性 poll 遍历 → 假 live
    // 到断连（跨机审计实锤，本地 diff_sessions 按 sid 集合 diff 无此病）。
    // 引用计数感知：其它 pidfile 仍持旧 sid 时只解绑本 entry、不发帧。
    if let Some(old) = state.sessions.remove(&key) {
        retire_sid_if_unreferenced(&old.sid, state, sink);
    }
    // Batch5-F20: add-time imposter check. `/proc/<pid>` existing is NOT enough:
    // a stale pidfile (CC force-killed, tmux server killed, power loss — nothing
    // ever cleans sessions/ up) plus PID reuse by any long-lived process (tmux
    // server, pane shell, sshd …) used to sail through and stream the whole dead
    // session's history as a live zombie tab, un-healable because the #34
    // procStart baseline below was captured FROM the imposter itself.
    //
    // Primary evidence: the pidfile's own `procStart` field — on Linux CC writes
    // the process's /proc starttime ticks verbatim (audit-verified bit-identical
    // on live sessions), so equality with the CURRENT occupant's starttime is
    // exact process identity (the same PID+starttime pair #34 uses), immune to
    // every wall-clock concern. Fallback heuristics (field absent, or mismatch
    // that could be CC format drift rather than reuse): the real claude wrote
    // this pidfile while alive, so its start must not be later than the file's
    // mtime; and its cmdline must look like claude. Missing data degrades to
    // allow (same philosophy as the local procStart-absent fallback).
    let current_ticks = proc_starttime(pid);
    match add_time_verdict(
        parse_procstart_ticks(&bytes),
        current_ticks,
        start_epoch_from_ticks(current_ticks),
        file_mtime_epoch(path),
        proc_cmdline(pid).as_deref(),
    ) {
        AddTimeVerdict::Imposter(reason) => {
            tracing::warn!(
                "stale sessions json ignored ({reason}): {} pid {pid} is not the claude that wrote it",
                path.display()
            );
            return;
        }
        AddTimeVerdict::Alive => {}
    }
    // #34: the poll baseline. Reuse the very ticks the verdict just examined —
    // no second /proc read, so no verdict-to-baseline TOCTOU window.
    let start = current_ticks;
    state.sessions.insert(
        key,
        SessionEntry {
            pid,
            sid: sid.clone(),
            start,
            status: meta_str("status"),
            waiting_for: meta_str("waitingFor"),
        },
    );
    state.active_sids.insert(sid.clone());
    // Batch8-F25：先定位该 sid 的 jsonl（帧要带 path 供 monitor 旁路快照；
    // mtime 降序，first=当前活跃文件。会话刚起还没写首行时为空 → path=None，
    // 此时无历史可拉，后续行天然从 tail 全量到达）。
    let projects = state.projects.clone();
    let jsonls = find_sid_jsonls(&projects, &sid);
    // 历史处理按模式分流（Batch8-F25）：
    // - tail-only：**先 prime**（推进 cursor/seq 到当前完整行数 L，零行帧）——
    //   帧要带 first 文件的 L 供 monitor 校验快照完整性（审计 D-I2），prime
    //   无行帧故"帧先于行"契约不受影响；
    // - 全量（默认，旧 monitor 兼容）：帧先行，再照旧全量推流（镜像本地
    //   session-added 触发的 force-rescan）。
    let mut first_lines: Option<u64> = None;
    if state.tail_only {
        for (i, p) in jsonls.iter().enumerate() {
            let n = prime_file_cursor(p, state);
            if i == 0 {
                first_lines = Some(n);
            }
        }
    }
    sink.send(Frame::SessionAdded {
        sid: sid.clone(),
        session_kind: meta_str("kind"),
        cwd: meta_str("cwd"),
        name: meta_str("name"),
        path: jsonls.first().map(|p| p.to_string_lossy().into_owned()),
        lines: first_lines,
        status: meta_str("status"),
        waiting_for: meta_str("waitingFor"),
    });
    if !state.tail_only {
        for p in &jsonls {
            process_jsonl(p, state, sink);
        }
    }
}

/// A `sessions/<PID>.json` was deleted: look up the cached sid (the file is
/// gone, so we cannot read it now) and retire the sid if unreferenced.
fn process_session_removed(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let key = path_key(path);
    if let Some(e) = state.sessions.remove(&key) {
        retire_sid_if_unreferenced(&e.sid, state, sink);
    }
}

/// Batch6-F22：sid 退休的**唯一**出口——`sessions` 表中已无任何存活 entry 持有
/// 该 sid 时才清 active_sids + 发 [`Frame::SessionRemoved`]。同 sid 多 pidfile
/// （resume 时原进程未死）场景下，先死的那个只解绑、不误杀整个 tab。
/// 调用方约定：先从 `state.sessions` remove 掉当事 entry 再调本函数。
fn retire_sid_if_unreferenced(sid: &str, state: &mut ReaderState, sink: &mut FrameSink) {
    let still_referenced = state.sessions.values().any(|e| e.sid == sid);
    if still_referenced {
        tracing::debug!("sid {sid} still referenced by another pidfile; not retiring");
        return;
    }
    state.active_sids.remove(sid);
    sink.send(Frame::SessionRemoved {
        sid: sid.to_string(),
    });
}

/// Walk `projects/` for this session's jsonl (`<sid>.jsonl`, non-subagent) and
/// stream its already-present lines. Called when a session becomes active so an
/// already-running session snapshots on session-added (mirrors local force-rescan).
fn find_sid_jsonls(projects: &Path, sid: &str) -> Vec<std::path::PathBuf> {
    if !projects.is_dir() {
        return Vec::new();
    }
    let mut v: Vec<std::path::PathBuf> = WalkDir::new(projects)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| {
            is_jsonl(p)
                && !is_subagent_path(p)
                && p.file_stem().and_then(|s| s.to_str()) == Some(sid)
        })
        .collect();
    // Batch8 审计（缝合-R4）：同 sid 多 jsonl（项目目录改名后 resume）时
    // WalkDir 顺序未定义——按 mtime 降序让 first = 当前活跃文件（帧的 path/
    // lines 取 first，快照拉错陈文件 = 当前历史全缺）。
    v.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    v
}

/// Batch8-F25：tail-only 的初扫/宣告路径——把 cursor 与 seq 计数器推进到当前
/// **最后一个完整行**（F14 torn-line 语义：残行不计数、留给 tail 阶段），
/// 不发任何行帧。之后 notify 到来的新行 seq == 此刻完整行数 L（行号语义），
/// 与 monitor 快照侧的 0..L'-1 编号同处一个行号空间，重叠区被 (sid,seq)
/// 去重精确吸收（MASTERPLAN-batch8 §2）。
fn prime_file_cursor(path: &Path, state: &mut ReaderState) -> u64 {
    let Some(session_id) = file_stem_str(path) else {
        return 0;
    };
    if !state.active_sids.contains(&session_id) {
        return 0;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return 0;
    };
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev = state.offsets.get(&key).copied().unwrap_or_default();
    let (lines, cursor) = read_new_lines(&bytes, prev, &key_str, &mut state.seqs);
    state.offsets.insert(key, cursor);
    tracing::debug!(
        "primed {key_str}: cursor→{} (+{} lines suppressed, tail seq starts here)",
        cursor.consumed,
        lines.len()
    );
    // Batch8 审计 D-I2：返回 prime 后的行号计数器现值（= 完整行总数 L），
    // session_added 帧带给 monitor 做快照完整性校验（拉到的行数 < L = 快照
    // 中途断/daemon 报错——exit status 拿不到，行数校验更强）。
    state.seqs.peek(&key_str)
}

/// Whether `pid` currently exists as a process on this host (existence only).
///
/// Linux (the daemon's real target): `/proc/<pid>` existence. This is the
/// add-time gate; the reuse-proof check is [`session_alive`].
fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Non-Linux (Windows compile/smoke only — not the real target): treat as
        // alive so the cross-platform smoke still exercises the pipeline.
        let _ = pid;
        true
    }
}

/// The PID's procStart (start time), used to defend against PID reuse (#34).
///
/// Linux: the `starttime` field (jiffies since boot) from `/proc/<pid>/stat`.
/// Non-Linux (Windows smoke): `None` — liveness then degrades to existence only.
fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_starttime_from_stat(&stat)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Add-time verdict on whether the current occupant of a PID is plausibly the
/// claude process that wrote the `sessions/<PID>.json` pidfile (Batch5-F20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddTimeVerdict {
    Alive,
    Imposter(&'static str),
}

/// A process that started noticeably later than the pidfile's last write cannot
/// be its author. 60s absorbs clock fuzz (mtime granularity, btime rounding,
/// NTP slew) — real reuse gaps are hours-to-weeks, so the tolerance is safe.
const ADD_TIME_TOLERANCE_SECS: u64 = 60;

/// Pure decision core (unit-tested on every platform):
///
/// - **identity evidence（primary）**: the pidfile's `procStart` field equals
///   the current occupant's `/proc/<pid>/stat` starttime ticks → the occupant
///   IS the author (PID + starttime is exact process identity, the same pair
///   #34 relies on) — Alive, no further checks, immune to every wall-clock
///   concern (NTP steps, NFS mtime, btime drift). A **mismatch** is NOT
///   immediately fatal: it is either PID reuse (imposter) or a CC version
///   writing a different format into `procStart` (a hard reject on format
///   drift would black out every real session) — fall through, the heuristics
///   below catch the stale-pidfile case either way.
/// - **time evidence**: `proc_start_epoch > file_mtime_epoch + tolerance` →
///   imposter. CC rewrites its pidfile on every state transition, so the file's
///   mtime is a lower bound on "the real claude was alive at this instant"; a
///   later-started process is a PID-reuse squatter. Both sides are wall-clock
///   seconds from the same host clock (btime + starttime/USER_HZ vs mtime), so
///   there is no timezone concern. This also subsumes the reboot case: after a
///   reboot every process starts after btime > old mtime.
/// - **cmdline evidence**: a readable, non-empty cmdline that mentions neither
///   `claude` nor `node` is not a claude CLI (tmux, bash, sshd …).
/// - Missing data (absent procStart, unreadable stat/mtime/cmdline) skips that
///   check — degrade to allow, mirroring the local procStart-absent fallback.
fn add_time_verdict(
    pidfile_procstart_ticks: Option<u64>,
    current_starttime_ticks: Option<u64>,
    proc_start_epoch: Option<u64>,
    file_mtime_epoch: Option<u64>,
    cmdline: Option<&str>,
) -> AddTimeVerdict {
    // F74b(#43「父会话恒绿」总闸)：bg-spare = 守护池停泊的备用进程（cmdline 含 "bg-spare"）。
    // 它是真 claude 进程、会写合规 pidfile、procStart 自洽——**必须在 exact-identity 之前拦**，
    // 否则下面的 `recorded == current` 会把它判 Alive 而恒绿。语义上它不是一个运行中的会话。
    if let Some(cmd) = cmdline {
        if cmd.to_lowercase().contains("bg-spare") {
            return AddTimeVerdict::Imposter("bg-spare");
        }
    }
    if let (Some(recorded), Some(current)) = (pidfile_procstart_ticks, current_starttime_ticks) {
        if recorded == current {
            return AddTimeVerdict::Alive; // exact identity: author confirmed
        }
        // mismatch: fall through to the heuristics (see doc comment)
    }
    if let (Some(start), Some(mtime)) = (proc_start_epoch, file_mtime_epoch) {
        if start > mtime + ADD_TIME_TOLERANCE_SECS {
            return AddTimeVerdict::Imposter("started-after-pidfile");
        }
    }
    if let Some(cmd) = cmdline {
        let lower = cmd.to_lowercase();
        if !lower.trim().is_empty() && !lower.contains("claude") && !lower.contains("node") {
            return AddTimeVerdict::Imposter("cmdline");
        }
    }
    AddTimeVerdict::Alive
}

/// Parse the pidfile's `kind` field ("interactive" / "bg" …，Batch6-F21)。
/// None = 字段缺失（旧 CC）或不可读 → 调用方放行。
fn parse_kind(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("kind")?.as_str().map(str::to_string)
}

/// Parse the pidfile's `procStart` field as starttime ticks. CC writes it as a
/// decimal string on Linux（audit-verified verbatim /proc starttime ticks）；
/// accept a bare number too. Anything else → None（fallback heuristics apply）.
fn parse_procstart_ticks(bytes: &[u8]) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

/// Parse the boot time (`btime <epoch-secs>` line) out of `/proc/stat` content.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_btime(proc_stat: &str) -> Option<u64> {
    proc_stat.lines().find_map(|l| {
        l.strip_prefix("btime ")
            .and_then(|v| v.trim().parse::<u64>().ok())
    })
}

/// `/proc` time values are exported in USER_HZ ticks, which is a compile-time
/// constant 100 on every mainstream Linux arch (independent of the kernel's
/// internal HZ) — hardcoding avoids a libc dependency for sysconf(_SC_CLK_TCK).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const USER_HZ: u64 = 100;

/// Starttime ticks → wall-clock epoch seconds: `/proc/stat` btime + ticks/USER_HZ.
///
/// btime is read FRESH on every call, deliberately un-cached: the kernel
/// computes it per-read as (wall clock − CLOCK_BOOTTIME), so an NTP **step**
/// moves it. A cached value taken before a backwards step would leave a
/// constant offset that mis-kills every future real session with no self-heal
/// (F20 audit I-1). Session-add is rare; one small /proc read is free.
fn start_epoch_from_ticks(ticks: Option<u64>) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let btime = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_btime(&s))?;
        Some(btime + ticks? / USER_HZ)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ticks;
        None
    }
}

/// The pidfile's mtime as epoch seconds (None on any error → check skipped).
fn file_mtime_epoch(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// `/proc/<pid>/cmdline`, NUL separators turned into spaces, lossily decoded.
/// None when unreadable (vanished PID, permissions) → check skipped.
fn proc_cmdline(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let spaced: Vec<u8> = bytes
            .into_iter()
            .map(|b| if b == 0 { b' ' } else { b })
            .collect();
        Some(String::from_utf8_lossy(&spaced).into_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Parse the `starttime` (field 22) out of a `/proc/<pid>/stat` line.
///
/// **The comm gotcha**: field 2 is `(comm)` and the executable name can contain
/// spaces and parentheses (e.g. `(my proc)` or `((odd))`). Splitting the whole
/// line on whitespace is therefore wrong. The robust parse — used by ps/htop —
/// is to find the **last** `')'`, then count fields in the remainder: the first
/// token after it is field 3 (`state`), so `starttime` (field 22) is token index
/// `22 - 3 = 19` (0-based) of the post-`)` whitespace split.
///
/// Only called from the Linux branch of [`proc_starttime`] (and by unit tests on
/// every platform); on a non-Linux build the function body is unreferenced.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_starttime_from_stat(stat: &str) -> Option<u64> {
    /// 0-based index of `starttime` (field 22) within the tokens that follow the
    /// closing paren of `comm` (field 3 = `state` is token 0).
    const STARTTIME_IDX_AFTER_COMM: usize = 22 - 3;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm
        .split_whitespace()
        .nth(STARTTIME_IDX_AFTER_COMM)?
        .parse::<u64>()
        .ok()
}

/// Reuse-proof liveness for an ACTIVE session (#34): the PID must still exist
/// **and** (when a procStart was captured at add-time) its current procStart
/// must match. A mismatch means the OS reused the PID for a different process —
/// the original session has ended.
///
/// Wires the real `/proc` reads into the pure [`is_same_live_process`] decision.
fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    is_same_live_process(exists, expected_start, current_start)
}

/// Pure liveness decision (testable without a real `/proc`), given whether the
/// PID currently **exists**, the procStart **captured** at add-time, and the
/// procStart **read now**.
///
/// Key correctness rule (#34): a PID reuse only ever shows up as a
/// *successfully-read, DIFFERENT* current start. So the only case that declares
/// "dead by reuse" is `(Some(captured), Some(current))` with `captured != current`.
/// Every other arm where the PID still exists returns alive — in particular a
/// **transient `/proc/<pid>/stat` read failure** (`current == None`) must NOT
/// false-archive a process that demonstrably still exists (that would be a
/// regression vs. the Phase-0 existence-only check). If the process is truly
/// gone, `exists` is already `false` and we return dead.
fn is_same_live_process(
    exists: bool,
    expected_start: Option<u64>,
    current_start: Option<u64>,
) -> bool {
    if !exists {
        return false;
    }
    match (expected_start, current_start) {
        // Baseline captured AND current readable: same process iff equal.
        (Some(captured), Some(current)) => captured == current,
        // No baseline, or current unreadable right now: existence is all we can
        // assert. Do not archive a still-existing PID on missing start info.
        _ => true,
    }
}

/// Pure parse of the `sessionId` field out of a sessions JSON blob.
fn parse_session_id(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("sessionId")?.as_str().map(str::to_string)
}

/// The reader's send half: a bounded-channel sender that turns a wedged pipe
/// into an explicit **overflow signal** (#32) instead of silently losing data.
///
/// A full channel means the writer/SSH pipe is wedged. We still `try_send` (never
/// blocking the notify reader), but now we **count** the frames we had to drop
/// and, once the channel drains enough to accept it, emit a single
/// [`Frame::Overflow`] carrying that count. The client warns the user that live
/// lines were lost. One signal per congestion burst — naturally throttled.
struct FrameSink {
    tx: mpsc::Sender<Frame>,
    /// Frames dropped since the last successfully-sent `Overflow` signal.
    dropped: u64,
}

impl FrameSink {
    fn new(tx: mpsc::Sender<Frame>) -> Self {
        FrameSink { tx, dropped: 0 }
    }

    /// Send `frame`, first flushing any owed overflow signal.
    ///
    /// Order matters: we try to emit the pending `Overflow` *before* the real
    /// frame so the client learns "you lost N frames" no later than the next
    /// frame it receives. If the channel is still full, we keep owing the count
    /// (it only ever grows until a send succeeds); a closed channel is a quiet
    /// shutdown (the loop checks `is_closed`).
    fn send(&mut self, frame: Frame) {
        if self.dropped > 0 {
            match self.tx.try_send(Frame::Overflow {
                dropped: self.dropped,
            }) {
                Ok(()) => {
                    tracing::warn!(
                        "recovered from frame-channel overflow; signalled {} dropped frame(s)",
                        self.dropped
                    );
                    self.dropped = 0;
                }
                // Still wedged: keep owing the count, retry on the next send.
                Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
        match self.tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped += 1;
                tracing::warn!(
                    "frame channel full (cap {CHANNEL_CAPACITY}); dropping frame \
                     ({} dropped since last overflow signal)",
                    self.dropped
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Writer gone (shutdown). Nothing to do; the loop checks is_closed.
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// `true` for a regular `*.jsonl` file.
fn is_jsonl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "jsonl")
}

/// `true` for a `sessions/<PID>.json` file. We only ever feed this paths under
/// the sessions dir, so an extension check suffices.
fn is_session_json(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "json")
}

/// subagent JSONL is excluded: any path containing a `subagents` segment.
/// Mirrors `../src-tauri/src/watcher.rs::is_subagent_path`.
fn is_subagent_path(p: &Path) -> bool {
    p.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("subagents"))
}

fn file_stem_str(p: &Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

/// Case-fold the path on Windows so notify's NTFS case variance does not double
/// emit; on other platforms keep the path verbatim. Mirrors `watcher.rs`.
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

    /// Build a byte buffer from JSONL lines joined with `\n` and a trailing one.
    fn jsonl(lines: &[&str]) -> Vec<u8> {
        let mut s = String::new();
        for l in lines {
            s.push_str(l);
            s.push('\n');
        }
        s.into_bytes()
    }

    const KEY: &str = "/some/session.jsonl";

    #[test]
    fn appending_lines_advances_offset_and_seq_monotonically() {
        let mut seqs = SeqCounter::new();

        let first = jsonl(&[r#"{"a":1}"#, r#"{"a":2}"#]);
        let (out, cur) = read_new_lines(&first, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].seq, 0);
        assert_eq!(out[1].seq, 1);
        assert_eq!(cur.consumed, first.len() as u64);
        assert_eq!(cur.seen_len, first.len() as u64);

        // Append two more lines (same prefix bytes, longer file).
        let mut second = first.clone();
        second.extend_from_slice(jsonl(&[r#"{"a":3}"#, r#"{"a":4}"#]).as_slice());
        let (out2, cur2) = read_new_lines(&second, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2, "only the newly-appended lines come back");
        assert_eq!(out2[0].seq, 2);
        assert_eq!(out2[1].seq, 3);
        assert_eq!(cur2.consumed, second.len() as u64);
        assert_eq!(out2[0].raw, r#"{"a":3}"#);
    }

    #[test]
    fn byte_offset_matches_aterm_lineframer() {
        // daemon-01（gap#2）：Line.byte_offset **逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、
        // 含 `\n`、残行不计、在**原始字节**上算（非解码后串）。移植自 aterm LineFramerTest 的关键语料。
        let mut seqs = SeqCounter::new();
        // aterm feedFramedCountsCrlfAndMultibyteRawBytes: "你\r\nx\n" → endOffset [5,7]
        // 你=3B + \r + \n = 5；x + \n = 2 → 累计 7。raw 剥 \r/\n。
        let (out, cur) = read_new_lines(
            "你\r\nx\n".as_bytes(),
            ReadCursor::default(),
            KEY,
            &mut seqs,
        );
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].raw.as_str(), out[0].byte_offset), ("你", 5));
        assert_eq!((out[1].raw.as_str(), out[1].byte_offset), ("x", 7));
        assert_eq!(cur.consumed, 7);

        // 无 CRLF 累计：jsonl(["ab","cde"]) = "ab\ncde\n" → [3, 7]。
        let mut s2 = SeqCounter::new();
        let (o2, _) = read_new_lines(&jsonl(&["ab", "cde"]), ReadCursor::default(), KEY, &mut s2);
        assert_eq!((o2[0].byte_offset, o2[1].byte_offset), (3, 7));

        // 增量续读用**绝对**文件 offset（start + line_end），非本次 slice 相对：
        let mut s3 = SeqCounter::new();
        let first = jsonl(&["x"]); // "x\n" = 2B
        let (_, cur3) = read_new_lines(&first, ReadCursor::default(), KEY, &mut s3);
        let mut second = first.clone();
        second.extend_from_slice(&jsonl(&["yy"])); // + "yy\n"
        let (o3, _) = read_new_lines(&second, cur3, KEY, &mut s3);
        assert_eq!(o3.len(), 1);
        assert_eq!(o3[0].byte_offset, 5, "绝对 offset = 2(x\\n) + 3(yy\\n)");

        // 残行（torn tail）不计入 byte_offset：
        let mut s4 = SeqCounter::new();
        let (o4, cur4) = read_new_lines(
            b"done\nhalf-no-newline",
            ReadCursor::default(),
            KEY,
            &mut s4,
        );
        assert_eq!(o4.len(), 1);
        assert_eq!((o4[0].byte_offset, cur4.consumed), (5, 5)); // done\n=5；残行不计

        // 空行跳过、不占 byte_offset 连续性（offset 仍按原始字节累计）：
        let mut s5 = SeqCounter::new();
        let (o5, _) = read_new_lines(b"a\n\nb\n", ReadCursor::default(), KEY, &mut s5);
        assert_eq!(o5.len(), 2); // 空行跳过
        assert_eq!((o5[0].byte_offset, o5[1].byte_offset), (2, 5)); // a\n=2；空\n 占 1B（→3，跳过）；b\n 到 5
    }

    #[test]
    fn no_new_bytes_yields_nothing_and_does_not_bump_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"x":1}"#]);
        let (_, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        // Re-process identical bytes: consumed == len, start >= len, nothing new.
        let (again, cur2) = read_new_lines(&buf, cur, KEY, &mut seqs);
        assert!(again.is_empty());
        // A fresh read of the same key still hands out seq 1 only if a line was
        // produced; here nothing new, so the next live line would be seq 1.
        assert_eq!(seqs.next(KEY), 1, "seq must not have advanced past 1");
        assert_eq!(cur2.consumed, buf.len() as u64);
    }

    #[test]
    fn truncation_resets_offset_but_seq_keeps_climbing() {
        let mut seqs = SeqCounter::new();

        let big = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#]);
        let (out, big_cur) = read_new_lines(&big, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1, 2]);

        // Simulated truncation: file is now SHORTER than the recorded cursor.
        let small = jsonl(&[r#"{"n":99}"#]);
        assert!((small.len() as u64) < big_cur.seen_len, "test precondition");
        let (out2, small_cur) = read_new_lines(&small, big_cur, KEY, &mut seqs);

        // Cursor reset to 0 then re-advanced to the new (smaller) length.
        assert_eq!(small_cur.consumed, small.len() as u64);
        // The whole truncated file is re-read from byte 0 ...
        assert_eq!(out2.len(), 1);
        // ... but seq KEEPS CLIMBING (3, not back to 0): the climbing invariant.
        assert_eq!(out2[0].seq, 3, "seq must never reset on truncation");
    }

    /// F14 audit fix: a rewrite whose new length lands inside the pending
    /// torn-tail window [consumed, seen_len) must still be detected as
    /// truncation — no garbage line from a stale offset.
    #[test]
    fn rewrite_within_torn_window_detected_as_truncation() {
        let mut seqs = SeqCounter::new();
        // 19 bytes: complete line (8) + torn tail (11). consumed=8, seen_len=19.
        let torn = b"{\"a\":1}\n{\"a\":2,\"tor".to_vec();
        let (out, cur) = read_new_lines(&torn, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 1);
        assert_eq!(
            cur,
            ReadCursor {
                consumed: 8,
                seen_len: 19
            }
        );

        // Whole-file rewrite to 18 bytes: len >= consumed(8) but < seen_len(19).
        let rewritten = b"{\"b\":111}\n{\"b\":2}\n".to_vec();
        let (out2, cur2) = read_new_lines(&rewritten, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2, "rewrite must be detected and re-read fully");
        assert_eq!(
            out2[0].raw, r#"{"b":111}"#,
            "no garbage from a stale offset"
        );
        assert_eq!(out2[0].seq, 1, "seq keeps climbing across truncation");
        assert_eq!(cur2.consumed, rewritten.len() as u64);
    }

    /// Truncate-to-empty must reset the cursor so a regrown file (even one
    /// longer than the old consumed offset) is read from byte 0.
    #[test]
    fn truncate_to_empty_then_regrow_reads_from_zero() {
        let mut seqs = SeqCounter::new();
        let old = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#]); // 16 bytes
        let (_, cur) = read_new_lines(&old, ReadCursor::default(), KEY, &mut seqs);

        let (empty_out, cur2) = read_new_lines(&[], cur, KEY, &mut seqs);
        assert!(empty_out.is_empty());
        assert_eq!(
            cur2,
            ReadCursor {
                consumed: 0,
                seen_len: 0
            }
        );

        let regrown = jsonl(&[r#"{"m":1}"#, r#"{"m":2}"#, r#"{"m":3}"#]); // 24 > 16
        let (out, cur3) = read_new_lines(&regrown, cur2, KEY, &mut seqs);
        assert_eq!(out.len(), 3, "must re-read from byte 0, no lost prefix");
        assert_eq!(out[0].raw, r#"{"m":1}"#);
        assert_eq!(out[0].seq, 2, "seq never resets");
        assert_eq!(cur3.consumed, regrown.len() as u64);
    }

    /// \r\n endings: raw must match str::lines() semantics (strip \n plus one
    /// adjacent \r); consumed advances by the byte count including \r\n.
    #[test]
    fn crlf_line_endings_are_stripped_like_lines() {
        let mut seqs = SeqCounter::new();
        let buf = b"{\"a\":1}\r\n{\"a\":2}\n".to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].raw, r#"{"a":1}"#, "\\r must be stripped");
        assert_eq!(out[1].raw, r#"{"a":2}"#);
        assert_eq!(cur.consumed, 17);
    }

    /// Old-bug regression: invalid UTF-8 inside a COMPLETE line is lossy-decoded
    /// for that line only — it must not abort the rest of the batch.
    #[test]
    fn invalid_utf8_in_complete_line_does_not_abort_batch() {
        let mut seqs = SeqCounter::new();
        let mut buf = b"{\"a\":1}\n".to_vec();
        buf.extend_from_slice(b"\xFF\xFEgarbage\n");
        buf.extend_from_slice(b"{\"a\":3}\n");
        let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 3, "batch must not be silently aborted");
        assert_eq!(out[2].raw, r#"{"a":3}"#, "lines after the bad one survive");
        assert!(out[1].raw.contains('\u{FFFD}'), "bad line delivered lossy");
    }

    #[test]
    fn leading_bom_is_stripped_for_the_empty_check_and_line_is_kept() {
        let mut seqs = SeqCounter::new();
        // A line that is ONLY a BOM + whitespace must be treated as empty.
        let only_bom = "\u{feff}   \n".as_bytes().to_vec();
        let (out, _) = read_new_lines(&only_bom, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "BOM-only/blank line is skipped");
        assert_eq!(seqs.next(KEY), 0, "skipped line must not consume a seq");

        // A BOM-prefixed real line is kept (and not double counted).
        let mut seqs2 = SeqCounter::new();
        let bom_line = "\u{feff}{\"k\":1}\n".as_bytes().to_vec();
        let (out2, _) = read_new_lines(&bom_line, ReadCursor::default(), KEY, &mut seqs2);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].seq, 0);
    }

    #[test]
    fn empty_lines_are_skipped_and_do_not_consume_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"a":1}"#, "", "   ", r#"{"a":2}"#, ""]);
        let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2, "two blank/whitespace lines dropped");
        assert_eq!(out[0].seq, 0);
        assert_eq!(out[1].seq, 1);
        // Only two seqs were consumed; the next one is 2.
        assert_eq!(seqs.next(KEY), 2);
    }

    #[test]
    fn subagents_path_is_excluded() {
        // A path containing a `subagents` segment must be filtered.
        let p = Path::new("/home/u/.claude/projects/foo/subagents/bar.jsonl");
        assert!(is_subagent_path(p));
        // Case-insensitive, mirrors watcher.rs.
        let p2 = Path::new("/home/u/.claude/projects/foo/SubAgents/bar.jsonl");
        assert!(is_subagent_path(p2));
        // A normal session file is not excluded.
        let p3 = Path::new("/home/u/.claude/projects/foo/abc-123.jsonl");
        assert!(!is_subagent_path(p3));
    }

    #[test]
    fn is_jsonl_and_is_session_json_classify_correctly() {
        assert!(is_jsonl(Path::new("/x/abc.jsonl")));
        assert!(!is_jsonl(Path::new("/x/abc.json")));
        assert!(is_session_json(Path::new("/x/1234.json")));
        assert!(!is_session_json(Path::new("/x/1234.jsonl")));
    }

    #[test]
    fn parse_session_id_extracts_the_field() {
        let blob = br#"{"sessionId":"abc-123","pid":4242}"#;
        assert_eq!(parse_session_id(blob), Some("abc-123".to_string()));
        // Missing field / wrong type / garbage → None.
        assert_eq!(parse_session_id(br#"{"pid":1}"#), None);
        assert_eq!(parse_session_id(br#"{"sessionId":5}"#), None);
        assert_eq!(parse_session_id(b"not json"), None);
    }

    #[test]
    fn torn_line_without_trailing_newline_is_deferred() {
        let mut seqs = SeqCounter::new();
        // Complete line + torn tail (no trailing \n).
        let buf = b"{\"a\":1}\n{\"a\":2,\"tex".to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 1, "torn tail must not be emitted");
        assert_eq!(out[0].raw, r#"{"a":1}"#);
        assert_eq!(
            cur.consumed, 8,
            "consumed stops after the complete line, not at EOF"
        );
        assert_eq!(cur.seen_len, buf.len() as u64, "seen_len covers the tail");

        // The tail completes (plus one more full line) — emitted exactly once,
        // seq continuous across the deferral.
        let mut healed = buf.clone();
        healed.extend_from_slice(b"t\":\"x\"}\n{\"a\":3}\n");
        let (out2, cur2) = read_new_lines(&healed, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2);
        assert_eq!(out2[0].raw, r#"{"a":2,"text":"x"}"#);
        assert_eq!(out2[0].seq, 1);
        assert_eq!(out2[1].seq, 2);
        assert_eq!(cur2.consumed, healed.len() as u64);
    }

    #[test]
    fn torn_multibyte_tail_does_not_decay_into_replacement_char() {
        let mut seqs = SeqCounter::new();
        let full = "{\"t\":\"文\"}\n".as_bytes(); // 文 = E6 96 87
        let torn = &full[..7]; // cut inside the multibyte sequence
        let (out, cur) = read_new_lines(torn, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "mid-multibyte torn tail must be deferred");
        assert_eq!(cur.consumed, 0);

        let (out2, cur2) = read_new_lines(full, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].raw, "{\"t\":\"文\"}", "no U+FFFD after healing");
        assert_eq!(cur2.consumed, full.len() as u64);
    }

    #[test]
    fn fully_unterminated_single_line_is_deferred() {
        // A file whose only content is a line still being written: nothing is
        // complete yet, so nothing is emitted and the cursor stays put.
        let mut seqs = SeqCounter::new();
        let buf = br#"{"only":1}"#.to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "unterminated line is deferred, not emitted");
        assert_eq!(cur.consumed, 0, "consumed must not advance past the tail");
        assert_eq!(cur.seen_len, buf.len() as u64);
        assert_eq!(seqs.next(KEY), 0, "deferral must not consume a seq");
    }

    // === Batch5-F20 add-time imposter check ===

    #[test]
    fn imposter_when_proc_started_after_pidfile() {
        // pidfile last written at t=1000, process started at t=2000 (> 1000+60).
        let v = add_time_verdict(None, None, Some(2000), Some(1000), None);
        assert_eq!(v, AddTimeVerdict::Imposter("started-after-pidfile"));
        // Reboot case is the same shape: old mtime, post-boot start.
        let v2 = add_time_verdict(
            None,
            None,
            Some(1_700_000_000),
            Some(1_600_000_000),
            Some("claude"),
        );
        assert_eq!(
            v2,
            AddTimeVerdict::Imposter("started-after-pidfile"),
            "time evidence must win even with a claude-looking cmdline (a NEW claude did not write the OLD pidfile)"
        );
    }

    #[test]
    fn alive_within_tolerance() {
        // Started slightly after mtime but inside the 60s fuzz window.
        assert_eq!(
            add_time_verdict(None, None, Some(1030), Some(1000), None),
            AddTimeVerdict::Alive
        );
        // Started before mtime (the normal case: claude starts, then writes).
        assert_eq!(
            add_time_verdict(None, None, Some(900), Some(1000), None),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn imposter_by_cmdline() {
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("tmux new-session -d")),
            AddTimeVerdict::Imposter("cmdline")
        );
        assert_eq!(
            add_time_verdict(None, None, Some(900), Some(1000), Some("-bash")),
            AddTimeVerdict::Imposter("cmdline"),
            "time check passing must not mask a non-claude cmdline"
        );
    }

    #[test]
    fn imposter_by_bg_spare_before_exact_identity() {
        // F74b(#43)：bg-spare 优先于 exact-identity——即便 procStart 自洽（recorded==current）
        // 也判 Imposter（否则守护池停泊备用进程恒绿）。
        assert_eq!(
            add_time_verdict(Some(555), Some(555), None, None, Some("claude bg-spare")),
            AddTimeVerdict::Imposter("bg-spare"),
            "bg-spare 必须在 exact-identity Alive 之前拦下"
        );
        assert_eq!(
            add_time_verdict(
                None,
                None,
                None,
                None,
                Some("/usr/bin/claude bg-spare --foo")
            ),
            AddTimeVerdict::Imposter("bg-spare")
        );
        // 普通 claude 会话不受影响（procStart 自洽仍 Alive）。
        assert_eq!(
            add_time_verdict(Some(555), Some(555), None, None, Some("claude --resume x")),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn claude_like_cmdlines_pass() {
        for cmd in [
            "claude --resume abc",
            "/usr/bin/node /home/u/.local/bin/claude",
            "NODE_OPTIONS=x node cli.js",
            "Claude", // case-insensitive
        ] {
            assert_eq!(
                add_time_verdict(None, None, Some(900), Some(1000), Some(cmd)),
                AddTimeVerdict::Alive,
                "{cmd}"
            );
        }
    }

    #[test]
    fn missing_data_degrades_to_allow() {
        assert_eq!(
            add_time_verdict(None, None, None, None, None),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, Some(2000), None, None),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, None, Some(1000), None),
            AddTimeVerdict::Alive
        );
        // Empty cmdline (kernel threads read as empty) is not evidence.
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("")),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("   ")),
            AddTimeVerdict::Alive
        );
    }

    // === Batch6-F22：远端会话生命周期 ===

    /// 同 pidfile 原地换 sid（/clear）：旧 sid 立即 Removed、新 sid Added，
    /// active_sids 恰含新 sid（跨机审计实锤的假 live 泄漏回归测试）。
    #[cfg(target_os = "linux")]
    #[test]
    fn sid_change_in_place_retires_old_sid() {
        let dir = std::env::temp_dir().join(format!("ccm-sidchange-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join(format!("{pid}.json"));

        let write = |sid: &str| {
            std::fs::write(
                &path,
                format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
            )
            .unwrap();
        };
        write("sid-1");
        process_session_added(&path, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-1"));

        write("sid-2"); // /clear：同文件重写 sessionId
        process_session_added(&path, &mut state, &mut sink);
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "sid-1"),
            "old sid must be retired BEFORE the new announcement"
        );
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-2"));
        assert!(!state.active_sids.contains("sid-1"));
        assert!(state.active_sids.contains("sid-2"));
        assert_eq!(state.sessions.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// daemon-09：`process_jsonl` 对 turn-end 记录发 **Line 后紧跟 TurnEnd**；非 turn-end 只发 Line；
    /// **畸形行照发 Line、不 panic、无 TurnEnd**（§2.1 逐行转发 + turn-end 是 raw 之外额外边沿）。
    #[test]
    fn process_jsonl_emits_turn_end_after_line_raw_per_record() {
        let dir = std::env::temp_dir().join(format!("ccm-turnend-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join("sess-1.jsonl");
        state.active_sids.insert("sess-1".to_string()); // process_jsonl 门控
                                                        // 三行：非 turn-end user / turn-end assistant / 畸形。
        let content = concat!(
            r#"{"type":"user","message":{}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"u-2","message":{"stop_reason":"end_turn"}}"#,
            "\n",
            "not json at all",
            "\n",
        );
        std::fs::write(&path, content).unwrap();
        process_jsonl(&path, &mut state, &mut sink);
        // 帧序：Line(user,seq0) / Line(end_turn,seq1) → TurnEnd(u-2) / Line(畸形,seq2)。
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })),
            "user 行 Line"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })),
            "end_turn 行 Line **先**发"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::TurnEnd { session_id, uuid }) if session_id == "sess-1" && uuid == "u-2"),
            "Line 后紧跟 TurnEnd(u-2)"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 2, .. })),
            "畸形行照发 Line、不 panic"
        );
        assert!(
            rx.try_recv().is_err(),
            "无多余帧（畸形行不产 TurnEnd、user 行不产 TurnEnd）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 同 sid 多 pidfile（resume 原进程未死）：删一个不发 Removed（引用计数），
    /// 删第二个才 Removed 恰一次。
    #[cfg(target_os = "linux")]
    #[test]
    fn same_sid_two_pidfiles_refcount() {
        let dir = std::env::temp_dir().join(format!("ccm-refcount-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);

        // 两个 pidfile 同 sid（借同一真实存活 pid；path key 不同即两个 entry）
        let p1 = dir.join(format!("{pid}.json"));
        // 第二个 pidfile 放子目录（path key 不同、file_stem 仍是 pid 数字）
        let sub = dir.join("dup");
        std::fs::create_dir_all(&sub).unwrap();
        let p2 = sub.join(format!("{pid}.json"));
        let body = format!(
            r#"{{"pid":{pid},"sessionId":"shared-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
        );
        std::fs::write(&p1, &body).unwrap();
        std::fs::write(&p2, &body).unwrap();
        process_session_added(&p1, &mut state, &mut sink);
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid")
        );
        process_session_added(&p2, &mut state, &mut sink);
        // 第二个 pidfile：幂等检查是 per-key 的 → 恰好再发一条 Added（前端
        // ensureTab 幂等）。断言帧序（审计 S3：吞帧会掩盖"先 Removed 再 Added
        // 闪烁"类回归）。
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid"),
            "second pidfile re-announces exactly once"
        );
        assert!(
            rx.try_recv().is_err(),
            "and nothing else (no spurious Removed)"
        );
        assert_eq!(state.sessions.len(), 2);

        // 删第一个 → 仍被 p2 引用 → 不发 Removed
        process_session_removed(&p1, &mut state, &mut sink);
        assert!(
            rx.try_recv().is_err(),
            "no Removed while another pidfile holds the sid"
        );
        assert!(state.active_sids.contains("shared-sid"));

        // 删第二个 → 归零 → Removed 恰一次
        process_session_removed(&p2, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "shared-sid"));
        assert!(!state.active_sids.contains("shared-sid"));
        assert!(rx.try_recv().is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 常规 added/removed 回归：单 pidfile 生命周期行为与 F22 前一致。
    #[cfg(target_os = "linux")]
    #[test]
    fn plain_lifecycle_regression() {
        let dir = std::env::temp_dir().join(format!("ccm-plainlife-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(r#"{{"pid":{pid},"sessionId":"solo","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "solo"));
        process_session_removed(&path, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "solo"));
        assert!(state.sessions.is_empty() && state.active_sids.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch9-F27：status 透传 ===

    /// 宣告帧带初始 status；同 pidfile modify：status 变 → session_status 帧、
    /// 不变 → 静默（幂等早退保留）。
    #[cfg(target_os = "linux")]
    #[test]
    fn status_diff_emits_session_status_frame() {
        let dir = std::env::temp_dir().join(format!("ccm-status-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, true);
        let pidfile = dir.join(format!("{pid}.json"));
        let write = |status: &str, waiting: Option<&str>| {
            let w = waiting
                .map(|x| format!(r#","waitingFor":"{x}""#))
                .unwrap_or_default();
            std::fs::write(
                &pidfile,
                format!(
                    r#"{{"pid":{pid},"sessionId":"st-sid","cwd":"/p","procStart":"{ticks}","status":"{status}"{w}}}"#
                ),
            )
            .unwrap();
        };
        write("busy", None);
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded { sid, status, .. }) => {
                assert_eq!(sid, "st-sid");
                assert_eq!(status.as_deref(), Some("busy"), "宣告带初始 status");
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        // 同内容 modify → 静默
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(rx.try_recv().is_err(), "status 未变不发帧");
        // status 变 → session_status 帧
        write("waiting", Some("permission prompt"));
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionStatus {
                sid,
                status,
                waiting_for,
            }) => {
                assert_eq!(sid, "st-sid");
                assert_eq!(status.as_deref(), Some("waiting"));
                assert_eq!(waiting_for.as_deref(), Some("permission prompt"));
            }
            other => panic!("expected SessionStatus, got {other:?}"),
        }
        // 再变回 → 再发
        write("idle", None);
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(matches!(
            rx.try_recv(),
            Ok(Frame::SessionStatus { status: Some(s), waiting_for: None, .. }) if s == "idle"
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch8-F25：tail-only 模式 ===

    /// tail-only 初扫：宣告帧带 path、零行帧；随后追加的新行 seq == 初扫时完整
    /// 行数 L（行号语义）；末尾残行不计数（F14 torn-line 语义）。
    #[cfg(target_os = "linux")]
    #[test]
    fn tail_only_primes_cursor_and_new_line_seq_is_line_number() {
        let dir = std::env::temp_dir().join(format!("ccm-tailonly-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        // 既有历史：3 个完整行 + 1 个残行（残行不计数 → L=3）
        let jsonl = proj.join("tail-sid.jsonl");
        std::fs::write(&jsonl, b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, true); // --tail-only
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"tail-sid","cwd":"/p","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        // ① 宣告帧带 path
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid, path, lines, ..
            }) => {
                assert_eq!(sid, "tail-sid");
                assert_eq!(path.as_deref(), Some(jsonl.to_string_lossy().as_ref()));
                assert_eq!(
                    lines,
                    Some(3),
                    "帧应带 prime 时的完整行数 L（快照完整性校验用）"
                );
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        // ② 零行帧（历史被 prime 吸收）
        assert!(rx.try_recv().is_err(), "tail-only 初扫不得发行帧");
        // ③ 补全残行 + 追加新行 → 唯一行帧 seq==3（残行补全后成为第 3 行，0-based）
        std::fs::write(
            &jsonl,
            b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn\":true}\n{\"new\":1}\n",
        )
        .unwrap();
        process_jsonl(&jsonl, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::Line { seq, raw, .. }) => {
                assert_eq!(seq, 3, "残行补全行的 seq 应为初扫完整行数 L=3");
                assert_eq!(raw, r#"{"torn":true}"#);
            }
            other => panic!("expected Line, got {other:?}"),
        }
        match rx.try_recv() {
            Ok(Frame::Line { seq, .. }) => assert_eq!(seq, 4),
            other => panic!("expected Line, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 默认（全量）模式行为不变：初扫把既有行全部推流（旧 monitor 兼容锚点）。
    #[cfg(target_os = "linux")]
    #[test]
    fn full_replay_mode_still_streams_history() {
        let dir = std::env::temp_dir().join(format!("ccm-fullmode-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-y");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        std::fs::write(proj.join("full-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false); // 默认全量
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"full-sid","cwd":"/p","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// F25 DoD ④：(with_bg, tail_only) = (true, true) 组合——bg 会话放行且
    /// tail-only 生效（宣告带元信息+path+lines，历史零行帧）。
    #[cfg(target_os = "linux")]
    #[test]
    fn with_bg_and_tail_only_combined() {
        let dir = std::env::temp_dir().join(format!("ccm-combo-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-c");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        std::fs::write(proj.join("combo-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), true, true); // 双开
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"combo-sid","cwd":"/p","kind":"bg","name":"任务","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid,
                session_kind,
                lines,
                path,
                ..
            }) => {
                assert_eq!(sid, "combo-sid");
                assert_eq!(session_kind.as_deref(), Some("bg"), "with_bg 放行");
                assert_eq!(lines, Some(2), "tail-only 带 L");
                assert!(path.is_some());
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "tail-only：历史零行帧");
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch7-F24：--with-bg 放行 + 帧元信息 ===

    #[cfg(target_os = "linux")]
    #[test]
    fn with_bg_announces_bg_with_metadata() {
        let dir = std::env::temp_dir().join(format!("ccm-withbg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), true, false); // --with-bg
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/proj/x","kind":"bg","jobId":"j","name":"评估任务","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid,
                session_kind,
                cwd,
                name,
                ..
            }) => {
                assert_eq!(sid, "bg-sid");
                assert_eq!(session_kind.as_deref(), Some("bg"));
                assert_eq!(cwd.as_deref(), Some("/proj/x"));
                assert_eq!(name.as_deref(), Some("评估任务"));
            }
            other => panic!("expected SessionAdded with metadata, got {other:?}"),
        }
        assert!(state.active_sids.contains("bg-sid"), "bg 行要能流出");
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch6-F21：kind 交互性门 ===

    #[test]
    fn parse_kind_variants() {
        assert_eq!(
            parse_kind(br#"{"sessionId":"s","kind":"bg","jobId":"j"}"#).as_deref(),
            Some("bg"),
            "真实 bg 样本形态"
        );
        assert_eq!(
            parse_kind(br#"{"sessionId":"s","kind":"interactive"}"#).as_deref(),
            Some("interactive")
        );
        assert_eq!(parse_kind(br#"{"sessionId":"s"}"#), None, "旧 CC 无 kind");
        assert_eq!(parse_kind(b"not json"), None);
    }

    /// 集成：kind:"bg" 的 pidfile（真实存活进程 = 本进程，身份/时间证据全过）
    /// 在 kind 门被拒——不发 SessionAdded、不进 sessions/active_sids。
    /// 对照组：同进程 interactive pidfile 正常宣告。
    #[cfg(target_os = "linux")]
    #[test]
    fn bg_pidfile_is_gated_even_when_author_is_alive() {
        let dir = std::env::temp_dir().join(format!("ccm-kind-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);

        // bg pidfile：作者活着、procStart 逐位相等——F20 证据全过，但 kind 门拒
        let bg_path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &bg_path,
            format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/x","kind":"bg","jobId":"j","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&bg_path, &mut state, &mut sink);
        assert!(state.sessions.is_empty(), "bg must not be tracked");
        assert!(!state.active_sids.contains("bg-sid"));
        assert!(rx.try_recv().is_err(), "no SessionAdded frame for bg");

        // 对照：interactive 正常宣告
        std::fs::write(
            &bg_path,
            format!(r#"{{"pid":{pid},"sessionId":"int-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&bg_path, &mut state, &mut sink);
        assert!(state.active_sids.contains("int-sid"));
        match rx.try_recv() {
            Ok(Frame::SessionAdded { sid, .. }) => assert_eq!(sid, "int-sid"),
            other => panic!("expected SessionAdded, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn procstart_identity_match_short_circuits_all_heuristics() {
        // Recorded ticks == current ticks → author confirmed, even when the
        // heuristics would individually scream imposter (stale mtime, bad
        // cmdline): identity evidence is strictly stronger.
        assert_eq!(
            add_time_verdict(
                Some(12285972),
                Some(12285972),
                Some(9_999_999),
                Some(1000),
                Some("tmux")
            ),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn procstart_mismatch_falls_through_to_heuristics() {
        // Mismatch + stale time evidence → imposter (the tmux reuse case).
        assert_eq!(
            add_time_verdict(Some(12285972), Some(99999999), Some(2000), Some(1000), None),
            AddTimeVerdict::Imposter("started-after-pidfile")
        );
        // Mismatch alone with fresh mtime and claude-like cmdline → allow
        // (defends against CC changing the procStart format: a hard reject
        // would black out every real session).
        assert_eq!(
            add_time_verdict(
                Some(12285972),
                Some(99999999),
                Some(990),
                Some(1000),
                Some("claude")
            ),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn tolerance_exact_boundary() {
        // start == mtime + 60 → still inside tolerance (uses >, not >=).
        assert_eq!(
            add_time_verdict(None, None, Some(1060), Some(1000), None),
            AddTimeVerdict::Alive
        );
        // One second past → imposter.
        assert_eq!(
            add_time_verdict(None, None, Some(1061), Some(1000), None),
            AddTimeVerdict::Imposter("started-after-pidfile")
        );
    }

    #[test]
    fn parse_procstart_ticks_variants() {
        assert_eq!(
            parse_procstart_ticks(br#"{"sessionId":"abc","procStart":"12285972"}"#),
            Some(12285972),
            "CC's real format: decimal string"
        );
        assert_eq!(
            parse_procstart_ticks(br#"{"procStart":12285972}"#),
            Some(12285972),
            "bare number tolerated"
        );
        assert_eq!(parse_procstart_ticks(br#"{"sessionId":"abc"}"#), None);
        assert_eq!(
            parse_procstart_ticks(br#"{"procStart":"133849906480000000"}"#),
            Some(133_849_906_480_000_000),
            "Windows FILETIME magnitude still parses (mismatch then falls to heuristics)"
        );
        assert_eq!(parse_procstart_ticks(b"not json"), None);
    }

    /// Integration sanity on the real /proc (Linux only): our own process's
    /// start epoch must be between boot and now — catches a broken btime +
    /// ticks/USER_HZ composition that pure-function tests cannot see.
    #[cfg(target_os = "linux")]
    #[test]
    fn own_process_start_epoch_is_sane() {
        let ticks = proc_starttime(std::process::id());
        assert!(ticks.is_some(), "own starttime must be readable");
        let epoch = start_epoch_from_ticks(ticks).expect("own start epoch");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            epoch <= now + 2,
            "start {epoch} must not be in the future (now {now})"
        );
        assert!(
            now - epoch < 24 * 3600,
            "test process started within a day (got {})",
            now - epoch
        );
    }

    #[test]
    fn parse_btime_from_realistic_proc_stat() {
        let stat = "cpu  123 0 456 789 0 0 0 0 0 0\n\
                    cpu0 61 0 228 394 0 0 0 0 0 0\n\
                    intr 12345 0 0\n\
                    ctxt 987654\n\
                    btime 1719900000\n\
                    processes 4321\n\
                    procs_running 2\n";
        assert_eq!(parse_btime(stat), Some(1_719_900_000));
        assert_eq!(parse_btime("cpu 1 2 3\n"), None, "no btime line");
        assert_eq!(parse_btime("btime notanumber\n"), None);
    }

    // === #34 procStart double-check (F04) ===

    /// A normal `/proc/<pid>/stat` line: starttime is field 22. Sample is a real
    /// kernel layout with a simple comm `(bash)`.
    #[test]
    fn parse_starttime_normal_line() {
        // pid=1234 comm=(bash) state=S ... field22(starttime)=9876543 ...
        let stat = "1234 (bash) S 1 1234 1234 0 -1 4194304 1 0 0 0 0 0 0 0 \
                    20 0 1 0 9876543 12345678 100 18446744073709551615 1 1 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(9876543));
    }

    /// The comm gotcha: a process named with a space inside the parens must not
    /// derail field counting (splitting the whole line would shift every field).
    #[test]
    fn parse_starttime_comm_with_space() {
        let stat = "4242 (my proc) R 1 4242 4242 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 555000 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(555000));
    }

    /// The hard comm gotcha: parentheses *inside* comm. We must key off the LAST
    /// `')'`, not the first, or the offset is wrong.
    #[test]
    fn parse_starttime_comm_with_inner_parens() {
        let stat = "7 ((odd) name)) S 1 7 7 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 424242 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(424242));
    }

    /// Malformed / too-few-fields stat → None (never panics, no bad starttime).
    #[test]
    fn parse_starttime_malformed_returns_none() {
        assert_eq!(parse_starttime_from_stat(""), None); // no ')'
        assert_eq!(parse_starttime_from_stat("123 (x) S 1 2 3"), None); // < 22 fields
                                                                        // ')' present but starttime token is non-numeric.
        let bad = "1 (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 notanum 0";
        assert_eq!(parse_starttime_from_stat(bad), None);
    }

    /// `session_alive` truth table around the captured procStart.
    ///
    /// The existence-dependent assertions only hold on Linux: on non-Linux
    /// `pid_alive` is a hardcoded `true` smoke stub (and `proc_starttime` is
    /// `None`), so `session_alive` is `true` for everything there. The
    /// reuse-detection logic — the whole point of #34 — is Linux-only, matching
    /// the `/proc` runtime target.
    #[test]
    fn session_alive_self_is_alive_in_existence_only_mode() {
        // Cross-platform: the current process is alive, and with no captured
        // baseline (`None`) liveness degrades to existence — must read alive.
        let me = std::process::id();
        assert!(
            session_alive(me, None),
            "self is alive in existence-only mode"
        );
    }

    /// Full, portable truth table for the pure liveness decision — including the
    /// transient-read-failure arm (`exists=true, expected=Some, current=None`)
    /// that must NOT archive a still-existing PID (the regression #34 audit
    /// flagged). No real `/proc` needed.
    #[test]
    fn is_same_live_process_truth_table() {
        // Process gone → dead regardless of start info.
        assert!(!is_same_live_process(false, Some(5), Some(5)));
        assert!(!is_same_live_process(false, None, None));

        // Exists + baseline + current readable: alive iff equal (reuse = differ).
        assert!(
            is_same_live_process(true, Some(5), Some(5)),
            "same start = alive"
        );
        assert!(
            !is_same_live_process(true, Some(5), Some(6)),
            "different read start = reused PID = dead"
        );

        // Exists but current start unreadable right now → DO NOT false-archive.
        assert!(
            is_same_live_process(true, Some(5), None),
            "transient /proc read failure on a live PID must stay alive"
        );

        // Exists, no baseline captured → existence-only degrade = alive.
        assert!(is_same_live_process(true, None, Some(9)));
        assert!(is_same_live_process(true, None, None));
    }

    // === #32 overflow signal (F05) ===

    /// FrameSink: a full channel drops + counts; once the channel drains, the
    /// next send emits a single `Overflow{dropped}` before the real frame and
    /// resets the counter. tokio's `try_send`/`try_recv` are sync, so no runtime.
    #[test]
    fn frame_sink_counts_drops_then_signals_overflow_on_recovery() {
        let (tx, mut rx) = mpsc::channel::<Frame>(2);
        let mut sink = FrameSink::new(tx);

        // Fill both slots — these go through cleanly, no overflow owed.
        sink.send(Frame::SessionAdded {
            sid: "a".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "b".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert_eq!(
            sink.dropped, 0,
            "nothing dropped while the channel had room"
        );

        // Channel is full now: three sends are dropped and counted.
        sink.send(Frame::SessionAdded {
            sid: "c".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "d".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "e".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert_eq!(sink.dropped, 3);

        // Drain both queued frames (they are the first two, not the dropped ones).
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));

        // Next send (channel now empty, cap 2): emits Overflow{3} into slot 1,
        // resets the counter, then the real frame into slot 2.
        sink.send(Frame::SessionRemoved { sid: "f".into() });
        assert_eq!(sink.dropped, 0, "overflow signal flushed, counter reset");
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Overflow { dropped: 3 })),
            "overflow signal carries the dropped count and arrives first"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { .. })),
            "the real frame follows the overflow signal"
        );

        // Steady state: no spurious Overflow once recovered.
        sink.send(Frame::SessionAdded {
            sid: "g".into(),
            session_kind: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn session_alive_decision_table_linux() {
        // A PID that cannot be alive on any sane host → dead regardless of start.
        let dead_pid = u32::MAX;
        assert!(
            !session_alive(dead_pid, Some(123)),
            "absent PID is dead even with an expected start"
        );
        assert!(
            !session_alive(dead_pid, None),
            "absent PID is dead in existence-only mode too"
        );

        // The current process IS alive. Baseline == its real start → alive;
        // a wrong baseline → dead (the PID-reuse signal).
        let me = std::process::id();
        let real = proc_starttime(me).expect("self has a /proc starttime");
        assert!(
            session_alive(me, Some(real)),
            "self is alive when start matches"
        );
        assert!(
            !session_alive(me, Some(real.wrapping_add(1))),
            "a mismatched start means the PID was reused → dead"
        );
    }
}
