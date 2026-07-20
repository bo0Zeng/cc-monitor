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

// F1a 起 `to_jsonl_record`（→ classify → 助手）经 `parser::parse_for_kind` 被 history 读路调用 = 已接线。
// 仅 `turn_id`/`token_usage_last`（F3 turn-end / F5 用量 accessor）尚未接 consumer → 各自 targeted staged。

use crate::messages::{ApiMessage, JsonlRecord};
use serde_json::{json, Value};

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
#[allow(dead_code)] // F3(turn-end) consumer 接线前 staged
pub fn turn_id(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("turn_id").and_then(Value::as_str)
}

/// token_count 的 `payload.info.last_token_usage`（本轮增量用量；F5 抽字段）。原样返回 Value。
/// **实测 total_token_usage 严格单调、final == Σlast**——故 F5 按 (model,天) 累加各事件 last 增量，
/// 与 Claude 逐 request 归桶一致（取 final total 会丢跨天/跨模型粒度）。
pub fn token_usage_last(v: &Value) -> Option<&Value> {
    unwrap_envelope(v)?.1.get("info")?.get("last_token_usage")
}

/// turn_context 的 `payload.model`（F5 用量按模型归桶；一会话可多 turn_context/换模型）。非 turn_context → None。
pub fn turn_context_model(v: &Value) -> Option<&str> {
    if classify(v) != CodexRecordKind::TurnContext {
        return None;
    }
    unwrap_envelope(v)?.1.get("model").and_then(Value::as_str)
}

/// 从 token 用量子对象（last_token_usage/total_token_usage）读 `(input_tokens, cached_input_tokens,
/// output_tokens)`。缺字段→0。**Codex `input_tokens` 含 cached**——映射进 Claude 口径时须 `input -= cached`
/// 防重复计（见 usage.rs `accumulate_codex_usage`）。`reasoning_output_tokens` 是 output 子集、`total_tokens`
/// 冗余 → 不单列。
pub fn token_usage_fields(usage: &Value) -> (u64, u64, u64) {
    let g = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    (
        g("input_tokens"),
        g("cached_input_tokens"),
        g("output_tokens"),
    )
}

/// response_item.message 的 `payload.role`（user/assistant/developer；F7 渲染用）。
pub fn message_role(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("role").and_then(Value::as_str)
}

// ─── F2b：Codex→JsonlRecord 映射的文本抽取助手（trap-critical，口径对齐 aterm CodexRecordParser.kt c03e46f）───

/// **数组文本拍平**——Codex 的 `message.content` 与 `custom_tool_call_output.output` **真机恒数组**
/// `[{type:input_text|output_text, text}]`（aterm Phase D 审计 9/9 坐实）。当 String 处理会静默丢**全部**
/// 文本（且 String fixture 绿着骗过）。数组→拼各项 `text`（`input_image` 等无 text 项自然跳过）；
/// 防御：裸 String→原样；其它→""。**fixture 必用真机数组 shape。**
pub fn flatten_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|it| match it {
                Value::String(s) => Some(s.clone()),
                _ => it.get("text").and_then(Value::as_str).map(str::to_string),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// reasoning 的文本：`payload.summary`（array `[{text}]` 或裸串）→ 拍平。**真机 summary 恒 []**
/// （仅 encrypted_content）→ ""；调用方据此空文本时给**空 blocks**、不产 `Thinking("")` 噪音。
pub fn reasoning_text(v: &Value) -> String {
    unwrap_envelope(v)
        .and_then(|(_, p)| p.get("summary").map(flatten_text))
        .unwrap_or_default()
}

/// tool_call 的 `payload.input`：Object→原样、String→包 `{"input": s}`、其它/缺→`Null`（保 name 可见）。
pub fn tool_input(v: &Value) -> Value {
    match unwrap_envelope(v).and_then(|(_, p)| p.get("input")) {
        Some(o) if o.is_object() => o.clone(),
        Some(Value::String(s)) => serde_json::json!({ "input": s }),
        _ => Value::Null,
    }
}

/// tool 的 `payload.call_id`（ToolUse.id / ToolResult.tool_use_id 配对键）。
pub fn call_id(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("call_id").and_then(Value::as_str)
}

/// role=user 但正文是 **CLI 注入的上下文块**（非真用户输入 → 去噪当 meta、渲染隐藏）。判据：trim 后以
/// 已知注入标记起头。**去噪集与 aterm 2C / 事实对照 doc §63 对齐（3 标记）**——真机核（aya `~/.codex`，
/// 两端同机同数据）47 条 user msg = 34 真输入 + 2 `<environment_context>` + 5 `<recommended_plugins>` +
/// 6 `# AGENTS.md instructions`，**34 真输入 0 误判**：
/// - `<environment_context>`（cwd/shell/…）、`<recommended_plugins>`（插件清单）——干净 XML wrapper。
/// - `# AGENTS.md instructions`——AGENTS.md 注入头，真机恒 `# AGENTS.md instructions\n\n<INSTRUCTIONS>\n…`
///   （机器生成、结构唯一 = 特征前缀，真用户几乎不以此整串起头；裸 `# xxx` markdown 标题不匹配）。
///
/// **不认** `You have an MCP server…`（MCP 指令注入无干净特征前缀、怕误伤正文 → 保守留，两端一致）。
pub(crate) fn is_injected_context(text: &str) -> bool {
    let t = text.trim_start();
    [
        "<environment_context>",
        "<recommended_plugins>",
        "# AGENTS.md instructions",
    ]
    .iter()
    .any(|m| t.starts_with(m))
}

/// session_meta 的 `payload.cwd`（F1a list：Codex 无 cwd-项目目录 → 用它内存分组成「项目」）。
/// 非 session_meta / 缺 → None。
#[allow(dead_code)] // F1a-3（list 枚举）consumer 接线前 staged
pub fn session_meta_cwd(v: &Value) -> Option<&str> {
    if classify(v) != CodexRecordKind::SessionMeta {
        return None;
    }
    unwrap_envelope(v)?.1.get("cwd").and_then(Value::as_str)
}

/// session_meta 的 `payload.timestamp`（会话起始，F1a list 的 lastActivity 兜底）。非 session_meta → None。
#[allow(dead_code)] // F1a-3（list 枚举）consumer 接线前 staged
pub fn session_meta_timestamp(v: &Value) -> Option<&str> {
    if classify(v) != CodexRecordKind::SessionMeta {
        return None;
    }
    unwrap_envelope(v)?
        .1
        .get("timestamp")
        .and_then(Value::as_str)
}

// ─── F2b-2：Codex 记录 → 现有 `JsonlRecord`（第三条路组装。口径对齐 aterm CodexRecordParser.kt c03e46f）───

/// Codex rollout 记录（已解析 `v` + 原始行 `raw`）→ 现有 `JsonlRecord`（复用渲染模型）。
/// - message/reasoning/tool → User/Assistant + content（`[{type,text/…}]` Value，喂现有 `renderMessage`）。
/// - event_msg/token_count/session_meta/turn_context/world_state/未知 → `Unrecognized`（保 `raw`；
///   turn-end/用量走 per-kind 从 raw 读，见 `turn_id`/`token_usage_last`）。
///
/// **cc-monitor 适配 vs aterm**：`JsonlRecord::User/Assistant.uuid` 是必填 `String` → 无 `payload.id`
/// 时给 `""`（Codex 无 parentUuid 链、`parent_uuid=None`；F7 渲染按文件序+timestamp、不套 Claude 链）。
pub fn to_jsonl_record(v: &Value, raw: &str) -> JsonlRecord {
    use CodexRecordKind as K;
    let ts = envelope_ts(v);
    let id = payload_id(v);
    match classify(v) {
        K::Message => {
            let text = flatten_text(payload_field(v, "content").unwrap_or(&Value::Null));
            let content = text_blocks(&text);
            match message_role(v) {
                Some("assistant") => assistant_rec(id, ts, "assistant", content),
                // developer=系统指令/元 → User(isMeta=true)（保文本、渲染当 meta 隐藏，同 Claude）。
                Some("developer") => user_rec(id, ts, "user", content, true),
                // role=user：CLI 注入的上下文块（<environment_context>/<recommended_plugins>）当 meta
                // 去噪（渲染隐藏、非真用户输入；事实对照 doc §63 + aterm 对齐）。真用户输入 → isMeta=false。
                _ => user_rec(id, ts, "user", content, is_injected_context(&text)),
            }
        }
        K::Reasoning => {
            // 真机 summary 恒 [] → 空文本给空 blocks（免 Thinking("") 噪音）。
            let t = reasoning_text(v);
            let content = if t.is_empty() {
                json!([])
            } else {
                json!([{"type": "thinking", "thinking": t}])
            };
            assistant_rec(id, ts, "assistant", content)
        }
        K::ToolCall => {
            let name = payload_field(v, "name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let content = json!([{
                "type": "tool_use",
                "id": call_id(v).unwrap_or(""),
                "name": name,
                "input": tool_input(v),
            }]);
            assistant_rec(id, ts, "assistant", content)
        }
        K::ToolResult => {
            // output 真机恒数组 → flatten（守丢文本坑）。tool_result.content = 文本串。
            let out = flatten_text(payload_field(v, "output").unwrap_or(&Value::Null));
            let content = json!([{
                "type": "tool_result",
                "tool_use_id": call_id(v).unwrap_or(""),
                "content": out,
            }]);
            user_rec(id, ts, "user", content, false)
        }
        // 事件/元记录 → Unrecognized（保 raw；turn-end/用量 per-kind 从 raw 读）。
        _ => unrecognized(v, ts, raw),
    }
}

/// 信封顶层 `timestamp`（所有记录种类；F5 用量按事件 timestamp 归天、F2b 渲染排序）。缺 → None。
pub fn envelope_ts(v: &Value) -> Option<String> {
    v.get("timestamp").and_then(Value::as_str).map(String::from)
}

/// `payload.id`（记录 uuid；user/developer/tool_output 常无 → ""）。
fn payload_id(v: &Value) -> String {
    unwrap_envelope(v)
        .and_then(|(_, p)| p.get("id").and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

/// `payload.<field>`（信封解包后取字段）。
fn payload_field<'a>(v: &'a Value, field: &str) -> Option<&'a Value> {
    unwrap_envelope(v)?.1.get(field)
}

/// 文本 → content 块 Value：空→`[]`（免空气泡）、非空→`[{"type":"text","text":t}]`。
fn text_blocks(t: &str) -> Value {
    if t.is_empty() {
        json!([])
    } else {
        json!([{"type": "text", "text": t}])
    }
}

fn api_msg(role: &str, content: Value) -> ApiMessage {
    ApiMessage {
        role: role.to_string(),
        content,
        model: None,
        usage: None,
        stop_reason: None,
    }
}

fn assistant_rec(uuid: String, ts: Option<String>, role: &str, content: Value) -> JsonlRecord {
    JsonlRecord::Assistant {
        uuid,
        timestamp: ts.unwrap_or_default(),
        message: api_msg(role, content),
        session_id: None,
        is_sidechain: false,
        request_id: None,
        parent_uuid: None,
        forked_from: None,
        is_api_error_message: false,
        error: None,
        api_error_status: None,
    }
}

fn user_rec(
    uuid: String,
    ts: Option<String>,
    role: &str,
    content: Value,
    is_meta: bool,
) -> JsonlRecord {
    JsonlRecord::User {
        uuid,
        timestamp: ts.unwrap_or_default(),
        message: api_msg(role, content),
        cwd: None,
        session_id: None,
        is_sidechain: false,
        is_meta,
        parent_uuid: None,
        forked_from: None,
    }
}

/// 非消息记录 → `Unrecognized`（保 raw；`original_type`=`顶层/payload.type` 便于诊断/per-kind 读）。
fn unrecognized(v: &Value, ts: Option<String>, raw: &str) -> JsonlRecord {
    let top = v.get("type").and_then(Value::as_str).unwrap_or("");
    let original = unwrap_envelope(v)
        .and_then(|(_, p)| payload_type(p))
        .map(|pt| format!("{top}/{pt}"))
        .unwrap_or_else(|| top.to_string());
    JsonlRecord::Unrecognized {
        uuid: unwrap_envelope(v)
            .and_then(|(_, p)| p.get("id").and_then(Value::as_str))
            .map(String::from),
        parent_uuid: None,
        timestamp: ts,
        original_type: Some(original),
        raw: raw.to_string(),
        reason: "codex-event".to_string(),
    }
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
}
