//! Claude Code 的**子运行形状**（注册表 `RecordFace` 的 `response_id` · `run_of` · `child_link` · `children` 四格，`DefaultUpstream.owner_header`）。
//!
//! - 对账键：assistant 记录的 `message.id`（同一次应答拆成几条记录，它们共用这一个）。
//! - 子 agent 的记录住 `<父记录去后缀>/subagents/[<子目录>/]agent-<agentId>.jsonl`；每条带 `isSidechain: true` 与 `agentId`，
//!   `sessionId` 与主会话相同。终局 ＝ 它那一轮以 `end_turn` 收尾（API 报错收尾 ⇒ 失败）。
//! - 父侧：Agent 工具（工具词表里卡型是 `agent` 的那几个名字）的 `tool_use` 给标签（`description`，没有就取 `prompt` 首行）与类别
//!   （`subagent_type`）；那次调用的结果记录在 `toolUseResult.agentId` 里说出它派出的是哪个子 agent（后台派出当场就回，前台跑完才回）。
//! - 请求：子 agent 发的每条请求带 `x-claude-code-agent-id`（值 ＝ 它的 `agentId`）；主运行的请求不带。

use serde_json::Value;
use std::path::{Path, PathBuf};

/// 子 agent 的记录目录名（在父记录去后缀那一层下面）。
const CHILD_DIR: &str = "subagents";
/// 子 agent 记录文件名的前缀（`agent-<agentId>.jsonl`）。
const CHILD_PREFIX: &str = "agent-";
/// 子 agent 记录目录下最多再下几层（子 agent 再派子 agent 时按层放）。
const CHILD_DEPTH: usize = 4;
/// 标签取 `prompt` 首行时最多几个字。
pub(crate) const LABEL_PROMPT_CHARS: usize = 80;
/// 子 agent 自报身份的请求头（值 ＝ 记录里的 `agentId`）。
pub(crate) const OWNER_HEADER: &str = "x-claude-code-agent-id";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn content(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// 对账键：assistant 记录的 `message.id`。
pub(crate) fn response_id(v: &Value) -> Option<String> {
    if s(v, "type") != Some("assistant") {
        return None;
    }
    v.get("message")
        .and_then(|m| s(m, "id"))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// 这条记录属于哪个子 agent（主会话的记录 ⇒ `None`）。
pub(crate) fn run_of(v: &Value) -> Option<super::super::RunMark> {
    use super::super::{RunDid, RunEnd, RunMark};
    if v.get("isSidechain").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let run = s(v, "agentId").filter(|a| !a.is_empty())?.to_string();
    let assistant = s(v, "type") == Some("assistant");
    let stop = v
        .get("message")
        .and_then(|m| s(m, "stop_reason"))
        .unwrap_or("");
    let end = (assistant && stop == "end_turn").then(|| {
        if v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
            RunEnd::Failed
        } else {
            RunEnd::Done
        }
    });
    // 子 agent 被打断（用户按了 Esc / 主运行被打断）：它最后写的是一条打断标记 ⇒ 不会再有终局，按失败收。
    let interrupted = s(v, "type") == Some("user")
        && v.get("message")
            .and_then(|m| m.get("content"))
            .is_some_and(|c| super::text::user_text(&super::text::extract_text_blocks(c)).interrupt);
    let end = end.or(interrupted.then_some(RunEnd::Failed));
    let did = if assistant {
        content(v).last().and_then(|b| match s(b, "type") {
            Some("tool_use") => s(b, "name").map(|n| RunDid::Tool {
                name: n.to_string(),
            }),
            Some("text") => Some(RunDid::Say),
            Some("thinking") => Some(RunDid::Think),
            _ => None,
        })
    } else {
        None
    };
    Some(RunMark { run, end, did })
}

/// trim：Unicode 空白 ＋ BOM。
fn trim(x: &str) -> &str {
    x.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// 父记录里派出子 agent 的那几条：Agent 工具的调用（标签 ＋ 类别）· 它的结果（子 agent 是哪个）。
pub(crate) fn child_link(v: &Value) -> Vec<super::super::ChildLink> {
    use super::super::{ChildLink, RunEnd};
    match s(v, "type") {
        Some("assistant") => v
            .get("message")
            .and_then(|m| m.get("content"))
            .map(links_in_content)
            .unwrap_or_default(),
        Some("user") => {
            let result = v.get("toolUseResult");
            let Some(run) = result
                .and_then(|r| s(r, "agentId"))
                .filter(|a| !a.is_empty())
            else {
                return Vec::new();
            };
            // 前台派出的那次，结果就是它跑完的时候；后台派出的结果当场回（还在跑）。
            let done = result.and_then(|r| s(r, "status")) == Some("completed");
            content(v)
                .filter(|b| s(b, "type") == Some("tool_result"))
                .filter_map(|b| {
                    let failed = b.get("is_error").and_then(Value::as_bool) == Some(true);
                    Some(ChildLink {
                        tool: s(b, "tool_use_id")?.to_string(),
                        run: Some(run.to_string()),
                        end: if failed {
                            Some(RunEnd::Failed)
                        } else {
                            done.then_some(RunEnd::Done)
                        },
                        ..ChildLink::default()
                    })
                })
                .take(1)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// 一条 assistant 记录的 `message.content` 里派出子 agent 的那几次调用（记录成品的 `childRuns` 与派出链接共用这一份）。
pub(crate) fn links_in_content(content: &Value) -> Vec<super::super::ChildLink> {
    use super::super::{ChildLink, ToolCard};
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| s(b, "type") == Some("tool_use"))
            .filter_map(|b| {
                let name = s(b, "name")?;
                if super::cards::tool_card(name) != Some(ToolCard::Agent) {
                    return None;
                }
                let input = b.get("input");
                let field = |k: &str| input.and_then(|i| s(i, k));
                let desc = trim(field("description").unwrap_or(""));
                let head: String = field("prompt")
                    .unwrap_or("")
                    .split('\n')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(LABEL_PROMPT_CHARS)
                    .collect();
                let label = if !desc.is_empty() {
                    desc.to_string()
                } else if !head.is_empty() {
                    head
                } else {
                    name.to_string()
                };
                Some(ChildLink {
                    tool: s(b, "id")?.to_string(),
                    label: Some(label),
                    kind: field("subagent_type").map(str::to_string),
                    run: None,
                    end: None,
                })
            })
            .collect()
}

/// 父记录 ⇒ 此刻在盘上的子 agent 记录（按路径排好序）。
pub(crate) fn sources(parent: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (parent.parent(), parent.file_stem()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(&dir.join(stem).join(CHILD_DIR), CHILD_DEPTH, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else {
            continue;
        };
        if t.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else if t.is_file()
            && super::records::is_session_file(&p)
            && p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(CHILD_PREFIX))
        {
            out.push(p);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/runs_tests.rs"]
mod tests;
