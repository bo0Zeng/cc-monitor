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
pub fn spawn(claude_dir: PathBuf) -> mpsc::Receiver<Frame> {
    let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    // notify-debouncer-mini is a synchronous std::sync::mpsc API; run it on a
    // blocking thread and hand frames to the async writer over tokio mpsc.
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || watch_loop(claude_dir, tx))
        .expect("spawn jsonl-watcher thread");
    rx
}

/// The reader half: initial walkdir scan, then the live debouncer loop.
///
/// Runs on its own OS thread. `tx` is the bounded sender; it is wrapped in a
/// [`FrameSink`] whose [`FrameSink::send`] never blocks the notify callback and
/// turns dropped frames into an [`Frame::Overflow`] signal (#32).
fn watch_loop(claude_dir: PathBuf, tx: mpsc::Sender<Frame>) {
    let projects = claude_dir.join("projects");
    let sessions = claude_dir.join("sessions");

    let mut state = ReaderState::new(projects.clone());
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
        let dead: Vec<PathBuf> = state
            .sessions
            .iter()
            .filter(|(_, e)| !session_alive(e.pid, e.start))
            .map(|(k, _)| k.clone())
            .collect();
        for k in dead {
            if let Some(e) = state.sessions.remove(&k) {
                state.active_sids.remove(&e.sid);
                sink.send(Frame::SessionRemoved { sid: e.sid });
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
}

impl ReaderState {
    fn new(projects: PathBuf) -> Self {
        ReaderState {
            projects,
            offsets: HashMap::new(),
            seqs: SeqCounter::new(),
            sessions: HashMap::new(),
            active_sids: HashSet::new(),
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
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
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
        // Lossy decode is now safe: a torn multibyte sequence can only live in
        // the deferred tail, never inside the complete region.
        let text = String::from_utf8_lossy(&slice[..complete_end]);
        for line in text.lines() {
            let trimmed = line.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                continue;
            }
            // Seq from the never-reset per-path counter. A skipped (blank) line
            // does not call `next`, so blanks do not consume a seq.
            let seq = seqs.next(key);
            out.push(ReadLine {
                seq,
                // Preserve the original line bytes (post BOM/whitespace only
                // used for the skip decision), matching watcher.rs `raw: line`.
                raw: line.to_string(),
            });
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
        sink.send(Frame::Line {
            session_id: session_id.clone(),
            path: path_str.clone(),
            seq: line.seq,
            raw: line.raw,
        });
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
    let Some(sid) = read_session_id(path) else {
        return;
    };
    // Only ACTIVE if the process is actually alive (mirrors local STILL_ACTIVE).
    // A stale pidfile for a dead process is NOT an active session.
    if !pid_alive(pid) {
        return;
    }
    // Idempotent: a debounced modify of an already-tracked session re-announces
    // nothing.
    if state.sessions.get(&key).map(|e| e.sid.as_str()) == Some(sid.as_str()) {
        return;
    }
    // #34: capture the PID's procStart now so the liveness poll can later tell a
    // still-running session from a PID the OS reused for an unrelated process.
    let start = proc_starttime(pid);
    state.sessions.insert(
        key,
        SessionEntry {
            pid,
            sid: sid.clone(),
            start,
        },
    );
    state.active_sids.insert(sid.clone());
    sink.send(Frame::SessionAdded { sid: sid.clone() });
    // Now that this session is active, stream its existing jsonl (mirrors the
    // local force-rescan triggered on session-added).
    let projects = state.projects.clone();
    rescan_sid_jsonl(&projects, &sid, state, sink);
}

/// A `sessions/<PID>.json` was deleted: look up the cached sid (the file is
/// gone, so we cannot read it now) and emit [`Frame::SessionRemoved`].
fn process_session_removed(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let key = path_key(path);
    if let Some(e) = state.sessions.remove(&key) {
        state.active_sids.remove(&e.sid);
        sink.send(Frame::SessionRemoved { sid: e.sid });
    }
}

/// Walk `projects/` for this session's jsonl (`<sid>.jsonl`, non-subagent) and
/// stream its already-present lines. Called when a session becomes active so an
/// already-running session snapshots on session-added (mirrors local force-rescan).
fn rescan_sid_jsonl(projects: &Path, sid: &str, state: &mut ReaderState, sink: &mut FrameSink) {
    if !projects.is_dir() {
        return;
    }
    for entry in WalkDir::new(projects).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if is_jsonl(p)
            && !is_subagent_path(p)
            && p.file_stem().and_then(|s| s.to_str()) == Some(sid)
        {
            process_jsonl(p, state, sink);
        }
    }
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

/// Extract the `sessionId` string field from a `sessions/<PID>.json` file.
fn read_session_id(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    parse_session_id(&bytes)
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
        sink.send(Frame::SessionAdded { sid: "a".into() });
        sink.send(Frame::SessionAdded { sid: "b".into() });
        assert_eq!(
            sink.dropped, 0,
            "nothing dropped while the channel had room"
        );

        // Channel is full now: three sends are dropped and counted.
        sink.send(Frame::SessionAdded { sid: "c".into() });
        sink.send(Frame::SessionAdded { sid: "d".into() });
        sink.send(Frame::SessionAdded { sid: "e".into() });
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
        sink.send(Frame::SessionAdded { sid: "g".into() });
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
