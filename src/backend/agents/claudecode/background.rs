//! Claude Code 的**后台命令**在记录里的样子（注册表 `RecordFace.background`）：
//!
//! - 起：assistant 记录里一次工具调用，入参 `run_in_background: true`（今天只有 Bash 会这样起）；那条命令是入参 `command`。
//! - 任务号：那次调用当场回的结果（user 记录的 `tool_result`）旁边，`toolUseResult.backgroundTaskId`。
//!   当场回的结果标了出错 ⇒ 没起来，按收场算。
//! - 收：收场通知 —— `<task-notification>` 打头的一段（认法住 `text.rs::task_notice`），住 `queue-operation` 的 `content` ·
//!   `attachment` 的 `prompt` · user 记录的正文；只读 `<task-id>` 与 `<tool-use-id>` 两格（完成 · 失败 · 被叫停都算收场）。
//!   同一种通知也说后台 agent，那些对不上任何一条起过的命令，核心不认。

use super::super::BgMark;
use serde_json::Value;

/// 后台起的那一格入参。
const IN_BACKGROUND: &str = "run_in_background";
/// 那条命令的入参。
const COMMAND: &str = "command";
/// 当场回的结果旁边说任务号的那一格。
const TASK_ID: &str = "backgroundTaskId";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn blocks(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// 一条记录里说到后台命令的那几笔（口径见头注）。
pub(crate) fn marks(v: &Value) -> Vec<BgMark> {
    match s(v, "type") {
        Some("assistant") => blocks(v)
            .filter(|b| s(b, "type") == Some("tool_use"))
            .filter(|b| {
                b.get("input")
                    .and_then(|i| i.get(IN_BACKGROUND))
                    .and_then(Value::as_bool)
                    == Some(true)
            })
            .filter_map(|b| {
                Some(BgMark::Started {
                    call: s(b, "id").filter(|i| !i.is_empty())?.to_string(),
                    cmd: b
                        .get("input")
                        .and_then(|i| s(i, COMMAND))
                        .map(str::to_string),
                })
            })
            .collect(),
        Some("user") => {
            let task = v
                .get("toolUseResult")
                .and_then(|r| s(r, TASK_ID))
                .filter(|t| !t.is_empty());
            let mut out: Vec<BgMark> = blocks(v)
                .filter(|b| s(b, "type") == Some("tool_result"))
                .filter_map(|b| {
                    let call = s(b, "tool_use_id")?.to_string();
                    if b.get("is_error").and_then(Value::as_bool) == Some(true) {
                        return Some(BgMark::Ended {
                            call: Some(call),
                            task: None,
                        });
                    }
                    Some(BgMark::Named {
                        call,
                        task: task?.to_string(),
                    })
                })
                .collect();
            let content = v.get("message").and_then(|m| m.get("content"));
            let text = match content {
                Some(Value::String(t)) => Some(t.clone()),
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .filter(|b| s(b, "type") == Some("text"))
                        .filter_map(|b| s(b, "text"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            };
            out.extend(text.as_deref().and_then(ended));
            out
        }
        Some("queue-operation") => s(v, "content").and_then(ended).into_iter().collect(),
        Some("attachment") => v
            .get("attachment")
            .and_then(|a| s(a, "prompt"))
            .and_then(ended)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// 收场通知 ⇒ 哪一条收场了（任务号 · 工具调用 id，至少一格）。
fn ended(text: &str) -> Option<BgMark> {
    let n = super::text::task_notice(text)?;
    (n.task_id.is_some() || n.tool_use_id.is_some()).then_some(BgMark::Ended {
        call: n.tool_use_id,
        task: n.task_id,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/background_tests.rs"]
mod tests;
