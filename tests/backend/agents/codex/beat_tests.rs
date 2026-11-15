//! Codex 的一条 rollout 记录 ⇒ 一拍（正文是占位）。冷开整份 rollout 出轮次 · 大纲 · 用量那几条经观测面，挂在 `observe` 底下。
use super::*;
use crate::agents::record::{BodyExt as _, RecordClass};
use serde_json::Value;

const SAMPLE: &str = include_str!("../../../__fixtures__/codex-rollout.turns.jsonl");

/// 流里不再有「认不出」：用量那一行与来信元数据那一行是簿记，不出记录。
#[test]
fn bookkeeping_lines_are_not_unread_cards() {
    let mut offset = 0u64;
    for raw in SAMPLE.lines() {
        let v: Value = serde_json::from_str(raw).unwrap();
        let rec = super::super::record::record_of(&v, offset);
        assert!(
            rec.as_ref()
                .is_none_or(|r| r.body.class() != RecordClass::Unread),
            "认不出：{raw}"
        );
        offset += raw.len() as u64 + 1;
    }
}

/// 快路只许放过、不许误拦：两份样本里每一行，不是簿记的一拍 ⇒ 快路放行。
#[test]
fn the_fast_path_never_drops_a_line_that_speaks() {
    let old = include_str!("../../../__fixtures__/codex-rollout.sample.jsonl");
    let open = crate::agents::beat::Watch::default();
    for raw in SAMPLE.lines().chain(old.lines()) {
        let v: Value = serde_json::from_str(raw).unwrap();
        let b = beat_of(&v).unwrap();
        if b.side != crate::agents::beat::Side::Note {
            assert!(could_matter(raw.as_bytes(), &open), "快路拦了：{raw}");
        }
        assert!(!could_matter(
            raw.as_bytes(),
            &crate::agents::beat::Watch {
                mcp_only: true,
                ..Default::default()
            }
        ));
    }
}

/// 用量那一行：输入含读缓存 ⇒ 新输入减掉它；请求键是 `response_id`；写缓存分不出 ⇒ 0。
#[test]
fn a_token_usage_record_is_a_reply_beat_carrying_only_usage() {
    let raw = SAMPLE.lines().find(|l| l.contains("resp-2")).unwrap();
    let v: Value = serde_json::from_str(raw).unwrap();
    let b = beat_of(&v).unwrap();
    assert_eq!(b.side, crate::agents::beat::Side::Reply);
    assert!(b.acts.is_empty());
    assert_eq!(
        b.usage,
        Some(crate::agents::beat::Usage {
            input: 100,
            cache_read: 1000,
            output: 8,
            request: Some("resp-2".into()),
            ..Default::default()
        })
    );
}

use crate::agents::beat::{Act, Side};

const PAGINATED: &str = include_str!("fixtures/rollout-paginated.jsonl");
const SUBAGENT: &str = include_str!("fixtures/rollout-subagent.jsonl");

/// 一拍里那几件事的名字（正文带原文，好比顺序）。
fn shape(b: &crate::agents::beat::Beat<'_>) -> (Side, Vec<String>) {
    let acts = b
        .acts
        .iter()
        .map(|a| match a {
            Act::Text(t) => format!("text:{t}"),
            Act::Image => "image".into(),
            Act::Call { id, .. } => format!("call:{id}"),
            Act::Result { of, .. } => format!("result:{}", of.as_deref().unwrap_or("")),
            Act::Thinking => "thinking".into(),
            Act::Stop(_) => "stop".into(),
            Act::Said(u) => format!("said:{:?}", std::mem::discriminant(&u.speaker)),
            _ => "other".into(),
        })
        .filter(|s| !s.starts_with("said:"))
        .collect();
    (b.side, acts)
}

/// 不是簿记的那几拍，照记录顺序。
fn spoken(text: &str) -> Vec<(Side, Vec<String>)> {
    text.lines()
        .map(|raw| serde_json::from_str::<Value>(raw).unwrap())
        .filter_map(|v| beat_of(&v).map(|b| shape(&b)))
        .filter(|(side, _)| *side != Side::Note)
        .collect()
}

/// Paginated 形：说话的拍照 `response_item` 的顺序出，`item_completed` 那一份不另出拍；图那几张各一件、路径框不进正文；
/// 用量只在 `token_usage_record`（只带用量）；`token_count` 是簿记（一发会写几次：限额变了也写，拿它会重算）。
#[test]
fn paginated_beats_come_in_record_order() {
    let r = |s: &str| (Side::Reply, vec![s.to_string()]);
    assert_eq!(
        spoken(PAGINATED),
        vec![
            (
                Side::Input,
                vec![
                    "text:<environment_context><cwd>/work/demo</cwd></environment_context>".into()
                ]
            ),
            (
                Side::Input,
                vec![
                    "text:what is in these".into(),
                    "image".into(),
                    "image".into()
                ]
            ),
            r("call:call-1"),
            (Side::Input, vec!["result:call-1".into()]),
            (Side::Reply, vec!["text:two pictures".into(), "stop".into()]),
            (Side::Reply, vec![]),
            (Side::Input, vec!["text:and the second one".into()]),
        ]
    );
    for raw in PAGINATED.lines() {
        let v: Value = serde_json::from_str(raw).unwrap();
        let b = beat_of(&v).unwrap();
        if raw.contains("\"item_completed\"") || raw.contains("\"token_count\"") {
            assert_eq!(
                (b.side, b.acts.len(), b.usage.is_some()),
                (Side::Note, 0, false),
                "{raw}"
            );
        }
        if raw.contains("\"token_usage_record\"") {
            assert!(b.acts.is_empty() && b.usage.is_some(), "{raw}");
        }
    }
}

/// Legacy 形：轮次与大纲照记录顺序出（`event_msg.user_message` 那一份不另出拍）。
#[test]
fn legacy_beats_come_in_record_order() {
    let old = include_str!("../../../__fixtures__/codex-rollout.sample.jsonl");
    let got = spoken(old);
    let texts: Vec<&str> = got
        .iter()
        .flat_map(|(_, a)| a.iter())
        .filter_map(|a| a.strip_prefix("text:"))
        .collect();
    assert!(texts.contains(&"list the files"));
    assert_eq!(texts.iter().filter(|t| **t == "list the files").count(), 1);
    assert_eq!(texts.last(), Some(&"two files"));
}

/// 叉出来的会话：开头那条 `session_meta` 说叉自哪一条 ⇒ 它的拍带着它，快路也放它过去（会话事实会变）。
#[test]
fn a_forked_session_says_where_it_came_from() {
    let raw = SUBAGENT.lines().next().unwrap();
    let v: Value = serde_json::from_str(raw).unwrap();
    let b = beat_of(&v).unwrap();
    assert_eq!(
        b.forked_from.as_deref(),
        Some("0000beef-0000-4000-8000-0000000000b0")
    );
    assert!(could_matter(
        raw.as_bytes(),
        &crate::agents::beat::Watch::default()
    ));
    let plain = PAGINATED.lines().next().unwrap();
    let v: Value = serde_json::from_str(plain).unwrap();
    assert_eq!(beat_of(&v).unwrap().forked_from, None);
}

/// 一拍里的图与记录里的图片块同一个数法：按文件号引用的不算。
#[test]
fn image_acts_count_like_record_images() {
    let v: Value = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_image","file_id":"f"},{"type":"input_image","image_url":"data:image/png;base64,AA"},{"type":"input_text","text":"hi"}]}}"#,
    )
    .unwrap();
    let b = beat_of(&v).unwrap();
    assert_eq!(b.acts.iter().filter(|a| matches!(a, Act::Image)).count(), 1);
}
