//! Claude 盘上一行（[`JsonlRecord`]，已带后端补的成品格）⇒ 通用记录（[`Record`]）。翻译表只住这里：
//!
//! | 盘上 | 通用记录 |
//! |---|---|
//! | `user` | `said`：`who` · `message.content` ⇒ `blocks` · 工具结果的一句 ⇒ `results` · `cwd` |
//! | `assistant` | `reply`；`isApiErrorMessage` ⇒ `error{reason, status}`；`message.model == "<synthetic>"`（且不是报错）⇒ `autoReply`；一轮结束 ⇒ `endsTurn` |
//! | `system` · `subtype == "api_error"` | `retry` |
//! | `ai-title` / `custom-title` | `title{by: agent / user}` |
//! | `queue-operation` · `remove` · 人说的 | `queued` |
//! | 其余（别的 `system` · `attachment` · 看不懂的 · 元数据） | 无记录（链上那一半由 [`super::chain`] 给） |

use super::schema::JsonlRecord;
use crate::agents::record::{Block, Body, Record, ReplyError, TitleBy};
use serde_json::Value;

/// 这一家在线上的 `agent` 值。
const AGENT: &str = "claude";
/// 代理那一侧自动写的应答，型号名写成这个。
const AUTO_REPLY_MODEL: &str = "<synthetic>";

/// 一条解析好的盘上记录 ⇒ 通用记录；不进界面 ⇒ `None`。没有身份的那几类（标题 · 排队）`id` 用 `fallback_id`（调用方给，会话内唯一）。
pub(crate) fn record_of(rec: JsonlRecord, fallback_id: &str) -> Option<Record> {
    let rec_of = |id: String, at: Option<String>, time_text: Option<String>, body: Body| Record {
        agent: AGENT.to_string(),
        id: if id.is_empty() {
            fallback_id.to_string()
        } else {
            id
        },
        at,
        time_text: time_text.map(crate::common::cells::Words),
        body,
    };
    match rec {
        JsonlRecord::User {
            uuid,
            timestamp,
            time_text,
            message,
            cwd,
            user_text,
            tool_results,
            ..
        } => Some(rec_of(
            uuid,
            Some(timestamp),
            time_text,
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
            time_text,
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
                time_text,
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
            time_text,
            uuid,
            retry_attempt,
            max_retries,
            api_reason,
            ..
        } if subtype.as_deref() == Some("api_error") => Some(rec_of(
            uuid.unwrap_or_default(),
            Some(timestamp),
            time_text,
            Body::Retry {
                reason: api_reason.unwrap_or(crate::agents::ApiReason::Unknown),
                attempt: retry_attempt,
                max: max_retries,
            },
        )),
        JsonlRecord::AiTitle { ai_title, .. } => Some(rec_of(
            String::new(),
            None,
            None,
            Body::Title {
                text: ai_title,
                by: TitleBy::Agent,
            },
        )),
        JsonlRecord::CustomTitle { custom_title, .. } => Some(rec_of(
            String::new(),
            None,
            None,
            Body::Title {
                text: custom_title,
                by: TitleBy::User,
            },
        )),
        JsonlRecord::QueueOperation {
            operation,
            timestamp,
            time_text,
            user_text: Some(who),
            ..
        } if operation.as_deref() == Some("remove")
            && matches!(who.speaker, crate::agents::Speaker::Human) =>
        {
            Some(rec_of(
                String::new(),
                timestamp,
                time_text,
                Body::Queued { who },
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
