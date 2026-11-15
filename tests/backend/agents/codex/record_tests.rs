use super::super::parse::session_meta_cwd;
use super::CodexRecordKind as K;
use super::*;
use crate::agents::Speaker;
use crate::agents::UserText;
use crate::agents::UserTextExt as _;
use serde_json::json;

fn env(top: &str, payload: Value) -> Value {
    json!({"timestamp": "2026-07-19T03:25:13.155Z", "type": top, "payload": payload})
}

/// 顶层 5 type（session_meta/turn_context/world_state 无 payload.type）。
#[test]
fn classifies_top_level_types() {
    assert_eq!(
        classify(&env("session_meta", json!({"session_id": "x"}))),
        K::SessionMeta
    );
    assert_eq!(
        classify(&env("turn_context", json!({"turn_id": "t"}))),
        K::TurnContext
    );
    assert_eq!(
        classify(&env("world_state", json!({"full": true}))),
        K::WorldState
    );
}

/// event_msg 子型（含 turn-end / usage / abort）+ alias 归一。
#[test]
fn classifies_event_msg_subtypes_with_alias() {
    // 本机 task_complete → TurnComplete。
    let tc = env(
        "event_msg",
        json!({"type": "task_complete", "turn_id": "019f7868-0e2d-7d73-bb7a-2f3837e5cb95", "duration_ms": 12104}),
    );
    assert_eq!(classify(&tc), K::TurnComplete);
    // 新版 alias turn_complete 也归到 TurnComplete（defensive）。
    assert_eq!(
        classify(&env(
            "event_msg",
            json!({"type": "turn_complete", "turn_id": "t"})
        )),
        K::TurnComplete
    );
    assert_eq!(
        classify(&env("event_msg", json!({"type": "turn_started"}))),
        K::TurnStarted
    );
    assert_eq!(
        classify(&env(
            "event_msg",
            json!({"type": "turn_aborted", "reason": "interrupted"})
        )),
        K::TurnAborted
    );
    assert_eq!(
        classify(&env("event_msg", json!({"type": "user_message"}))),
        K::UserMessage
    );
    assert_eq!(
        classify(&env("event_msg", json!({"type": "agent_message"}))),
        K::AgentMessage
    );
    // token_count → TokenCount。
    let tok = env(
        "event_msg",
        json!({"type": "token_count", "info": {"last_token_usage": {"input_tokens": 13839, "output_tokens": 157, "total_tokens": 13996}}}),
    );
    assert_eq!(classify(&tok), K::TokenCount);
    // 其它 event 子型 → OtherEvent（不崩、不误判）。
    assert_eq!(
        classify(&env("event_msg", json!({"type": "mcp_tool_call_end"}))),
        K::OtherEvent
    );
    assert_eq!(
        classify(&env("event_msg", json!({"type": "thread_rolled_back"}))),
        K::OtherEvent
    );
}

/// response_item 子型（含 OpenAI function_call 变体的容忍）。
#[test]
fn classifies_response_item_subtypes() {
    let msg = env(
        "response_item",
        json!({"type": "message", "role": "assistant", "content": []}),
    );
    assert_eq!(classify(&msg), K::Message);
    assert_eq!(
        classify(&env("response_item", json!({"type": "reasoning"}))),
        K::Reasoning
    );
    assert_eq!(
        classify(&env(
            "response_item",
            json!({"type": "custom_tool_call", "call_id": "c1"})
        )),
        K::ToolCall
    );
    assert_eq!(
        classify(&env(
            "response_item",
            json!({"type": "function_call", "call_id": "c2"})
        )),
        K::ToolCall
    );
    assert_eq!(
        classify(&env(
            "response_item",
            json!({"type": "custom_tool_call_output", "call_id": "c1"})
        )),
        K::ToolResult
    );
}

/// 防御：缺信封 / 未知顶层 type / payload 非对象 → Other，不 panic。
#[test]
fn defensive_on_malformed_and_unknown() {
    assert_eq!(
        classify(&json!({"type": "event_msg"})),
        K::Other,
        "无 payload → Other"
    );
    assert_eq!(
        classify(&json!({"payload": {"type": "x"}})),
        K::Other,
        "无 top type → Other"
    );
    assert_eq!(classify(&json!("not even an object")), K::Other);
    assert_eq!(
        classify(&env("some_future_kind", json!({}))),
        K::Other,
        "未来新顶层 type → Other"
    );
    assert_eq!(
        classify(&env("response_item", json!({"type": "some_future_item"}))),
        K::Other
    );
    // accessor 在非匹配记录上安全返回 None。
}

/// ⚠️ F2b trap #1/#2：`output` 与 `content` **真机恒数组** `[{type,text}]`——flatten_text 拼数组文本。
/// **用真机数组 shape、不用 String fixture 自欺**（String 会掩盖「数组落 else→"" 静默丢文本」的 bug）。
#[test]
fn flatten_text_handles_real_array_shape() {
    // 工具输出：真机数组（若当 String 处理 → 全丢）。
    let output = json!([
        {"type": "input_text", "text": "命令输出第一行"},
        {"type": "input_text", "text": "第二行"}
    ]);
    assert_eq!(flatten_text(&output), "命令输出第一行\n第二行");
    // message content：数组。
    assert_eq!(
        flatten_text(&json!([{"type": "output_text", "text": "hi"}])),
        "hi"
    );
    // input_image 等无 text 项 → 自然跳过（不产空行/不崩）。
    assert_eq!(
        flatten_text(
            &json!([{"type": "input_image", "image_url": "x"}, {"type": "input_text", "text": "cap"}])
        ),
        "cap"
    );
    // 防御：裸 String→原样；非数组/串→""。
    assert_eq!(flatten_text(&json!("bare")), "bare");
    assert_eq!(flatten_text(&json!({"not": "array"})), "");
    assert_eq!(flatten_text(&json!(null)), "");
}

/// F2b trap #3：reasoning.summary **真机恒 []** → reasoning_text=""（调用方据此给空 blocks、免空 Thinking）。
#[test]
fn reasoning_text_empty_summary_yields_empty() {
    // 真机形：summary=[]，仅 encrypted_content。
    let r = env(
        "response_item",
        json!({"type": "reasoning", "summary": [], "encrypted_content": "opaque"}),
    );
    assert_eq!(
        reasoning_text(&r),
        "",
        "空 summary → 空文本（调用方给空 blocks）"
    );
    // 有 summary text 才产文本。
    let r2 = env(
        "response_item",
        json!({"type": "reasoning", "summary": [{"type": "summary_text", "text": "推理了一步"}]}),
    );
    assert_eq!(reasoning_text(&r2), "推理了一步");
}

/// F2b：tool_input（Object 原样 / String 包 {input} / 缺→Null）+ call_id 配对键。
#[test]
fn tool_input_and_call_id() {
    let with_obj = env(
        "response_item",
        json!({"type": "custom_tool_call", "call_id": "c9", "name": "shell", "input": {"cmd": "ls"}}),
    );
    assert_eq!(tool_input(&with_obj), json!({"cmd": "ls"}));
    assert_eq!(call_id(&with_obj), Some("c9"));
    let with_str = env(
        "response_item",
        json!({"type": "custom_tool_call", "call_id": "c1", "input": "raw string arg"}),
    );
    assert_eq!(tool_input(&with_str), json!({"input": "raw string arg"}));
    // 缺 input → Null（不崩，name 仍可见）。
    assert_eq!(
        tool_input(&env(
            "response_item",
            json!({"type": "custom_tool_call", "call_id": "c2"})
        )),
        json!(null)
    );
}

/// `function_call` 的入参写在 `arguments`（一个 JSON 串）：解得出对象 ⇒ 那个对象；解不出 ⇒ 原串包成 `{input}`（不吞）；
/// `input` 与 `arguments` 都在 ⇒ 认 `input`。
#[test]
fn function_call_arguments_become_the_input() {
    let call = |p: Value| env("response_item", p);
    assert_eq!(
        tool_input(&call(
            json!({"type": "function_call", "call_id": "c", "name": "shell", "arguments": "{\"command\":[\"ls\"]}"})
        )),
        json!({"command": ["ls"]})
    );
    assert_eq!(
        tool_input(&call(
            json!({"type": "function_call", "call_id": "c", "arguments": "not json"})
        )),
        json!({"input": "not json"})
    );
    assert_eq!(
        tool_input(&call(
            json!({"type": "function_call", "call_id": "c", "arguments": "[1,2]"})
        )),
        json!({"input": "[1,2]"}),
        "解出来不是对象 ⇒ 照原串包"
    );
    assert_eq!(
        tool_input(&call(
            json!({"type": "function_call", "call_id": "c", "input": {"a": 1}, "arguments": "{\"b\":2}"})
        )),
        json!({"a": 1})
    );
    // `custom_tool_call` 的 `input` 是自由文本：长得像 JSON 也不解，照原串包。
    assert_eq!(
        tool_input(&call(
            json!({"type": "custom_tool_call", "call_id": "c", "input": "{\"x\":1}"})
        )),
        json!({"input": "{\"x\":1}"})
    );
}

// ─── 翻成通用记录 ───

/// 翻一条（起点偏移随便给一个）。
fn rec(v: &Value) -> Option<Record> {
    record_of(v, 40)
}

fn blocks_of(r: &Record) -> Value {
    match &r.body {
        Body::Said { blocks, .. } | Body::Reply { blocks, .. } => {
            serde_json::to_value(blocks).unwrap()
        }
        _ => Value::Null,
    }
}

fn who_of(r: &Record) -> Option<&UserText> {
    match &r.body {
        Body::Said { who, .. } => Some(who),
        _ => None,
    }
}

/// message：assistant ⇒ reply ＋ 正文块；developer ⇒ said（系统注入）；user 空 ⇒ said、没有块。
/// 记录上自带 id 就用它；没有 ⇒ 按起点偏移合成，从不给空串。
#[test]
fn maps_message_to_said_and_reply() {
    let asst = env(
        "response_item",
        json!({"type": "message", "role": "assistant", "id": "m1", "content": [{"type": "output_text", "text": "回复"}]}),
    );
    let r = rec(&asst).unwrap();
    assert!(matches!(
        &r.body,
        Body::Reply {
            auto_reply: false,
            ..
        }
    ));
    assert_eq!((r.agent.as_str(), r.id.as_str()), ("codex", "m1"));
    let more = copy_core::copy_text("cards.text.showAll", &[("size", &copy_core::size_text(6))]);
    assert_eq!(
        blocks_of(&r),
        json!([{"type": "prose", "doc": {"blocks": [{"t": "p", "k": ["回复"]}], "segs": []}, "more": more}])
    );

    let dev = env(
        "response_item",
        json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "sys"}]}),
    );
    let r = rec(&dev).unwrap();
    assert_eq!(r.id, "@40");
    assert!(matches!(
        who_of(&r).map(|w| &w.speaker),
        Some(Speaker::System { .. })
    ));

    let u = env(
        "response_item",
        json!({"type": "message", "role": "user", "content": []}),
    );
    let r = rec(&u).unwrap();
    assert!(who_of(&r).is_some_and(|w| w.speaker == Speaker::Human && w.text.is_empty()));
    assert_eq!(blocks_of(&r), json!([]));
}

/// 没有自己 id 的几条：合成的 id 各不相同（按行的起点偏移），同一行从哪条读路读都一样。
#[test]
fn synthesized_ids_are_distinct_and_stable() {
    let out = env(
        "response_item",
        json!({"type": "function_call_output", "call_id": "c1", "output": "x"}),
    );
    let a = record_of(&out, 0).unwrap().id;
    let b = record_of(&out, 812).unwrap().id;
    assert!(!a.is_empty() && a != b);
    assert_eq!(record_of(&out, 812).unwrap().id, b);
}

/// F7 去噪：role=user 但正文是 CLI 注入的上下文块（3 标记）→ 系统注入（界面不画）；
/// 真用户输入（含裸 # 标题/提及标签名）→ 人（正常气泡，正文 trim 过）。
#[test]
fn denoise_injected_context_user_messages() {
    let mk = |text: &str| {
        env(
            "response_item",
            json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}),
        )
    };
    for inj in [
        "<environment_context>\n  <cwd>/home/user</cwd>\n</environment_context>",
        "  <recommended_plugins>\nHere is a list of plugins…",
        "# AGENTS.md instructions\n\n<INSTRUCTIONS>\n# AGENTS.md\n本文件…",
    ] {
        let r = rec(&mk(inj)).unwrap();
        assert!(
            who_of(&r).is_some_and(|w| w.speaker == Speaker::System { body: None }),
            "注入块应去噪当系统注入: {inj:?}"
        );
    }
    for real in [
        "codex怎么换行",
        "帮我看看 <environment_context> 是什么",
        "# 我的笔记\n随便写的",
        "# AGENTS.md 里写了啥?",
    ] {
        let r = rec(&mk(real)).unwrap();
        assert!(
            who_of(&r).is_some_and(|w| w.speaker == Speaker::Human && w.text == real.trim()),
            "真用户输入不应被去噪: {real:?}"
        );
    }
}

/// reasoning：空 summary ⇒ reply、没有块（免空推理块）；有 text ⇒ 推理块。
#[test]
fn maps_reasoning_empty_and_nonempty() {
    let empty = env("response_item", json!({"type": "reasoning", "summary": []}));
    assert_eq!(blocks_of(&rec(&empty).unwrap()), json!([]));
    let think = env(
        "response_item",
        json!({"type": "reasoning", "summary": [{"text": "想了想"}]}),
    );
    assert_eq!(
        blocks_of(&rec(&think).unwrap()),
        json!([{"type": "thinking", "text": "想了想", "preview": "想了想"}])
    );
}

/// 工具调用 ⇒ reply ＋ 调用块；工具输出（数组）⇒ said ＋ 结果块（内容是拼好的正文块，守丢文本坑）。
#[test]
fn maps_tool_call_and_output() {
    let call = env(
        "response_item",
        json!({"type": "custom_tool_call", "call_id": "c1", "name": "shell", "input": {"cmd": "ls"}}),
    );
    assert_eq!(
        blocks_of(&rec(&call).unwrap()),
        json!([{"type": "tool_use", "id": "c1", "name": "shell", "input": {"cmd": "ls"}}])
    );
    let out = env(
        "response_item",
        json!({"type": "custom_tool_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "文件列表"}]}),
    );
    let r = rec(&out).unwrap();
    assert!(who_of(&r).is_some_and(|w| w.speaker == Speaker::ToolResult));
    assert_eq!(
        blocks_of(&r),
        json!([{"type": "tool_result", "for": "c1", "content": [{"type": "text", "text": "文件列表", "bytes": 12}], "isError": false, "bytes": 12, "sizeText": "12 B"}])
    );
}

/// session_meta cwd 抽取（Codex 无 cwd-项目目录 → list 用 cwd 内存分组）。
#[test]
fn session_meta_cwd_is_read_from_session_meta_only() {
    let sm = env(
        "session_meta",
        json!({"session_id": "s", "cwd": "/home/u/proj", "timestamp": "2026-07-19T03:25:05.382Z"}),
    );
    assert_eq!(session_meta_cwd(&sm), Some("/home/u/proj"));
    // 非 session_meta（如 turn_context 也有 cwd）→ None（只认 session_meta）。
    let tc = env("turn_context", json!({"cwd": "/other", "turn_id": "t"}));
    assert_eq!(session_meta_cwd(&tc), None);
}

/// 事件 · 元记录 ⇒ 不出记录（不上线；看不懂的那一半归漂移账）。
#[test]
fn events_and_meta_records_do_not_come_out() {
    for v in [
        json!({"type": "event_msg", "payload": {"type": "task_complete", "turn_id": "t1"}}),
        env("event_msg", json!({"type": "token_count"})),
        env("session_meta", json!({"id": "s"})),
        env("turn_context", json!({"cwd": "/w"})),
    ] {
        assert!(rec(&v).is_none(), "{v}");
        let t = translated(&crate::agents::Line::new(&v.to_string(), 0))
            .unwrap()
            .unwrap();
        assert!(t.record.is_none(), "{v}");
    }
}

/// 带两窗的 `token_count` ⇒ 交给通用层的那一形带上自报的额度（读记录那一路据它进账）；别的行 ⇒ 不带。
#[test]
fn a_token_count_with_rate_limits_hands_its_windows_on() {
    let tc = env(
        "event_msg",
        json!({"type": "token_count", "info": null, "rate_limits": {
            "primary": {"used_percent": 42.0, "window_minutes": 300, "resets_at": 1_794_150_000u64},
            "secondary": {"used_percent": 13.5, "window_minutes": 10080, "resets_at": 1_794_600_000u64}
        }}),
    );
    let t = translated(&crate::agents::Line::new(&tc.to_string(), 0))
        .unwrap()
        .unwrap();
    assert_eq!(t.quota_said.map(|w| w.len()), Some(2));
    let other = env("session_meta", json!({"id": "s"}));
    let t = translated(&crate::agents::Line::new(&other.to_string(), 0))
        .unwrap()
        .unwrap();
    assert!(t.quota_said.is_none());
}

/// 认不出的顶层类型 · 认不出的 `response_item` 子型 ⇒ 一条 `unread`（类型照那一行说的，写好的一句带它）。
#[test]
fn unknown_kinds_come_out_as_unread() {
    for (v, want) in [
        (
            json!({"type": "some_future_kind", "payload": {"id": "x"}}),
            "some_future_kind",
        ),
        (
            env("response_item", json!({"type": "future_item"})),
            "future_item",
        ),
    ] {
        let r = rec(&v).unwrap_or_else(|| panic!("{v}"));
        let crate::agents::record::Body::Unread {
            kind, text, count, ..
        } = &r.body
        else {
            panic!("{r:?}")
        };
        assert_eq!(kind.as_deref(), Some(want));
        assert!(text.0.contains(want), "{text:?}");
        assert_eq!(*count, 1);
    }
}

// ─── 谁说的：金样（只采结构：字段名 · 类型 · 判别值；正文全是占位）───

const SPEAKER_GOLDEN: &str = include_str!("../../../__fixtures__/codex-speaker.golden.jsonl");

/// 金样每一行：一条 rollout 记录 ⇒ 通用记录 `said` 的 `who`（不是人那一侧的记录 ⇒ `null`）。
#[test]
fn speaker_golden() {
    let mut n = 0;
    for row in SPEAKER_GOLDEN.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(row).unwrap();
        let case = row["case"].as_str().unwrap();
        let raw = row["line"].to_string();
        let got = translated(&crate::agents::Line::new(&raw, 0))
            .unwrap()
            .unwrap()
            .record
            .map(|b| b.rec);
        assert_eq!(
            got.as_ref()
                .and_then(who_of)
                .map_or(Value::Null, |w| serde_json::to_value(w).unwrap()),
            row["userText"],
            "金样 {case} 判错了"
        );
        n += 1;
    }
    assert!(n >= 20, "金样只读到 {n} 行");
}

/// Codex 能写出来的那几类来源，金样里每一类至少一行（漏了一类 ⇒ 那一类没人钉）。
#[test]
fn speaker_golden_covers_every_source_codex_writes() {
    let kinds: std::collections::BTreeSet<String> = SPEAKER_GOLDEN
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|r| {
            r["userText"]["speaker"]["kind"]
                .as_str()
                .map(str::to_string)
        })
        .collect();
    for want in [
        "human",
        "system",
        "interrupt",
        "bashInput",
        "taskNotification",
        "agentMessage",
        "agentTask",
        "coordinator",
        "compactSummary",
        "toolResult",
    ] {
        assert!(kinds.contains(want), "金样里没有 {want}");
    }
}

/// 首条真用户话与渲染那一侧同一个判定：注入 · 中断 · 来信都跳过，`!` 命令照人说的算。
#[test]
fn excerpt_uses_the_same_speaker() {
    let user = |text: &str, kinds: &[&str]| {
        json!({"type": "message", "role": "user",
               "content": [{"type": "input_text", "text": text}],
               "internal_chat_message_metadata_passthrough": {"content_item_kinds": kinds}})
    };
    assert_eq!(
        message_said(&user("<x>", &["environments.environment_context"]))
            .and_then(|s| s.who.speech()),
        None
    );
    assert_eq!(
        message_said(&user("<y>", &["user.text"])).and_then(|s| s.who.speech()),
        Some("<y>".to_string())
    );
}

/// Codex 那一家的成品同样带着信封上的时刻 `at`（出口那一下照它写钟面，钉在 `wire/stamp_tests.rs`）；解析那一层不写钟面。
#[test]
fn codex_records_carry_the_envelope_at() {
    let ts = "2026-10-07T20:30:15.123Z";
    let msg = json!({"timestamp": ts, "type": "response_item", "payload": {"type": "message", "role": "assistant", "id": "m1", "content": [{"type": "output_text", "text": "ok"}]}});
    let out = json!({"timestamp": ts, "type": "response_item", "payload": {"type": "function_call_output", "call_id": "c", "output": "x"}});
    for v in [msg, out] {
        let got = translated(&crate::agents::Line::new(&v.to_string(), 0))
            .unwrap()
            .unwrap()
            .record
            .map(|b| b.rec)
            .unwrap();
        assert!(got.time_text.is_none(), "解析那一层不写钟面");
        assert_eq!(got.at.as_deref(), Some(ts), "{v}");
    }
}

/// read8c：同一行交给这一家的几问（成品 · 此刻在干什么）共用一份解析 —— 从文本只解一次。
#[test]
fn the_three_asks_on_one_line_share_one_parse() {
    let raw = json!({"timestamp": "t", "type": "event_msg", "payload": {"type": "task_complete", "turn_id": "t1"}}).to_string();
    let line = crate::agents::Line::new(&raw, 0);
    crate::agents::LINE_PARSES.with(|c| c.set(0));
    let _ = translated(&line);
    assert_eq!(
        activity_of(&line),
        Some(crate::agents::SessionActivity::Idle)
    );
    assert_eq!(
        crate::agents::LINE_PARSES.with(std::cell::Cell::get),
        1,
        "一行解析了不止一遍"
    );
}

// ─── 两种记录形 · 附件 · 子 agent · 漂移账 · 活动 · 轮的 id（夹具 `fixtures/`）───

const PAGINATED: &str = include_str!("fixtures/rollout-paginated.jsonl");
const SUBAGENT: &str = include_str!("fixtures/rollout-subagent.jsonl");
const LEGACY: &str = include_str!("../../../__fixtures__/codex-rollout.sample.jsonl");

/// 一份记录逐行翻（起点偏移照真的累加）。
fn records_of(text: &str) -> Vec<Record> {
    let mut at = 0u64;
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = crate::agents::Line::new(raw, at);
        if let Some(r) = translated(&line).unwrap().and_then(|t| t.record) {
            let r = r.rec;
            out.push(r);
        }
        at += raw.len() as u64 + 1;
    }
    out
}

/// 人说的那几条的正文 · 代理回话的正文 · 工具结果条数。
fn talk_of(rs: &[Record]) -> (Vec<String>, Vec<String>, usize) {
    let (mut said, mut replies, mut results) = (Vec::new(), Vec::new(), 0);
    for r in rs {
        match &r.body {
            Body::Said { who, .. } if who.speaker == Speaker::Human => said.push(who.text.clone()),
            Body::Said { who, .. } if who.speaker == Speaker::ToolResult => results += 1,
            Body::Reply { blocks, .. } => {
                let t = serde_json::to_string(blocks).unwrap();
                if t.contains("\"prose\"") {
                    replies.push(t);
                }
            }
            _ => {}
        }
    }
    (said, replies, results)
}

/// Paginated 形：会话正文照 `response_item` 读；`item_completed`（同一句话 · 同一段命令输出的另一份投影）不另出记录，
/// 两条人话 · 一条回话 · 一段工具输出各只一份。
#[test]
fn paginated_rollout_reads_each_line_of_talk_once() {
    let rs = records_of(PAGINATED);
    let (said, replies, results) = talk_of(&rs);
    assert_eq!(said, vec!["what is in these", "and the second one"]);
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(replies[0].contains("two pictures"));
    assert_eq!(results, 1);
    for raw in PAGINATED
        .lines()
        .filter(|l| l.contains("\"item_completed\""))
    {
        let v: Value = serde_json::from_str(raw).unwrap();
        assert!(rec(&v).is_none(), "item_completed 出了记录：{raw}");
    }
}

/// Legacy 形：正文照 `response_item` 读，`event_msg.user_message` 那一份不另出。
#[test]
fn legacy_rollout_reads_its_talk() {
    let (said, replies, results) = talk_of(&records_of(LEGACY));
    assert_eq!(said, vec!["list the files"]);
    assert_eq!(replies.len(), 1);
    assert_eq!(results, 2);
}

/// 上下文片段只认起头、不分大小写。
#[test]
fn context_frames_open_the_line_in_any_case() {
    let mk = |text: &str| {
        env(
            "response_item",
            json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}),
        )
    };
    for (text, human) in [
        (
            "<environment_context><cwd>/w</cwd></environment_context>",
            false,
        ),
        (
            "<Environment_Context><cwd>/w</cwd></Environment_Context>",
            false,
        ),
        ("see <environment_context> here", true),
    ] {
        let r = rec(&mk(text)).unwrap();
        assert_eq!(
            who_of(&r).map(|w| w.speaker == Speaker::Human),
            Some(human),
            "{text}"
        );
    }
}

/// 一条记录里的图（`Block::Image` 的媒体类型 · 第几张 · 在哪一行）。
fn images_of(r: &Record) -> Vec<(String, u32, u64)> {
    match &r.body {
        Body::Said { blocks, .. } => blocks
            .iter()
            .filter_map(|b| match b {
                Block::Image {
                    media_type, n, at, ..
                } => Some((media_type.clone(), *n, *at)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 附件（Paginated 形）：人说的那一条带着它的两张图（本地图那一张 Codex 内联成 `input_image`，路径框不进正文）。
#[test]
fn paginated_user_line_carries_its_images() {
    let raw = PAGINATED
        .lines()
        .find(|l| l.contains("\"m-u1\"") && l.contains("response_item"))
        .unwrap();
    let v: Value = serde_json::from_str(raw).unwrap();
    let r = record_of(&v, 4096).unwrap();
    assert_eq!(
        who_of(&r).map(|w| w.text.as_str()),
        Some("what is in these")
    );
    assert_eq!(
        images_of(&r),
        vec![
            ("image/png".into(), 0, 4096),
            ("image/jpeg".into(), 1, 4096)
        ]
    );
    // 取图那一条读回的是那一张的字节：按 `n` 排在只给后端看的那一份里，不上线。
    let t = translated(&crate::agents::Line::new(raw, 4096))
        .unwrap()
        .unwrap();
    assert_eq!(t.unsent.images.get(1).map(String::as_str), Some("BBBB"));
    assert_eq!(t.unsent.images.len(), 2);
    let wire = serde_json::to_string(&r).unwrap();
    assert!(!wire.contains("BBBB"), "图的字节不上线：{wire}");
}

/// 附件（Legacy 形）：`user_message.images` · `local_images` 那一份不另出；人说的那一条（`response_item`）带着那几张。
#[test]
fn legacy_user_line_carries_its_images() {
    let said = env(
        "response_item",
        json!({"type": "message", "role": "user", "content": [
            {"type": "input_image", "image_url": "data:image/gif;base64,R0lG"},
            {"type": "input_text", "text": "<image name=[Image #1] path=\"/w/a.webp\">"},
            {"type": "input_image", "image_url": "data:image/webp;base64,UklG"},
            {"type": "input_text", "text": "</image>"},
            {"type": "input_text", "text": "compare"}
        ]}),
    );
    let event = env(
        "event_msg",
        json!({"type": "user_message", "message": "compare", "images": ["data:image/gif;base64,R0lG"], "local_images": ["/w/a.webp"]}),
    );
    let r = rec(&said).unwrap();
    assert_eq!(who_of(&r).map(|w| w.text.as_str()), Some("compare"));
    assert_eq!(
        images_of(&r),
        vec![("image/gif".into(), 0, 40), ("image/webp".into(), 1, 40)]
    );
    assert!(rec(&event).is_none());
}

/// 子 agent 线程：`source.subagent.thread_spawn` ⇒ 父 · 昵称 · 角色；`forked_from_id` ⇒ 叉自；没有那一格 ⇒ 不挂。
#[test]
fn a_subagent_thread_names_its_parent_and_fork() {
    let head: Value = serde_json::from_str(SUBAGENT.lines().next().unwrap()).unwrap();
    let t = thread_of(&head).unwrap();
    assert_eq!(t.parent, Some("0000beef-0000-4000-8000-0000000000a0"));
    assert_eq!((t.nickname, t.role), (Some("Scout"), Some("explorer")));
    assert_eq!(t.forked_from, Some("0000beef-0000-4000-8000-0000000000b0"));
    let plain: Value = serde_json::from_str(PAGINATED.lines().next().unwrap()).unwrap();
    let t = thread_of(&plain).unwrap();
    assert_eq!((t.parent, t.forked_from), (None, None));
    // 不是 session_meta ⇒ 不说。
    assert!(thread_of(&env("turn_context", json!({}))).is_none());
}

/// 子会话挂在父会话底下：父那一份底下列得出它（带昵称 · 角色），它说得出父那一份在哪；不是子线程的不挂。
#[test]
fn a_subagent_rollout_hangs_under_its_parent() {
    let home = test_home::scratch("codex-child");
    let root = super::super::parse::sessions_root(&home);
    let (pa, ch, other) = (
        "0000beef-0000-4000-8000-0000000000a0",
        "0000beef-0000-4000-8000-0000000000c1",
        "0000beef-0000-4000-8000-0000000000d9",
    );
    let meta = |id: &str| {
        format!(
            "{}\n",
            json!({"timestamp": "t", "type": "session_meta", "payload": {"id": id, "cwd": "/w", "source": "cli"}})
        )
    };
    let d1 = root.join("2026/11/01");
    let d2 = root.join("2026/11/02");
    std::fs::create_dir_all(&d1).unwrap();
    std::fs::create_dir_all(&d2).unwrap();
    let parent = d1.join(format!("rollout-2026-11-01T00-00-00-{pa}.jsonl"));
    let child = d2.join(format!("rollout-2026-11-02T00-00-00-{ch}.jsonl"));
    let loose = d2.join(format!("rollout-2026-11-02T00-00-01-{other}.jsonl"));
    std::fs::write(&parent, meta(pa)).unwrap();
    std::fs::write(&child, SUBAGENT).unwrap();
    std::fs::write(&loose, meta(other)).unwrap();

    assert_eq!(child_sources(&parent), vec![child.clone()]);
    assert!(child_sources(&loose).is_empty());
    assert_eq!(child_owner(&child), Some(parent.clone()));
    assert_eq!(child_owner(&loose), None);
    assert_eq!(child_find(&root, ch), Some(child.clone()));
    assert_eq!(child_find(&root, other), None);
    let l = child_spawned(&child).unwrap();
    assert_eq!(l.run.as_deref(), Some(ch));
    assert_eq!(
        (l.label.as_deref(), l.kind.as_deref()),
        (Some("Scout"), Some("explorer"))
    );
    assert!(child_spawned(&loose).is_none());
    let _ = std::fs::remove_dir_all(&home);
}

/// 认父走多少：记录根下六十天各五份 rollout。父子同一天 ⇒ 只读那一天的目录；
/// 父在别的天 ⇒ 往外扫整个记录根照样找到，自己那一天只读一次；父已删 ⇒ `None`。数的是读过的目录，不计时。
#[test]
fn a_child_finds_its_parent_in_its_own_day_first() {
    let home = test_home::scratch("codex-child-day");
    let root = super::super::parse::sessions_root(&home);
    let pa = "0000beef-0000-4000-8000-0000000000a0";
    let meta = |id: &str| {
        format!(
            "{}\n",
            json!({"timestamp": "t", "type": "session_meta", "payload": {"id": id, "cwd": "/w", "source": "cli"}})
        )
    };
    let day = |n: u32| root.join(format!("2026/{:02}/{:02}", 9 + n / 30, 1 + n % 30));
    for n in 0..60u32 {
        std::fs::create_dir_all(day(n)).unwrap();
        for k in 0..5u32 {
            let id = format!("0000beef-0000-4000-8000-{:012x}", 0x1000 + n * 10 + k);
            let f = day(n).join(format!("rollout-2026-09-01T00-00-0{k}-{id}.jsonl"));
            std::fs::write(f, meta(&id)).unwrap();
        }
    }
    let child_in = |n: u32| {
        let ch = format!("0000beef-0000-4000-8000-{:012x}", 0xc000 + n);
        let p = day(n).join(format!("rollout-2026-09-01T00-00-09-{ch}.jsonl"));
        std::fs::write(&p, SUBAGENT).unwrap();
        p
    };
    let walk = |child: &Path| {
        let mut dirs = Vec::new();
        let found = owner_walk(child, &mut |d| dirs.push(d.to_path_buf()));
        (found, dirs)
    };

    // 父已删：孤儿 ⇒ None（扫过整个根也没有）。
    let orphan = child_in(31);
    let (found, dirs) = walk(&orphan);
    assert_eq!(found, None);
    assert!(dirs.len() > 60, "扫了整个根：{}", dirs.len());

    // 父子同一天 ⇒ 只读那一天。
    let parent = day(40).join(format!("rollout-2026-09-01T00-00-08-{pa}.jsonl"));
    std::fs::write(&parent, meta(pa)).unwrap();
    let same_day = child_in(40);
    let (found, dirs) = walk(&same_day);
    assert_eq!(found, Some(parent.clone()));
    assert_eq!(dirs, vec![day(40)]);
    assert_eq!(child_owner(&same_day), Some(parent.clone()));

    // 父在别的天 ⇒ 照样找到；先读了自己那一天，往外扫时不再读第二遍。
    let other_day = child_in(20);
    let (found, dirs) = walk(&other_day);
    assert_eq!(found, Some(parent.clone()));
    assert_eq!(dirs.first(), Some(&day(20)));
    assert_eq!(dirs.iter().filter(|d| **d == day(20)).count(), 1);
    assert!(dirs.contains(&day(40)));
    assert_eq!(child_owner(&other_day), Some(parent));

    // 父 sid 不成形 ⇒ 不读盘，答 None。
    let bad =
        day(10).join("rollout-2026-09-01T00-00-09-0000beef-0000-4000-8000-00000000bad0.jsonl");
    std::fs::write(&bad, SUBAGENT.replace(pa, "../escape")).unwrap();
    let (found, dirs) = walk(&bad);
    assert_eq!(found, None);
    assert!(dirs.is_empty(), "不成形的父 sid 也走了盘：{dirs:?}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 父记录里说到子线程的那几条：派它的那次调用对上它（带昵称 · 角色）；它收场了。
#[test]
fn the_parent_rollout_links_its_subagent_threads() {
    let ch = "0000beef-0000-4000-8000-0000000000c1";
    let spawn = env(
        "event_msg",
        json!({"type": "item_completed", "turn_id": "t", "item": {"type": "CollabAgentToolCall", "id": "call-9",
               "tool": "spawn_agent", "status": "completed", "sender_thread_id": "x", "receiver_thread_ids": [ch],
               "receiver_agents": [{"thread_id": ch, "agent_nickname": "Scout", "agent_role": "explorer"}]}}),
    );
    assert!(child_hint(&spawn.to_string()));
    assert_eq!(
        child_link(&spawn),
        vec![crate::agents::ChildLink {
            tool: Some("call-9".into()),
            run: Some(ch.into()),
            label: Some("Scout".into()),
            kind: Some("explorer".into()),
            ..Default::default()
        }]
    );
    let done = env(
        "event_msg",
        json!({"type": "item_completed", "turn_id": "t", "item": {"type": "SubAgentActivity", "id": "a-1",
               "kind": "completed", "agent_thread_id": ch, "agent_path": "/root/scout", "model": null, "reasoning_effort": null}}),
    );
    assert!(child_hint(&done.to_string()));
    assert_eq!(
        child_link(&done),
        vec![crate::agents::ChildLink {
            run: Some(ch.into()),
            end: Some(crate::agents::RunEnd::Done),
            ..Default::default()
        }]
    );
    // 别的调用 · 别的行 ⇒ 不说。
    let send = env(
        "event_msg",
        json!({"type": "item_completed", "turn_id": "t", "item": {"type": "CollabAgentToolCall", "id": "call-8",
               "tool": "send_input", "status": "completed", "sender_thread_id": "x", "receiver_thread_ids": [ch]}}),
    );
    assert!(child_link(&send).is_empty());
    assert!(child_link(&env("event_msg", json!({"type": "task_complete"}))).is_empty());
}

/// 活动：一轮开始 ⇒ 在做；完了 ⇒ 闲着；开始之后中止 ⇒ 闲着（Paginated 夹具第二轮就是中止的）。
#[test]
fn activity_follows_turn_start_end_and_abort() {
    use crate::agents::SessionActivity as A;
    let said: Vec<A> = PAGINATED
        .lines()
        .filter_map(|raw| activity_of(&crate::agents::Line::new(raw, 0)))
        .collect();
    assert_eq!(said, vec![A::Working, A::Idle, A::Working, A::Idle]);
}

/// 轮的 id：两轮的 `task_started` 各带 `turn_id` ⇒ 读出那两个（别的行不说）。
#[test]
fn turn_ids_come_from_turn_start() {
    let ids: Vec<String> = PAGINATED
        .lines()
        .filter_map(|raw| {
            let v: Value = serde_json::from_str(raw).unwrap();
            turn_id(&v).map(str::to_string)
        })
        .collect();
    assert_eq!(ids, vec!["turn-a", "turn-b"]);
    assert_eq!(
        turn_id(&env(
            "event_msg",
            json!({"type": "turn_started", "turn_id": "v2"})
        )),
        Some("v2")
    );
}

/// 这台的账里那一面那一键记了几笔。
fn drift_count(face: &str, key: &str) -> u64 {
    drift_snapshot()
        .into_iter()
        .filter(|f| f.face == face)
        .flat_map(|f| f.entries)
        .find(|e| e.key == key)
        .map_or(0, |e| e.count)
}

/// 漂移账（接缝）：一条不认得的顶层类型 · 一条不认得的 `event_msg` 子类型 · 一条不认得的 `response_item` 子类型 ⇒ 账上各一笔；
/// 认得的（`item_completed` · 簿记行 · Codex 写的那几种元记录）不记。键名取得独一份（账本是进程内全局的）。
#[test]
fn unknown_types_land_on_the_drift_ledger() {
    let lines = [
        json!({"type": "p31_future_top", "payload": {}}),
        env("event_msg", json!({"type": "p31_future_event"})),
        env("response_item", json!({"type": "p31_future_item"})),
    ];
    for v in &lines {
        let _ = translated(&crate::agents::Line::new(&v.to_string(), 0));
    }
    assert_eq!(drift_count("unknown_record_type", "p31_future_top"), 1);
    assert_eq!(drift_count("unknown_event_type", "p31_future_event"), 1);
    assert_eq!(drift_count("unknown_item_type", "p31_future_item"), 1);
    for raw in PAGINATED.lines().chain(SUBAGENT.lines()) {
        let _ = translated(&crate::agents::Line::new(raw, 0));
    }
    for (face, key) in [
        ("unknown_event_type", "item_completed"),
        ("unknown_event_type", "token_count"),
        ("unknown_record_type", "token_usage_record"),
        ("unknown_record_type", "session_meta"),
    ] {
        assert_eq!(drift_count(face, key), 0, "{face} {key}");
    }
    for top in [
        "compacted",
        "dynamic_tools_updated",
        "retained_context",
        "security_risk_score",
        "realtime_item",
    ] {
        let v = json!({"type": top, "payload": {}});
        assert!(rec(&v).is_none(), "{top}");
        assert_eq!(drift_count("unknown_record_type", top), 0, "{top}");
    }
}

/// 记录根下的 `.jsonl.zst`（压缩存的 rollout）：不当会话读，账上一笔。
#[test]
fn a_compressed_rollout_is_not_a_session_but_lands_on_the_ledger() {
    let home = test_home::scratch("codex-zst");
    let day = super::super::parse::sessions_root(&home).join("2026/11/03");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join("rollout-2026-11-03T00-00-00-0000beef-0000-4000-8000-0000000000f1.jsonl.zst"),
        b"\x28\xb5\x2f\xfd",
    )
    .unwrap();
    let before = drift_count("compressed_rollout", "jsonl.zst");
    assert!(super::super::history::sessions_under(&home).is_empty());
    assert_eq!(drift_count("compressed_rollout", "jsonl.zst"), before + 1);
    let _ = std::fs::remove_dir_all(&home);
}

/// 图只挂在人说的那一条上；按文件号引用的图画不出 ⇒ 不出块。
#[test]
fn images_hang_only_on_human_lines() {
    let sys = env(
        "response_item",
        json!({"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "<environment_context>x</environment_context>"},
            {"type": "input_image", "image_url": "data:image/png;base64,AAAA"}
        ]}),
    );
    assert!(images_of(&rec(&sys).unwrap()).is_empty());
    let by_file = env(
        "response_item",
        json!({"type": "message", "role": "user", "content": [
            {"type": "input_image", "file_id": "file-1"},
            {"type": "input_text", "text": "hi"}
        ]}),
    );
    assert!(images_of(&rec(&by_file).unwrap()).is_empty());
}
