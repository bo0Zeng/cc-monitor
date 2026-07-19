//! Phase 2 · F2a：Codex rollout 记录的**防御式分类器**（keystone 第一块）。
//!
//! Codex 格式**未文档、每几个 minor 版 churn**（openai/codex 源码 + web 交叉证，见
//! `code-picture/codex-vs-claude-事实对照_2026-07-18.md`）→ **不建 rigid typed enum**（每字段漂移即崩），
//! 在 `serde_json::Value` 上**宽容抽取**（同 daemon `usage_query`/`turn_detect` 套路）：逐行不崩、
//! 每 record/field 当 optional、**alias 归一 `turn_*`↔`task_*`**（EventMsg 被改名，`task_*` 是 v1 别名）、
//! 未知 type → `Other`（前向兼容不崩）。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者的 `payload.type` 再细分。
//!
//! **本 slice 只落分类 + 关键字段 accessor**（turn-end/usage/UI 各 feature 消费它）。中立 CanonicalRecord
//! 统一 vs per-kind adapter 方法的取舍，留到接 consumer 时定（见 `features/02-canonical-record.md`）。

// 分类器已就绪 + golden 测覆盖；production consumer 在 F3(turn-end)/F5(usage)/F7(UI) 接线——在此之前
// 全模块 staged，故 `#![allow(dead_code)]`（接线的 commit 摘掉，同 turn_detect 先例）。
#![allow(dead_code)]

use serde_json::Value;

/// Codex 记录的**语义种类**（防御分类；未知/未来 → `Other*`，不崩）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexRecordKind {
    SessionMeta,
    TurnContext,
    WorldState,
    /// response_item.message（role ∈ user/assistant/developer）。
    Message,
    /// response_item.reasoning。
    Reasoning,
    /// response_item.custom_tool_call / function_call / local_shell_call（按 call_id 配 result）。
    ToolCall,
    /// response_item.custom_tool_call_output / function_call_output。
    ToolResult,
    /// event_msg task_started/turn_started（一轮开始）。
    TurnStarted,
    /// event_msg task_complete/turn_complete —— **turn-end 边沿**（F3；uuid=turn_id）。
    TurnComplete,
    /// event_msg turn_aborted —— 中止轮（aterm 决策：静默不发 TurnEnd）。
    TurnAborted,
    /// event_msg user_message。
    UserMessage,
    /// event_msg agent_message。
    AgentMessage,
    /// event_msg token_count —— **用量**（F5；info.total/last_token_usage）。
    TokenCount,
    /// event_msg 其它子型（mcp_tool_call_end / thread_rolled_back / thread_settings_applied / …）。
    OtherEvent,
    /// 未知顶层 type / 信封缺失（前向兼容、坏行）。
    Other,
}

/// 解包信封 `{type, payload}` → `(顶层 type, payload)`。缺 type / payload 非对象 → `None`。
pub fn unwrap_envelope(v: &Value) -> Option<(&str, &Value)> {
    let top = v.get("type")?.as_str()?;
    let payload = v.get("payload").filter(|p| p.is_object())?;
    Some((top, payload))
}

/// payload 的 `type` 子判别（response_item/event_msg 用）。
fn payload_type(payload: &Value) -> Option<&str> {
    payload.get("type").and_then(Value::as_str)
}

/// alias 归一：`turn_started`→`task_started`、`turn_complete`→`task_complete`（EventMsg v1 别名兼容，
/// 新旧版本都吃）。其它原样。
fn normalize_event(t: &str) -> &str {
    match t {
        "turn_started" => "task_started",
        "turn_complete" => "task_complete",
        other => other,
    }
}

/// 防御分类：unwrap 信封 → 顶层 type（+ 必要时 payload.type，alias 归一）→ [`CodexRecordKind`]。
/// 任何缺失/未知 → `Other`/`OtherEvent`（不崩、前向兼容）。
pub fn classify(v: &Value) -> CodexRecordKind {
    use CodexRecordKind as K;
    let Some((top, payload)) = unwrap_envelope(v) else {
        return K::Other;
    };
    match top {
        "session_meta" => K::SessionMeta,
        "turn_context" => K::TurnContext,
        "world_state" => K::WorldState,
        "response_item" => match payload_type(payload) {
            Some("message") => K::Message,
            Some("reasoning") => K::Reasoning,
            Some("custom_tool_call" | "function_call" | "local_shell_call") => K::ToolCall,
            Some("custom_tool_call_output" | "function_call_output") => K::ToolResult,
            _ => K::Other, // 未知 response_item 子型（前向兼容）
        },
        "event_msg" => match payload_type(payload).map(normalize_event) {
            Some("task_started") => K::TurnStarted,
            Some("task_complete") => K::TurnComplete,
            Some("turn_aborted") => K::TurnAborted,
            Some("user_message") => K::UserMessage,
            Some("agent_message") => K::AgentMessage,
            Some("token_count") => K::TokenCount,
            _ => K::OtherEvent, // mcp_tool_call_end / thread_rolled_back / … / 未知
        },
        _ => K::Other, // 未知顶层 type（未来新增记录种类）
    }
}

/// event_msg 的 `payload.turn_id`（TurnStarted/Complete/Aborted 用；F3 turn-end uuid=此）。
pub fn turn_id(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("turn_id").and_then(Value::as_str)
}

/// token_count 的 `payload.info.last_token_usage`（本轮增量用量；F5 抽字段）。原样返回 Value。
pub fn token_usage_last(v: &Value) -> Option<&Value> {
    unwrap_envelope(v)?.1.get("info")?.get("last_token_usage")
}

/// response_item.message 的 `payload.role`（user/assistant/developer；F7 渲染用）。
pub fn message_role(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("role").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
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
}
