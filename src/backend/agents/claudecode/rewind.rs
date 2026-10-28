//! Claude Code 的回退菜单（注册表 `LaunchFace.rewind` 那一格）：怎么调出来 · 每一屏怎么认 · 确认页上那几项叫什么。
//!
//! 照 v2.1.312 的实测（私有 socket，测完停掉）：
//! - 主 agent 空着、输入行空 ⇒ 双击 Esc 弹「Rewind」列表：人说过的每一句一行（下面一行标「No code changes」或改了哪些），
//!   末行「(current)」；↑↓ 选、Enter 进确认页、Esc 取消。
//! - 确认页（没改代码的那句）四项：`1. Restore conversation` · `2. Summarize from here` · `3. Summarize up to here` · `4. Never mind`，
//!   写明「This message and everything after it will be removed. …」；Esc 回到列表。
//! - 屏底「esc to interrupt」＝ 主 agent 在出字：第一下 Esc 是打断这一轮，被打断的那一句放回输入行，不弹列表。
//! - 只有后台命令在跑（屏底「1 shell still running」）、主 agent 空着 ⇒ 照常弹列表。
//! - 同样「Esc、0.3 秒、Esc」一次弹了一次没弹（疑两下被并成一键）⇒ 两下之间隔得比那次长，且第二下之前读屏（通用层做）。
//!
//! ⚠ 没实测到、照字认的：改过代码那一句确认页上多的那一项（这里按「Restore code and conversation」认，屏上没有这一项就不按）；
//! 选中标是 `❯`、输入行是横线下面那一行的 `>`；退完之后输入行里是不是填回原句（填回了就照删字键删掉）。认不出的一屏一律 `Unknown`。

use super::super::{RewindChoice, RewindFace, RewindItem, RewindRow, RewindScreen};
use cmd_wire::rewind::RewindHow;

/// 回退列表那一屏的标题。
const LIST_TITLE: &str = "Rewind";
/// 列表末行「此刻」。
const CURRENT_ROW: &str = "(current)";
/// 确认页上那一句说明里必有的字。
const CONFIRM_MARK: &str = "will be removed";
/// 在出字时屏底那一句（小写比）。
const BUSY_MARK: &str = "esc to interrupt";
/// 选中标。
const SELECTED: char = '❯';
/// 空输入行里的占位提示（`Try "…"`，不是人打的字）。
const PLACEHOLDER: &str = "Try \"";

/// 确认页上「只退对话」那一项（实测）。
pub(crate) const RESTORE_CONVERSATION: &str = "Restore conversation";
/// 确认页上「对话和代码一起退」那一项（⚠ 没实测到，照字认）。
pub(crate) const RESTORE_BOTH: &str = "Restore code and conversation";

pub(crate) const FACE: RewindFace = RewindFace {
    open: &["esc", "esc"],
    open_gap_ms: 500,
    up: "up",
    down: "down",
    enter: "enter",
    back: "esc",
    interrupt: Some("esc"),
    erase: Some("backspace"),
    read,
    choices: CHOICES,
};

const CHOICES: &[RewindChoice] = &[
    RewindChoice {
        how: RewindHow::Conversation,
        label: RESTORE_CONVERSATION,
        text: || copy_core::copy_text("beRewind.how.conversation", &[]),
        default: true,
    },
    RewindChoice {
        how: RewindHow::CodeAndConversation,
        label: RESTORE_BOTH,
        text: || copy_core::copy_text("beRewind.how.codeAndConversation", &[]),
        default: false,
    },
];

/// 去掉框线（行首行尾的 `│`）之后的那一行，保留框里的缩进。
fn inner(l: &str) -> &str {
    let l = l.trim_end();
    let l = l.strip_suffix('│').unwrap_or(l).trim_end();
    let lead = l.len() - l.trim_start().len();
    match l[lead..].strip_prefix('│') {
        Some(rest) => rest,
        None => l,
    }
}

/// 整行只是横线（输入框上下那两条）。
fn is_rule(l: &str) -> bool {
    let t = l.trim();
    t.chars().count() >= 10 && t.chars().all(|c| matches!(c, '─' | '╭' | '╮' | '╰' | '╯'))
}

/// 此刻那一屏是哪一屏（纯文本行）。
pub(crate) fn read(lines: &[String]) -> RewindScreen {
    if let Some(items) = confirm_items(lines) {
        return RewindScreen::Confirm { items };
    }
    if let Some(rows) = list_rows(lines) {
        return RewindScreen::List { rows };
    }
    if lines.iter().any(|l| l.to_lowercase().contains(BUSY_MARK)) {
        return RewindScreen::Busy;
    }
    match prompt_typed(lines) {
        Some(typed) => RewindScreen::Prompt { typed },
        None => RewindScreen::Unknown,
    }
}

/// 确认页：有那一句说明 ＋ 至少两项 `N. 字`、恰一项带选中标。
fn confirm_items(lines: &[String]) -> Option<Vec<RewindItem>> {
    if !lines.iter().any(|l| l.contains(CONFIRM_MARK)) {
        return None;
    }
    let items: Vec<RewindItem> = lines
        .iter()
        .filter_map(|l| {
            let t = inner(l).trim();
            let (selected, rest) = match t.strip_prefix(SELECTED) {
                Some(r) => (true, r.trim_start()),
                None => (false, t),
            };
            let (n, label) = rest.split_once(". ")?;
            (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| RewindItem {
                label: label.trim().to_string(),
                selected,
            })
        })
        .collect();
    (items.len() >= 2 && items.iter().filter(|i| i.selected).count() == 1).then_some(items)
}

/// 回退列表：标题行之后，与选中那一行同一缩进的是一句一行（下面更深一层的「No code changes」· 改了哪些不算）。
fn list_rows(lines: &[String]) -> Option<Vec<RewindRow>> {
    let title = lines.iter().position(|l| inner(l).trim() == LIST_TITLE)?;
    let body: Vec<&str> = lines[title + 1..].iter().map(|l| inner(l)).collect();
    // 选中标在第几列（按字数，不按字节）；一句的字从它后面隔一格起。
    let col = body
        .iter()
        .find_map(|l| l.find(SELECTED).map(|b| l[..b].chars().count()))?;
    let text_col = col + 2;
    let rows: Vec<RewindRow> = body
        .iter()
        .filter_map(|l| {
            let lead = l.len() - l.trim_start().len();
            if lead == col && l.trim_start().starts_with(SELECTED) {
                let text = l.trim_start()[SELECTED.len_utf8()..].trim().to_string();
                return Some((true, text));
            }
            (lead == text_col && !l.trim().is_empty()).then(|| (false, l.trim().to_string()))
        })
        .map(|(selected, text)| RewindRow {
            current: text.starts_with(CURRENT_ROW),
            text,
            selected,
        })
        .collect();
    (rows.iter().filter(|r| r.selected).count() == 1 && rows.iter().any(|r| r.current))
        .then_some(rows)
}

/// 输入行：横线下面那一行 `> 字`（空着 ⇒ 空串；占位提示也算空着）。
fn prompt_typed(lines: &[String]) -> Option<String> {
    lines.windows(2).rev().find_map(|w| {
        if !is_rule(&w[0]) {
            return None;
        }
        let t = inner(&w[1]).trim_start();
        let rest = t.strip_prefix('>').or_else(|| t.strip_prefix(SELECTED))?;
        let typed = rest.trim();
        Some(if typed.starts_with(PLACEHOLDER) {
            String::new()
        } else {
            typed.to_string()
        })
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/rewind_tests.rs"]
mod tests;
