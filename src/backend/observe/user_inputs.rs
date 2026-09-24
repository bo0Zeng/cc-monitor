//! 〔`设计/10 §2.2b ⑥` · SE1〕**「你说过的话」清单**：会话里每一条**主线用户输入**，按文件顺序。
//!
//! 大纲（原名「我说过的 N 句」）的数据源。前端从此不再自己攒这份清单 —— 它问后端要。
//!
//! # 🔴 改之前的现打（子步 1，基线 `2b2fde3b`）：旁路账本是哪份、大纲的数据今天从哪来
//!
//! | 宿主 | 今天的数据源 | 病 |
//! |---|---|---|
//! | 实时 tab | **旁路账本** `tabs.ts` 的 `Tab.userInputs`：`onLine` 在双重去重之后调 `trackUserInput`，一条一条喂 `user-input-index.ts::toUserInputEntry`（它再调 `collectUserInputs`）攒出来的数组，每来一句整表交给 `UserInputPanel.setEntries` 就地对账 | ① **到达序不是对话序**：重放是「尾块先到、老块后到」⇒ 编号错（`设计/10 §2.2` 第 2 条）；② monitor 起得晚 / 只重放尾部时**清单不全**；③ 每个 tab 各攒一份 |
//! | 历史查看器 | `session-viewer.ts::rebuildUserInputs`：`collectUserInputs(this.payloads…)`，payloads 是收集阶段读进来的**全量** | 判定口径与实时 tab 共用一个 TS 住址，没毛病；但它是前端的判定 —— 后端一旦出这份清单，留着它就是「各写一遍判定」 |
//!
//! ⇒ 判定（四条口径）从 TS 搬到这里，**全仓只剩这一个住址**；两个宿主都经 monitor 的
//!   `list_user_inputs`（走 `subagent::Backend` 的本机/远端分流）来要，TS 那份删掉。
//!
//! # 口径（四条，逐字从被删掉的 `user-input-index.ts::collectUserInputs` 搬过来）
//!
//! 「一条用户输入」= 同时满足四条的 jsonl 记录：
//! 1. `type == "user"`；
//! 2. `isMeta != true` —— Claude Code 注入的 skill/command 展开、system-reminder、caveat 都带 `isMeta`；
//! 3. `isSidechain != true` —— 子 agent 里的用户消息**不算**（选出来的口径，不是漏的：
//!    这份清单回答「**人**在这个会话里说过什么」，子 agent 的 prompt 是主线派下去的活）；
//! 4. 抽出来的**纯文本** trim 之后非空 —— 工具结果回灌（`content` 全是 `tool_result` 块）靠这条排除。
//!
//! 另：**没有 uuid 的不要**（跳不过去，列出来就是一条点了没反应的项）。
//!
//! ⚠ **已知不等价**（原样搬过来，别当没有）：渲染那边还会再剥一层 `stripInternalNoise`，
//! 剥空了就不建卡 ⇒ 本清单可能多出极少数「没有卡」的项（最常见的是 ESC 打断留下的
//! `[Request interrupted by user]`）。前端跳空时**标出来**，不静默（`user-input-panel.ts`）。
//!
//! # 出什么（逐行 JSON，形状登记在 `IPC-PROTOCOL.md §10.4`）
//!
//! 1. 头 `{"kind":"user_inputs","v":1,"from":F}` —— 首行就认得出「对面会出这份清单」；
//! 2. 每条一行 [`UserInputRow`]；
//! 3. 尾 `{"kind":"user_inputs_end","count":N,"end":E}` —— `E` = 最后一个**完整行**的末字节
//!    （torn 残尾不计）＝ 下一次增量该带的 `--from`。**没有尾行 ⇒ 输出被截断**，不许当全量。
//!
//! 本文件**不含任何 `--旗标` 字面量**：argv 在 `history_query.rs` 里解析（那份在
//! `protocol_doc_guard::DISPATCH_FILES` 里，旗标受 IPC-PROTOCOL 对拍约束）；这里只有纯核。

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

/// 一行 jsonl → 是用户输入就给一条 [`UserInputRow`]，不是 ⇒ `None`。**纯函数**。
///
/// 四条口径的**唯一住址**（见头注）。解析不出的行（半截 / 非 JSON）⇒ `None`。
pub(crate) fn user_input_row(line: &[u8]) -> Option<UserInputRow> {
    let text = String::from_utf8_lossy(line);
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}').trim()).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let uuid = v
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty())?;
    let body = plain_text(v.get("message").and_then(|m| m.get("content")));
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
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

/// 抽纯文本：`content` 是字符串就是它本身；是数组就把 `type:"text"` 的块用 `\n` 拼起来
/// （空块不算）。别的形状 ⇒ 空串。
fn plain_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
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

/// 摘要：**先截断再折叠**（`设计/17 §2.1`：44 万字符的一条整段折叠要 11.56 ms，只为取 80 个字）。
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
    mut r: R,
    from: u64,
    out: &mut W,
) -> std::io::Result<u64> {
    writeln!(out, "{{\"kind\":\"user_inputs\",\"v\":1,\"from\":{from}}}")?;
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
            serde_json::to_writer(&mut *out, &row)?;
            out.write_all(b"\n")?;
            count += 1;
        }
    }
    writeln!(
        out,
        "{{\"kind\":\"user_inputs_end\",\"count\":{count},\"end\":{end}}}"
    )?;
    Ok(count)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/user_inputs_tests.rs"]
mod tests;
