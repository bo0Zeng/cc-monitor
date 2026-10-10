//! 「你说过的话」清单：会话里每一条主线用户输入，按文件顺序。大纲的数据源，判定（四条口径）全仓只这一个住址；
//! 界面经通道直接说帧命令 `history-user-inputs`，本文件的扫描直接出成品（`read_face.rs`），本机与远端同一条路。
//!
//! # 口径
//!
//! 「一条用户输入」＝ 同时满足的 jsonl 记录：
//! 1. 是 user 记录，且人在这里说了话（适配层判「谁说的」，经注册表 `agents::user_text_of` 够；人打的 · 粘贴的 ·
//!    斜杠命令 · `!` 输入算，agent 发来的 · 后台通知 · 系统注入 · 压缩摘要 · 中断标记 · 工具结果 · 输出回显都不算）；
//! 2. 不属于任何子运行（适配层 `RecordFace::run_of` 答不出）—— 子运行里的话是主线派下去的活，不是人在这个会话里说的；
//! 3. 有 uuid（列出来要跳得过去）。
//!
//! # 出什么（逐行 JSON，形状登记在 `IPC-PROTOCOL.md §10.4`）
//!
//! 1. 头 `{"kind":"user_inputs","v":1,"from":F}` —— 首行就认得出「对面会出这份清单」；
//! 2. 每条一行 [`UserInputRow`]；
//! 3. 尾 `{"kind":"user_inputs_end","count":N,"end":E}` —— `E` = 最后一个完整行的末字节（torn 残尾不计）＝ 下一次增量该带的 `--from`。
//!    没有尾行 ⇒ 输出被截断，不许当全量。
//!
//! 本文件不含任何 `--旗标` 字面量：argv 在 `history_query.rs` 里解析（那份在 `protocol_doc_guard::DISPATCH_FILES` 里）；这里只有纯核。

use serde_json::Value;

/// 清单上一条显示多少字（Unicode 标量计）。超出截断加省略号。
pub(crate) const EXCERPT_MAX: usize = 80;

/// 一条可点的清单项。键名全称（一个会话最多几百条，省字节不值得牺牲可读）。
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub(crate) struct UserInputRow {
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

/// 一行 jsonl → 是用户输入就给一条 [`UserInputRow`]，不是 ⇒ `None`。**纯函数**。
/// 解析不出的行（半截 / 非 JSON）⇒ `None`；判定本身在 [`user_input_of`]。
pub(crate) fn user_input_row(line: &[u8]) -> Option<UserInputRow> {
    user_input_of(&super::record_scan::parse_record(line)?)
}

/// 一条**已解析**的记录 → 是用户输入就给一条 [`UserInputRow`]。**口径的唯一住址**（见头注）。
///
/// 从 [`user_input_row`] 里拆出来：骨架索引（`history_query::index_row`）已经解析过这一行，
/// 顺带问一句「是不是用户输入」就不用再解析一遍 —— 首屏的「索引」与「大纲清单」由此合成一趟读
/// （那条欠账）。**判定没有第二份**：两个出口都调这里。
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

/// 读 `r`（已定位在字节 `from`）逐行出清单（头 ＋ 行 ＋ 尾）。**纯 I/O 泛型**，单测直接喂字节。
///
/// 只看**完整行**（`\n` 收尾）；torn 残尾不看、不计进 `end` —— 下一次从 `end` 接着要时它已写完。
/// 返回出了几条（不含头尾）。
pub(crate) fn write_user_inputs<R: std::io::BufRead, W: std::io::Write>(
    r: R,
    from: u64,
    out: &mut W,
) -> std::io::Result<u64> {
    writeln!(out, "{{\"kind\":\"user_inputs\",\"v\":1,\"from\":{from}}}")?;
    let (count, end) = scan_user_inputs(r, from, |row| {
        serde_json::to_writer(&mut *out, row)?;
        out.write_all(b"\n")
    })?;
    writeln!(
        out,
        "{{\"kind\":\"user_inputs_end\",\"count\":{count},\"end\":{end}}}"
    )?;
    Ok(count)
}

/// 从头那一份已经扫好（共用扫描图）⇒ 照同一个头尾三段写出。
pub(crate) fn write_rows<W: std::io::Write>(
    rows: &[UserInputRow],
    end: u64,
    out: &mut W,
) -> std::io::Result<()> {
    writeln!(out, "{{\"kind\":\"user_inputs\",\"v\":1,\"from\":0}}")?;
    for row in rows {
        serde_json::to_writer(&mut *out, row)?;
        out.write_all(b"\n")?;
    }
    writeln!(
        out,
        "{{\"kind\":\"user_inputs_end\",\"count\":{},\"end\":{end}}}",
        rows.len()
    )
}

/// [`write_user_inputs`] 的中段：逐条交给 `on_row`，回 `(count, end)`（`end` = 最后一个
/// 完整行的末字节）。CLI 那一臂（写头尾三段）与帧面那一臂（`read_face.rs` 的 `history-user-inputs`，
/// 装成成品 `{from, end, entries}`）跑的是**同一个**扫描 —— 「什么算一条用户输入」仍只住 [`user_input_of`]。
pub(crate) fn scan_user_inputs<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    mut on_row: impl FnMut(&UserInputRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let mut end = from;
    let mut count: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        end += read as u64;
        if let Some(row) = user_input_row(&buf[..buf.len() - 1]) {
            on_row(&row)?;
            count += 1;
        }
    }
    Ok((count, end))
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/user_inputs_tests.rs"]
mod tests;
