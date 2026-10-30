//! Claude 的**过程一步一行**：一次工具调用的主参数与说明、展开要画的改动 / 命令 / 文件（[`step_of`]）·
//! 它的结果一句（[`result_of`]，读了几行 / `+N −M` / 命中几个文件 / 被拒 / 提问与计划答了什么）·
//! 报错与重试的原因种类（[`api_reason`]）。
//!
//! 这些是 Claude Code 的格式知识（入参字段名 · `toolUseResult` 的形状 · 固定英文句），只住这里；
//! 解析时填进记录成品（`schema.rs`：assistant 的 `toolSteps` · user 的 `toolResults` · 报错的 `apiReason`），界面只排版。
//! 读不出的格就缺，不猜。

use super::asks::{ASK_TOOL, INTERACTIVE_TOOLS, PLAN_TOOL};
use crate::agents::PlanTitleExt as _;
use crate::agents::StepResultExt as _;
use crate::agents::{
    Answer, ApiReason, AskOption, AskQuestion, DiffKind, DiffRow, LineDiff, StepAsk, StepBody,
    StepResult, ToolStep,
};
use serde_json::Value;

/// 一次改动的行差至多带多少字（各段行正文的字数之和；调用那一份 · 结果那一份同一个上界）：超了就只给前几段、立 `truncated`，
/// **整段不劈**；放不下的段一律不给（连第一段也不例外 —— 不然它就不是上界）。本机 1,139 份记录实测：32 KiB 让 98.99% 的改动整份给全。
const PATCH_MAX: usize = 32 * 1024;
/// 行差收着时先画几行（两端同一个数）。
const DIFF_SHOWN: usize = 400;
/// 行差里一行至多留多少字（按字符），截了以「…」收尾：压缩过的单行几十万字不进界面。
const DIFF_LINE_MAX: usize = 2000;
/// 从改前改后逐行比的格数上界（改前行数 × 改后行数）：超了不比，出整删 ＋ 整增（数照这一形）。
const DIFF_CELLS_MAX: usize = 4_000_000;

/// 一行正文进行差的样子：去掉行尾的 `\r`，过长截到 [`DIFF_LINE_MAX`] 字。
fn diff_cell(text: &str) -> String {
    let t = text.strip_suffix('\r').unwrap_or(text);
    match t.char_indices().nth(DIFF_LINE_MAX) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t.to_string(),
    }
}

fn diff_row(kind: DiffKind, text: String, old_no: Option<u32>, new_no: Option<u32>) -> DiffRow {
    DiffRow {
        kind,
        text,
        old_no,
        new_no,
    }
}

/// 攒好的行 ＋ 整份的增删数 ⇒ 行差：收着画几行、「+N −M」那一句（与过程那一行右侧那一句同一处写，[`StepResult::written`]）。
fn line_diff(rows: Vec<DiffRow>, added: u32, removed: u32, truncated: bool) -> LineDiff {
    let stat = StepResult {
        ok: true,
        added: Some(added),
        removed: Some(removed),
        ..StepResult::default()
    }
    .written(false)
    .text;
    LineDiff {
        shown: u32::try_from(rows.len().min(DIFF_SHOWN)).unwrap_or(u32::MAX),
        rows,
        truncated,
        added,
        removed,
        stat,
    }
}

/// 结果里的逐段改动（`structuredPatch`）⇒ 整份的增删数 ＋ 行差（真行号从各段的 `oldStart` / `newStart` 起算；
/// 行头 `+` 加的 · `-` 删的 · ` ` 没变 · `\`（`\ No newline at end of file`）不占行 · 别的当没变原样）。
/// 数照整份；行到 [`PATCH_MAX`] 在段的边界停下；形状读不出的段跳过，也算「不全」；一段也没放下 ⇒ 行差缺。
fn patch_of(hunks: &[Value]) -> (u32, u32, Option<LineDiff>) {
    let num = |h: &Value, k: &str| {
        h.get(k)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    let multi = hunks.len() > 1;
    let (mut added, mut removed) = (0u32, 0u32);
    let mut rows = Vec::new();
    let mut used = 0usize;
    let (mut truncated, mut full) = (false, false);
    for h in hunks {
        let Some(lines) = h.get("lines").and_then(Value::as_array) else {
            truncated = true;
            continue;
        };
        let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
        added += lines.iter().filter(|l| l.starts_with('+')).count() as u32;
        removed += lines.iter().filter(|l| l.starts_with('-')).count() as u32;
        let cost = lines.iter().map(|l| l.len()).sum::<usize>();
        full = full || used + cost > PATCH_MAX;
        let starts = (
            num(h, "oldStart"),
            num(h, "oldLines"),
            num(h, "newStart"),
            num(h, "newLines"),
        );
        let (false, Some(old_start), Some(old_lines), Some(new_start), Some(new_lines)) =
            (full, starts.0, starts.1, starts.2, starts.3)
        else {
            truncated = true;
            continue;
        };
        used += cost;
        if multi {
            rows.push(diff_row(
                DiffKind::Head,
                format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
                None,
                None,
            ));
        }
        let (mut o, mut n) = (old_start, new_start);
        for l in lines {
            let mut chars = l.chars();
            match chars.next() {
                Some('+') => {
                    rows.push(diff_row(DiffKind::Add, diff_cell(chars.as_str()), None, Some(n)));
                    n += 1;
                }
                Some('-') => {
                    rows.push(diff_row(DiffKind::Del, diff_cell(chars.as_str()), Some(o), None));
                    o += 1;
                }
                Some('\\') => {}
                Some(' ') => {
                    rows.push(diff_row(DiffKind::Ctx, diff_cell(chars.as_str()), Some(o), Some(n)));
                    o += 1;
                    n += 1;
                }
                _ => {
                    rows.push(diff_row(DiffKind::Ctx, diff_cell(l), Some(o), Some(n)));
                    o += 1;
                    n += 1;
                }
            }
        }
    }
    let diff = (!rows.is_empty()).then(|| line_diff(rows, added, removed, truncated));
    (added, removed, diff)
}

/// 改前改后的原文 ⇒ 逐行的种类（行尾风格不算差别：`\r\n` · `\r` 当 `\n`，末尾恰好一个换行不成行）。
/// 一样 ⇒ 全是没变；改前空 ⇒ 全是加的；改后空 ⇒ 全是删的；行数之积超了 [`DIFF_CELLS_MAX`] ⇒ 整删 ＋ 整增；
/// 余下按最长公共子序列（迭代的表 ＋ 迭代回溯，上千行不靠调用栈），同长时删的排在加的前面。
fn line_ops(old: &str, new: &str) -> Vec<(DiffKind, String)> {
    let norm = |s: &str| {
        let t = s.replace("\r\n", "\n").replace('\r', "\n");
        match t.strip_suffix('\n') {
            Some(x) => x.to_string(),
            None => t,
        }
    };
    let (o, n) = (norm(old), norm(new));
    let split = |s: &str| -> Vec<String> {
        if s.is_empty() {
            Vec::new()
        } else {
            s.split('\n').map(str::to_string).collect()
        }
    };
    let (a, b) = (split(&o), split(&n));
    let all = |v: Vec<String>, k: DiffKind| v.into_iter().map(move |l| (k, l));
    if o == n {
        return all(a, DiffKind::Ctx).collect();
    }
    let (m, w) = (a.len(), b.len());
    if m == 0 || w == 0 || m.saturating_mul(w) > DIFF_CELLS_MAX {
        return all(a, DiffKind::Del).chain(all(b, DiffKind::Add)).collect();
    }
    let width = w + 1;
    let mut c = vec![0u32; (m + 1) * width];
    for i in 1..=m {
        for j in 1..=w {
            c[i * width + j] = if a[i - 1] == b[j - 1] {
                c[(i - 1) * width + j - 1] + 1
            } else {
                c[(i - 1) * width + j].max(c[i * width + j - 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(m + w);
    let (mut i, mut j) = (m, w);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            ops.push((DiffKind::Ctx, a[i - 1].clone()));
            i -= 1;
            j -= 1;
        } else if c[i * width + j - 1] >= c[(i - 1) * width + j] {
            ops.push((DiffKind::Add, b[j - 1].clone()));
            j -= 1;
        } else {
            ops.push((DiffKind::Del, a[i - 1].clone()));
            i -= 1;
        }
    }
    ops.extend(a[..i].iter().rev().map(|l| (DiffKind::Del, l.clone())));
    ops.extend(b[..j].iter().rev().map(|l| (DiffKind::Add, l.clone())));
    ops.reverse();
    ops
}

/// 入参里的几处改前改后 ⇒ 行差（不知道在文件第几行 ⇒ 行号缺）。几处时每处前一行段头；数照整份，行到 [`PATCH_MAX`] 在段的边界停下。
fn changes_of(changes: &[(String, String)]) -> LineDiff {
    let multi = changes.len() > 1;
    let (mut added, mut removed) = (0u32, 0u32);
    let mut rows = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for (i, (old, new)) in changes.iter().enumerate() {
        let ops = line_ops(old, new);
        added += ops.iter().filter(|(k, _)| *k == DiffKind::Add).count() as u32;
        removed += ops.iter().filter(|(k, _)| *k == DiffKind::Del).count() as u32;
        let cost = ops.iter().map(|(_, l)| l.len()).sum::<usize>();
        if truncated || used + cost > PATCH_MAX {
            truncated = true;
            continue;
        }
        used += cost;
        if multi {
            let n = (i + 1).to_string();
            rows.push(diff_row(
                DiffKind::Head,
                copy_core::copy_text("cards.diff.segment", &[("n", &n)]),
                None,
                None,
            ));
        }
        rows.extend(ops.into_iter().map(|(k, l)| diff_row(k, diff_cell(&l), None, None)));
    }
    line_diff(rows, added, removed, truncated)
}

/// 工具 ⇒ 主参数取哪个入参 · 它是不是路径。**「工具 → 主参数」只此一张**：消息流那一行（[`step_of`]）·
/// 没结果的调用那一行（`beat::what_of`）· 待答请求的钉条（`asks`）都查它，各按自己的长度截。
const MAIN_ARGS: &[(&str, &str, bool)] = &[
    ("Read", "file_path", true),
    ("Edit", "file_path", true),
    ("MultiEdit", "file_path", true),
    ("Write", "file_path", true),
    ("NotebookEdit", "notebook_path", true),
    ("NotebookRead", "notebook_path", true),
    ("Bash", "command", false),
    ("Grep", "pattern", false),
    ("Glob", "pattern", false),
    ("WebFetch", "url", false),
    ("WebSearch", "query", false),
    ("Agent", "description", false),
    ("Task", "description", false),
    ("Skill", "skill", false),
    ("BashOutput", "bash_id", false),
    ("KillShell", "shell_id", false),
    ("SlashCommand", "command", false),
];

/// 这个工具的主参数取哪个入参 · 是不是路径（[`MAIN_ARGS`]）；没登记 ⇒ `None`。
pub(crate) fn main_arg(name: &str) -> Option<(&'static str, bool)> {
    MAIN_ARGS
        .iter()
        .find(|(t, _, _)| *t == name)
        .map(|&(_, k, path)| (k, path))
}
/// 跑 shell 命令的那个工具（主参数是一整条命令）。
const SHELL_TOOL: &str = "Bash";
/// 主参数原文里摆得出的那一段：跑命令的工具 ⇒ 说得出用意的那一段起（[`crate::agents::beat::command_intent`]）；别的工具原样。
/// 待答请求的钉条（`asks`）· 没结果的调用那一行（`beat::what_of`）都经它，各按自己的长度截。
pub(crate) fn main_text<'a>(tool: &str, text: &'a str) -> &'a str {
    if tool == SHELL_TOOL {
        crate::agents::beat::command_intent(text)
    } else {
        text
    }
}
/// 认得、但没有主参数的工具（只写工具名；提问 / 计划查 [`INTERACTIVE_TOOLS`]）。
const BARE_TOOLS: &[&str] = &["TodoWrite", "LS"];
/// 说明那一格（Bash 的 `description`）。
const NOTE_FIELD: &str = "description";
/// 主参数至多留多少字（一行；协议上的定长，两个前端都不再截）。
const ARG_MAX: usize = 200;

/// 一次工具调用 ⇒ 它的一行人话（工具名 · 主参数 · 说明 · 认不认得）与展开要画的东西（[`body_of`]）。
pub(crate) fn step_of(name: &str, input: &Value) -> ToolStep {
    let mut s = named(name, input);
    s.text = (!s.known)
        .then(|| crate::common::cells::Words(copy_core::copy_text("stream.step.unknown", &[])));
    s.body = body_of(name, input);
    s
}

/// 展开那一格：写类工具的改前改后（`Edit` 的 `old_string` / `new_string` · `Write` 的 `content` · `MultiEdit` 的 `edits[]`）·
/// 命令的整行（`command`）。形状不对（缺格 · 非字符串 · 改几处那张表空或有一项不对）⇒ `None`。
fn body_of(name: &str, input: &Value) -> Option<StepBody> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let change = |v: &Value| Some((text(v, "old_string")?, text(v, "new_string")?));
    let changes = match name {
        "Edit" => vec![change(input)?],
        "Write" => vec![(String::new(), text(input, "content")?)],
        "MultiEdit" => input
            .get("edits")?
            .as_array()
            .filter(|e| !e.is_empty())?
            .iter()
            .map(change)
            .collect::<Option<Vec<_>>>()?,
        "Bash" => {
            return text(input, "command")
                .filter(|c| !c.is_empty())
                .map(|line| StepBody::Command { line })
        }
        _ => return None,
    };
    // 一行也画不出（新写一份空文件 · 改前改后一样且为空）⇒ 缺，界面照原文画入参。
    Some(changes_of(&changes))
        .filter(|d| !d.rows.is_empty())
        .map(|diff| StepBody::Changes { diff })
}

/// [`step_of`] 的认法那一半（认不出的工具那一句由 [`step_of`] 添）。
fn named(name: &str, input: &Value) -> ToolStep {
    let field = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(one_line)
            .filter(|s| !s.is_empty())
    };
    if let Some((k, path)) = main_arg(name) {
        // 路径类没有说明格；Agent / Task 的主参数就是它的说明 ⇒ 说明格不再重复。
        let note = (!path && k != NOTE_FIELD)
            .then(|| field(NOTE_FIELD))
            .flatten();
        // 文件窗口按远端的 POSIX 路径定位：只给绝对路径（相对的会落到错的目录）。
        let file = input
            .get(k)
            .and_then(Value::as_str)
            .filter(|p| path && p.starts_with('/'))
            .map(str::to_string);
        return ToolStep {
            tool: name.to_string(),
            arg: field(k),
            path,
            note,
            file,
            body: None,
            known: true,
            text: None,
            ask: None,
        };
    }
    ToolStep {
        tool: name.to_string(),
        arg: None,
        path: false,
        note: None,
        file: None,
        body: None,
        known: BARE_TOOLS.contains(&name) || INTERACTIVE_TOOLS.contains(&name),
        text: None,
        ask: ask_of(name, input),
    }
}

/// 提问 / 计划那一格（[`ToolStep::ask`]）：`AskUserQuestion` 的 `questions[]`（`header` · `question` · `multiSelect` · `options[]{label, description}`）·
/// `ExitPlanMode` 的 `plan`。形状不对（没有题 · 题没有选项表 · 选项没有名 · 计划是空的）⇒ `None`。
pub(crate) fn ask_of(name: &str, input: &Value) -> Option<StepAsk> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    match name {
        ASK_TOOL => {
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
        PLAN_TOOL => text(input, "plan")
            .filter(|p| !p.trim().is_empty())
            .map(|text| StepAsk::Plan {
                text,
                title: crate::agents::PlanTitle::written(),
            }),
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
    let read = tur
        .and_then(|t| t.get("file"))
        .and_then(|f| f.get("numLines"))
        .is_some();
    counted(block, tur).written(read)
}

/// [`result_of`] 的数那一半（右侧那一句由 [`StepResult::written`] 写）。读文件那一类 ＝ 结果里有 `file.numLines`。
fn counted(block: &Value, tur: Option<&Value>) -> StepResult {
    let text = super::text::stringify_json(block.get("content").unwrap_or(&Value::Null));
    let is_error = block.get("is_error").and_then(Value::as_bool) == Some(true);
    let mut r = StepResult {
        ok: !is_error,
        preview: preview_of(&text),
        chars: (!text.is_empty()).then(|| u32::try_from(text.chars().count()).unwrap_or(u32::MAX)),
        kind: super::runs::result_kind(tur),
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
            let (add, del, diff) = patch_of(hunks);
            r.added = Some(add);
            r.removed = Some(del);
            r.patch = diff;
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
