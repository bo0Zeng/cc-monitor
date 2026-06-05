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
//!   [`Frame`]s into the channel. It uses `try_send`, so a full channel drops
//!   the frame with a `tracing::warn!` rather than ever blocking the notify
//!   callback (the documented Phase-0 no-overflow-signal gap — real overflow
//!   signalling is Phase 1).
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
//! The incremental read mirrors `process_file`: a per-file byte offset, read
//! from `last_offset` to EOF, BOM strip via `trim_start_matches('\u{feff}')`,
//! skip blank lines, and `is_subagent_path` excludes any path containing a
//! `subagents` segment. On truncation (`len < last_offset`) the offset resets
//! to 0 **but the per-file seq keeps climbing** (the seq comes from
//! [`SeqCounter`], which is never reset) — see [`read_new_lines`].

use crate::wire::{Frame, SeqCounter};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::collections::HashMap;
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
/// Runs on its own OS thread. `tx` is the bounded sender; every emitted frame
/// goes through [`send_frame`] which never blocks the notify callback.
fn watch_loop(claude_dir: PathBuf, tx: mpsc::Sender<Frame>) {
    let projects = claude_dir.join("projects");
    let sessions = claude_dir.join("sessions");

    let mut state = ReaderState::default();

    // --- Phase 1: synchronous initial scan of already-existing files. ---
    // Mirrors watcher.rs: pick up already-running sessions on startup so the
    // client gets a snapshot before any live event arrives.
    if projects.is_dir() {
        for entry in WalkDir::new(&projects).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if is_jsonl(p) && !is_subagent_path(p) {
                process_jsonl(p, &mut state, &tx);
            }
        }
    }
    if sessions.is_dir() {
        for entry in WalkDir::new(&sessions).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if is_session_json(p) {
                process_session_added(p, &mut state, &tx);
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

    for evt in notify_rx {
        let events = match evt {
            Ok(events) => events,
            Err(errs) => {
                tracing::warn!("debouncer error: {errs:?}");
                continue;
            }
        };
        for ev in events {
            let p = ev.path.as_path();
            if is_jsonl(p) && !is_subagent_path(p) {
                process_jsonl(p, &mut state, &tx);
            } else if is_session_json(p) {
                // notify-debouncer-mini coalesces to "something happened to this
                // path". Decide add vs remove by current existence on disk.
                if p.exists() {
                    process_session_added(p, &mut state, &tx);
                } else {
                    process_session_removed(p, &mut state, &tx);
                }
            }
        }
        if tx.is_closed() {
            break;
        }
    }
}

/// Reader-side bookkeeping shared across `process_*` calls.
///
/// Not behind a lock: the reader is single-threaded (one OS thread), so all
/// access is serialized by construction.
#[derive(Default)]
struct ReaderState {
    /// Per-file consumed byte offset, keyed by [`path_key`]. Reset to 0 on
    /// truncation; the climbing seq lives separately in [`Self::seqs`] so a
    /// truncation never rolls the seq back.
    offsets: HashMap<PathBuf, u64>,
    /// Per-file monotonic seq source. `SeqCounter` only ever climbs for a given
    /// path (it is never reset), so truncation resetting `offsets` cannot pull
    /// the seq back — exactly the `watcher.rs:243-247` invariant.
    seqs: SeqCounter,
    /// PID-file path → cached `sessionId`, so a delete (which can no longer
    /// read the file) can still emit the right `SessionRemoved`.
    pid_to_sid: HashMap<PathBuf, String>,
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
}

/// Pure bookkeeping core, factored out so it is unit-testable without a real
/// filesystem watcher.
///
/// Given the file's *full current bytes*, the prior byte `offset`, the file's
/// `key` and the shared [`SeqCounter`], return the newly-appeared lines (with
/// seqs assigned) and the new offset. The seq for each kept line comes from
/// `seqs.next(key)`, so it is per-path monotonic and **never reset**.
///
/// Mirrors `../src-tauri/src/watcher.rs` `process_file` (lines 233-274):
///
/// - read from `offset` to EOF; the returned offset is the new length;
/// - **truncation**: if the file is now shorter than `offset`, start over from
///   byte 0 (`len < last_offset → start = 0`);
/// - on truncation the byte offset resets but the seq keeps climbing (it comes
///   from `SeqCounter`, which never resets), so a client that already placed
///   the old seqs still sorts the new lines after them;
/// - strip a leading UTF-8 BOM (`\u{feff}`) and skip blank lines;
/// - the returned `raw` is the original (untrimmed) line, exactly as
///   `watcher.rs` pushes `line` (not `trimmed`) into the batch.
pub fn read_new_lines(
    bytes: &[u8],
    offset: u64,
    key: &str,
    seqs: &mut SeqCounter,
) -> (Vec<ReadLine>, u64) {
    let len = bytes.len() as u64;
    // Truncation guard, mirrors `let start = if len < last_offset { 0 } ...`.
    let start = if len < offset { 0 } else { offset };

    let mut out = Vec::new();
    if start < len {
        let slice = &bytes[start as usize..];
        // Lossily decode so a torn multibyte tail at EOF can't abort the read;
        // BufReader::lines() in watcher.rs likewise tolerates partial reads.
        let text = String::from_utf8_lossy(slice);
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
    // Offset always advances to the current EOF (even if start was reset to 0).
    (out, len)
}

/// Read a JSONL file incrementally and send a [`Frame::Line`] per new line.
fn process_jsonl(path: &Path, state: &mut ReaderState, tx: &mpsc::Sender<Frame>) {
    let Some(session_id) = file_stem_str(path) else {
        return;
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_offset = state.offsets.get(&key).copied().unwrap_or(0);
    let (lines, new_offset) = read_new_lines(&bytes, prev_offset, &key_str, &mut state.seqs);
    state.offsets.insert(key, new_offset);
    let path_str = path.to_string_lossy().into_owned();
    for line in lines {
        send_frame(
            tx,
            Frame::Line {
                session_id: session_id.clone(),
                path: path_str.clone(),
                seq: line.seq,
                raw: line.raw,
            },
        );
    }
}

/// A `sessions/<PID>.json` appeared (or was already present): read it, extract
/// `sessionId`, cache PID→sid, emit [`Frame::SessionAdded`].
///
/// Idempotent: if we already cached the same sid for this path, skip the emit
/// so a debounced modify event does not re-announce an existing session.
fn process_session_added(path: &Path, state: &mut ReaderState, tx: &mpsc::Sender<Frame>) {
    let key = path_key(path);
    let Some(sid) = read_session_id(path) else {
        return;
    };
    if state.pid_to_sid.get(&key) == Some(&sid) {
        return;
    }
    state.pid_to_sid.insert(key, sid.clone());
    send_frame(tx, Frame::SessionAdded { sid });
}

/// A `sessions/<PID>.json` was deleted: look up the cached sid (the file is
/// gone, so we cannot read it now) and emit [`Frame::SessionRemoved`].
fn process_session_removed(path: &Path, state: &mut ReaderState, tx: &mpsc::Sender<Frame>) {
    let key = path_key(path);
    if let Some(sid) = state.pid_to_sid.remove(&key) {
        send_frame(tx, Frame::SessionRemoved { sid });
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

/// `try_send` a frame, dropping (with a warning) on a full channel.
///
/// **Phase-0 gap (documented):** a full channel means the writer/SSH pipe is
/// wedged; we drop the frame and warn rather than block the notify reader.
/// There is no overflow signal to the client — that is Phase 1.
fn send_frame(tx: &mpsc::Sender<Frame>, frame: Frame) {
    match tx.try_send(frame) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!(
                "frame channel full (cap {CHANNEL_CAPACITY}); dropping frame \
                 (Phase-0 no-overflow-signal gap)"
            );
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            // Writer gone (shutdown). Nothing to do; the loop checks is_closed.
        }
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
        let mut offset = 0u64;

        let first = jsonl(&[r#"{"a":1}"#, r#"{"a":2}"#]);
        let (out, off) = read_new_lines(&first, offset, KEY, &mut seqs);
        offset = off;
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].seq, 0);
        assert_eq!(out[1].seq, 1);
        assert_eq!(offset, first.len() as u64);

        // Append two more lines (same prefix bytes, longer file).
        let mut second = first.clone();
        second.extend_from_slice(jsonl(&[r#"{"a":3}"#, r#"{"a":4}"#]).as_slice());
        let (out2, off2) = read_new_lines(&second, offset, KEY, &mut seqs);
        offset = off2;
        assert_eq!(out2.len(), 2, "only the newly-appended lines come back");
        assert_eq!(out2[0].seq, 2);
        assert_eq!(out2[1].seq, 3);
        assert_eq!(offset, second.len() as u64);
        assert_eq!(out2[0].raw, r#"{"a":3}"#);
    }

    #[test]
    fn no_new_bytes_yields_nothing_and_does_not_bump_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"x":1}"#]);
        let (_, offset) = read_new_lines(&buf, 0, KEY, &mut seqs);
        // Re-process identical bytes: offset == len, start >= len, nothing new.
        let (again, offset2) = read_new_lines(&buf, offset, KEY, &mut seqs);
        assert!(again.is_empty());
        // A fresh read of the same key still hands out seq 1 only if a line was
        // produced; here nothing new, so the next live line would be seq 1.
        assert_eq!(seqs.next(KEY), 1, "seq must not have advanced past 1");
        assert_eq!(offset2, buf.len() as u64);
    }

    #[test]
    fn truncation_resets_offset_but_seq_keeps_climbing() {
        let mut seqs = SeqCounter::new();

        let big = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#]);
        let (out, big_off) = read_new_lines(&big, 0, KEY, &mut seqs);
        assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1, 2]);

        // Simulated truncation: file is now SHORTER than the recorded offset.
        let small = jsonl(&[r#"{"n":99}"#]);
        assert!((small.len() as u64) < big_off, "test precondition");
        let (out2, small_off) = read_new_lines(&small, big_off, KEY, &mut seqs);

        // Offset reset to 0 then re-advanced to the new (smaller) length.
        assert_eq!(small_off, small.len() as u64);
        // The whole truncated file is re-read from byte 0 ...
        assert_eq!(out2.len(), 1);
        // ... but seq KEEPS CLIMBING (3, not back to 0): the climbing invariant.
        assert_eq!(out2[0].seq, 3, "seq must never reset on truncation");
    }

    #[test]
    fn leading_bom_is_stripped_for_the_empty_check_and_line_is_kept() {
        let mut seqs = SeqCounter::new();
        // A line that is ONLY a BOM + whitespace must be treated as empty.
        let only_bom = "\u{feff}   \n".as_bytes().to_vec();
        let (out, _) = read_new_lines(&only_bom, 0, KEY, &mut seqs);
        assert!(out.is_empty(), "BOM-only/blank line is skipped");
        assert_eq!(seqs.next(KEY), 0, "skipped line must not consume a seq");

        // A BOM-prefixed real line is kept (and not double counted).
        let mut seqs2 = SeqCounter::new();
        let bom_line = "\u{feff}{\"k\":1}\n".as_bytes().to_vec();
        let (out2, _) = read_new_lines(&bom_line, 0, KEY, &mut seqs2);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].seq, 0);
    }

    #[test]
    fn empty_lines_are_skipped_and_do_not_consume_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"a":1}"#, "", "   ", r#"{"a":2}"#, ""]);
        let (out, _) = read_new_lines(&buf, 0, KEY, &mut seqs);
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
    fn read_line_without_trailing_newline_is_still_emitted() {
        // str::lines() yields a final line even without a trailing '\n'.
        let mut seqs = SeqCounter::new();
        let buf = br#"{"only":1}"#.to_vec();
        let (out, offset) = read_new_lines(&buf, 0, KEY, &mut seqs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw, r#"{"only":1}"#);
        assert_eq!(offset, buf.len() as u64);
    }
}
