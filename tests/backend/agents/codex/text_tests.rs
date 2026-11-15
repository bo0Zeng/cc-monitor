//! Codex 的文本面（注册表 `RecordFace.text`）：一行 rollout ⇒ 搜索 · 查找要的那几格。夹具是结构占位，正文都是编的。
//! 经全文搜索 · 会话内查找走一遍的那几条要观测面，挂在 `observe` 底下（`tests/backend/agents/codex/search_tests.rs`）。
use super::*;
use crate::agents::record::Body;
use crate::agents::UserTextExt as _;
use serde_json::Value;

const TURNS: &str = include_str!("../../../__fixtures__/codex-rollout.turns.jsonl");
const SAMPLE: &str = include_str!("../../../__fixtures__/codex-rollout.sample.jsonl");

fn line_with(fixture: &str, needle: &str) -> Value {
    let raw = fixture
        .lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("夹具里没有 {needle}"));
    serde_json::from_str(raw).unwrap()
}

/// 谁那一侧与出记录逐行同一个分法：出 `said` ⇔ 人那一侧，出 `reply` ⇔ 回话，不出记录（簿记 · 事件）或认不出 ⇔ 不是。
#[test]
fn every_line_sits_on_the_side_its_record_says() {
    for raw in TURNS.lines().chain(SAMPLE.lines()) {
        let v: Value = serde_json::from_str(raw).unwrap();
        let want = match super::super::record::record_of(&v, 0).map(|r| r.body) {
            Some(Body::Said { .. }) => Some(LineRole::Said),
            Some(Body::Reply { .. }) => Some(LineRole::Replied),
            _ => None,
        };
        assert_eq!(line_of_value(&v).role, want, "{raw}");
        assert_eq!(
            line_of(raw).map(|l| l.role),
            Some(want),
            "原文那一口：{raw}"
        );
    }
}

/// 人说的一句：谁说的是人、带消息 id 与时刻；回话的正文是 `content` 拍平。
#[test]
fn the_users_words_and_the_reply_carry_id_time_and_text() {
    let u = line_with(TURNS, "\"m-u1\",\"role\"");
    let l = line_of_value(&u);
    assert_eq!(l.id.as_deref(), Some("m-u1"));
    assert_eq!(
        l.at_ms,
        crate::common::time::parse_iso8601_ms("2026-10-20T00:00:02.000Z")
    );
    assert_eq!(
        user_of(&u).and_then(|t| t.who.speech()).as_deref(),
        Some("list the files")
    );

    let a = line_with(TURNS, "\"m-a1\"");
    assert_eq!(line_of_value(&a).role, Some(LineRole::Replied));
    assert_eq!(main_of(&a), "two files");
    assert!(user_of(&a).is_none(), "回话不是谁说的那一句");

    // 系统注入（环境那一段）不是人说的：搜索不收它。
    let env = line_with(TURNS, "\"m-env\"");
    assert!(user_of(&env).and_then(|t| t.who.speech()).is_none());
}

/// 工具调用 · 工具输出：工具内容各在自己那一侧；输出收在那次调用的那一块里。
#[test]
fn tool_calls_and_outputs_are_tool_text_on_their_own_side() {
    let call = line_with(TURNS, "\"fc-1\"");
    let t = tool_of(&call, true);
    assert!(t.contains("exec_command") && t.contains("ls"), "{t}");
    assert_eq!(tool_of(&call, false), "");
    assert_eq!(lands_of(&call), Lands::Own);

    let out = line_with(TURNS, "\"fo-1\"");
    assert_eq!(tool_of(&out, false), "a.txt\nb.txt");
    assert_eq!(tool_of(&out, true), "");
    assert_eq!(lands_of(&out), Lands::InTool("call-1".into()));
    assert!(matches!(
        user_of(&out).map(|u| u.who.speaker),
        Some(Speaker::ToolResult)
    ));
}

/// 会话开头那条给工作目录；事件不是谁说的一句、不带时刻与 id；没有 `payload.id` 的那一句不编 id（通用层照行起点合成）。
#[test]
fn meta_gives_the_cwd_events_say_nothing_and_ids_are_not_made_up() {
    let meta = line_of_value(&line_with(TURNS, "\"session_meta\""));
    assert_eq!(meta.cwd.as_deref(), Some("/work/demo"));
    assert_eq!((meta.role, meta.id, meta.at_ms), (None, None, None));

    let ev = line_of_value(&line_with(TURNS, "\"task_started\""));
    assert_eq!((ev.role, ev.id, ev.at_ms), (None, None, None));

    let bare = line_of_value(&line_with(SAMPLE, "\"text\":\"list the files\""));
    assert_eq!(bare.role, Some(LineRole::Said));
    assert_eq!(bare.id, None);
}
