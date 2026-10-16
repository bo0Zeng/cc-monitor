//! Claude Code 的一条记录 ⇒ 一拍（[`crate::agents::beat`]）。读法原样从 observe 搬来（轮次 · 会话事实那几处），不新写判别。
//!
//! | 记录 | 一拍 |
//! |---|---|
//! | `uuid` · `timestamp` | `id` · `at` |
//! | `type` = assistant / user / 其余 | `side` = Reply / Input / Note；`isMeta` ⇒ `injected` |
//! | `message.content` 的块（没有 ⇒ 顶层 `content`；是串 ⇒ 一段正文） | `text` ⇒ `Text` · `tool_use` ⇒ `Call` · `tool_result` ⇒ `Result` · `thinking` / `redacted_thinking` ⇒ `Thinking` · `image` ⇒ `Image` |
//! | assistant 的 `isApiErrorMessage` · `message.stop_reason == "end_turn"` | `Stop(Failed)` · `Stop(Done)` |
//! | `system` ＋ `subtype == "api_error"` | `Retry` |
//! | 「谁说的」（`text.user` 那一格，同一个函数） | `Said` |
//! | assistant 的 `message.usage` ＋ `requestId` · `message.model` | `usage` · `model` |
//! | `cost-state{totalCostUSD, hasUnknownModelCost}` · `permission-mode{permissionMode}` | `cost` · `mode` |
//! | user / assistant 的 `forkedFrom` | `forked_from`（[`super::branch::fork_origin`]） |

use std::borrow::Cow;

use serde_json::Value;

use crate::agents::beat::{
    contains, one_line, Act, Beat, CallAsk, CostSaid, Side, StopWhy, Usage, Watch,
};

/// 写类工具 → 取路径的键。Edit / Write / MultiEdit 用 `file_path`；NotebookEdit 用 `notebook_path`。
/// 与渲染那边的「写类」（卡型 `diff`，[`super::cards`] 那张写类工具表，行级 diff）**不是同一个问题**：
/// 这里多一个 NotebookEdit（它改文件、但不按行 diff 渲染）。
pub(crate) const EDIT_TOOL_PATH_KEYS: &[(&str, &str)] = &[
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
];

/// 交互工具：在等**回答**的 · 在等**批准计划**的。
const ANSWER_TOOLS: &[&str] = &["AskUserQuestion"];
const PLAN_TOOLS: &[&str] = &["ExitPlanMode"];

/// 工具 → 「它在做什么」那一格取哪个入参（一行人话的主参数）。没登记的工具 ⇒ 不给（界面只写工具名）。
const WHAT_KEYS: &[(&str, &str)] = &[
    ("Bash", "command"),
    ("Read", "file_path"),
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
    ("Grep", "pattern"),
    ("Glob", "pattern"),
    ("WebFetch", "url"),
    ("WebSearch", "query"),
    ("Task", "description"),
    ("Agent", "description"),
];

/// 注册表 `RecordFace.beats` 那一格。
pub(crate) const BEATS: crate::agents::beat::BeatFace = crate::agents::beat::BeatFace {
    beat: beat_of,
    hint: could_matter,
    ask: ask_of,
};

fn ask_of(tool: &str) -> CallAsk {
    if ANSWER_TOOLS.contains(&tool) {
        CallAsk::Answer
    } else if PLAN_TOOLS.contains(&tool) {
        CallAsk::Plan
    } else {
        CallAsk::Plain
    }
}

/// 一个工具调用的主参数一行（[`WHAT_KEYS`]；提问工具取第一问）。
pub(crate) fn what_of(name: &str, input: Option<&Value>) -> Option<String> {
    let input = input?;
    if ANSWER_TOOLS.contains(&name) {
        return input
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|q| q.first())
            .and_then(|q| q.get("question"))
            .and_then(Value::as_str)
            .and_then(one_line);
    }
    let key = WHAT_KEYS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, k)| *k)?;
    input.get(key).and_then(Value::as_str).and_then(one_line)
}

fn edits_of<'a>(name: &str, input: Option<&'a Value>) -> Option<Cow<'a, str>> {
    let key = EDIT_TOOL_PATH_KEYS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, k)| *k)?;
    input?
        .get(key)
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(Cow::Borrowed)
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// 一条已解析的记录 ⇒ 一拍（恒有：认不出的类型就是什么都没说的簿记）。
pub(crate) fn beat_of(v: &Value) -> Option<Beat<'_>> {
    let kind = str_of(v, "type");
    let side = match kind {
        Some("assistant") => Side::Reply,
        Some("user") => Side::Input,
        _ => Side::Note,
    };
    let msg = v.get("message");
    let mut acts = Vec::new();
    let content = msg
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));
    match content {
        Some(Value::String(s)) => acts.push(Act::Text(Cow::Borrowed(s))),
        Some(Value::Array(blocks)) => {
            let tur = v.get("toolUseResult");
            for b in blocks {
                match str_of(b, "type") {
                    Some("text") => {
                        if let Some(s) = str_of(b, "text") {
                            acts.push(Act::Text(Cow::Borrowed(s)));
                        }
                    }
                    Some("tool_use") => {
                        let name = str_of(b, "name").unwrap_or_default();
                        let input = b.get("input");
                        acts.push(Act::Call {
                            id: Cow::Borrowed(str_of(b, "id").unwrap_or_default()),
                            name: Cow::Borrowed(name),
                            what: what_of(name, input),
                            edits: edits_of(name, input),
                        });
                    }
                    Some("tool_result") => acts.push(Act::Result {
                        of: str_of(b, "tool_use_id").map(Cow::Borrowed),
                        failed: b.get("is_error").and_then(Value::as_bool) == Some(true)
                            && !super::steps::result_of(b, tur).rejected,
                    }),
                    Some("thinking" | "redacted_thinking") => acts.push(Act::Thinking),
                    Some("image") => acts.push(Act::Image),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    let mut usage = None;
    let mut model = None;
    if side == Side::Reply {
        if v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
            acts.push(Act::Stop(StopWhy::Failed));
        }
        if msg.and_then(|m| str_of(m, "stop_reason")) == Some("end_turn") {
            acts.push(Act::Stop(StopWhy::Done));
        }
        usage = msg
            .and_then(|m| m.get("usage"))
            .filter(|u| u.is_object())
            .map(|u| usage_of(u, v));
        model = msg.and_then(|m| str_of(m, "model")).map(Cow::Borrowed);
    }
    if kind == Some("system") && str_of(v, "subtype") == Some("api_error") {
        acts.push(Act::Retry);
    }
    if let Some(said) = super::text::user_text_of_record(v) {
        acts.push(Act::Said(said));
    }
    let cost = (kind == Some("cost-state"))
        .then(|| {
            let usd = v
                .get("totalCostUSD")
                .and_then(Value::as_f64)
                .filter(|x| x.is_finite() && *x >= 0.0)?;
            Some(CostSaid {
                micros: (usd * 1e6).round() as u64,
                partial: v.get("hasUnknownModelCost").and_then(Value::as_bool) == Some(true),
            })
        })
        .flatten();
    let mode = (kind == Some("permission-mode"))
        .then(|| str_of(v, "permissionMode").map(Cow::Borrowed))
        .flatten();
    Some(Beat {
        id: str_of(v, "uuid").map(Cow::Borrowed),
        at: str_of(v, "timestamp").map(Cow::Borrowed),
        side,
        injected: side == Side::Input && v.get("isMeta").and_then(Value::as_bool) == Some(true),
        acts,
        usage,
        model,
        cost,
        mode,
        forked_from: super::branch::fork_origin(v).map(|(sid, _)| sid),
    })
}

/// `message.usage` ＋ 记录的 `requestId` ⇒ 用量（写缓存两档照 `cache_creation` 有就给）。
fn usage_of<'a>(u: &Value, v: &'a Value) -> Usage<'a> {
    let num = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    let tier = |k: &str| {
        u.get("cache_creation")
            .and_then(|c| c.get(k))
            .and_then(Value::as_u64)
    };
    Usage {
        input: num("input_tokens"),
        cache_read: num("cache_read_input_tokens"),
        cache_write: num("cache_creation_input_tokens"),
        write_5m: tier("ephemeral_5m_input_tokens"),
        write_1h: tier("ephemeral_1h_input_tokens"),
        output: num("output_tokens"),
        request: str_of(v, "requestId").map(Cow::Borrowed),
    }
}

/// 说 MCP 样子的那种记录行里必有的字。
const MCP_MARK: &[u8] = b"\"deferred_tools_delta\"";

/// 这一行有没有可能改动会话事实（快路，`BeatFace.hint`）。**只许放过、不许误拦**：
/// 能改动事实的记录必然带着那几个键名（Claude Code 写 JSON 不转义 ASCII 字母）。
fn could_matter(line: &[u8], w: &Watch) -> bool {
    if w.mcp_only {
        return contains(line, MCP_MARK);
    }
    (!w.fork_locked && contains(line, b"\"forkedFrom\""))
        || contains(line, b"\"permission-mode\"")
        // 认不出、又不成卡的行（`quiet_unread`）：没有链上身份 ⇒ 行里没有 `"uuid"` 字样（簿记行都短）。
        || !contains(line, b"\"uuid\"")
        || contains(line, b"\"cost-state\"")
        || contains(line, b"\"usage\"")
        || contains(line, b"\"tool_use\"")
        || (contains(line, b"\"assistant\"") && contains(line, b"\"text\""))
        // 交回：记录级 `origin.handback`（键名在行里）。
        || contains(line, b"\"handback\"")
        // MCP 状态：「延后加载的工具变了」那种附件（行里带它的类型名）。
        || contains(line, MCP_MARK)
        // 重试：它本身（`api_error`）· 一串还没下文时，它后面的回复 / 人发的一句。
        || contains(line, b"\"api_error\"")
        || (w.retry_open && (contains(line, b"\"assistant\"") || contains(line, b"\"user\"")))
        // 后台命令：起它的调用 · 当场回的任务号 · 有没收场的 ⇒ 收场通知（三处都带着那个框）。
        || contains(line, b"\"run_in_background\"")
        || contains(line, b"\"backgroundTaskId\"")
        || (w.background && contains(line, b"task-notification"))
        // 有没结果的调用：又发了一句（`user` 记录、没有工具结果）。别人的工具结果（常是整份文件内容）照旧连解析都不做。
        || (w.pending && contains(line, b"\"user\"") && !contains(line, b"\"tool_result\""))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/beat_tests.rs"]
mod tests;
