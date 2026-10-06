//! **一轮的摘要**（主窗口稿 §5.2.2 过程行 · §5.2.6 刻度 · §7 B4）：从你的一句话到下一句话算一轮；
//! 每轮出边界 · 工具调用数 · 思考段数 · 失败数 · 起止时刻 · 结论是哪几条 · 你那句的第一行 · 回复头三行。
//!
//! 与大纲同一个来源：「你说的一句」就是 [`super::user_inputs::user_input_of`] 认的那一条（口径不另写）；子运行的记录不算进主线的轮。
//! 界面只按它排版（折哪几条、过程行写几次调用、刻度悬停写什么），不从流上自己攒。
//!
//! 按段取：`from` 是某一轮开头那一行的字节位置（`at`）或 0；还在跑的最后一轮下次从它的 `at` 再取一次，整轮重算（后端零状态）。
//! `from` 之前的那半轮不出（它的开头不在这一段里）。

use super::user_inputs::user_input_given;
use copy_core::copy_text;
use serde_json::Value;

/// 你那句留多少字。
pub(crate) const SAID_MAX: usize = 50;
/// 回复头留多少字、几行。
pub(crate) const REPLY_MAX: usize = 120;
pub(crate) const REPLY_LINES: usize = 3;

/// 一轮。键名全称。
#[derive(Debug, Default, serde::Serialize, PartialEq, Eq)]
pub(crate) struct TurnRow {
    /// 这一轮开头那一行（你说的那句）的字节位置 ⇒ 下次从这里接着取。
    pub(crate) at: u64,
    /// 你那句的 uuid（跳过去用）。
    pub(crate) uuid: String,
    /// 起：你那句的时刻；止：这一轮最后一条主线记录的时刻（还没有回应 ⇒ 同起）。
    pub(crate) start: String,
    pub(crate) end: String,
    /// 你那句的第一行（≤ [`SAID_MAX`] 字）。
    pub(crate) said: String,
    /// 工具调用几次 · 思考几段 · 失败几次（结果标了出错、且不是人拒的）。
    pub(crate) tools: u32,
    pub(crate) thinking: u32,
    pub(crate) fails: u32,
    /// 结论：这一轮最后一次工具调用之后、带正文的 assistant 记录（uuid，文件序）。还没有 ⇒ 空。
    pub(crate) conclusion: Vec<String>,
    /// 回复头三行（≤ [`REPLY_MAX`] 字）：结论正文的开头；没有结论 ⇒ 空串。
    pub(crate) reply: String,
    /// 这一轮收尾了：后面又有你的一句，或 Claude 说完了（`end_turn`）。
    pub(crate) done: bool,
}

#[derive(Default)]
struct Open {
    row: TurnRow,
    reply_text: String,
}

impl Open {
    /// 回复头：头三行**正文**——代码块整块不算（围栏连同里面的行），空行不算；只有代码 ⇒ 「仅代码」，不露代码原文。
    /// 行内的排版记号（粗体 / 斜体的 `**` `__`、行内代码的反引号、标题的 `#`、引用的 `>`）去掉，只留字（悬停卡是纯文本）。
    fn close(mut self) -> TurnRow {
        let mut fenced = false;
        let mut code = false;
        let mut head: Vec<String> = Vec::new();
        for l in self.reply_text.lines().map(str::trim) {
            if l.starts_with("```") {
                fenced = !fenced;
                code = true;
                continue;
            }
            if fenced {
                continue;
            }
            let l = plain_line(l);
            if !l.is_empty() {
                head.push(l);
                if head.len() == REPLY_LINES {
                    break;
                }
            }
        }
        self.row.reply = if head.is_empty() && code {
            copy_text("rsTurns.reply.codeOnly", &[])
        } else {
            clip(&head.join("\n"), REPLY_MAX)
        };
        self.row
    }
}

/// 一行正文去掉行内排版记号：行首的 `#` / `>`，以及 `**` `__` 与反引号。
fn plain_line(l: &str) -> String {
    let l = l.trim_start_matches(['#', '>']).trim_start();
    l.replace("**", "").replace("__", "").replace('`', "")
}

/// 截到 `max` 个字（Unicode 标量），截了加省略号。
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// 一条主线记录喂进开着的那一轮。
fn feed(tree: &str, t: &mut Open, v: &Value) {
    let ts = v.get("timestamp").and_then(Value::as_str).unwrap_or("");
    let blocks = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);
    match v.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            if !ts.is_empty() {
                t.row.end = ts.to_string();
            }
            let mut text = String::new();
            for b in blocks.into_iter().flatten() {
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        t.row.tools += 1;
                        // 又调了工具 ⇒ 之前的正文是「中间的话」，不是结论。
                        t.row.conclusion.clear();
                        t.reply_text.clear();
                    }
                    Some("thinking") | Some("redacted_thinking") => t.row.thinking += 1,
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(Value::as_str) {
                            text.push_str(s);
                        }
                    }
                    _ => {}
                }
            }
            if !text.trim().is_empty()
                && v.get("isApiErrorMessage").and_then(Value::as_bool) != Some(true)
            {
                if let Some(u) = v.get("uuid").and_then(Value::as_str) {
                    t.row.conclusion.push(u.to_string());
                }
                // 结论正文攒够回复头要的那么多就停（四倍余量给空行与短行）。
                if t.reply_text.chars().count() < REPLY_MAX * 4 {
                    if !t.reply_text.is_empty() {
                        t.reply_text.push('\n');
                    }
                    t.reply_text.push_str(&text);
                }
            }
            let stop = v
                .get("message")
                .and_then(|m| m.get("stop_reason"))
                .and_then(Value::as_str);
            if stop == Some("end_turn") {
                t.row.done = true;
            }
        }
        Some("user") => {
            if !ts.is_empty() {
                t.row.end = ts.to_string();
            }
            let tur = v.get("toolUseResult");
            for b in blocks.into_iter().flatten() {
                if b.get("type").and_then(Value::as_str) == Some("tool_result")
                    && b.get("is_error").and_then(Value::as_bool) == Some(true)
                    && !crate::agents::step_result_of(tree, b, tur).rejected
                {
                    t.row.fails += 1;
                }
            }
        }
        _ => {}
    }
}

/// 从 `from` 起逐行扫，每收尾一轮交一条；返回 `(轮数, end)`，`end` ＝ 最后一个完整行的末字节（残尾不计）。
pub(crate) fn scan_turns<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    mut on_row: impl FnMut(&TurnRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let tree = crate::agents::record_tree_kind().unwrap_or_default();
    let mut end = from;
    let mut count: u64 = 0;
    let mut open: Option<Open> = None;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        let at = end;
        end += read as u64;
        let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        let spoke = crate::agents::user_text_of(tree, &v);
        if let Some(said) = user_input_given(&v, spoke.as_ref()) {
            let speech = spoke.and_then(|s| s.speech()).unwrap_or_default();
            let first = speech
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("");
            if let Some(mut t) = open.take() {
                t.row.done = true;
                on_row(&t.close())?;
                count += 1;
            }
            open = Some(Open {
                row: TurnRow {
                    at,
                    uuid: said.uuid,
                    end: said.timestamp.clone(),
                    start: said.timestamp,
                    said: clip(first, SAID_MAX),
                    ..TurnRow::default()
                },
                reply_text: String::new(),
            });
            continue;
        }
        if crate::agents::run_of_record(tree, &v).is_some() {
            continue;
        }
        if let Some(t) = open.as_mut() {
            feed(tree, t, &v);
        }
    }
    if let Some(t) = open.take() {
        on_row(&t.close())?;
        count += 1;
    }
    Ok((count, end))
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/turns_tests.rs"]
mod tests;
