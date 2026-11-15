//! Codex rollout 记录的**防御式分类器** ＋ 翻成通用记录（[`Record`]，不借别家的记录类型）。
//!
//! 从 monitor 的 `codex_record.rs` 搬进后端：记录解释只住后端，界面只收成品。
//! 信封助手（解包 · 子型 · alias 归一 · 那几个字段）只用 [`super::parse`] 那一份 —— 搬进来之前两侧各写一份、
//! 靠注释「同语义」对齐，今天同一个模块里没有理由再留第二份。
//!
//! Codex 格式**未文档、每几个 minor 版 churn**（openai/codex 源码 + web 交叉证）→ **不建 rigid typed enum**（每字段漂移即崩），
//! 在 `serde_json::Value` 上**宽容抽取**：逐行不崩、每 record/field 当 optional、**alias 归一 `turn_*`↔`task_*`**、
//! 未知 type → `Other`（前向兼容不崩），登漂移账（[`drift_snapshot`]）。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type`
//! 照 openai/codex 的 `RolloutItem`（`codex-history` 那个包，核到 rust-v0.167.0）；`response_item` · `event_msg` 的 `payload.type` 再细分。
//!
//! 两种记录形（`session_meta.history_mode`）：Legacy 形另写 `event_msg.user_message` · `agent_message`，Paginated 形（新会话的默认）
//! 另写 `event_msg.item_completed`；两形都写 `response_item`（模型看到的那一份，resume 也照它重建），会话正文只照它出，
//! 那两种另写的投影不另出记录（一句话只出一次）。

use super::parse::{normalize_event, payload_type, unwrap_envelope};
use crate::agents::drift::{DriftFaceWord, DriftLedger};
use crate::agents::record::{Block, Body, Heard, Record};
use crate::agents::{Speaker, StepResult, Translated, UserText};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Codex 记录的**语义种类**（防御分类；未知/未来 → `Other*`，不崩）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexRecordKind {
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
    /// event_msg task_complete/turn_complete（一轮完了）。
    TurnComplete,
    /// event_msg turn_aborted（中止轮）。
    TurnAborted,
    /// event_msg user_message。
    UserMessage,
    /// event_msg agent_message。
    AgentMessage,
    /// event_msg token_count —— **用量**（F5；info.total/last_token_usage）。
    TokenCount,
    /// 顶层 token_usage_record —— 一次请求的用量（0.159 写：`usage` · `response_id`）。簿记，不出记录。
    TokenUsage,
    /// 顶层 inter_agent_communication_metadata —— 来信的元数据（`trigger_turn`）。簿记，不出记录。
    AgentMailMeta,
    /// Codex 自己的执行标记（顶层 compacted · dynamic_tools_updated · retained_context · security_risk_score · realtime_item）：不出记录。
    Marker,
    /// event_msg item_completed —— 一件事做完的投影（Paginated 形每件都写，Legacy 形只写几种）。
    /// 说的话 · 工具调用与输出同一件事 `response_item` 里另有一份（两形都写，记录照那一份出），这一份不另出记录；
    /// 子线程的派出 · 收场只在这里说（[`child_link`]）。
    ItemCompleted,
    /// event_msg 其它认得的子型（Codex 落盘的那几种：mcp_tool_call_end / thread_rolled_back / thread_settings_applied / …）。
    OtherEvent,
    /// event_msg 认不得的子型（登漂移账）。
    UnknownEvent,
    /// 未知顶层 type / 信封缺失（前向兼容、坏行）。
    Other,
}

/// 防御分类：unwrap 信封 → 顶层 type（+ 必要时 payload.type，alias 归一）→ [`CodexRecordKind`]。
/// 任何缺失/未知 → `Other`/`UnknownEvent`（不崩、前向兼容）。
pub(crate) fn classify(v: &Value) -> CodexRecordKind {
    use CodexRecordKind as K;
    let Some((top, payload)) = unwrap_envelope(v) else {
        return K::Other;
    };
    match top {
        "session_meta" => K::SessionMeta,
        "turn_context" => K::TurnContext,
        "world_state" => K::WorldState,
        "inter_agent_communication" => K::AgentMail,
        "inter_agent_communication_metadata" => K::AgentMailMeta,
        "token_usage_record" => K::TokenUsage,
        "compacted"
        | "dynamic_tools_updated"
        | "retained_context"
        | "security_risk_score"
        | "realtime_item" => K::Marker,
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
            Some("item_completed") => K::ItemCompleted,
            Some(t) if KNOWN_EVENTS.contains(&t) => K::OtherEvent,
            _ => K::UnknownEvent,
        },
        _ => K::Other, // 未知顶层 type（未来新增记录种类）
    }
}

/// Codex 落盘、这里不翻的那几种 `event_msg` 子型（照 openai/codex 落盘规则（`codex_rollout::policy` 里给 `event_msg` 的那张表）抄，核到 rust-v0.167.0；
/// 上面单列的那几种不在这里）。不在这张表里的 ⇒ 认不得，登漂移账。
const KNOWN_EVENTS: &[&str] = &[
    "thread_goal_updated",
    "thread_rolled_back",
    "thread_settings_applied",
    "agent_reasoning",
    "agent_reasoning_raw_content",
    "entered_review_mode",
    "exited_review_mode",
    "patch_apply_end",
    "context_compacted",
    "mcp_tool_call_end",
    "web_search_end",
    "image_generation_end",
    "sub_agent_activity",
];

/// 一轮的 id：那一轮 `task_started` 的 `turn_id`（分叉认轮用）。别的行 ⇒ `None`。
pub fn turn_id(v: &Value) -> Option<&str> {
    if classify(v) != CodexRecordKind::TurnStarted {
        return None;
    }
    unwrap_envelope(v)?.1.get("turn_id").and_then(Value::as_str)
}

/// rollout 一行 ⇒ Codex 自报的两窗额度（`event_msg` `token_count` 的 `rate_limits`；形状与窗口名见 [`super::parse::snapshot_windows`]）。
/// 别的行 · 没带两窗 · 别的一族 ⇒ `None`。记进额度账走 `ledger::record_probe` 那一口（来源 `usage`）。
pub fn quota_said(v: &Value) -> Option<Vec<crate::agents::QuotaWindow>> {
    if classify(v) != CodexRecordKind::TokenCount {
        return None;
    }
    let (_, payload) = unwrap_envelope(v)?;
    let at = super::parse::envelope_ts(v)
        .and_then(crate::common::time::parse_iso8601_ms)
        .and_then(|ms| u64::try_from(ms / 1000).ok());
    super::parse::snapshot_windows(payload.get("rate_limits")?, at)
}

/// **此刻在干什么**（注册表 `RecordFace.activity`；这一家没有进程状态文件，只有记录里的事件说得出）：
/// 一轮开始（`task_started`）⇒ 在跑；一轮完了（`task_complete`）· 中止（`turn_aborted`）⇒ 闲着，等下一句；别的行不说。
/// 先过子串闸：连 `"event_msg"` 都没有的行（绝大多数）零解析直接回。
pub(crate) fn activity_of(
    line: &crate::agents::Line<'_>,
) -> Option<crate::agents::SessionActivity> {
    use crate::agents::SessionActivity as A;
    if !line.text().contains("\"event_msg\"") {
        return None;
    }
    match classify(line.value()?) {
        CodexRecordKind::TurnStarted => Some(A::Working),
        CodexRecordKind::TurnComplete | CodexRecordKind::TurnAborted => Some(A::Idle),
        _ => None,
    }
}

// ─── 文本抽取助手（trap-critical，口径对齐手机端 CodexRecordParser.kt c03e46f）───

/// **数组文本拍平**——Codex 的 `message.content` 与 `custom_tool_call_output.output` **真机恒数组**
/// `[{type:input_text|output_text, text}]`（手机端 Phase D 审计 9/9 坐实）。当 String 处理会静默丢**全部**
/// 文本（且 String fixture 绿着骗过）。数组→拼各项 `text`（`input_image` 等无 text 项自然跳过）；
/// 防御：裸 String→原样；其它→""。**fixture 必用真机数组 shape。**
/// 图片两边的框（[`is_image_frame`]）不是话，不进正文（图本身在图片块里，[`image_blocks`]）。
pub(crate) fn flatten_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|it| match it {
                Value::String(s) => Some(s.clone()),
                _ => it
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|t| !is_image_frame(t))
                    .map(str::to_string),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Codex 把一张图夹在两段框里交给模型：本地图 `<image name=[Image #N] path="…">` · 内联图 `<image>`，收尾都是 `</image>`
/// （照 openai/codex `codex_protocol::models::is_local_image_open_tag_text` 等几问抄，整项相等才算）。
fn is_image_frame(t: &str) -> bool {
    t == "<image>" || t == "</image>" || (t.starts_with("<image name=") && t.ends_with('>'))
}

/// 人说的那一条里的图（`content` 里的 `input_image`，内联的 `data:image/…;base64,…` 那一形；本地图 Codex 落盘前已内联成这一形）⇒
/// 图片块（`at` ＝ 这一行的起点字节偏移，`n` 按先后数；取图那一条照它读回字节）。按文件号引用的（`file_id`）画不出 ⇒ 不出块。
fn image_blocks(content: &Value, at: u64) -> Vec<Block> {
    inline_images(content)
        .enumerate()
        .map(|(n, (media_type, data))| Block::Image {
            media_type: media_type.to_string(),
            bytes: (data.trim_end_matches('=').len() as u64) * 3 / 4,
            at,
            n: u32::try_from(n).unwrap_or(u32::MAX),
        })
        .collect()
}

/// `content` 里画得出的那几张图（媒体类型 · base64 字节），按先后。记录的图片块与一拍里的 `Image` 都只照它数。
pub(crate) fn inline_images(content: &Value) -> impl Iterator<Item = (&str, &str)> {
    let items = content.as_array().map(Vec::as_slice).unwrap_or(&[]);
    items.iter().filter_map(|it| {
        if it.get("type").and_then(Value::as_str) != Some("input_image") {
            return None;
        }
        let url = it.get("image_url").and_then(Value::as_str)?;
        let (media_type, data) = url.strip_prefix("data:")?.split_once(";base64,")?;
        media_type
            .starts_with("image/")
            .then_some((media_type, data))
    })
}

/// reasoning 的文本：`payload.summary`（array `[{text}]` 或裸串）→ 拍平。**真机 summary 恒 []**
/// （仅 encrypted_content）→ ""；调用方据此空文本时给**空 blocks**、不产 `Thinking("")` 噪音。
pub(crate) fn reasoning_text(v: &Value) -> String {
    unwrap_envelope(v)
        .and_then(|(_, p)| p.get("summary").map(flatten_text))
        .unwrap_or_default()
}

/// tool_call 的入参：`payload.input`（`custom_tool_call` 写这一格，自由文本）：Object→原样、String→包 `{"input": s}`；
/// 没有就 `payload.arguments`（`function_call` 写这一格，是一个 JSON 串）：解得出对象→那个对象、解不出→同样包 `{"input": s}`（不吞）；
/// 其它/缺→`Null`（保 name 可见）。
pub(crate) fn tool_input(v: &Value) -> Value {
    let p = unwrap_envelope(v).map_or(&Value::Null, |(_, p)| p);
    let wrap = |s: &str| serde_json::json!({ "input": s });
    match (p.get("input"), p.get("arguments")) {
        (Some(o), _) if o.is_object() => o.clone(),
        (Some(Value::String(s)), _) => wrap(s),
        (None, Some(Value::String(s))) => match serde_json::from_str::<Value>(s) {
            Ok(o) if o.is_object() => o,
            _ => wrap(s),
        },
        (None, Some(o)) if o.is_object() => o.clone(),
        _ => Value::Null,
    }
}

/// tool 的 `payload.call_id`（ToolUse.id / ToolResult.tool_use_id 配对键）。
pub(crate) fn call_id(v: &Value) -> Option<&str> {
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
        end: status.as_deref().and_then(subagent_end),
        status,
        summary: None,
        tool_use_id: None,
        row: None,
    }
}

/// 这一家子 agent 的状态词 ⇒ 怎么收场的（只翻这一处）；还在跑 · 认不出 ⇒ `None`。
fn subagent_end(status: &str) -> Option<cmd_wire::record::TaskEnd> {
    use cmd_wire::record::TaskEnd;
    match status {
        "completed" => Some(TaskEnd::Done),
        "errored" => Some(TaskEnd::Failed),
        "interrupted" | "shutdown" => Some(TaskEnd::Stopped),
        _ => None,
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
fn mail_said(text: &str, author: Option<&str>, recipient: Option<&str>) -> Heard {
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
            parts: face_core::record::brief_parts(m.payload),
            text: m.payload.to_string(),
        }
        .into(),
        (true, _) => Heard::of(
            Speaker::Coordinator { doc: None },
            Some(m.payload.trim().to_string()).filter(|b| !b.is_empty()),
        ),
        (false, kind) => Heard::of(
            Speaker::AgentMessage {
                from: from.map(str::to_string),
                name: None,
                handback: kind == Some("FINAL_ANSWER"),
                doc: None,
            },
            Some(m.payload.trim().to_string()).filter(|b| !b.is_empty()),
        ),
    }
}

fn said_as(says: KindSays, text: &str, human_text: String) -> Heard {
    match says {
        KindSays::Human | KindSays::Unknown => UserText {
            speaker: Speaker::Human,
            text: human_text,
            parts: Vec::new(),
        }
        .into(),
        KindSays::Interrupt => UserText::of(Speaker::Interrupt).into(),
        KindSays::Shell => UserText::of(shell(text)).into(),
        KindSays::SubagentNotice => UserText::of(subagent_notice(text)).into(),
        KindSays::AgentMail => mail_said(text, None, None),
        KindSays::Compact => UserText {
            speaker: Speaker::CompactSummary { title_text: None },
            text: text.trim().to_string(),
            parts: Vec::new(),
        }
        .into(),
        KindSays::System => UserText::of(Speaker::System { body: None }).into(),
    }
}

/// 有宿主注解时：先看有没有非人的那几类（中断 · `!` 命令 · 通知 · 来信 · 压缩摘要），再看有没有人说的那几项
/// （正文只取那几项），全是系统注入 ⇒ 系统；剩下的（种类表里没有）⇒ 人。注解与 `content` 对不齐 ⇒ `None`（退到具名框）。
fn by_kinds(kinds: &[&str], items: &[Value]) -> Option<Heard> {
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
        return Some(UserText::of(Speaker::System { body: None }).into());
    }
    let all = flatten_text(&Value::Array(items.to_vec()));
    Some(said_as(KindSays::Unknown, "", all.trim().to_string()))
}

/// 一条 `response_item.message`（`role` ∈ user / developer / assistant）是谁说的（来信的正文原文跟着交出，[`Heard`]）；
/// 是 agent 自己的回复 ⇒ `None`。
pub(crate) fn message_said(payload: &Value) -> Option<Heard> {
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
            Some(
                UserText::of(if interrupt {
                    Speaker::Interrupt
                } else {
                    Speaker::System { body: None }
                })
                .into(),
            )
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
pub(crate) fn agent_mail_said(payload: &Value) -> Heard {
    let text = flatten_text(payload.get("content").unwrap_or(&Value::Null));
    let s = |k: &str| payload.get(k).and_then(Value::as_str);
    mail_said(&text, s("author"), s("recipient"))
}

// ─── Codex 记录 ⇒ 通用记录（[`Record`]）───

/// 这一家在线上的 `agent` 值。
const AGENT: &str = super::AGENT_KIND;

/// 一行 rollout 原文（与它的起点字节偏移）⇒ 交给通用层的那一形（注册表 `RecordFace.parse`；契约同 Claude 那一家：
/// 空行 `Ok(None)` · 连 JSON 都不是 `Err`）。这一家的记录上不带 `cwd`。
pub(crate) fn translated(line: &crate::agents::Line<'_>) -> Result<Option<Translated>, String> {
    if line.is_blank() {
        return Ok(None);
    }
    let v = line.value().ok_or_else(|| "not JSON".to_string())?;
    let record = record_of(v, line.start());
    Ok(Some(Translated {
        unsent: unsent_of(v, record.as_ref()),
        // 这一家没有重试那一类 ⇒ 后端的行不带私有格。
        record: record.map(crate::agents::record::BackRecord::from),
        cwd: None,
        // `token_count` 那一行自报的两窗（读记录那一路据它进账）。
        quota_said: quota_said(v),
    }))
}

/// 这一行只给后端看的那几格：记录里出了图片块（[`image_blocks`]）⇒ 那几张图的字节，与块同一个数法（[`inline_images`]，按先后）。
/// 这一家的结果没有读文件那一类（`reads` 空）。
fn unsent_of(v: &Value, record: Option<&Record>) -> crate::agents::Unsent {
    let imaged = matches!(
        record.map(|r| &r.body),
        Some(Body::Said { blocks, .. }) if blocks.iter().any(|b| matches!(b, Block::Image { .. }))
    );
    let content = unwrap_envelope(v)
        .and_then(|(_, p)| p.get("content"))
        .unwrap_or(&Value::Null);
    crate::agents::Unsent {
        images: if imaged {
            inline_images(content).map(|(_, d)| d.to_string()).collect()
        } else {
            Vec::new()
        },
        reads: Default::default(),
    }
}

/// Codex rollout 记录 ⇒ 通用记录：
///
/// | rollout | 通用记录 |
/// |---|---|
/// | `response_item.message`（user / developer 角色，或别家来信）· `agent_message` · `inter_agent_communication` | `said`（谁说的本家判，[`message_said`]；`input_image` ⇒ 图片块，[`image_blocks`]） |
/// | `response_item.message`（assistant 角色）· `reasoning` · 工具调用 | `reply`（`autoReply` 恒假：没考据到等价物） |
/// | 工具调用的输出 | `said`（工具结果） |
/// | 认不出的顶层类型 · 认不出的 `response_item` 子型 | `unread`（写好的一句 ＋ 原文摘录），登漂移账 |
/// | 事件 · 元记录（`item_completed` 是同一件事在 `response_item` 之外的另一份投影：两形都另写 `response_item`）| 无记录；认不出的 `event_msg` 子型登漂移账 |
///
/// `id`：`payload.id`；没有（user / developer / 工具输出常没有）⇒ 按这一行的起点字节偏移合成（[`crate::agents::line_id`]）。
pub fn record_of(v: &Value, start: u64) -> Option<Record> {
    use CodexRecordKind as K;
    let payload = unwrap_envelope(v).map_or(&Value::Null, |(_, p)| p);
    let body = match classify(v) {
        K::Message => {
            let content = payload.get("content").unwrap_or(&Value::Null);
            let text = flatten_text(content);
            match message_said(payload) {
                Some(who) => {
                    let mut blocks = text_blocks(text);
                    if who.who.speaker == Speaker::Human {
                        blocks.extend(image_blocks(content, start));
                    }
                    said(who.said(), blocks)
                }
                None => reply(crate::agents::record::reply_blocks(text_blocks(text))),
            }
        }
        K::AgentMail => {
            let text = flatten_text(payload.get("content").unwrap_or(&Value::Null));
            said(agent_mail_said(payload).said(), text_blocks(text))
        }
        // 真机 summary 恒 [] ⇒ 空文本给空 blocks（免一个空的推理块）。
        K::Reasoning => reply(match reasoning_text(v) {
            t if t.is_empty() => Vec::new(),
            text => vec![crate::agents::beat::thinking(text)],
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
        // output 真机恒数组 ⇒ 拍平（守丢文本坑）。这一家的输出不标失败 ⇒ 结果块不标错、结果照成功交；
        // 那一句与用时由通用层一处写（`StepClock`）。
        K::ToolResult => {
            let of = call_id(v).unwrap_or("").to_string();
            let result = StepResult {
                ok: true,
                ..StepResult::default()
            };
            Body::Said {
                who: UserText::of(Speaker::ToolResult),
                blocks: vec![Block::tool_result(
                    of.clone(),
                    text_blocks(flatten_text(payload.get("output").unwrap_or(&Value::Null))),
                    false,
                    copy_core::size_text,
                )],
                results: std::iter::once((of, result)).collect(),
                cwd: None,
                attachments: Vec::new(),
            }
        }
        // 认不出的顶层类型 · 认不出的 response_item 子型：每行一条 `unread`（相邻同类由出记录页那一遍并）。
        K::Other => {
            let top = v.get("type").and_then(Value::as_str);
            let (face, kind) = match top {
                Some("response_item") => (
                    DriftFace::UnknownItemType,
                    payload.get("type").and_then(Value::as_str),
                ),
                t => (DriftFace::UnknownRecordType, t),
            };
            drift(face, kind.unwrap_or_default());
            crate::agents::record::unread_body(
                crate::agents::record::UnreadWhy::UnknownType,
                kind.map(str::to_string),
                &v.to_string(),
            )
        }
        K::UnknownEvent => {
            drift(
                DriftFace::UnknownEventType,
                payload_type(payload).unwrap_or_default(),
            );
            return None;
        }
        _ => return None,
    };
    let at = super::parse::envelope_ts(v).map(String::from);
    Some(crate::agents::record::record(
        AGENT.into(),
        record_id(v, start),
        at,
        body,
    ))
}

/// 一行记录的 id（起点字节偏移是 `start`）：`payload.id`；没有 ⇒ 按起点合成（[`crate::agents::line_id`]）。
/// 记录页与分叉认「叉在哪一条」读的是同一个。
pub(crate) fn record_id(v: &Value, start: u64) -> String {
    unwrap_envelope(v)
        .and_then(|(_, p)| p.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map_or_else(|| crate::agents::line_id(start), str::to_string)
}

/// 文本 ⇒ 内容块：空 ⇒ 没有块（免空气泡）。
fn text_blocks(text: String) -> Vec<Block> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Block::text(text)]
    }
}

fn said(who: UserText, blocks: Vec<Block>) -> Body {
    Body::Said {
        who,
        blocks,
        results: Default::default(),
        cwd: None,
        attachments: Vec::new(),
    }
}

/// Codex 的工具名今天没人考据过 ⇒ 不带卡型；它不声明子运行 ⇒ 不带派出标签。
fn reply(blocks: Vec<Block>) -> Body {
    Body::Reply {
        blocks,
        auto_reply: false,
        cards: Default::default(),
        steps: Default::default(),
        runs: Default::default(),
        error: None,
    }
}

// ─── 子 agent 线程：一条会话是谁派出的 · 叉自哪一条（`session_meta`）───

/// `session_meta` 说的这条线程的来处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thread<'a> {
    /// 派出它的那条线程（`source.subagent.thread_spawn.parent_thread_id`）；不是派出来的 ⇒ `None`。
    pub parent: Option<&'a str>,
    /// 派出时取的昵称 · 角色（`agent_nickname` · `agent_role`，旧名 `agent_type`）。
    pub nickname: Option<&'a str>,
    pub role: Option<&'a str>,
    /// 叉自哪一条（`forked_from_id`）。
    pub forked_from: Option<&'a str>,
}

/// 一行 `session_meta` ⇒ 这条线程的来处；别的行 ⇒ `None`。形状照 openai/codex `codex-rs/protocol/src/protocol.rs`
/// （`SessionMeta` · `SessionSource::SubAgent(SubAgentSource::ThreadSpawn{…})`，核到 rust-v0.167.0）。
pub fn thread_of(v: &Value) -> Option<Thread<'_>> {
    let p = match unwrap_envelope(v)? {
        ("session_meta", p) => p,
        _ => return None,
    };
    let spawn = p
        .get("source")
        .and_then(|s| s.get("subagent"))
        .and_then(|s| s.get("thread_spawn"))
        .filter(|s| s.is_object());
    // 昵称 · 角色：派出那一格写着的为准，没写 ⇒ `session_meta` 自己那一格。
    let of = |k: &str| spawn.and_then(|sp| str_at(sp, k).or_else(|| str_at(p, k)));
    Some(Thread {
        parent: spawn.and_then(|sp| str_at(sp, "parent_thread_id")),
        nickname: of("agent_nickname"),
        role: of("agent_role").or_else(|| of("agent_type")),
        forked_from: str_at(p, "forked_from_id"),
    })
}

fn str_at<'a>(o: &'a Value, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}

/// 一份 rollout 开头那条 `session_meta` 说的父线程（不是子线程 · 读不出 ⇒ `None`）。
fn parent_sid_of(p: &Path) -> Option<String> {
    head_spawn(p).map(|(parent, _, _)| parent)
}

/// 一份 rollout 开头那条 `session_meta`：是子线程 ⇒（父线程 · 昵称 · 角色）；不是 · 读不出 ⇒ `None`。
fn head_spawn(p: &Path) -> Option<(String, Option<String>, Option<String>)> {
    let pick = |v: &Value| {
        let t = thread_of(v)?;
        Some(serde_json::json!([t.parent, t.nickname, t.role]).to_string())
    };
    let said: Vec<Option<String>> =
        serde_json::from_str(&crate::agents::first_in_head(p, pick)?).ok()?;
    let mut it = said.into_iter();
    let parent = it.next().flatten().filter(|s| !s.is_empty())?;
    Some((parent, it.next().flatten(), it.next().flatten()))
}

/// 记录根下按日期放的一份 rollout（`<根>/YYYY/MM/DD/rollout-….jsonl`）⇒（根 · 那一天的相对目录）。
fn dated(p: &Path) -> Option<(&Path, &Path)> {
    let root = p.ancestors().nth(4)?;
    Some((root, p.parent()?.strip_prefix(root).ok()?))
}

/// 子线程的记录住哪：子线程是记录根下自己的一份 rollout，开头 `session_meta` 写着父线程（注册表 `RecordFace.children` 那一格）。
pub const CHILDREN: crate::agents::ChildFace = crate::agents::ChildFace {
    sources: child_sources,
    owner: child_owner,
    find: child_find,
    hint: child_hint,
    written: child_written,
    spawned: child_spawned,
};

/// 父那一份 ⇒ 它派出的子线程那几份（按名字次序）。子线程不早于父线程起 ⇒ 只看父那一天及以后的日期目录。
fn child_sources(parent: &Path) -> Vec<PathBuf> {
    let (Some(sid), Some((root, day))) = (super::parse::codex_sid_from_path(parent), dated(parent))
    else {
        return Vec::new();
    };
    walkdir::WalkDir::new(root)
        .min_depth(4)
        .max_depth(4)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|p| {
            p != parent
                && super::history::is_session_file(p)
                && dated(p).is_some_and(|(_, d)| d >= day)
                && parent_sid_of(p).as_deref() == Some(sid.as_str())
        })
        .collect()
}

/// 子线程那一份 ⇒ 父那一份（同一个记录根下按 sid 找）；不是子线程 · 父记录已删 ⇒ `None`。
fn child_owner(child: &Path) -> Option<PathBuf> {
    owner_walk(child, &mut |_| {})
}

/// [`child_owner`] 的本体：父子多在同一天起 ⇒ 先只读子线程那一天的目录，找不到再扫整个记录根（跳过已读的那一天）；
/// 两步都调按 sid 找会话那一处找法（父 sid 不成形 ⇒ 它不读盘、答 `None`）。每读一个目录告诉 `seen` 一声（判据数它）。
fn owner_walk(child: &Path, seen: &mut dyn FnMut(&Path)) -> Option<PathBuf> {
    use super::history::{find_session_in, DATED_DEPTH};
    let (root, _) = dated(child)?;
    let day = child.parent()?;
    let sid = parent_sid_of(child)?;
    find_session_in(day, 1, None, &sid, seen)
        .or_else(|| find_session_in(root, DATED_DEPTH, Some(day), &sid, seen))
}

/// 记录根下 id 为这个的子线程那一份；那一条不是子线程 ⇒ `None`。
fn child_find(root: &Path, id: &str) -> Option<PathBuf> {
    super::history::find_session(root, id)
        .ok()
        .filter(|p| parent_sid_of(p).is_some())
}

/// 父记录的一行可能说到子线程（[`child_link`] 会答出东西）：派出的那次调用做完了 · 子线程有了动静。
fn child_hint(line: &str) -> bool {
    line.contains("\"CollabAgentToolCall\"") || line.contains("\"SubAgentActivity\"")
}

/// 一条记录写着的时刻（信封 `timestamp`，毫秒）。
fn child_written(v: &Value) -> Option<u64> {
    super::parse::envelope_ts(v)
        .and_then(crate::common::time::parse_iso8601_ms)
        .and_then(|ms| u64::try_from(ms).ok())
}

/// 子线程那一份 ⇒ 它是哪个子运行 · 昵称 · 角色。派它的那次调用的 id 子线程那一侧不写 ⇒ 说不出（对上靠父那一侧的 [`child_link`]）。
fn child_spawned(child: &Path) -> Option<crate::agents::ChildLink> {
    let sid = super::parse::codex_sid_from_path(child)?;
    let (_, label, kind) = head_spawn(child)?;
    Some(crate::agents::ChildLink {
        run: Some(sid),
        label,
        kind,
        ..Default::default()
    })
}

/// 父记录里说到子线程的那几条（注册表 `RecordFace.child_link` 那一格），只在 `item_completed` 里：
/// 派出（`CollabAgentToolCall`，`tool` 为 `spawn_agent`）⇒ 那次调用对上它派出的每一条（带昵称 · 角色）；
/// 子线程做完（`SubAgentActivity`，`kind` 为 `completed`）⇒ 收场；被打断（`interrupted`）⇒ 叫停。
pub fn child_link(v: &Value) -> Vec<crate::agents::ChildLink> {
    use crate::agents::{ChildLink, RunEnd};
    if classify(v) != CodexRecordKind::ItemCompleted {
        return Vec::new();
    }
    let Some(item) = unwrap_envelope(v).and_then(|(_, p)| p.get("item")) else {
        return Vec::new();
    };
    let s = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    match item.get("type").and_then(Value::as_str) {
        Some("CollabAgentToolCall")
            if item.get("tool").and_then(Value::as_str) == Some("spawn_agent") =>
        {
            let Some(tool) = s(item, "id") else {
                return Vec::new();
            };
            let agents = item.get("receiver_agents").and_then(Value::as_array);
            let ids = item.get("receiver_thread_ids").and_then(Value::as_array);
            let link = |run: String, a: Option<&Value>| ChildLink {
                tool: Some(tool.clone()),
                run: Some(run),
                label: a.and_then(|a| s(a, "agent_nickname")),
                kind: a.and_then(|a| s(a, "agent_role").or_else(|| s(a, "agent_type"))),
                ..Default::default()
            };
            match agents.filter(|a| !a.is_empty()) {
                Some(agents) => agents
                    .iter()
                    .filter_map(|a| Some(link(s(a, "thread_id")?, Some(a))))
                    .collect(),
                None => ids
                    .into_iter()
                    .flatten()
                    .filter_map(|id| Some(link(id.as_str()?.to_string(), None)))
                    .collect(),
            }
        }
        Some("SubAgentActivity") => {
            let end = match item.get("kind").and_then(Value::as_str) {
                Some("completed") => RunEnd::Done,
                Some("interrupted") => RunEnd::Stopped,
                _ => return Vec::new(),
            };
            s(item, "agent_thread_id")
                .map(|run| ChildLink {
                    run: Some(run),
                    end: Some(end),
                    ..Default::default()
                })
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

// ─── 漂移账：认不出的记录（通用形 `agents::drift`；面的名字是这一家的词）───

/// 这一家记账的几处。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DriftFace {
    /// 认不出的顶层 `type`（出一条 `unread`）。
    UnknownRecordType,
    /// 认不出的 `response_item` 子型（出一条 `unread`）。
    UnknownItemType,
    /// 认不出的 `event_msg` 子型（不出记录）。
    UnknownEventType,
    /// 记录根下压缩存的 rollout（`.jsonl.zst`，Codex 那个本地线程存储压缩的特性，默认关）：这里读不了，不当会话。
    CompressedRollout,
}

impl DriftFaceWord for DriftFace {
    fn word(self) -> &'static str {
        match self {
            DriftFace::UnknownRecordType => "unknown_record_type",
            DriftFace::UnknownItemType => "unknown_item_type",
            DriftFace::UnknownEventType => "unknown_event_type",
            DriftFace::CompressedRollout => "compressed_rollout",
        }
    }
}

/// 这一家的账（计数 · 封顶 · 排序是通用那一本的）。
static DRIFT_LEDGER: std::sync::Mutex<DriftLedger<DriftFace>> =
    std::sync::Mutex::new(DriftLedger::new());

/// 记一笔（本进程内计数，不落盘）。
pub(crate) fn drift(face: DriftFace, key: &str) {
    DriftLedger::lock(&DRIFT_LEDGER).record(face, key);
}

/// 这一家的账（注册表 `RecordFace.drift` 那一格）。
pub fn drift_snapshot() -> Vec<crate::agents::drift::DriftFaceReport> {
    DriftLedger::lock(&DRIFT_LEDGER).snapshot()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/record_tests.rs"]
mod tests;
