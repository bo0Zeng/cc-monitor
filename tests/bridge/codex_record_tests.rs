use super::CodexRecordKind as K;
use super::*;
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
    // 本机 task_complete → TurnComplete，turn_id 抽出（F3 turn-end 键）。
    let tc = env(
        "event_msg",
        json!({"type": "task_complete", "turn_id": "019f7868-0e2d-7d73-bb7a-2f3837e5cb95", "duration_ms": 12104}),
    );
    assert_eq!(classify(&tc), K::TurnComplete);
    assert_eq!(turn_id(&tc), Some("019f7868-0e2d-7d73-bb7a-2f3837e5cb95"));
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
    // token_count → TokenCount，last usage 抽出（F5）。
    let tok = env(
        "event_msg",
        json!({"type": "token_count", "info": {"last_token_usage": {"input_tokens": 13839, "output_tokens": 157, "total_tokens": 13996}}}),
    );
    assert_eq!(classify(&tok), K::TokenCount);
    assert_eq!(
        token_usage_last(&tok)
            .and_then(|u| u.get("total_tokens"))
            .and_then(Value::as_u64),
        Some(13996)
    );
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
    assert_eq!(message_role(&msg), Some("assistant"));
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
    assert_eq!(turn_id(&json!({})), None);
    assert_eq!(
        token_usage_last(&env("event_msg", json!({"type": "token_count"}))),
        None
    );
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

// ─── F2b-2：to_jsonl_record 组装 ───

fn content_of(r: &JsonlRecord) -> Value {
    match r {
        JsonlRecord::User { message, .. } | JsonlRecord::Assistant { message, .. } => {
            message.content.clone()
        }
        _ => Value::Null,
    }
}

/// message：assistant→Assistant+text block；developer→User(isMeta)；user 空→User content []。
#[test]
fn maps_message_to_user_assistant() {
    let asst = env(
        "response_item",
        json!({"type": "message", "role": "assistant", "id": "m1", "content": [{"type": "output_text", "text": "回复"}]}),
    );
    let r = to_jsonl_record(&asst, "raw");
    assert!(matches!(&r, JsonlRecord::Assistant { uuid, .. } if uuid == "m1"));
    assert_eq!(content_of(&r), json!([{"type": "text", "text": "回复"}]));

    // developer → User isMeta=true、无 id → uuid ""。
    let dev = env(
        "response_item",
        json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "sys"}]}),
    );
    assert!(
        matches!(to_jsonl_record(&dev, "r"), JsonlRecord::User { is_meta: true, uuid, .. } if uuid.is_empty())
    );

    // user 空 content → User，content []（免空气泡）。
    let u = env(
        "response_item",
        json!({"type": "message", "role": "user", "content": []}),
    );
    let r = to_jsonl_record(&u, "r");
    assert!(matches!(&r, JsonlRecord::User { is_meta: false, .. }));
    assert_eq!(content_of(&r), json!([]));
}

/// F7 去噪：role=user 但正文是 CLI 注入的上下文块（3 标记，aterm 2C/doc §63 对齐）→ User
/// isMeta=true（渲染隐藏）；真用户输入（含裸 # 标题/提及标签名）→ isMeta=false（正常气泡）。真机核 0 误判。
#[test]
fn denoise_injected_context_user_messages() {
    let mk = |text: &str| {
        env(
            "response_item",
            json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}),
        )
    };
    // 注入块（含前导空白）→ isMeta=true。3 标记与 aterm 2C / doc §63 对齐。
    for inj in [
        "<environment_context>\n  <cwd>/home/zbl</cwd>\n</environment_context>",
        "  <recommended_plugins>\nHere is a list of plugins…",
        "# AGENTS.md instructions\n\n<INSTRUCTIONS>\n# AGENTS.md\n本文件…",
    ] {
        assert!(
            matches!(
                to_jsonl_record(&mk(inj), "r"),
                JsonlRecord::User { is_meta: true, .. }
            ),
            "注入块应去噪当 meta: {inj:?}"
        );
    }
    // 真用户输入 → isMeta=false（碰巧提及标签名但非以之起头的、及裸 markdown 标题 → 不误伤）。
    for real in [
        "codex怎么换行",
        "帮我看看 <environment_context> 是什么",
        "# 我的笔记\n随便写的",  // 裸 # 标题 ≠ `# AGENTS.md instructions`
        "# AGENTS.md 里写了啥?", // 提及但非机器注入整串前缀
    ] {
        assert!(
            matches!(
                to_jsonl_record(&mk(real), "r"),
                JsonlRecord::User { is_meta: false, .. }
            ),
            "真用户输入不应被去噪: {real:?}"
        );
    }
}

/// reasoning：空 summary→Assistant content []（免 Thinking 噪音）；有 text→thinking block。
#[test]
fn maps_reasoning_empty_and_nonempty() {
    let empty = env("response_item", json!({"type": "reasoning", "summary": []}));
    assert_eq!(content_of(&to_jsonl_record(&empty, "r")), json!([]));
    let think = env(
        "response_item",
        json!({"type": "reasoning", "summary": [{"text": "想了想"}]}),
    );
    assert_eq!(
        content_of(&to_jsonl_record(&think, "r")),
        json!([{"type": "thinking", "thinking": "想了想"}])
    );
}

/// tool_call→Assistant+tool_use；tool_output(数组)→User+tool_result（content=拼接文本、守丢文本坑）。
#[test]
fn maps_tool_call_and_output() {
    let call = env(
        "response_item",
        json!({"type": "custom_tool_call", "call_id": "c1", "name": "shell", "input": {"cmd": "ls"}}),
    );
    assert_eq!(
        content_of(&to_jsonl_record(&call, "r")),
        json!([{"type": "tool_use", "id": "c1", "name": "shell", "input": {"cmd": "ls"}}])
    );
    // output 真机数组 → tool_result.content 拼接文本（非空！守坑）。
    let out = env(
        "response_item",
        json!({"type": "custom_tool_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "文件列表"}]}),
    );
    let r = to_jsonl_record(&out, "r");
    assert!(matches!(&r, JsonlRecord::User { .. }));
    assert_eq!(
        content_of(&r),
        json!([{"type": "tool_result", "tool_use_id": "c1", "content": "文件列表"}])
    );
}

/// F1a-3：session_meta cwd/timestamp 抽取（Codex 无 cwd-项目目录 → list 用 cwd 内存分组）。
#[test]
fn session_meta_cwd_and_timestamp() {
    let sm = env(
        "session_meta",
        json!({"session_id": "s", "cwd": "/home/u/proj", "timestamp": "2026-07-19T03:25:05.382Z"}),
    );
    assert_eq!(session_meta_cwd(&sm), Some("/home/u/proj"));
    assert_eq!(
        session_meta_timestamp(&sm),
        Some("2026-07-19T03:25:05.382Z")
    );
    // 非 session_meta（如 turn_context 也有 cwd）→ None（只认 session_meta）。
    let tc = env("turn_context", json!({"cwd": "/other", "turn_id": "t"}));
    assert_eq!(session_meta_cwd(&tc), None);
}

/// 事件/元记录 → Unrecognized（保 raw、original_type、reason=codex-event；turn-end/用量 per-kind 从 raw 读）。
#[test]
fn maps_events_to_unrecognized_preserving_raw() {
    let raw = r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#;
    let v: Value = serde_json::from_str(raw).unwrap();
    match to_jsonl_record(&v, raw) {
        JsonlRecord::Unrecognized {
            raw: r,
            original_type,
            reason,
            ..
        } => {
            assert_eq!(r, raw, "raw 原样保留（turn-end 从中读 turn_id）");
            assert_eq!(original_type.as_deref(), Some("event_msg/task_complete"));
            assert_eq!(reason, "codex-event");
        }
        other => panic!("event 应落 Unrecognized，得 {other:?}"),
    }
    // session_meta 也 → Unrecognized。
    assert!(matches!(
        to_jsonl_record(&env("session_meta", json!({"id": "s"})), "r"),
        JsonlRecord::Unrecognized { .. }
    ));
}
