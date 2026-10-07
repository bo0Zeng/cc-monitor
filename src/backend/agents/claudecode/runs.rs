//! Claude Code 的**子运行形状**（注册表 `RecordFace` 的 `response_id` · `run_of` · `child_link` · `children` 四格，`DefaultUpstream.owner_header`）。
//!
//! - 对账键：assistant 记录的 `message.id`（同一次应答拆成几条记录，它们共用这一个）。
//! - 子 agent 的记录住 `<父记录去后缀>/subagents/[<子目录>/]agent-<agentId>.jsonl`；每条带 `isSidechain: true` 与 `agentId`，
//!   `sessionId` 与主会话相同。
//! - 父侧：Agent 工具（工具词表里卡型是 `agent` 的那几个名字）的 `tool_use` 给标签（`description`，没有就取 `prompt` 首行）与类别
//!   （`subagent_type`）；那次调用的结果记录在 `toolUseResult.agentId` 里说出它派出的是哪个子 agent。
//! - 收场以派出那一方为准（子记录常以一次工具调用收尾、没有 `end_turn`；被额度打断 / 被叫停的根本没有终局）。三路，先到先算：
//!   ① 前台派出：那次调用拿到结果 ⇒ 完成（`is_error` ⇒ 失败）；后台派出当场回的那次（`toolUseResult.status` ＝ `async_launched`）不算。
//!   ② 后台派出：父记录里关于它的通知 —— `<task-notification>` 打头的一段，住 `queue-operation` 的 `content` · `attachment` 的
//!      `prompt` · user 记录的字符串正文；只读 `<task-id>`（＝ agentId）与 `<status>`（completed · failed · killed / stopped）两格，不读正文。
//!   ③ 子记录自己那一轮以 `end_turn` 收尾；API 报错的那条（`isApiErrorMessage`，`stop_reason` 不定）本身就是失败收场；
//!      最后一条是打断标记 ⇒ 被叫停。
//! - 请求：子 agent 发的每条请求带 `x-claude-code-agent-id`（值 ＝ 它的 `agentId`）；主运行的请求不带。
//! - 时刻：每条记录的 `timestamp`（ISO 8601）。在等工具：子记录里带 `tool_result` 的 user 记录交回了之前那次调用的结果。
//!   报错原话：前台那次调用 `is_error` 的结果正文 · API 报错那条 assistant 记录的正文。

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
    let api_error = v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
    let end = if assistant && api_error {
        Some(RunEnd::Failed)
    } else {
        (assistant && stop == "end_turn").then_some(RunEnd::Done)
    };
    // 子 agent 被打断（用户按了 Esc / 主运行被打断）：它最后写的是一条打断标记 ⇒ 不会再有终局，按被叫停收。
    let interrupted = s(v, "type") == Some("user")
        && v.get("message")
            .and_then(|m| m.get("content"))
            .is_some_and(super::text::is_interrupt_content);
    let end = end.or(interrupted.then_some(RunEnd::Stopped));
    let answered =
        s(v, "type") == Some("user") && content(v).any(|b| s(b, "type") == Some("tool_result"));
    let error = (assistant && api_error)
        .then(|| texts(v.get("message").and_then(|m| m.get("content"))))
        .flatten();
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
    Some(RunMark {
        run,
        end,
        did,
        answered,
        error,
    })
}

/// 一段正文（字符串，或 `[{type:"text",text}]` 那一形）⇒ 文字（空 ⇒ `None`）。
fn texts(c: Option<&Value>) -> Option<String> {
    let t = match c? {
        Value::String(t) => t.clone(),
        Value::Array(a) => a
            .iter()
            .filter(|b| s(b, "type") == Some("text"))
            .filter_map(|b| s(b, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let t = trim(&t);
    (!t.is_empty()).then(|| t.to_string())
}

/// 一条记录写着的时刻（`timestamp`，ISO 8601 ⇒ 自 1970 起的毫秒）。
pub(crate) fn written(v: &Value) -> Option<u64> {
    u64::try_from(crate::observe::search_query::parse_iso8601_ms(s(
        v,
        "timestamp",
    )?)?)
    .ok()
}

/// trim：Unicode 空白 ＋ BOM。
fn trim(x: &str) -> &str {
    x.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// 父记录里说到子 agent 的那几条：Agent 工具的调用（标签 ＋ 类别）· 它的结果（子 agent 是哪个，前台的那次也是收场）· 后台派出的收场通知。
pub(crate) fn child_link(v: &Value) -> Vec<super::super::ChildLink> {
    match s(v, "type") {
        Some("assistant") => v
            .get("message")
            .and_then(|m| m.get("content"))
            .map(links_in_content)
            .unwrap_or_default(),
        Some("user") => {
            let mut out = result_link(v);
            if let Some(text) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
            {
                out.extend(notice(text));
            }
            out
        }
        Some("queue-operation") => v
            .get("content")
            .and_then(Value::as_str)
            .map(notice)
            .unwrap_or_default(),
        Some("attachment") => v
            .get("attachment")
            .and_then(|a| s(a, "prompt"))
            .map(notice)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// 后台派出那次当场回的结果（还在跑）。
const ASYNC_LAUNCHED: &str = "async_launched";

/// Agent 调用拿到的那次结果：说出子 agent 是哪个；前台的那次同时就是它收场的时候。
fn result_link(v: &Value) -> Vec<super::super::ChildLink> {
    use super::super::{ChildLink, RunEnd};
    let result = v.get("toolUseResult");
    let Some(run) = result
        .and_then(|r| s(r, "agentId"))
        .filter(|a| !a.is_empty())
    else {
        return Vec::new();
    };
    let launched = result.and_then(|r| s(r, "status")) == Some(ASYNC_LAUNCHED);
    content(v)
        .filter(|b| s(b, "type") == Some("tool_result"))
        .filter_map(|b| {
            let failed = b.get("is_error").and_then(Value::as_bool) == Some(true);
            Some(ChildLink {
                tool: Some(s(b, "tool_use_id")?.to_string()),
                run: Some(run.to_string()),
                end: if failed {
                    Some(RunEnd::Failed)
                } else {
                    (!launched).then_some(RunEnd::Done)
                },
                error: failed.then(|| texts(b.get("content"))).flatten(),
                background: launched,
                ..ChildLink::default()
            })
        })
        .take(1)
        .collect()
}

/// 收场通知 ⇒ 哪个子 agent 怎么收场的（通知的认法与几格住 `text.rs::task_notice`）。只读 `task-id` 与 `status` 两格；
/// 同一种通知也说后台命令，那些 `task-id` 对不上任何子 agent，通用层不立新行。
fn notice(text: &str) -> Vec<super::super::ChildLink> {
    use super::super::{ChildLink, RunEnd};
    let Some(n) = super::text::task_notice(text) else {
        return Vec::new();
    };
    let end = match n.status.as_deref() {
        Some("completed") => RunEnd::Done,
        Some("failed") => RunEnd::Failed,
        Some("killed" | "stopped") => RunEnd::Stopped,
        _ => return Vec::new(),
    };
    match n.task_id {
        Some(run) => vec![ChildLink {
            run: Some(run),
            end: Some(end),
            ..ChildLink::default()
        }],
        None => Vec::new(),
    }
}

/// 父记录的一行原文可能说到子 agent（[`child_link`] 会答出东西）：Agent 工具调用 · 带 `agentId` 的结果 · 收场通知。
pub(crate) fn hint(line: &str) -> bool {
    static CALLS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    line.contains(super::text::NOTICE_OPEN)
        || line.contains("\"agentId\"")
        || CALLS
            .get_or_init(|| {
                super::cards::agent_tool_names()
                    .iter()
                    .map(|n| format!("\"name\":\"{n}\""))
                    .collect()
            })
            .iter()
            .any(|c| line.contains(c.as_str()))
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
                tool: Some(s(b, "id")?.to_string()),
                label: Some(label),
                kind: field("subagent_type").map(str::to_string),
                ..ChildLink::default()
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

/// [`sources`] 的反方向：`<dir>/<stem>/subagents/[至多 CHILD_DEPTH 层]/agent-*.jsonl` ⇒ `<dir>/<stem>.jsonl`；别的形状 ⇒ `None`。
pub(crate) fn owner(p: &Path) -> Option<PathBuf> {
    let name = p.file_name()?.to_str()?;
    if !name.starts_with(CHILD_PREFIX) || !super::records::is_session_file(p) {
        return None;
    }
    let child_dir = p
        .ancestors()
        .skip(1)
        .take(CHILD_DEPTH + 1)
        .find(|a| a.file_name().is_some_and(|n| n == CHILD_DIR))?;
    let stem_dir = child_dir.parent()?;
    let ext = p.extension()?;
    let mut parent = stem_dir.as_os_str().to_owned();
    parent.push(".");
    parent.push(ext);
    Some(PathBuf::from(parent))
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
