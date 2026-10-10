//! Codex rollout 记录的**防御式分类器** ＋ 翻成通用记录（[`Record`]，不借别家的记录类型）。
//!
//! 从 monitor 的 `codex_record.rs` 搬进后端：记录解释只住后端，界面只收成品。
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
use crate::agents::record::{Block, Body, Record};
use crate::agents::{Speaker, Translated, UserText};
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
    /// agent 之间的一封信：response_item.agent_message（0.159 写）· 顶层 inter_agent_communication。
    AgentMail,
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
        "inter_agent_communication" => K::AgentMail,
        "response_item" => match payload_type(payload) {
            Some("message") => K::Message,
            Some("reasoning") => K::Reasoning,
            Some("custom_tool_call" | "function_call" | "local_shell_call") => K::ToolCall,
            Some("custom_tool_call_output" | "function_call_output") => K::ToolResult,
            Some("agent_message") => K::AgentMail,
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

// ─── 文本抽取助手（trap-critical，口径对齐 aterm CodexRecordParser.kt c03e46f）───

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

// ─── 谁说的：字段先、具名框后、认不出归人（与 Claude 那一份同一个顺序）───

/// 记录级的宿主注解 `internal_chat_message_metadata_passthrough.content_item_kinds`：与 `content` 逐项对齐的种类名。
fn content_kinds(payload: &Value) -> Option<Vec<&str>> {
    let kinds = payload
        .get("internal_chat_message_metadata_passthrough")?
        .get("content_item_kinds")?
        .as_array()?;
    Some(kinds.iter().map(|k| k.as_str().unwrap_or("")).collect())
}

/// 一个种类名说的是哪一类来源。种类表照 Codex 源码（`core/src/context/*.rs` 各片段的 `content_kind`，0.159.2）抄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KindSays {
    /// 人说的：`user.*`，以及 Codex 判「这是不是用户授权」时也保守算作用户的那几种。
    Human,
    Interrupt,
    Shell,
    SubagentNotice,
    AgentMail,
    Compact,
    System,
    /// 表里没有：归人（与 Claude 同规矩）。
    Unknown,
}

/// 不以 `.instructions` 结尾、也不在 `environments.*` · `plugins.*` 下的系统注入种类（源码逐个列）。
const SYSTEM_KINDS: &[&str] = &[
    "apply_patch.legacy_exec_command_warning",
    "compaction.auto_fallback_prompt",
    "current_time.reminder",
    "current_time.unavailable",
    "extension.internal_context",
    "generic.developer_instructions",
    "generic.developer_policy",
    "hooks.additional_context",
    "images.resize_notice",
    "managed_config.developer_instructions",
    "model.base_instructions",
    "model_switch.legacy_mismatch_warning",
    "multi_agent.usage_hint",
    "network_proxy.rule_saved",
    "permissions.approved_command_prefix_saved",
    "realtime_conversation.delegation",
    "rollout_budget.remaining_tokens",
    "skills.catalog",
    "skills.selected_skill_instructions",
    "token_budget.context_window",
    "token_budget.context_window_guidance",
    "token_budget.remaining_tokens",
    "token_budget.reminder",
    "tools.deferred_namespaces",
    "unified_exec.legacy_process_limit_warning",
    "user_verification.notice",
];

fn kind_says(kind: &str) -> KindSays {
    match kind {
        k if k.starts_with("user.") => KindSays::Human,
        ""
        | "unknown"
        | "images.preparation_error"
        | "images.unsupported"
        | "audio.unsupported" => KindSays::Human,
        "generic.turn_aborted" => KindSays::Interrupt,
        "shell.user_command" => KindSays::Shell,
        "multi_agent.subagent_notification" => KindSays::SubagentNotice,
        "multi_agent.inter_agent_message"
        | "multi_agent.inter_agent_completion_message"
        | "agent_message_board.notification" => KindSays::AgentMail,
        "compaction.summary" => KindSays::Compact,
        k if k.ends_with(".instructions")
            || k.starts_with("environments.")
            || k.starts_with("plugins.")
            || k.starts_with("guardian.")
            || k.starts_with("memories.")
            || SYSTEM_KINDS.contains(&k) =>
        {
            KindSays::System
        }
        _ => KindSays::Unknown,
    }
}

/// 没有宿主注解时（旧版记录）认的具名框：照 Codex 认「上下文片段」的那张匹配表
/// （`core/src/context/contextual_user_message.rs`）与各片段的起头标记抄；不分大小写，只认起头。
const SYSTEM_FRAMES: &[&str] = &[
    "<user_instructions>",
    "# AGENTS.md instructions",
    "<environment_context>",
    "<external_",
    "<agent_message_board_notification>",
    "<skill>",
    "<codex_internal_context",
    "<goal_context>",
    "<recommended_plugins>",
    "Warning: The maximum number of unified exec processes you can keep open is",
    "Warning: apply_patch was requested via ",
    "Warning: Your account was flagged for potentially high-risk cyber activity",
];
const SHELL_FRAME: &str = "<user_shell_command>";
const ABORT_FRAME: &str = "<turn_aborted>";
const NOTICE_FRAME: &str = "<subagent_notification>";

fn opens_with(t: &str, mark: &str) -> bool {
    t.get(..mark.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(mark))
}

fn framed_says(text: &str) -> KindSays {
    let t = text.trim_start();
    if opens_with(t, SHELL_FRAME) {
        KindSays::Shell
    } else if opens_with(t, ABORT_FRAME) {
        KindSays::Interrupt
    } else if opens_with(t, NOTICE_FRAME) {
        KindSays::SubagentNotice
    } else if SYSTEM_FRAMES.iter().any(|m| opens_with(t, m)) {
        KindSays::System
    } else {
        KindSays::Human
    }
}

/// `<名>…</名>` 里的那一段（不在 ⇒ `None`）。
fn between<'a>(t: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = &t[t.find(open)? + open.len()..];
    Some(rest[..rest.find(close)?].trim())
}

/// `!` 命令：`<user_shell_command><command>…</command><result>…</result></user_shell_command>` ⇒ 那一行命令。
fn shell(text: &str) -> Speaker {
    Speaker::BashInput {
        command: between(text, "<command>", "</command>")
            .unwrap_or_default()
            .to_string(),
    }
}

/// 子 agent 收场通知：框里是 `{"agent_path", "status"}`；`status` 是串或单键对象（键 ＝ 状态名）。
fn subagent_notice(text: &str) -> Speaker {
    let body = between(text, NOTICE_FRAME, "</subagent_notification>")
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .unwrap_or(Value::Null);
    let status = match body.get("status") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(o)) => o.keys().next().cloned(),
        _ => None,
    };
    Speaker::TaskNotification {
        task_id: body
            .get("agent_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        status,
        summary: None,
        tool_use_id: None,
    }
}

/// agent 之间的一封信（`Message Type: …` 头 ＋ `Payload:` 之后是正文）的头里那几格。
struct Mail<'a> {
    kind: Option<&'a str>,
    sender: Option<&'a str>,
    payload: &'a str,
}

fn mail_of(text: &str) -> Mail<'_> {
    let head = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let payload = text
        .find("Payload:")
        .map_or("", |i| text[i + "Payload:".len()..].trim());
    Mail {
        kind: head("Message Type:"),
        sender: head("Sender:"),
        payload,
    }
}

/// 一封信是谁说的：`author` 是收信方的上级（收信方路径在它底下）⇒ 主会话那一侧发来的（派活 ⇒ `agentTask`，后来的话 ⇒ `coordinator`）；
/// 否则 ⇒ 子 agent（或同级）发来的，`FINAL_ANSWER` ＝ 交回。
fn mail_said(text: &str, author: Option<&str>, recipient: Option<&str>) -> UserText {
    let m = mail_of(text);
    let from = author.or(m.sender);
    let from_above = match (from, recipient) {
        (Some(a), Some(r)) => {
            r.len() > a.len() && r.starts_with(a) && r[a.len()..].starts_with('/')
        }
        _ => false,
    };
    match (from_above, m.kind) {
        (true, Some("NEW_TASK")) => UserText {
            speaker: Speaker::AgentTask,
            text: m.payload.to_string(),
            pasted: Vec::new(),
        },
        (true, _) => UserText::of(Speaker::Coordinator {
            body: Some(m.payload.trim().to_string()).filter(|b| !b.is_empty()),
        }),
        (false, kind) => UserText::of(Speaker::AgentMessage {
            from: from.map(str::to_string),
            name: None,
            handback: kind == Some("FINAL_ANSWER"),
            body: Some(m.payload.trim().to_string()).filter(|b| !b.is_empty()),
        }),
    }
}

fn said_as(says: KindSays, text: &str, human_text: String) -> UserText {
    match says {
        KindSays::Human | KindSays::Unknown => UserText {
            speaker: Speaker::Human,
            text: human_text,
            pasted: Vec::new(),
        },
        KindSays::Interrupt => UserText::of(Speaker::Interrupt),
        KindSays::Shell => UserText::of(shell(text)),
        KindSays::SubagentNotice => UserText::of(subagent_notice(text)),
        KindSays::AgentMail => mail_said(text, None, None),
        KindSays::Compact => UserText {
            speaker: Speaker::CompactSummary,
            text: text.trim().to_string(),
            pasted: Vec::new(),
        },
        KindSays::System => UserText::of(Speaker::System { body: None }),
    }
}

/// 有宿主注解时：先看有没有非人的那几类（中断 · `!` 命令 · 通知 · 来信 · 压缩摘要），再看有没有人说的那几项
/// （正文只取那几项），全是系统注入 ⇒ 系统；剩下的（种类表里没有）⇒ 人。注解与 `content` 对不齐 ⇒ `None`（退到具名框）。
fn by_kinds(kinds: &[&str], items: &[Value]) -> Option<UserText> {
    if kinds.is_empty() || kinds.len() != items.len() {
        return None;
    }
    let says: Vec<KindSays> = kinds.iter().map(|k| kind_says(k)).collect();
    let text_of = |i: usize| flatten_text(&Value::Array(vec![items[i].clone()]));
    let special =
        |k: KindSays| !matches!(k, KindSays::Human | KindSays::System | KindSays::Unknown);
    if let Some(i) = says.iter().position(|k| special(*k)) {
        return Some(said_as(says[i], &text_of(i), String::new()));
    }
    let human: Vec<String> = (0..items.len())
        .filter(|&i| says[i] == KindSays::Human)
        .map(text_of)
        .collect();
    if !human.is_empty() {
        return Some(said_as(
            KindSays::Human,
            "",
            human.join("\n").trim().to_string(),
        ));
    }
    if says.iter().all(|k| *k == KindSays::System) {
        return Some(UserText::of(Speaker::System { body: None }));
    }
    let all = flatten_text(&Value::Array(items.to_vec()));
    Some(said_as(KindSays::Unknown, "", all.trim().to_string()))
}

/// 一条 `response_item.message`（`role` ∈ user / developer / assistant）是谁说的；是 agent 自己的回复 ⇒ `None`。
pub(crate) fn message_said(payload: &Value) -> Option<UserText> {
    let role = payload.get("role").and_then(Value::as_str);
    let content = payload.get("content").unwrap_or(&Value::Null);
    let items = content.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let kinds = content_kinds(payload);
    let text = flatten_text(content);
    match role {
        Some("developer") => {
            // 中断那一句这一版写在 developer 里（种类 `generic.turn_aborted`）；其余 developer 一律系统。
            let interrupt = kinds
                .as_ref()
                .is_some_and(|k| k.iter().any(|k| kind_says(k) == KindSays::Interrupt));
            Some(UserText::of(if interrupt {
                Speaker::Interrupt
            } else {
                Speaker::System { body: None }
            }))
        }
        Some("assistant") => {
            // 别的 agent 的来信这一版也可能以 assistant 角色写进来（种类说得出）；其余是这一家自己的回复。
            let mail = kinds
                .as_ref()
                .is_some_and(|k| k.iter().any(|k| kind_says(k) == KindSays::AgentMail));
            mail.then(|| mail_said(&text, None, None))
        }
        _ => Some(
            kinds
                .as_deref()
                .and_then(|k| by_kinds(k, items))
                .unwrap_or_else(|| said_as(framed_says(&text), &text, text.trim().to_string())),
        ),
    }
}

/// agent 之间的一封信（`response_item.agent_message` · 顶层 `inter_agent_communication`）是谁说的。
fn agent_mail_said(payload: &Value) -> UserText {
    let text = flatten_text(payload.get("content").unwrap_or(&Value::Null));
    let s = |k: &str| payload.get(k).and_then(Value::as_str);
    mail_said(&text, s("author"), s("recipient"))
}

// ─── Codex 记录 ⇒ 通用记录（[`Record`]）───

/// 这一家在线上的 `agent` 值。
const AGENT: &str = super::AGENT_KIND;

/// 一行 rollout 原文（与它的起点字节偏移）⇒ 交给通用层的那一形（注册表 `RecordFace.parse`；契约同 Claude 那一家：
/// 空行 `Ok(None)` · 连 JSON 都不是 `Err`）。这一家没有排队那一对、记录上也不带 `cwd`。
pub(crate) fn translated(raw: &str, start: u64) -> Result<Option<Translated>, String> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    Ok(Some(Translated {
        record: record_of(&v, start),
        cwd: None,
        queue: None,
    }))
}

/// Codex rollout 记录 ⇒ 通用记录：
///
/// | rollout | 通用记录 |
/// |---|---|
/// | `response_item.message`（user / developer 角色，或别家来信）· `agent_message` · `inter_agent_communication` | `said`（谁说的本家判，[`message_said`]） |
/// | `response_item.message`（assistant 角色）· `reasoning` · 工具调用 | `reply`（`autoReply` 恒假：没考据到等价物；`endsTurn` 恒假：一轮的结束是另一条事件） |
/// | 工具调用的输出 | `said`（工具结果） |
/// | 事件 · 元记录 · 认不出的 | 无记录 |
///
/// `id`：`payload.id`；没有（user / developer / 工具输出常没有）⇒ 按这一行的起点字节偏移合成（[`crate::agents::line_id`]）。
pub fn record_of(v: &Value, start: u64) -> Option<Record> {
    use CodexRecordKind as K;
    let payload = unwrap_envelope(v).map_or(&Value::Null, |(_, p)| p);
    let body = match classify(v) {
        K::Message => {
            let text = flatten_text(payload.get("content").unwrap_or(&Value::Null));
            match message_said(payload) {
                Some(who) => said(who, text_blocks(text)),
                None => reply(text_blocks(text)),
            }
        }
        K::AgentMail => {
            let text = flatten_text(payload.get("content").unwrap_or(&Value::Null));
            said(agent_mail_said(payload), text_blocks(text))
        }
        // 真机 summary 恒 [] ⇒ 空文本给空 blocks（免一个空的推理块）。
        K::Reasoning => reply(match reasoning_text(v) {
            t if t.is_empty() => Vec::new(),
            text => vec![Block::Thinking { text }],
        }),
        K::ToolCall => reply(vec![Block::ToolUse {
            id: call_id(v).unwrap_or("").to_string(),
            name: payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            input: tool_input(v),
        }]),
        // output 真机恒数组 ⇒ 拍平（守丢文本坑）。
        K::ToolResult => said(
            UserText::of(Speaker::ToolResult),
            vec![Block::ToolResult {
                of: call_id(v).unwrap_or("").to_string(),
                content: text_blocks(flatten_text(payload.get("output").unwrap_or(&Value::Null))),
                is_error: false,
            }],
        ),
        _ => return None,
    };
    let at = super::parse::envelope_ts(v).map(String::from);
    Some(Record {
        agent: AGENT.to_string(),
        id: payload
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map_or_else(|| crate::agents::line_id(start), str::to_string),
        time_text: at
            .as_deref()
            .and_then(crate::common::time::iso_hm_here)
            .map(crate::common::cells::Words),
        at,
        body,
    })
}

/// 文本 ⇒ 内容块：空 ⇒ 没有块（免空气泡）。
fn text_blocks(text: String) -> Vec<Block> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Block::Text { text }]
    }
}

fn said(who: UserText, blocks: Vec<Block>) -> Body {
    Body::Said {
        who,
        blocks,
        results: Default::default(),
        cwd: None,
    }
}

/// Codex 的工具名今天没人考据过 ⇒ 不带卡型；它不声明子运行 ⇒ 不带派出标签。
fn reply(blocks: Vec<Block>) -> Body {
    Body::Reply {
        blocks,
        model: None,
        auto_reply: false,
        ends_turn: false,
        cards: Default::default(),
        steps: Default::default(),
        runs: Default::default(),
        error: None,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/record_tests.rs"]
mod tests;
