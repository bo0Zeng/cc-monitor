//! Claude Code 的答题口（注册表 `Adapter.asks` 那一格）：会话级 `PermissionRequest` hook 的输入怎么读、答案怎么写、起会话时怎么挂上。
//!
//! 实测（2.1.296，调研报告「答批准不送键」§真机实测 · 补验）：交互会话里这个 hook 与终端框并行、谁先答算谁的；
//! 晚到的答案被丢、不会落到下一个框上；提问框要带 `updatedInput.answers` 才算答了；批计划要带原样 `updatedInput`，
//! 「批准并切档」经 `updatedPermissions` 的 `setMode`；子 agent 的请求多 `agent_id` · `agent_type`，后台那种是 hook 先跑完框才出。
//! exec 形必须写 `command`（只写 `args` 整份设置被跳过、开局还弹一个框）。
//!
//! 这一家的字段名只住这里；待答登记表、选项写字、谁先答算谁的住通用层（`observe::asks`）。

use crate::agents::{AskFace, AskIn, AskKind, AskQuestion, AskSuggestion, PlanMode, Verdict};
use copy_core::copy_text;
use serde_json::{json, Map, Value};

/// 注册表 `Adapter.asks` 那一格。
pub(crate) const FACE: AskFace = AskFace {
    launch_args,
    launch_flag: SETTINGS_FLAG,
    read,
    decide,
};

/// 会话级设置那个旗标（`--settings <文件或 JSON 串>`：hooks 与各层设置合并，不替换用户自己的）。
pub(crate) const SETTINGS_FLAG: &str = "--settings";
/// 唯一挂的那一种 hook。
const EVENT: &str = "PermissionRequest";
/// hook 的期限（秒）：到点它被杀、不给决定 ⇒ 框照常在终端里（挡着终端的那种到点才出框）。
pub(crate) const HOOK_TIMEOUT_SECS: u64 = 3600;
/// hook 那一趟叫 ccm 的哪几个词（后端子命令 ＋ 这一家的 kind）。
pub(crate) fn hook_words() -> [&'static str; 3] {
    ["--", crate::ANSWER_HOOK_FLAG, super::AGENT_KIND]
}

/// 起会话时垫的那两个词：`--settings` ＋ 只含这一个 hook 的 JSON 串（exec 形，不经 shell；`entry` ＝ ccm 绝对路径）。
pub(crate) fn launch_args(entry: &str) -> Vec<String> {
    let hook = json!({
        "type": "command",
        "command": entry,
        "args": hook_words(),
        "timeout": HOOK_TIMEOUT_SECS,
    });
    let settings = json!({ "hooks": { EVENT: [ { "hooks": [hook] } ] } });
    vec![SETTINGS_FLAG.to_string(), settings.to_string()]
}

const ASK_TOOL: &str = "AskUserQuestion";
const PLAN_TOOL: &str = "ExitPlanMode";

/// hook 的 stdin ⇒ 通用待答请求。不是 `PermissionRequest` · 缺会话 id / 工具名 ⇒ `None`。
pub(crate) fn read(v: &Value) -> Option<AskIn> {
    if v.get("hook_event_name").and_then(Value::as_str) != Some(EVENT) {
        return None;
    }
    let sid = nonempty(v.get("session_id"))?;
    let tool = nonempty(v.get("tool_name"))?;
    let input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    let kind = match tool.as_str() {
        ASK_TOOL => AskKind::Question,
        PLAN_TOOL => AskKind::Plan,
        _ => AskKind::Approve,
    };
    let questions = if kind == AskKind::Question {
        questions_of(&input)
    } else {
        Vec::new()
    };
    let suggestions = v
        .get("permission_suggestions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(suggestion_of).collect())
        .unwrap_or_default();
    let agent_id = nonempty(v.get("agent_id"));
    Some(AskIn {
        sid,
        kind,
        what: what_of(&tool, &input, &questions),
        detail: detail_of(&tool, &input, &questions),
        tool,
        suggestions,
        questions,
        // 子 agent 的请求：这一版里我们起的会话子 agent 全在后台（fork 开关默认开着，Agent 一律后台），
        // 后台那种是 hook 先跑完框才出 ⇒ 挡着终端。hook 输入分不出前后台，带 `agent_id` 的一律按挡着算。
        blocks_terminal: agent_id.is_some(),
        agent_id,
        agent_type: nonempty(v.get("agent_type")),
        input,
    })
}

fn nonempty(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn questions_of(input: &Value) -> Vec<AskQuestion> {
    // 与过程里那张提问卡同一处读（形状不对 ⇒ 没有题，答不了，通用层只给「拒绝」）。
    match super::steps::ask_of(ASK_TOOL, input) {
        Some(crate::agents::StepAsk::Questions { questions }) => questions,
        _ => Vec::new(),
    }
}

fn str_of<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

/// 钉条里那一段：命令头一行 · 文件路径 · 第一道题；说不出 ⇒ `None`。
fn what_of(tool: &str, input: &Value, questions: &[AskQuestion]) -> Option<String> {
    let first_line = |s: &str| s.lines().next().map(str::trim).map(str::to_string);
    match tool {
        ASK_TOOL => questions.first().map(|q| q.question.clone()),
        PLAN_TOOL => None,
        _ => [
            "command",
            "file_path",
            "notebook_path",
            "url",
            "pattern",
            "path",
        ]
        .iter()
        .find_map(|k| str_of(input, k))
        .and_then(first_line)
        .filter(|s| !s.is_empty()),
    }
}

/// 要批的东西的全文（不截）：命令全文 · 写哪个文件写成什么 · 改哪里改成什么 · 计划原文；认不出的工具 ⇒ 参数原样排开。
fn detail_of(tool: &str, input: &Value, questions: &[AskQuestion]) -> String {
    let path = str_of(input, "file_path").or_else(|| str_of(input, "notebook_path"));
    match tool {
        PLAN_TOOL => str_of(input, "plan").unwrap_or_default().to_string(),
        ASK_TOOL => questions
            .iter()
            .map(|q| q.question.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        "Bash" if str_of(input, "command").is_some() => {
            str_of(input, "command").unwrap_or_default().to_string()
        }
        "Write" if path.is_some() => format!(
            "{}\n\n{}",
            path.unwrap_or_default(),
            str_of(input, "content").unwrap_or_default()
        ),
        "Edit" if path.is_some() => format!(
            "{}\n\n- {}\n+ {}",
            path.unwrap_or_default(),
            str_of(input, "old_string").unwrap_or_default(),
            str_of(input, "new_string").unwrap_or_default()
        ),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// 一条「以后不再问」建议写成人话；不是放宽的那几种（删规则 · 删目录）⇒ 不给。
fn suggestion_of(s: &Value) -> Option<AskSuggestion> {
    let scope = match s.get("destination").and_then(Value::as_str) {
        Some("session") => copy_text("beAsk.scope.session", &[]),
        Some("localSettings") => copy_text("beAsk.scope.local", &[]),
        Some("projectSettings") => copy_text("beAsk.scope.project", &[]),
        Some("userSettings") => copy_text("beAsk.scope.user", &[]),
        _ => return None,
    };
    let text = match s.get("type").and_then(Value::as_str)? {
        "addDirectories" => {
            let dirs: Vec<&str> = s
                .get("directories")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if dirs.is_empty() {
                return None;
            }
            copy_text(
                "beAsk.rule.dirs",
                &[("dirs", &dirs.join("、")), ("scope", &scope)],
            )
        }
        "setMode" => match s.get("mode").and_then(Value::as_str)? {
            "acceptEdits" => copy_text("beAsk.rule.acceptEdits", &[("scope", &scope)]),
            m => copy_text("beAsk.rule.mode", &[("mode", m), ("scope", &scope)]),
        },
        "addRules" | "replaceRules"
            if s.get("behavior").and_then(Value::as_str) == Some("allow") =>
        {
            let rules: Vec<String> = s
                .get("rules")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(|r| {
                    let t = r.get("toolName").and_then(Value::as_str)?;
                    Some(match r.get("ruleContent").and_then(Value::as_str) {
                        Some(c) if !c.is_empty() => format!("{t}({c})"),
                        _ => t.to_string(),
                    })
                })
                .collect();
            if rules.is_empty() {
                return None;
            }
            copy_text(
                "beAsk.rule.rules",
                &[("rules", &rules.join("、")), ("scope", &scope)],
            )
        }
        _ => return None,
    };
    Some(AskSuggestion {
        text,
        raw: s.clone(),
    })
}

/// 人的答案 ⇒ hook 打到 stdout 的那一整份。
pub(crate) fn decide(ask: &AskIn, verdict: &Verdict) -> Option<Value> {
    let mut d = Map::new();
    match verdict {
        Verdict::Allow {
            rule,
            mode,
            answers,
        } => {
            d.insert("behavior".into(), json!("allow"));
            match ask.kind {
                // 提问：原样题目 ＋ 每题的答案（多选已由通用层用「, 」连好）；不带 `updatedInput` 那一家当没答。
                AskKind::Question => {
                    let mut input = ask.input.as_object().cloned().unwrap_or_default();
                    let a: Map<String, Value> =
                        answers.iter().map(|(q, v)| (q.clone(), json!(v))).collect();
                    input.insert("answers".into(), Value::Object(a));
                    d.insert("updatedInput".into(), Value::Object(input));
                }
                // 批计划：原样计划（同样不带就当没答）＋ 切到哪一档。
                AskKind::Plan => {
                    d.insert("updatedInput".into(), ask.input.clone());
                    let m = match mode.unwrap_or(PlanMode::Manual) {
                        PlanMode::Auto => "auto",
                        PlanMode::AcceptEdits => "acceptEdits",
                        PlanMode::Manual => "default",
                    };
                    d.insert(
                        "updatedPermissions".into(),
                        json!([{ "type": "setMode", "mode": m, "destination": "session" }]),
                    );
                }
                AskKind::Approve => {
                    if let Some(s) = rule.and_then(|i| ask.suggestions.get(i)) {
                        d.insert("updatedPermissions".into(), json!([s.raw]));
                    }
                }
            }
        }
        Verdict::Deny { note, stop } => {
            d.insert("behavior".into(), json!("deny"));
            if let Some(n) = note.as_deref().filter(|n| !n.trim().is_empty()) {
                d.insert("message".into(), json!(n));
            }
            if *stop {
                d.insert("interrupt".into(), json!(true));
            }
        }
    }
    Some(json!({ "hookSpecificOutput": { "hookEventName": EVENT, "decision": Value::Object(d) } }))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/asks_tests.rs"]
mod tests;
