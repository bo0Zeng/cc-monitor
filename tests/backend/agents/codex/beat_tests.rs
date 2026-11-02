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
