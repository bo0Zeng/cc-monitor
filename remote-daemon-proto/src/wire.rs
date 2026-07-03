//! Phase-0 daemon→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].

use serde::Serialize;
use std::collections::HashMap;

/// A single daemon→client frame.
///
/// Serializes with an external `kind` tag, e.g.
/// `{"kind":"hello","v":1,...}` or `{"kind":"session_added","sid":"..."}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    /// Handshake sent once when a client connects.
    Hello {
        v: u32,
        build_id: String,
        host_arch: String,
        claude_dir: String,
    },
    /// One raw JSONL line tailed from a session file.
    Line {
        session_id: String,
        path: String,
        seq: u64,
        raw: String,
    },
    /// A new session file appeared.
    ///
    /// Batch7-F24（additive，向后兼容）：附带 pidfile 元信息——`session_kind`
    /// （"interactive"/"bg"；字段名避开 enum tag `kind`）、`cwd`、`name`。
    /// None 时不上线（旧行为字节不变）；旧 monitor 忽略未知字段。
    SessionAdded {
        sid: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// A session file went away.
    SessionRemoved { sid: String },
    /// The bounded frame channel back-pressured and the reader had to drop
    /// `dropped` frames (a slow/wedged SSH pipe). Emitted once when the channel
    /// drains enough to accept it, so the client can warn the user that live
    /// lines were lost (#32). `dropped` counts frames dropped since the last
    /// overflow signal.
    Overflow { dropped: u64 },
}

/// Serialize a frame to its compact one-line wire form with a trailing `\n`.
///
/// The returned string is exactly one JSON object followed by a single `\n`;
/// any newline inside string fields is escaped by `serde_json` as `\n`.
pub fn to_line(frame: &Frame) -> serde_json::Result<String> {
    let mut s = serde_json::to_string(frame)?;
    s.push('\n');
    Ok(s)
}

/// Per-file monotonic sequence counter.
///
/// Faithful port of the per-file seq semantics in
/// `../src-tauri/src/watcher.rs` (process_file): the counter is keyed by file
/// path, returns the current value then increments by 1 (so the first line of
/// a file gets seq 0, then 1, 2, ...), is monotonic across calls, and is never
/// reset — there is no truncation handling here, the counter only ever climbs
/// for a given path within the process.
#[derive(Debug)]
pub struct SeqCounter {
    next: HashMap<String, u64>,
}

impl SeqCounter {
    pub fn new() -> Self {
        SeqCounter {
            next: HashMap::new(),
        }
    }

    /// Return the current seq for `path`, then bump it by one.
    pub fn next(&mut self, path: &str) -> u64 {
        let slot = self.next.entry(path.to_string()).or_insert(0);
        let seq = *slot;
        *slot += 1;
        seq
    }
}

impl Default for SeqCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Helper: assert a line is a single JSON object ending in exactly one
    /// `\n` with no other bare newline, then return its parsed `kind`.
    fn parse_kind(line: &str) -> String {
        assert!(line.ends_with('\n'), "line must end in newline: {line:?}");
        let body = line.strip_suffix('\n').unwrap();
        assert!(
            !body.contains('\n'),
            "body must contain no bare newline: {body:?}"
        );
        assert!(
            !body.contains('\r'),
            "body must contain no bare carriage return: {body:?}"
        );
        let v: Value = serde_json::from_str(body).expect("body must be valid JSON");
        v.get("kind")
            .and_then(Value::as_str)
            .expect("kind must be a string")
            .to_string()
    }

    #[test]
    fn each_variant_serializes_to_single_line_with_expected_kind() {
        let cases: Vec<(Frame, &str)> = vec![
            (
                Frame::Hello {
                    v: 1,
                    build_id: "b".into(),
                    host_arch: "x86_64".into(),
                    claude_dir: "/home/u/.claude".into(),
                },
                "hello",
            ),
            (
                Frame::Line {
                    session_id: "s".into(),
                    path: "/p".into(),
                    seq: 0,
                    raw: "{}".into(),
                },
                "line",
            ),
            (
                Frame::SessionAdded {
                    sid: "s".into(),
                    session_kind: None,
                    cwd: None,
                    name: None,
                },
                "session_added",
            ),
            (Frame::SessionRemoved { sid: "s".into() }, "session_removed"),
            (Frame::Overflow { dropped: 7 }, "overflow"),
        ];

        for (frame, expected_kind) in cases {
            let line = to_line(&frame).expect("serialize");
            assert_eq!(parse_kind(&line), expected_kind);
        }
    }

    #[test]
    fn overflow_frame_serializes_with_dropped_count() {
        let line = to_line(&Frame::Overflow { dropped: 42 }).expect("serialize");
        let body = line.strip_suffix('\n').unwrap();
        let v: Value = serde_json::from_str(body).expect("valid json");
        assert_eq!(v["kind"], "overflow");
        assert_eq!(v["dropped"], 42);
    }

    #[test]
    fn seq_counter_is_monotonic_per_path_and_independent_across_paths() {
        let mut c = SeqCounter::new();
        assert_eq!(c.next("/a"), 0);
        assert_eq!(c.next("/a"), 1);
        assert_eq!(c.next("/a"), 2);

        // A different path starts its own sequence at 0.
        assert_eq!(c.next("/b"), 0);
        assert_eq!(c.next("/b"), 1);

        // /a is unaffected and keeps climbing.
        assert_eq!(c.next("/a"), 3);

        // Default impl behaves the same.
        let mut d = SeqCounter::default();
        assert_eq!(d.next("/x"), 0);
        assert_eq!(d.next("/x"), 1);
    }

    #[test]
    fn line_with_quotes_backslashes_and_newline_roundtrips() {
        let raw = "before\"quote\\backslash\nafter-newline";
        let frame = Frame::Line {
            session_id: "sid".into(),
            path: "/some/path.jsonl".into(),
            seq: 42,
            raw: raw.to_string(),
        };

        let line = to_line(&frame).expect("serialize");
        assert!(line.ends_with('\n'));

        // Minus the single trailing terminator, there must be NO bare newline:
        // the embedded newline in `raw` is escaped by serde_json as "\n".
        let body = &line[..line.len() - 1];
        assert!(
            !body.contains('\n'),
            "embedded newline must be escaped, got bare newline in: {body:?}"
        );

        // Parses back and the raw field is recovered byte-for-byte.
        let v: Value = serde_json::from_str(body).expect("parse");
        assert_eq!(v["kind"], "line");
        assert_eq!(v["raw"], raw);
        assert_eq!(v["seq"], 42);
    }
}
