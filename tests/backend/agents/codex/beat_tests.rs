//! Codex 的一条 rollout 记录 ⇒ 一拍：冷开一份合成 rollout（0.159 的形状，正文是占位）轮次 · 大纲 · 用量都出得来。
use super::*;
use crate::agents::record::{BodyExt as _, RecordClass};
use crate::observe::record_scan::ScanMap;
use serde_json::Value;

const SAMPLE: &str = include_str!("../../../__fixtures__/codex-rollout.turns.jsonl");

fn cold_open() -> ScanMap {
    let face = super::super::RECORDS;
    ScanMap::scan_with(
        SAMPLE.as_bytes(),
        SAMPLE.len() as u64,
        super::super::AGENT_KIND,
        Some(face),
    )
    .unwrap()
}

#[test]
fn a_cold_open_of_a_codex_rollout_has_turns_an_outline_and_usage() {
    let map = cold_open();
    // 大纲：人说的两句（开发者指令 · 环境注入不算）。
    let outline: Vec<(&str, &str)> = map
        .inputs
        .iter()
        .map(|r| (r.uuid.as_str(), r.excerpt.as_str()))
        .collect();
    assert_eq!(
        outline,
        vec![("m-u1", "list the files"), ("m-u2", "now count them")]
    );
    // 轮次：两轮，各从那一句起；第一轮一次工具 · 一次思考，答完了。
    let turns: Vec<(&str, u32, u32, bool)> = map
        .turns
        .iter()
        .map(|t| (t.uuid.as_str(), t.tools, t.thinking, t.done))
        .collect();
    assert_eq!(turns, vec![("m-u1", 1, 1, true), ("m-u2", 0, 0, true)]);
    assert!(
        map.turns[0].reply.contains("two files"),
        "{:?}",
        map.turns[0].reply
    );
    assert!(map.turns.iter().all(|t| t.pending.is_empty()));
    // 用量：最后一次请求的提示 ＝ 新输入 ＋ 读缓存；全会话三次请求。
    let usage = map.facts.usage.as_ref().expect("用量");
    assert_eq!(usage.prompt_tokens, 1200);
    assert_eq!(usage.peak_prompt_tokens, 1200);
    let tokens = map.facts.tokens.as_ref().expect("全会话用量");
    assert_eq!(tokens.requests, 3);
    assert_eq!(
        (tokens.input, tokens.cache_read, tokens.output),
        (600 + 100 + 100, 400 + 1000 + 1100, 31)
    );
    assert_eq!(
        map.facts.last_say.as_ref().map(|s| s.text.as_str()),
        Some("two")
    );
}

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

/// 骨架索引：人说的那两句带 id 与大纲摘要；系统注入那一行说是系统、不进大纲。
#[test]
fn the_index_rows_carry_ids_and_the_outline() {
    let map = cold_open();
    let with_x: Vec<(&str, &str)> = map
        .index
        .iter()
        .filter_map(|r| Some((r.u.as_deref()?, r.x.as_deref()?)))
        .collect();
    assert_eq!(
        with_x,
        vec![("m-u1", "list the files"), ("m-u2", "now count them")]
    );
    let env = map
        .index
        .iter()
        .find(|r| r.u.as_deref() == Some("m-env"))
        .unwrap();
    assert_eq!(env.sp, Some("system"));
    assert!(env.ch > 0);
}
