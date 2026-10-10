//! Claude 盘上一行（[`JsonlRecord`]，已带后端补的成品格）⇒ 通用记录（[`Record`]）。翻译表只住这里：
//!
//! | 盘上 | 通用记录 |
//! |---|---|
//! | `user` | `said`：`who` · `message.content` ⇒ `blocks` · 工具结果的一句 ⇒ `results` · `cwd` |
//! | `assistant` | `reply`；`isApiErrorMessage` ⇒ `error{reason, status}`；`message.model == "<synthetic>"`（且不是报错）⇒ `autoReply`；一轮结束 ⇒ `endsTurn` |
//! | `system` · `subtype == "api_error"` | `retry` |
//! | `ai-title` / `custom-title` | `title{by: agent / user}` |
//! | `queue-operation` · `remove` · 人说的 | `queued` |
//! | 看不懂的（`Unrecognized`：没见过的类型 · 形状变了） | `unread`（写好的一句 ＋ 原文摘录） |
//! | 其余（别的 `system` · `attachment` · 状态行 · 元数据） | 无记录（链上那一半由 [`super::chain`] 给） |

use super::schema::JsonlRecord;
use crate::agents::record::{Block, Body, Record, RecordClass, ReplyError, TitleBy, UnreadWhy};
use serde_json::Value;

/// 这一家在线上的 `agent` 值。
const AGENT: &str = "claude";
/// 代理那一侧自动写的应答，型号名写成这个。
const AUTO_REPLY_MODEL: &str = "<synthetic>";

/// 一条已解析的记录（`Value`）⇒ [`record_of`] 会给它的类，不走整条翻译（骨架索引每行一问）。
/// 只看判别字段与那几类的必填格（缺了 ⇒ 盘上形状认不出、不出记录）；排队那一条还要认出是人说的。
pub(crate) fn class_of(v: &Value) -> Option<RecordClass> {
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    let message = || {
        v.get("message").is_some_and(|m| {
            m.get("role").is_some_and(Value::is_string) && m.get("content").is_some()
        })
    };
    let said_or_reply = || s("uuid").is_some() && s("timestamp").is_some() && message();
    match s("type")? {
        "user" if said_or_reply() => Some(RecordClass::Said),
        "assistant" if said_or_reply() => Some(RecordClass::Reply),
        "system" if s("subtype") == Some("api_error") && s("timestamp").is_some() => {
            Some(RecordClass::Retry)
        }
        "ai-title" if s("aiTitle").is_some() && s("sessionId").is_some() => {
            Some(RecordClass::Title)
        }
        "custom-title" if s("customTitle").is_some() && s("sessionId").is_some() => {
            Some(RecordClass::Title)
        }
        "queue-operation"
            if s("operation") == Some("remove")
                && s("content").is_some_and(|c| {
                    matches!(
                        super::text::queued_text(c).speaker,
                        crate::agents::Speaker::Human
                    )
                }) =>
        {
            Some(RecordClass::Queued)
        }
        _ => None,
    }
}

/// 一条解析好的盘上记录 ⇒ 通用记录；不进界面 ⇒ `None`。没有身份的那几类（标题 · 排队）`id` 用 `fallback_id`（调用方给，会话内唯一）。
pub(crate) fn record_of(rec: JsonlRecord, fallback_id: &str) -> Option<Record> {
    let rec_of = |id: String, at: Option<String>, body: Body| Record {
        agent: AGENT.to_string(),
        id: if id.is_empty() {
            fallback_id.to_string()
        } else {
            id
        },
        at,
        // 钟面按看的那一台的时区，出口那一下写（[`Record::stamp`]）。
        time_text: None,
        at_ms: None,
        body,
    };
    match rec {
        JsonlRecord::User {
            uuid,
            timestamp,
            message,
            cwd,
            user_text,
            tool_results,
            ..
        } => Some(rec_of(
            uuid,
            Some(timestamp),
            Body::Said {
                who: user_text,
                blocks: blocks_of(&message.content),
                results: tool_results,
                cwd,
            },
        )),
        JsonlRecord::Assistant {
            uuid,
            timestamp,
            message,
            is_api_error_message,
            api_error_status,
            api_reason,
            tool_cards,
            tool_steps,
            child_runs,
            ..
        } => {
            let error = is_api_error_message.then(|| ReplyError {
                reason: api_reason.unwrap_or(crate::agents::ApiReason::Unknown),
                status: api_error_status,
            });
            let auto_reply = error.is_none() && message.model.as_deref() == Some(AUTO_REPLY_MODEL);
            let ends_turn =
                super::turn::ends_turn(message.stop_reason.as_deref(), is_api_error_message);
            Some(rec_of(
                uuid,
                Some(timestamp),
                Body::Reply {
                    blocks: blocks_of(&message.content),
                    model: message.model.filter(|m| m != AUTO_REPLY_MODEL),
                    auto_reply,
                    ends_turn,
                    cards: tool_cards,
                    steps: tool_steps,
                    runs: child_runs,
                    error,
                },
            ))
        }
        JsonlRecord::System {
            subtype,
            timestamp,
            uuid,
            retry_attempt,
            max_retries,
            api_reason,
            ..
        } if subtype.as_deref() == Some("api_error") => Some(rec_of(
            uuid.unwrap_or_default(),
            Some(timestamp),
            Body::Retry {
                reason: api_reason.unwrap_or(crate::agents::ApiReason::Unknown),
                attempt: retry_attempt,
                max: max_retries,
            },
        )),
        JsonlRecord::AiTitle { ai_title, .. } => Some(rec_of(
            String::new(),
            None,
            Body::Title {
                text: ai_title,
                by: TitleBy::Agent,
            },
        )),
        JsonlRecord::CustomTitle { custom_title, .. } => Some(rec_of(
            String::new(),
            None,
            Body::Title {
                text: custom_title,
                by: TitleBy::User,
            },
        )),
        JsonlRecord::QueueOperation {
            operation,
            timestamp,
            user_text: Some(who),
            ..
        } if operation.as_deref() == Some("remove")
            && matches!(who.speaker, crate::agents::Speaker::Human) =>
        {
            Some(rec_of(String::new(), timestamp, Body::Queued { who }))
        }
        // 认不出（没见过的类型 · 见过的类型形状变了）：每行一条 `unread`；相邻同类并成一条是出记录页那一遍的事。
        JsonlRecord::Unrecognized {
            uuid,
            timestamp,
            original_type,
            raw,
            reason,
            ..
        } => {
            // 连 `type` 都没有的一行算「没见过的类型」（serde 报的是缺判别格，不是认识的类型形状变了）。
            let why = if reason == "unknown-type" || original_type.is_none() {
                UnreadWhy::UnknownType
            } else {
                UnreadWhy::ParseFailed
            };
            Some(rec_of(
                uuid.unwrap_or_default(),
                timestamp,
                Body::unread(why, original_type, &raw),
            ))
        }
        _ => None,
    }
}

/// `message.content`（字符串 · 块数组）⇒ 通用内容块；认不出的块跳过。
pub(crate) fn blocks_of(content: &Value) -> Vec<Block> {
    match content {
        Value::String(s) => vec![Block::Text { text: s.clone() }],
        Value::Array(a) => a.iter().filter_map(block_of).collect(),
        _ => Vec::new(),
    }
}

fn block_of(b: &Value) -> Option<Block> {
    let s = |k: &str| b.get(k).and_then(Value::as_str).map(str::to_string);
    Some(match b.get("type").and_then(Value::as_str)? {
        "text" => Block::Text { text: s("text")? },
        "thinking" => Block::Thinking {
            text: s("thinking").unwrap_or_default(),
        },
        "tool_use" => Block::ToolUse {
            id: s("id")?,
            name: s("name")?,
            input: b.get("input").cloned().unwrap_or(Value::Null),
        },
        "tool_result" => Block::ToolResult {
            of: s("tool_use_id")?,
            content: b.get("content").map(blocks_of).unwrap_or_default(),
            is_error: b.get("is_error").and_then(Value::as_bool) == Some(true),
        },
        "image" => Block::Image {
            source: b.get("source").cloned().unwrap_or(Value::Null),
        },
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/record_of_tests.rs"]
mod tests;
