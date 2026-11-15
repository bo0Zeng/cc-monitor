//! **客户端解记录那一层**：解得开的那一条原样；解不开的成认不出那一条（[`OrUnread`]，句子是后端适配层那一份 `face_core::record::unread_body`），
//! 不让整份回包 / 整帧失败；带记录的回包交 Kotlin 之前照同一判法改那一格（[`mend_records`]）。
//! 线上形（`Body::Unread` · `t` 的闭集 · 哪几条回包带记录 `records_cell`）住 `cmd_wire::record`。

use cmd_wire::cells::Words;
use cmd_wire::record::{records_cell, Draw, Record, UnreadWhy, BODY_TAGS};
use face_core::record::unread_body;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 客户端解记录那一层：[`Record`] 外面包一层，解得开 ⇒ 那一条原样；解不开 ⇒ 认不出那一支（`unread_body`，带原样的类名与摘录），
/// 不让整份回包 / 整帧失败（后端照原样的一团写记录，记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出）。
/// 线上照 [`Record`] 写（`transparent`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OrUnread(pub Record);

impl OrUnread {
    /// 一团 JSON ⇒ 一条（解不开 ⇒ 认不出那一条：类在 `t` 的闭集里 ⇒ 形状变了，否则没见过的类；
    /// 身份原样有就用，没有 ⇒ 按原文摘一个稳定的；时刻那几格型对就带上）。
    pub fn of(raw: Value) -> Self {
        if let Ok(r) = Record::deserialize(&raw) {
            return Self(r);
        }
        let s = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_string);
        let kind = s("t");
        let why = match kind.as_deref() {
            Some(t) if BODY_TAGS.contains(&t) => UnreadWhy::ParseFailed,
            _ => UnreadWhy::UnknownType,
        };
        let text = raw.to_string();
        let id = s("id")
            .filter(|i| !i.is_empty())
            .unwrap_or_else(|| format!("unread:{:016x}", cmd_wire::fnv::fnv1a64(text.as_bytes())));
        Self(Record {
            agent: s("agent").unwrap_or_default().into(),
            id,
            at: s("at"),
            time_text: s("timeText").map(Words),
            at_ms: raw.get("atMs").and_then(Value::as_i64),
            // 认不出的那一条恒画成一张卡（一行 warn 细条），不判。
            draw: Draw::Card,
            human: false,
            body: unread_body(why, kind, &text),
        })
    }
}

impl<'de> Deserialize<'de> for OrUnread {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Value::deserialize(d).map(Self::of)
    }
}

/// 带记录的那几条回包（`cmd_wire::record::records_cell`）：记录那一格解不开完整记录的，换成认不出那一条（[`OrUnread::of`]：
/// 带原样的类名与摘录，同桌面那几句）；解得开的、别的格都原样不动。
/// 后端照原样的一团写记录、记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出，这一条判定只在这里（交 Kotlin 之前）。
pub fn mend_records(cmd: &str, data: &mut Value) {
    let Some(list) = records_cell(cmd) else {
        return;
    };
    let Some(items) = data.get_mut(list).and_then(Value::as_array_mut) else {
        return;
    };
    for rec in items.iter_mut().filter_map(|i| i.get_mut("record")) {
        if Record::deserialize(&*rec).is_err() {
            let mended = OrUnread::of(std::mem::take(rec));
            *rec = serde_json::to_value(&mended).unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    //! 1. 一团 JSON 解不开完整记录 ⇒ 认不出那一条：类在 `t` 的闭集里 ⇒ 形状变了，否则没见过；身份原样有就用、没有按原文摘一个稳定的。
    //! 2. 带记录的三条回包：解不开的那一格换成认不出那一条，解得开的 · 别的格原样不动；别的命令不碰。
    //! 3. 按记录解的那三条回包（照 `cmd_wire::call` 的回包型 ＋ [`OrUnread`] 解、照类型写回）与跨语言金样逐字节同
    //!    （生成的 Kotlin 类写回要与它同；别的命令的金样由 `cmd-wire` 那边现打）。
    //! 4. 会话流的 `line` 帧按 [`OrUnread`] 解：金样逐字节写回；解不开的那一条成认不出那一条，整帧照样解得开。
    use super::*;
    use cmd_wire::frame::{decode, Frame};
    use cmd_wire::record::Body;
    use cmd_wire::session::HistoryTail;
    use copy_core::copy_text;
    use serde_json::json;

    /// 解不开的那一团 ⇒ 认不出那一条，那一句同后端适配层那一份（`face_core::record::unread_text`）。
    #[test]
    fn an_unknown_kind_becomes_the_unread_one_with_the_shared_sentence() {
        let r = OrUnread::of(json!({"t": "nope"}));
        let (why, kind, text, _, count) = unread_parts(&r);
        assert_eq!(why, UnreadWhy::UnknownType);
        assert_eq!(kind.as_deref(), Some("nope"));
        assert_eq!(
            text,
            copy_text("record.unread.unknownType", &[("type", "nope")])
        );
        assert_eq!(count, 1);
    }

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    fn read(rel: &str) -> String {
        let path = root().join(rel);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {}：{e}", path.display()))
    }

    fn unread_parts(r: &OrUnread) -> (UnreadWhy, Option<String>, String, String, u32) {
        match &r.0.body {
            Body::Unread {
                why,
                kind,
                text,
                excerpt,
                count,
                ..
            } => (
                *why,
                kind.clone(),
                text.0.clone(),
                excerpt.0.clone(),
                *count,
            ),
            other => panic!("不是认不出那一条：{other:?}"),
        }
    }

    #[test]
    fn a_known_tag_with_a_changed_shape_is_parse_failed_and_keeps_its_identity() {
        let raw = json!({"agent": "claude", "id": "r1", "at": "2026-10-09T01:30:03.000Z", "atMs": 7, "t": "said"});
        let r = OrUnread::of(raw.clone());
        let (why, kind, _, excerpt, _) = unread_parts(&r);
        assert_eq!(why, UnreadWhy::ParseFailed);
        assert_eq!(kind.as_deref(), Some("said"));
        assert_eq!(excerpt, raw.to_string());
        assert_eq!(r.0.id, "r1");
        assert_eq!(r.0.at.as_deref(), Some("2026-10-09T01:30:03.000Z"));
        assert_eq!(r.0.at_ms, Some(7));
        // 没有身份 ⇒ 按原文摘一个，同一团摘出同一个。
        let a = OrUnread::of(json!({"t": "later", "x": 1}));
        let b = OrUnread::of(json!({"t": "later", "x": 1}));
        let c = OrUnread::of(json!({"t": "later", "x": 2}));
        assert!(a.0.id.starts_with("unread:"));
        assert_eq!(a.0.id, b.0.id);
        assert_ne!(a.0.id, c.0.id);
    }

    #[test]
    fn a_record_that_reads_stays_itself() {
        let said = json!({"agent": "claude", "id": "t1", "draw": "card", "human": false,
            "t": "title", "text": "标题", "by": "agent"});
        let r = OrUnread::of(said.clone());
        assert!(matches!(r.0.body, Body::Title { .. }));
        assert_eq!(serde_json::to_value(&r).unwrap(), said);
        let back: OrUnread = serde_json::from_value(said.clone()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn only_the_record_cell_of_the_three_record_replies_is_mended() {
        let good = json!({"agent": "claude", "id": "t1", "draw": "card", "human": false,
            "t": "title", "text": "标题", "by": "agent"});
        let mut lines = json!({"from": 0, "next": 1, "eof": true, "end": 9,
        "lines": [
            {"session_id": "s", "path": "/p", "seq": 0, "end": 9, "cwd": null,
                "record": {"agent": "claude", "id": "r1", "t": "future-kind"}},
            {"session_id": "s", "path": "/p", "seq": 1, "end": 9, "cwd": null, "record": good.clone()}
        ]});
        mend_records("history-lines", &mut lines);
        assert_eq!(lines["lines"][0]["record"]["t"], "unread");
        assert_eq!(lines["lines"][0]["record"]["type"], "future-kind");
        assert_eq!(lines["lines"][0]["record"]["id"], "r1");
        assert_eq!(lines["lines"][1]["record"], good, "解得开的那一条原样");
        assert_eq!(lines["lines"][0]["cwd"], serde_json::Value::Null);

        for cmd in ["history-read", "history-tail"] {
            let mut rows = json!({"rows": [{"end": 1, "hash": 1, "record": {"t": "x"}}]});
            mend_records(cmd, &mut rows);
            assert_eq!(rows["rows"][0]["record"]["t"], "unread", "{cmd}");
        }
        let mut other = json!({"rows": [{"record": {"t": "x"}}]});
        let before = other.clone();
        mend_records("history-list", &mut other);
        assert_eq!(other, before, "别的命令不碰");
    }

    /// 照 `cmd_wire::call` 的回包型解、记录那一格按 [`OrUnread`] 解，再照类型写成一行（同 `cmd_wire::call::typed_text` 那一形）。
    fn typed_text(cmd: &str, data: serde_json::Value) -> Option<Result<String, String>> {
        use cmd_wire::record::{HistoryLines, HistoryRead};
        fn text<T: serde::Serialize + serde::de::DeserializeOwned>(
            data: serde_json::Value,
        ) -> Result<String, String> {
            let typed: T = serde_json::from_value(data).map_err(|e| e.to_string())?;
            serde_json::to_string(&typed).map_err(|e| e.to_string())
        }
        Some(match cmd {
            "history-read" => text::<HistoryRead<OrUnread>>(data),
            "history-tail" => text::<HistoryTail<OrUnread>>(data),
            "history-lines" => text::<HistoryLines<OrUnread>>(data),
            _ => return None,
        })
    }

    #[test]
    fn the_record_replies_match_the_cross_language_golden() {
        const REL: &str = "tests/__fixtures__/wire-replies.golden.json";
        let golden: serde_json::Value = serde_json::from_str(&read(REL)).expect("金样不是 JSON");
        let cases = golden["cases"].as_object().expect("金样没有 cases");
        let mine: Vec<&String> = cases.keys().filter(|k| records_cell(k).is_some()).collect();
        assert_eq!(
            mine,
            ["history-lines", "history-read", "history-tail"],
            "带记录的回包"
        );
        for cmd in mine {
            let rust = typed_text(cmd, cases[cmd]["sample"].clone())
                .expect("带记录的回包")
                .unwrap_or_else(|e| panic!("{cmd} 的结构夹具解不开：{e}"));
            assert_eq!(
                cases[cmd]["rust"].as_str(),
                Some(rust.as_str()),
                "{cmd}：现打的与 {REL} 不一致"
            );
        }
    }

    /// 助手那一行的正文上线是块树（`prose`：解析好的 `doc` ＋ 写好的「展开」那一格），不是原文：照类型解得开、写回逐字同；
    /// 解不开就成了「认不出」那一条（`t: unread`），这里红。夹具是造的短样例，不放真正文。
    #[test]
    fn a_reply_carrying_a_prose_tree_reads_back_as_itself() {
        let reply = json!({
            "agent": "claude", "at": "2026-10-09T01:30:03.000Z", "atMs": 1791509403000_u64, "autoReply": false,
            "blocks": [{
                "type": "prose",
                "doc": {
                    "blocks": [
                        { "t": "p", "k": ["改好了，", { "t": "b", "k": ["两处"] }, "：", { "t": "c", "x": "a.rs" }] },
                        { "t": "code", "lang": "rust", "x": "fn main() {}", "cont": false },
                        { "t": "list", "start": null, "tight": true, "items": [{ "task": null, "blocks": [{ "t": "p", "k": ["一"] }] }] }
                    ],
                    "segs": []
                },
                "more": "more-x"
            }],
            "draw": "card", "human": false, "id": "a-prose", "t": "reply", "timeText": "01:30"
        });
        let mut sample = json!({
            "end": 7, "eof": true, "from": 7, "how": "tail",
            "held": { "mark": "mark-x", "next": 7, "next_byte": 7, "path": "path-x" }, "next": 7, "split_at": 7, "tail_from": 7, "total": 7,
            "rows": [{ "cwd": "cwd-x", "end": 7, "hash": 7 }]
        });
        sample["rows"][0]["record"] = reply.clone();
        let rust = typed_text("history-tail", sample)
            .expect("带记录的回包")
            .unwrap_or_else(|e| panic!("history-tail 解不开：{e}"));
        let back: serde_json::Value = serde_json::from_str(&rust).unwrap();
        assert_eq!(
            back["rows"][0]["record"], reply,
            "助手正文那一块的树没照原样读回来"
        );
    }

    /// 记录那一格按 [`OrUnread`] 解：解不开完整记录的那一条成认不出那一条，整份照样解得开。
    #[test]
    fn a_record_the_client_cannot_read_becomes_the_unread_one_in_a_reply() {
        let lines = json!({"from": 0, "next": 1, "eof": true, "end": 9,
            "lines": [{"session_id": "s", "path": "/p", "seq": 0, "end": 9, "cwd": null,
                "record": {"agent": "claude", "id": "r1", "t": "future-kind"}}]});
        let back: serde_json::Value =
            serde_json::from_str(&typed_text("history-lines", lines).unwrap().unwrap()).unwrap();
        assert_eq!(back["lines"][0]["record"]["t"], "unread");
    }

    #[test]
    fn the_golden_line_frames_read_back_byte_for_byte_with_records_or_unread() {
        let mut seen = 0;
        for rel in [
            "tests/__fixtures__/session-stream.golden.jsonl",
            "tests/__fixtures__/codex-session-stream.golden.jsonl",
        ] {
            for line in read(rel)
                .lines()
                .filter(|l| l.starts_with(r#"{"kind":"line","#))
            {
                let f: Frame<OrUnread> =
                    decode(line).unwrap_or_else(|e| panic!("{rel} 这一行解不开（{e}）：{line}"));
                assert_eq!(
                    serde_json::to_string(&f).unwrap(),
                    line,
                    "{rel} 这一行解开再写回变了"
                );
                seen += 1;
            }
        }
        assert!(seen > 0, "金样里要有 line 帧");
    }

    #[test]
    fn a_record_that_does_not_read_becomes_the_unread_one_and_the_frame_still_decodes() {
        let line = r#"{"kind":"line","session_id":"s","path":"/p","seq":1,"record":{"agent":"a","id":"x","t":"later"},"byte_offset":9}"#;
        assert!(decode::<Record>(line).is_err());
        let Frame::Line(l) = decode::<OrUnread>(line).unwrap() else {
            panic!("不是 line 帧");
        };
        assert!(matches!(l.record.unwrap().0.body, Body::Unread { .. }));
    }
}
