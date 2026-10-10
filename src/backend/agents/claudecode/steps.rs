//! Claude 的**过程一步一行**：一次工具调用的主参数与说明（[`step_of`]）·
//! 它的结果一句（[`result_of`]，读了几行 / `+N −M` / 命中几个文件 / 被拒 / 提问与计划答了什么）·
//! 报错与重试的原因种类（[`api_reason`]）。
//!
//! 这些是 Claude Code 的格式知识（入参字段名 · `toolUseResult` 的形状 · 固定英文句），只住这里；
//! 解析时填进记录成品（`schema.rs`：assistant 的 `toolSteps` · user 的 `toolResults` · 报错的 `apiReason`），界面只排版。
//! 读不出的格就缺，不猜。

use crate::agents::{
    Answer, ApiReason, AskOption, AskQuestion, PatchHunk, StepAsk, StepResult, ToolStep,
};

/// 一次改动的 diff 至多带多少字（各段行正文的字数之和）：超了就只给前几段、立 `patchTruncated`，**整段不劈**；
/// 放不下的段一律不给（连第一段也不例外 —— 不然它就不是上界）。本机 1,139 份记录实测：32 KiB 让 98.99% 的改动整份给全。
const PATCH_MAX: usize = 32 * 1024;

/// 原文的逐段改动 ⇒ 段 ＋ 全不全。形状读不出的段跳过，也算「不全」。
fn patch_of(hunks: &[Value]) -> (Vec<PatchHunk>, bool) {
    let num = |h: &Value, k: &str| {
        h.get(k)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    let mut out = Vec::new();
    let mut used = 0usize;
    let mut skipped = false;
    for h in hunks {
        let Some(lines) = h.get("lines").and_then(Value::as_array) else {
            skipped = true;
            continue;
        };
        let lines: Vec<String> = lines
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let cost = lines.iter().map(String::len).sum::<usize>();
        if used + cost > PATCH_MAX {
            return (out, true);
        }
        let (Some(old_start), Some(old_lines), Some(new_start), Some(new_lines)) = (
            num(h, "oldStart"),
            num(h, "oldLines"),
            num(h, "newStart"),
            num(h, "newLines"),
        ) else {
            skipped = true;
            continue;
        };
        used += cost;
        out.push(PatchHunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            lines,
        });
    }
    (out, skipped)
}
use serde_json::Value;

/// 主参数是路径的工具（入参 `file_path` / `notebook_path`）。
const PATH_TOOLS: &[(&str, &str)] = &[
    ("Read", "file_path"),
    ("Edit", "file_path"),
    ("MultiEdit", "file_path"),
    ("Write", "file_path"),
    ("NotebookEdit", "notebook_path"),
    ("NotebookRead", "notebook_path"),
];
/// 主参数是别的一格的工具（工具名 ⇒ 入参字段）。
const ARG_TOOLS: &[(&str, &str)] = &[
    ("Bash", "command"),
    ("Grep", "pattern"),
    ("Glob", "pattern"),
    ("WebFetch", "url"),
    ("WebSearch", "query"),
    ("Agent", "description"),
    ("Task", "description"),
    ("Skill", "skill"),
    ("BashOutput", "bash_id"),
    ("KillShell", "shell_id"),
    ("SlashCommand", "command"),
];
/// 认得、但没有主参数的工具（只写工具名）。
const BARE_TOOLS: &[&str] = &["TodoWrite", "ExitPlanMode", "AskUserQuestion", "LS"];
/// 说明那一格（Bash 的 `description`）。
const NOTE_FIELD: &str = "description";
/// 主参数至多留多少字（一行；协议上的定长，两个前端都不再截）。
const ARG_MAX: usize = 200;

/// 一次工具调用 ⇒ 它的一行人话（工具名 · 主参数 · 说明 · 认不认得）。
pub(crate) fn step_of(name: &str, input: &Value) -> ToolStep {
    let field = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(one_line)
            .filter(|s| !s.is_empty())
    };
    if let Some((_, k)) = PATH_TOOLS.iter().find(|(t, _)| *t == name) {
        return ToolStep {
            tool: name.to_string(),
            arg: field(k),
            path: true,
            note: None,
            known: true,
            ask: None,
        };
    }
    if let Some((_, k)) = ARG_TOOLS.iter().find(|(t, _)| *t == name) {
        // Agent / Task 的主参数就是它的说明 ⇒ 说明格不再重复。
        let note = (*k != NOTE_FIELD).then(|| field(NOTE_FIELD)).flatten();
        return ToolStep {
            tool: name.to_string(),
            arg: field(k),
            path: false,
            note,
            known: true,
            ask: None,
        };
    }
    ToolStep {
        tool: name.to_string(),
        arg: None,
        path: false,
        note: None,
        known: BARE_TOOLS.contains(&name),
        ask: ask_of(name, input),
    }
}

/// 提问 / 计划那一格（[`ToolStep::ask`]）：`AskUserQuestion` 的 `questions[]`（`header` · `question` · `multiSelect` · `options[]{label, description}`）·
/// `ExitPlanMode` 的 `plan`。形状不对（没有题 · 题没有选项表 · 选项没有名 · 计划是空的）⇒ `None`。
fn ask_of(name: &str, input: &Value) -> Option<StepAsk> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    match name {
        "AskUserQuestion" => {
            let qs = input
                .get("questions")?
                .as_array()
                .filter(|q| !q.is_empty())?;
            let questions = qs
                .iter()
                .map(|q| {
                    let options = q
                        .get("options")?
                        .as_array()?
                        .iter()
                        .map(|o| {
                            Some(AskOption {
                                label: text(o, "label")?,
                                description: text(o, "description").filter(|d| !d.is_empty()),
                            })
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(AskQuestion {
                        header: text(q, "header").filter(|h| !h.is_empty()),
                        question: text(q, "question")?,
                        multi: q.get("multiSelect").and_then(Value::as_bool) == Some(true),
                        options,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(StepAsk::Questions { questions })
        }
        "ExitPlanMode" => text(input, "plan")
            .filter(|p| !p.trim().is_empty())
            .map(|text| StepAsk::Plan { text }),
        _ => None,
    }
}

/// 结果首行预览至多几个字（按字符；协议上的定长，两个前端都不再截）。
const PREVIEW_MAX: usize = 60;

/// 第一条非空行、去掉两头空白、至多 [`PREVIEW_MAX`] 字，截了以「…」收尾（留 59 字 ＋「…」）；没有非空行 ⇒ `None`。
fn preview_of(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(match line.char_indices().nth(PREVIEW_MAX) {
        Some(_) => {
            let cut = line
                .char_indices()
                .nth(PREVIEW_MAX - 1)
                .map_or(line.len(), |(i, _)| i);
            format!("{}…", &line[..cut])
        }
        None => line.to_string(),
    })
}

/// 压成一行（换行与连串空白 ⇒ 一个空格）并截到 [`ARG_MAX`] 字。
fn one_line(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.char_indices().nth(ARG_MAX) {
        Some((i, _)) => format!("{}…", &joined[..i]),
        None => joined,
    }
}

/// 人拒了这一步时 Claude Code 回给模型的固定句（结果正文的开头）。
const REJECT_LEADS: &[&str] = &[
    "The user doesn't want to proceed with this tool use",
    "User rejected tool use",
];
/// 计划批准后的固定句（老版本没有 `toolUseResult.plan` 时靠它）。
const PLAN_APPROVED_LEAD: &str = "User has approved your plan";
/// 提问答了之后的固定句：`User has answered your questions: "问"="答", "问"="答". …`
const ANSWERED_LEAD: &str = "User has answered your questions:";

/// 一个 `tool_result` 块（＋ 记录级 `toolUseResult`）⇒ 结果一句。
pub(crate) fn result_of(block: &Value, tur: Option<&Value>) -> StepResult {
    let text = super::text::stringify_json(block.get("content").unwrap_or(&Value::Null));
    let is_error = block.get("is_error").and_then(Value::as_bool) == Some(true);
    let mut r = StepResult {
        ok: !is_error,
        preview: preview_of(&text),
        chars: (!text.is_empty()).then(|| u32::try_from(text.chars().count()).unwrap_or(u32::MAX)),
        ..StepResult::default()
    };
    let tur_text = tur.and_then(Value::as_str).unwrap_or("");
    if REJECT_LEADS
        .iter()
        .any(|l| text.trim_start().starts_with(l) || tur_text.starts_with(l))
    {
        r.ok = false;
        r.rejected = true;
        return r;
    }
    if is_error {
        r.exit_code = exit_code_of(&text);
        return r;
    }
    let obj = tur.filter(|v| v.is_object());
    let num = |v: Option<&Value>| {
        v.and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    if let Some(t) = obj {
        // 计划：结果里带着计划本身 ⇒ 批准了。提问：`answers` 是 问 ⇒ 答。
        if t.get("plan").is_some_and(Value::is_string) {
            r.answer = Some(Answer::Approved);
        } else if let Some(a) = t.get("answers").and_then(Value::as_object) {
            r.answer = Some(Answer::Picked {
                options: a
                    .values()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            });
        }
        // 读文件：`file.numLines`。
        r.lines =
            num(t.get("file").and_then(|f| f.get("numLines"))).or_else(|| num(t.get("numLines")));
        r.files = num(t.get("numFiles"));
        // 改文件：`structuredPatch` 里每段的 `lines` 以 `+` / `-` 起头的行数。
        // 空表当没有：新建整份文件时原文写的是 `"structuredPatch": []`，照「有这一格」走就报成 `+0 −0`，
        // 下面「整份都是加的」那一支永远走不到。
        let hunks = t
            .get("structuredPatch")
            .and_then(Value::as_array)
            .filter(|h| !h.is_empty());
        if let Some(hunks) = hunks {
            let (mut add, mut del) = (0u32, 0u32);
            for l in hunks
                .iter()
                .filter_map(|h| h.get("lines").and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
            {
                if l.starts_with('+') {
                    add += 1;
                } else if l.starts_with('-') {
                    del += 1;
                }
            }
            r.added = Some(add);
            r.removed = Some(del);
            let (patch, truncated) = patch_of(hunks);
            r.patch = (!patch.is_empty()).then_some(patch);
            r.patch_truncated = truncated;
            r.file = t
                .get("filePath")
                .and_then(Value::as_str)
                .map(str::to_string);
        } else if t.get("type").and_then(Value::as_str) == Some("create") {
            // 新建的文件：整份都是加的；没有「改之前」⇒ 不给 diff，只说哪个文件。
            if let Some(c) = t.get("content").and_then(Value::as_str) {
                r.added = Some(c.lines().count() as u32);
                r.removed = Some(0);
                r.file = t
                    .get("filePath")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        // 命令：输出几行（stdout ＋ stderr）。
        if r.lines.is_none() && (t.get("stdout").is_some() || t.get("stderr").is_some()) {
            let n = ["stdout", "stderr"]
                .iter()
                .filter_map(|k| t.get(*k).and_then(Value::as_str))
                .map(|s| s.lines().count())
                .sum::<usize>();
            r.lines = Some(n as u32);
        }
    }
    if r.answer.is_none() {
        let t = text.trim_start();
        if t.starts_with(PLAN_APPROVED_LEAD) {
            r.answer = Some(Answer::Approved);
        } else if let Some(rest) = t.strip_prefix(ANSWERED_LEAD) {
            let options = answered_pairs(rest);
            if !options.is_empty() {
                r.answer = Some(Answer::Picked { options });
            }
        }
    }
    r
}

/// 失败的命令结果里那一行 `Exit code N` ⇒ N（Claude Code 的写法；没有 ⇒ `None`）。
fn exit_code_of(text: &str) -> Option<i32> {
    let rest = &text[text.find("Exit code ")? + "Exit code ".len()..];
    let n = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[..n].parse().ok()
}

/// `"问"="答", "问"="答"` ⇒ 各个答（引号里不含引号；读不出 ⇒ 空）。
fn answered_pairs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("\"=\"") {
        let after = &rest[i + 3..];
        let Some(j) = after.find('"') else { break };
        out.push(after[..j].to_string());
        rest = &after[j + 1..];
    }
    out
}

/// 一条报错 / 重试（状态码 · `error` 那一格 · 报错正文）⇒ 原因种类。认不出 ⇒ `Unknown`，不猜。
///
/// 次序：额度满（429 · rate_limit · usage limit）→ 登录（401 / 403 · authentication · OAuth）→ 上下文超长（prompt is too long）
/// → 服务器过载（529 · overloaded · 5xx）→ 网络（有连接错误 / 没有状态码的连接类原文）→ 原因不明。
pub(crate) fn api_reason(status: Option<u32>, error: Option<&Value>, text: &str) -> ApiReason {
    let mut hay = text.to_ascii_lowercase();
    let mut status = status;
    let mut connection = false;
    if let Some(e) = error {
        hay.push(' ');
        hay.push_str(&e.to_string().to_ascii_lowercase());
        status = status.or_else(|| {
            e.get("status")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
        });
        connection = e.get("connection").is_some_and(|c| !c.is_null());
    }
    let has = |ks: &[&str]| ks.iter().any(|k| hay.contains(k));
    if status == Some(429) || has(&["rate_limit", "usage limit", "rate limit"]) {
        return ApiReason::Quota;
    }
    if matches!(status, Some(401 | 403))
        || has(&[
            "authentication",
            "oauth",
            "invalid api key",
            "please run /login",
        ])
    {
        return ApiReason::Auth;
    }
    if has(&[
        "prompt is too long",
        "context length",
        "context window",
        "too many tokens",
    ]) {
        return ApiReason::Context;
    }
    if status.is_some_and(|s| (500..600).contains(&s)) || has(&["overloaded"]) {
        return ApiReason::Overloaded;
    }
    if connection
        || has(&[
            "econnreset",
            "econnrefused",
            "etimedout",
            "enotfound",
            "socket hang up",
            "connection error",
            "network",
            "fetch failed",
        ])
    {
        return ApiReason::Network;
    }
    ApiReason::Unknown
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/steps_tests.rs"]
mod tests;
