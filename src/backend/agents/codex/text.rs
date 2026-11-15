//! Codex rollout 记录的文本面（注册表 `RecordFace.text` 那一格）：全文搜索 · 会话内查找 · 历史清单那几条通用路照它读这一家的记录。
//!
//! 分类与「谁说的」只用 [`super::record`] 那一份（与出记录同一个判定）；这里只答通用层要的那几问：
//!
//! | 记录 | 是谁那一侧（[`LineRole`]） | 正文 | 工具内容 |
//! |---|---|---|---|
//! | `response_item.message`：「谁说的」答得出（user / developer / 别家来信） | 人那一侧 | 谁说的那一句（通用层只搜人说的） | — |
//! | `response_item.message`：答不出（这一家自己的回复） | 回话 | `content` 拍平 | — |
//! | `agent_message` · `inter_agent_communication` | 人那一侧（来信） | `content` 拍平 | — |
//! | `reasoning` | 回话 | — | `summary` 拍平 |
//! | 工具调用 | 回话 | — | 工具名 ＋ 入参（与出记录同一份，`record::tool_input`） |
//! | 工具输出 | 人那一侧（工具结果），收在那次调用的那一块里 | — | `output` 拍平 |
//! | 别的（元记录 · 事件 · 认不出的） | 不是 | — | — |
//!
//! 时刻 ＝ 信封 `timestamp`；消息 id ＝ `payload.id`（没有 ⇒ 不给，通用层照这一行的起点合成，与出记录那一下同一个数）；
//! 工作目录 ＝ `session_meta.cwd`。这一家的记录里没有标题 · 后台会话 · 分叉来源这几件 ⇒ 不答。
//! 正文的形状不另数：骨架索引照一拍数（`observe/recognize/index_row.rs` 没有文本面形状的那一臂），与出记录同一个拍平。

use super::parse::{envelope_ts, session_meta_cwd, unwrap_envelope};
use super::record::{
    agent_mail_said, call_id, classify, flatten_text, message_said, reasoning_text, tool_input,
    CodexRecordKind as K,
};
use crate::agents::{BodyShape, Lands, LineRole, SessionLine, Speaker, TextFace, UserText};
use serde_json::Value;

/// 注册表 `RecordFace.text` 那一格。
pub(crate) const TEXT: TextFace = TextFace {
    main: main_of,
    tool: tool_of,
    user: user_of,
    lands: lands_of,
    line: line_of,
    line_of: line_of_value,
    shape: shape_of,
};

fn payload_of(v: &Value) -> &Value {
    unwrap_envelope(v).map_or(&Value::Null, |(_, p)| p)
}

fn content_of(v: &Value) -> String {
    flatten_text(payload_of(v).get("content").unwrap_or(&Value::Null))
}

/// 这一条是谁那一侧的（与 `record::record_of` 出 `said` / `reply` 的分法逐类相同）；不出记录的那几类 ⇒ `None`。
fn role_of(v: &Value) -> Option<LineRole> {
    match classify(v) {
        K::Message => Some(match message_said(payload_of(v)) {
            Some(_) => LineRole::Said,
            None => LineRole::Replied,
        }),
        K::AgentMail | K::ToolResult => Some(LineRole::Said),
        K::Reasoning | K::ToolCall => Some(LineRole::Replied),
        _ => None,
    }
}

/// 正文（注册表 `TextFace.main`）：消息与来信的 `content` 拍平；别的没有正文。
fn main_of(v: &Value) -> String {
    match classify(v) {
        K::Message | K::AgentMail => content_of(v),
        _ => String::new(),
    }
}

/// 工具内容（注册表 `TextFace.tool`）：回话那一侧是思考与工具调用，人那一侧是工具输出。
fn tool_of(v: &Value, is_assistant: bool) -> String {
    let p = payload_of(v);
    match classify(v) {
        K::Reasoning if is_assistant => reasoning_text(v),
        K::ToolCall if is_assistant => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            // 入参照出记录那一份取（`record::tool_input`），工具卡上画的就是它。
            let input = match tool_input(v) {
                Value::Null => String::new(),
                o => o.to_string(),
            };
            [name, input.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        K::ToolResult if !is_assistant => flatten_text(p.get("output").unwrap_or(&Value::Null)),
        _ => String::new(),
    }
}

/// 谁说的（注册表 `TextFace.user`）：与出记录同一个判定；回话那一侧 ⇒ `None`。
fn user_of(v: &Value) -> Option<crate::agents::record::Heard> {
    match classify(v) {
        K::Message => message_said(payload_of(v)),
        K::AgentMail => Some(agent_mail_said(payload_of(v))),
        K::ToolResult => Some(UserText::of(Speaker::ToolResult).into()),
        _ => None,
    }
}

/// 落在哪张卡上（注册表 `TextFace.lands`）：工具输出收在那次调用的那一块里；别的落在自己那张卡上。
fn lands_of(v: &Value) -> Lands {
    match (classify(v), call_id(v).filter(|id| !id.is_empty())) {
        (K::ToolResult, Some(id)) => Lands::InTool(id.to_string()),
        _ => Lands::Own,
    }
}

/// 一行原文 ⇒ 会话级那几格（注册表 `TextFace.line`）；不是 JSON ⇒ `None`。
fn line_of(text: &str) -> Option<SessionLine> {
    serde_json::from_str::<Value>(text)
        .ok()
        .map(|v| line_of_value(&v))
}

/// 已解析好的一条 ⇒ 会话级那几格（注册表 `TextFace.line_of`）：时刻 · 消息 id 只给人那一侧 / 回话那两类。
fn line_of_value(v: &Value) -> SessionLine {
    let role = role_of(v);
    let talk = role.is_some();
    SessionLine {
        role,
        title: None,
        bg: false,
        at_ms: envelope_ts(v)
            .filter(|_| talk)
            .and_then(crate::common::time::parse_iso8601_ms),
        id: payload_of(v)
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| talk && !id.is_empty())
            .map(str::to_string),
        cwd: session_meta_cwd(v)
            .filter(|c| !c.is_empty())
            .map(str::to_string),
        forked: None,
    }
}

/// 正文的形状（注册表 `TextFace.shape`）：不另数，骨架索引照一拍数（见头注）。
fn shape_of(_: &Value) -> Option<BodyShape<'_>> {
    None
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/text_tests.rs"]
mod tests;
