//! 「你说过的话」清单：会话里每一条主线用户输入，按文件顺序。大纲的数据源，判定（四条口径）全仓只这一个住址；
//! 冷开扫描图（`record_scan`）逐行问它；帧命令 `history-user-inputs` 从那张图上切页（`faces/history_face.rs`），本机与远端同一条路。
//!
//! # 口径
//!
//! 「一条用户输入」＝ 同时满足的 jsonl 记录：
//! 1. 是 user 记录，且人在这里说了话（适配层判「谁说的」，即那一家一拍里的 `Said`；人打的 · 粘贴的 ·
//!    斜杠命令 · `!` 输入算，agent 发来的 · 后台通知 · 系统注入 · 压缩摘要 · 中断标记 · 工具结果 · 输出回显都不算）；
//! 2. 不属于任何子运行（适配层 `RecordFace::run_of` 答不出）—— 子运行里的话是主线派下去的活，不是人在这个会话里说的；
//! 3. 有 id（列出来要跳得过去）。

use crate::agents::beat::Beat;
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

/// 一条**已解析**的记录（`kind` 那一家的）→ 是用户输入就给一条 [`UserInputRow`]（一拍现算；手上已有一拍的走 [`user_input_given`]）。
pub(crate) fn user_input_of(kind: &str, v: &Value) -> Option<UserInputRow> {
    user_input_given(kind, v, &crate::agents::beat_of(kind, v)?)
}

/// 一条**已解析**的记录（`kind` 那一家的，`beat` 是它的一拍）→ 是用户输入就给一条 [`UserInputRow`]。**口径的唯一住址**（见头注）。
///
/// 冷开扫描图（[`super::super::record_scan`]）每行解析一次、一拍算一次，轮次 · 骨架索引 · 这里吃同一拍；**判定没有第二份**。
pub(crate) fn user_input_given(kind: &str, v: &Value, beat: &Beat) -> Option<UserInputRow> {
    if crate::agents::run_of_record(kind, v).is_some() {
        return None;
    }
    let uuid = beat.id.as_deref().filter(|u| !u.is_empty())?;
    let body = beat.said()?.speech()?;
    let body = body.as_str();
    Some(UserInputRow {
        at: 0,
        uuid: uuid.to_string(),
        timestamp: beat.at.as_deref().unwrap_or("").to_string(),
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
#[path = "../../../../tests/backend/observe/recognize/user_inputs_tests.rs"]
mod tests;
