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
        /// F66（#58③，additive）：本 daemon 声明支持的**能力 token 集**（开放字符串，
        /// 加法式）。monitor 按此声明决定发哪些流模式 flag（`--with-bg`/`--tail-only`），
        /// **不再靠 build_id 精确匹配**——闭合 2026-07-09 那类「身份确认不了就全降级」事故。
        /// 旧 monitor 忽略此字段（additive）；空/缺 = 按最小能力集待它。
        /// **§26 死循环护栏**：只声明本 daemon **会先剥离对应 flag** 的能力（老到不剥离
        /// 未知 flag 的 daemon 也老到不声明该能力，声明 = 自证认识该 flag）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        capabilities: Vec<String>,
    },
    /// One raw JSONL line tailed from a session file.
    Line {
        session_id: String,
        path: String,
        seq: u64,
        raw: String,
        /// daemon-01（gap#2，additive 不 bump PROTO_VERSION）：本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——
        /// 语义**逐字节对齐 aterm `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒
        /// `tail -c +(N+1)`。给 offset 续拉/截断检测（`seq` 是 per-stream 序数、非 resume 键）。
        /// 注（审计 quality）：`Frame` 仅 derive `Serialize`，故此 `#[serde(default)]` 在**本 crate 装饰性**——
        /// 「旧 daemon 缺字段 → client 得 0」的向后兼容实现在 **cc-monitor 反序列化侧**（此处 default 无实效但无害，留作意图标注）。
        #[serde(default)]
        byte_offset: u64,
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
        /// Batch8-F25（additive）：该会话 jsonl 的远端绝对路径（同 sid 多文件时
        /// 取 mtime 最新者）——monitor 旁路快照（`--read-session`）用。宣告时
        /// 未找到 jsonl（会话刚起还没写首行）→ None：此时无历史可拉，后续行
        /// 天然从 tail 的 seq 0 起全量到达，无需快照。
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Batch8 审计 D-I2（additive）：tail-only 模式下 prime 时的完整行数 L
        /// ——monitor 校验快照拉到的行数 ≥ L 才算成功（不足 = 中途断/daemon
        /// 报错，触发重试；exit status 经 ChannelStream 拿不到，行数校验更强）。
        /// 全量模式 None。
        #[serde(skip_serializing_if = "Option::is_none")]
        lines: Option<u64>,
        /// Batch9-F27（additive）：宣告时的初始 status/waitingFor——连接建立灯就对。
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        waiting_for: Option<String>,
    },
    /// Batch9-F27：会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，
    /// 天然稀疏）。远端红绿灯数据源；旧 monitor 未知 kind 忽略（additive）。
    SessionStatus {
        sid: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        waiting_for: Option<String>,
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

    /// Batch8：当前计数器值（= 下一个将分配的 seq = 已计完整行数），不推进。
    pub fn peek(&self, path: &str) -> u64 {
        self.next.get(path).copied().unwrap_or(0)
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
                    capabilities: vec!["bg".into(), "tail-only".into()],
                },
                "hello",
            ),
            (
                Frame::Line {
                    session_id: "s".into(),
                    path: "/p".into(),
                    seq: 0,
                    raw: "{}".into(),
                    byte_offset: 0,
                },
                "line",
            ),
            (
                Frame::SessionAdded {
                    sid: "s".into(),
                    session_kind: None,
                    cwd: None,
                    name: None,
                    path: None,
                    lines: None,
                    status: None,
                    waiting_for: None,
                },
                "session_added",
            ),
            (
                Frame::SessionStatus {
                    sid: "s".into(),
                    status: Some("busy".into()),
                    waiting_for: None,
                },
                "session_status",
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

    /// F66（#58③）wire 契约：hello 的 `capabilities`。
    /// ① 非空 → 序列化为数组（monitor 据此发 flag）。
    /// ② 空集 → `skip_serializing_if` 省略字段（additive：等价旧 hello，旧 monitor
    ///    收到即空集缺省——向后兼容的关键）。
    fn hello(caps: Vec<String>) -> Frame {
        Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            capabilities: caps,
        }
    }

    #[test]
    fn hello_capabilities_serializes_when_present_and_omits_when_empty() {
        // ① 非空 → 数组在线上
        let line = to_line(&hello(vec!["bg".into(), "tail-only".into()])).expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert_eq!(v["kind"], "hello");
        assert_eq!(v["capabilities"], serde_json::json!(["bg", "tail-only"]));
        // ② 空集 → 字段省略（旧 hello 等价形态，旧 monitor 忽略缺失 = 空集缺省）
        let line = to_line(&hello(vec![])).expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert!(
            v.get("capabilities").is_none(),
            "空 capabilities 必须被 skip（additive 向后兼容）"
        );
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
            byte_offset: 99,
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
