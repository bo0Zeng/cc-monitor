//! Phase 2 · F2a：Codex rollout 记录的**防御式分类器** ＋ 映射进渲染模型（`JsonlRecord`）。
//!
//! 〔MOD · `设计/90 §3` 判据 3〕从 monitor 的 `codex_record.rs` 搬进后端：记录解释只住后端，界面只收成品。
//! 信封助手（解包 · 子型 · alias 归一 · 那几个字段）只用 [`super::parse`] 那一份 —— 搬进来之前两侧各写一份、
//! 靠注释「同语义」对齐，今天同一个模块里没有理由再留第二份。
//!
//! Codex 格式**未文档、每几个 minor 版 churn**（openai/codex 源码 + web 交叉证）→ **不建 rigid typed enum**（每字段漂移即崩），
//! 在 `serde_json::Value` 上**宽容抽取**：逐行不崩、每 record/field 当 optional、**alias 归一 `turn_*`↔`task_*`**、
//! 未知 type → `Other`（前向兼容不崩）。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者的 `payload.type` 再细分。

use super::parse::{normalize_event, payload_type, unwrap_envelope};
use crate::agents::claudecode::schema::{ApiMessage, JsonlRecord};
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

/// token_count 的 `payload.info.last_token_usage`（本轮增量用量；F5 抽字段）。原样返回 Value。
/// **实测 total_token_usage 严格单调、final == Σlast**——故 F5 按 (model,天) 累加各事件 last 增量，
/// 与 Claude 逐 request 归桶一致（取 final total 会丢跨天/跨模型粒度）。
#[allow(dead_code)] // staged：用量那一轴随 `设计/50` 删了，复活点就是这里（codex 专项）。
pub fn token_usage_last(v: &Value) -> Option<&Value> {
    unwrap_envelope(v)?.1.get("info")?.get("last_token_usage")
}

// 此处原有 `token_usage_fields`：从 token 用量子对象读三元组、由调用方各自做〔散文墓碑〕
// `input -= cached`。它与 backend `agents/codex/parse.rs` 的那份**逐字相同却各写一遍**
// ——U7-2 收 Claude 口径时漏了 Codex 这半。现已收进 `token::codex_delta`
// （唯一权威源），调用方直接拿映射好的增量。
// 🔴 〔`设计/50`〕**钉住它「是唯一家」的那条判据没了**：它住 monitor 的用量模块，
// 而用量 ②③ 两轴整轴退役、那份文件整删 ⇒ 今天没有任何东西在数这个映射有几个家。
// ⚠ 同一刀还让**本文件对那个 crate 零调用** —— 上面这段说的是历史，别读成今天还在共用。

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

/// 〔MOD〕一行 rollout 原文 ⇒ 交给通用层的那一形（注册表 `RecordFace.parse`；契约同 Claude 那一家：
/// 空行 `Ok(None)` · 连 JSON 都不是 `Err`）。
pub(crate) fn parsed_line(raw: &str) -> Result<Option<crate::agents::ParsedLine>, String> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    let rec = to_jsonl_record(&v, trimmed);
    Ok(Some(crate::agents::ParsedLine {
        displayable: rec.is_displayable(),
        cwd: rec.cwd().map(str::to_string),
        message: serde_json::to_value(&rec).map_err(|e| e.to_string())?,
    }))
}

/// Codex rollout 记录（已解析 `v` + 原始行 `raw`）→ 现有 `JsonlRecord`（复用渲染模型）。
/// - message/reasoning/tool → User/Assistant + content（`[{type,text/…}]` Value，喂现有 `renderMessage`）。
/// - event_msg/token_count/session_meta/turn_context/world_state/未知 → `Unrecognized`（保 `raw`；
///   turn-end/用量走 per-kind 从 raw 读，见 `turn_id`/`token_usage_last`）。
///
/// **cc-monitor 适配 vs aterm**：`JsonlRecord::User/Assistant.uuid` 是必填 `String` → 无 `payload.id`
/// 时给 `""`（Codex 无 parentUuid 链、`parent_uuid=None`；F7 渲染按文件序+timestamp、不套 Claude 链）。
pub fn to_jsonl_record(v: &Value, raw: &str) -> JsonlRecord {
    use CodexRecordKind as K;
    let ts = super::parse::envelope_ts(v).map(String::from);
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
        // 〔THIN〕Codex 的工具名今天没人考据过⇒ 不带卡型。
        tool_cards: Default::default(),
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
        user_text: Default::default(),
    }
    .with_user_text()
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
#[path = "../../../../tests/backend/agents/codex/record_tests.rs"]
mod tests;
