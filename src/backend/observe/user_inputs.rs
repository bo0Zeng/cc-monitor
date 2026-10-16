//! 「你说过的话」清单：会话里每一条主线用户输入，按文件顺序。大纲的数据源，判定（四条口径）全仓只这一个住址；
//! 冷开扫描图（`record_scan`）逐行问它；帧命令 `history-user-inputs` 从那张图上切页（`faces/history_face.rs`），本机与远端同一条路。
//!
//! # 口径
//!
//! 「一条用户输入」＝ 同时满足的 jsonl 记录：
//! 1. 是 user 记录，且人在这里说了话（适配层判「谁说的」，经注册表 `agents::user_text_of` 够；人打的 · 粘贴的 ·
//!    斜杠命令 · `!` 输入算，agent 发来的 · 后台通知 · 系统注入 · 压缩摘要 · 中断标记 · 工具结果 · 输出回显都不算）；
//! 2. 不属于任何子运行（适配层 `RecordFace::run_of` 答不出）—— 子运行里的话是主线派下去的活，不是人在这个会话里说的；
//! 3. 有 uuid（列出来要跳得过去）。

use crate::agents::UserTextExt as _;
use serde_json::Value;

/// 清单上一条显示多少字（Unicode 标量计）。超出截断加省略号。
pub(crate) const EXCERPT_MAX: usize = 80;

/// 一条可点的清单项。键名全称（一个会话最多几百条，省字节不值得牺牲可读）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UserInputRow {
    /// 这一句那一行在记录里的字节起点（分页的续点按它给；逐条认的那一处不知道位置时是 0，由扫的那一处填）。
    #[serde(default)]
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    pub(crate) uuid: String,
    /// 记录的 `timestamp`；没有 ⇒ 空串。
    pub(crate) timestamp: String,
    /// 摘要：多行/多空白压成一行，截到 [`EXCERPT_MAX`]。
    pub(crate) excerpt: String,
}

/// 用户输入列表读的是记录树那一家的记录（`agents::record_tree_kind`）。
fn tree() -> &'static str {
    crate::agents::record_tree_kind().unwrap_or_default()
}

/// 一条**已解析**的记录 → 是用户输入就给一条 [`UserInputRow`]。**口径的唯一住址**（见头注）。
///
/// 冷开扫描图（[`super::record_scan`]）每行解析一次、顺带问这一句；**判定没有第二份**。
pub(crate) fn user_input_of(v: &Value) -> Option<UserInputRow> {
    user_input_given(v, crate::agents::user_text_of(tree(), v).as_ref())
}

/// 同 [`user_input_of`]，「谁说的」已经判过（骨架索引那一行顺手也要它，不判两遍）。
pub(crate) fn user_input_given(
    v: &Value,
    said: Option<&crate::agents::UserText>,
) -> Option<UserInputRow> {
    if crate::agents::run_of_record(tree(), v).is_some() {
        return None;
    }
    let uuid = v
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty())?;
    let body = said?.speech()?;
    let body = body.as_str();
    Some(UserInputRow {
        at: 0,
        uuid: uuid.to_string(),
        timestamp: v
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        excerpt: excerpt(body),
    })
}

/// 把空白串压成一个空格，再 trim。
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(EXCERPT_MAX * 8));
    let mut gap = false;
    for c in s.chars() {
        if c.is_whitespace() {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        out.push(c);
    }
    out
}

/// 摘要：**先截断再折叠**（44 万字符的一条整段折叠要 11.56 ms，只为取 80 个字）。
///
/// 边界：前缀里几乎全是空白（640 个空格 + 正文）时截过再折叠会少字 ⇒ 折叠后不够长就退回整条
/// ⇒ 摘要语义不因「先截断」而变。这一支只在病态输入上走。
pub(crate) fn excerpt(text: &str) -> String {
    let cap = EXCERPT_MAX * 8;
    let head = match text.char_indices().nth(cap) {
        Some((i, _)) => &text[..i],
        None => text,
    };
    let mut flat = collapse_ws(head);
    if flat.chars().count() < EXCERPT_MAX && head.len() < text.len() {
        flat = collapse_ws(text);
    }
    match flat.char_indices().nth(EXCERPT_MAX) {
        Some((i, _)) => format!("{}…", &flat[..i]),
        None => flat,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/user_inputs_tests.rs"]
mod tests;
