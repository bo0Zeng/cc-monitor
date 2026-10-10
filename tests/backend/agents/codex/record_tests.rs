use super::super::parse::session_meta_cwd;
use super::CodexRecordKind as K;
use super::*;
use crate::agents::Speaker;
use crate::agents::UserText;
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
            ends_turn: false,
            ..
        }
    ));
    assert_eq!((r.agent.as_str(), r.id.as_str()), ("codex", "m1"));
    assert_eq!(blocks_of(&r), json!([{"type": "text", "text": "回复"}]));

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
        json!([{"type": "thinking", "text": "想了想"}])
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
        json!([{"type": "tool_result", "for": "c1", "content": [{"type": "text", "text": "文件列表"}], "isError": false}])
    );
}

/// F1a-3：session_meta cwd 抽取（Codex 无 cwd-项目目录 → list 用 cwd 内存分组）。
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
        json!({"type": "some_future_kind", "payload": {"id": "x"}}),
    ] {
        assert!(rec(&v).is_none(), "{v}");
        let t = translated(&v.to_string(), 0).unwrap().unwrap();
        assert!(t.record.is_none() && t.queue.is_none(), "{v}");
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
        let got = translated(&raw, 0).unwrap().unwrap().record;
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
        message_said(&user("<x>", &["environments.environment_context"])).and_then(|s| s.speech()),
        None
    );
    assert_eq!(
        message_said(&user("<y>", &["user.text"])).and_then(|s| s.speech()),
        Some("<y>".to_string())
    );
}

/// Codex 那一家的成品同样在出口那一下得到 `timeText`（信封上的时刻，按看的那一台的时区写 `HH:MM`）。
#[test]
fn codex_records_carry_the_clock_face_too() {
    let ts = "2026-10-07T20:30:15.123Z";
    let want = "04:30".to_string();
    let msg = json!({"timestamp": ts, "type": "response_item", "payload": {"type": "message", "role": "assistant", "id": "m1", "content": [{"type": "output_text", "text": "ok"}]}});
    let out = json!({"timestamp": ts, "type": "response_item", "payload": {"type": "function_call_output", "call_id": "c", "output": "x"}});
    for v in [msg, out] {
        let got = translated(&v.to_string(), 0)
            .unwrap()
            .unwrap()
            .record
            .unwrap();
        assert!(got.time_text.is_none(), "解析那一层不写钟面");
        let mut got = got;
        got.stamp(&crate::Tz::named("Asia/Shanghai").unwrap());
        assert_eq!(
            got.time_text.as_ref().map(|w| w.0.as_str()),
            Some(want.as_str()),
            "{v}"
        );
        assert_eq!(got.at.as_deref(), Some(ts), "{v}");
    }
}
