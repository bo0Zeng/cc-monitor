//! **按路径读会话正文、出成品** —— 后端读正文的冷路（旁路快照那一页 ·
//! 查看器整页 · 按偏移 · 按行号 · 子 agent）共用的那一个核。
//!
//! 它只做「通用的流式读取机器」那一半：切行 · 可计行编号（口径只住 `history_query::line_counts`）· 挑哪一家来解释
//! （注册表 `agents::record_face_of`，按那份文件落在谁的根下）· 装成品。**一行在渲染模型里是什么**住适配层
//! （`agents/<名>/` 的 `RecordFace.parse`）—— 进不进界面、`cwd` 都是适配层给的。
//!
//! 这里**只读渲染模型里正文住的那三个键名**（[`fold_body`]，只在客户端索要折起那一形时），**一家自己的字段一个都不读**：
//! 渲染模型（`JsonlRecord`）是**所有适配层共用的那一个**（Codex 那一家也映射进它，见 `agents/codex/record.rs`）
//! ⇒「正文住哪几格」是通用层的契约，不是某一家的形状；折起那一形**只有一份**，走适配层的能力位只会是一份实现加一道转手。
//!
//! 三种成品（前两种的第三个参数 `summary_only` 选装哪一形，见 [`fold_body`]）：
//! - **行摘要**（[`rows_of`]，给 monitor 旁路快照）：每个可计行 `{end, hash, message?, cwd?}` —— `end` 是这一行（含 `\n`）
//!   之后那个字节的偏移（后端读的是原始字节 ⇒ 永远说得准；没 `\n` 收尾的残尾 ⇒ `null`），`hash` 是这一行正文的摘要
//!   （续传前核「还是不是那一行」用，[`line_hash`]）。`message` 缺 ＝ 不进界面（照占号）。
//! - **记录行**（[`record_lines`]，给界面）：只装进界面的那些，`{session_id, path, seq, cwd, message}`（`JsonlLinePayload`
//!   去掉流机器那两格：`origin` 由问的那一方知道 · `skipped_from` 只属实时流）。
//! - **折起那一行的成品**（上面两种各自的 `summary_only` 置真）：形状与行数一格不差，只是每条的 `message` 剥掉正文那几格。

use crate::agents::{ParsedLine, RecordFace};
use serde_json::{json, Value};
use std::path::Path;

/// 一行正文（不含 `\n`、去掉行尾 `\r`）的摘要：FNV-1a 64。
///
/// 选它而不选 `DefaultHasher`：摘要要**跨进程**比（快照那一刻的后端与续传那一刻的后端可以是两个进程、两个版本），
/// 而 `DefaultHasher` 不承诺跨 Rust 版本稳定。不是安全用途（只防「断线期间文件被改写」那种非恶意的变化）。
pub(crate) fn line_hash(body: &[u8]) -> u64 {
    body.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// 一页原始字节 ⇒ 逐行 `(正文, 末端)`：正文去掉 `\n` 与行尾 `\r`；末端 ＝ 这一行（含 `\n`）之后那个字节的偏移，残尾 ⇒ `None`。
fn split_lines(offset: u64, bytes: &[u8]) -> Vec<(&[u8], Option<u64>)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let (seg, had_nl) = match bytes[at..].iter().position(|&b| b == b'\n') {
            Some(i) => (&bytes[at..at + i], true),
            None => (&bytes[at..], false),
        };
        let consumed = seg.len() + usize::from(had_nl);
        let end = had_nl.then(|| offset + (at + consumed) as u64);
        out.push((seg.strip_suffix(b"\r").unwrap_or(seg), end));
        at += consumed;
    }
    out
}

/// 一行交适配层解释：解析不出（连 JSON 都不是）与不进界面同一个结局 —— 照占号、不出成品。
/// `summary_only` 置真 ⇒ 出成品之前先剥正文（[`fold_body`]）：进不进界面、行号怎么占**都不受它影响**（剥的只是已出成品的内容）。
fn interpret(face: &RecordFace, body: &[u8], summary_only: bool) -> Option<ParsedLine> {
    match (face.parse)(&String::from_utf8_lossy(body)) {
        Ok(Some(mut p)) if p.displayable => {
            if summary_only {
                fold_body(&mut p.message);
            }
            Some(p)
        }
        Ok(_) => None,
        Err(e) => {
            tracing::debug!("record_page: 解析不出的一行（照占号、不出成品）: {e}");
            None
        }
    }
}

/// **剥掉渲染模型里正文住的那几格** —— 客户端索要「折起那一行的成品」（`summaryOnly`）时走这一下。
///
/// 剥的是三个键名，按名字剥、不认记录类型（通用层不许按 `type` 分支）：
/// - `message.content` —— user / assistant 的正文 · 思考 · 工具入参 · 工具结果，**省流量的就是这一格**；
///   `message` 这个对象留着（`role` · `model` · `usage` · `stop_reason` 合起来几十字节，而折起那一行要按 `usage` 报字数）。
/// - `raw` —— `cc-monitor-unrecognized` 抢救下来的整行原文（Codex 那一家的事件行全是这一形）：它就是那一条的全部正文。
/// - `content` —— `queue-operation` 的排队正文；它的折起形是 `userText`（后端已判好「谁说的」），界面本来就只读成品那一格。
///
/// 剥完**剩下的正好是折起那一行要用的**：时刻 `timeText` · 谁说的 `userText` · 一行人话 `toolSteps` · 卡型 `toolCards`
/// · 结果一句 `toolResults` · 报错种类 `apiReason` 与机器可读的 `error`（都不是正文：前者是后端判好的种类，后者是折起的报错卡要读的那一格）
/// · 链上身份 `uuid` / `parentUuid` / `sessionId`。
///
/// **剥 ≠ 置空**：这几格是**删掉**而不是给 `null` / `[]` —— 给个空值等于说「这一条没有正文」，那是假话；
/// 删掉才说得准「这一帧里没有这一格」。要正文的客户端不置这个开关，一切照旧（见 [`rows_of`] / [`record_lines`] 的默认实参）。
fn fold_body(message: &mut Value) {
    if let Some(m) = message.get_mut("message").and_then(Value::as_object_mut) {
        m.remove("content");
    }
    if let Some(o) = message.as_object_mut() {
        o.remove("raw");
        o.remove("content");
    }
}

/// **行摘要**（旁路快照那一页，`history-read`）：页里每个可计行一条，次序同文件。
/// `summary_only` 置真 ⇒ 每条的 `message` 剥掉正文（[`fold_body`]）；`end` / `hash` / `cwd` 与条数一格不变。
pub(crate) fn rows_of(
    face: &RecordFace,
    offset: u64,
    bytes: &[u8],
    summary_only: bool,
) -> Vec<Value> {
    split_lines(offset, bytes)
        .into_iter()
        .filter(|(body, _)| super::history_query::line_counts(body))
        .map(|(body, end)| {
            let mut row = json!({ "end": end, "hash": line_hash(body) });
            if let Some(p) = interpret(face, body, summary_only) {
                row["message"] = p.message;
                if let Some(cwd) = p.cwd {
                    row["cwd"] = Value::String(cwd);
                }
            }
            row
        })
        .collect()
}

/// **记录行**：`lines` 里第 k 个可计行的行号是 `seq + k`；只出进界面的那些。回 `(成品, 下一个行号)`。
/// `summary_only` 置真 ⇒ 每条的 `message` 剥掉正文（[`fold_body`]）；行号怎么占、哪几条出成品**都不变**。
pub(crate) fn record_lines<'a>(
    face: &RecordFace,
    path: &Path,
    seq: u64,
    lines: impl IntoIterator<Item = &'a [u8]>,
    summary_only: bool,
) -> (Vec<Value>, u64) {
    let sid = (face.sid)(path).unwrap_or_default();
    let path_str = path.to_string_lossy();
    let mut next = seq;
    let mut out = Vec::new();
    for body in lines {
        if !super::history_query::line_counts(body) {
            continue;
        }
        let at = next;
        next += 1;
        if let Some(p) = interpret(face, body, summary_only) {
            out.push(json!({
                "session_id": sid,
                "path": path_str,
                "seq": at,
                "cwd": p.cwd,
                "message": p.message,
            }));
        }
    }
    (out, next)
}

/// [`record_lines`] 喂一页原始字节（`history-page`）；`summary_only` 原样递下去。
pub(crate) fn record_lines_of_page(
    face: &RecordFace,
    path: &Path,
    seq: u64,
    bytes: &[u8],
    summary_only: bool,
) -> (Vec<Value>, u64) {
    record_lines(
        face,
        path,
        seq,
        split_lines(0, bytes).into_iter().map(|(body, _)| body),
        summary_only,
    )
}

/// 一页子运行记录（`history-run`）：认得出的每一条给 `{message, rid?}`（进不进界面由界面按记录类型定 —— 这一面是
/// 「整份摆出来看」，与主会话那条路不同）；`rid` 是它的对账键（界面拿它撤那个子运行的活卡）。解析不出的行跳过。
pub(crate) fn run_rows(face: &RecordFace, bytes: &[u8]) -> Vec<Value> {
    let faces = crate::agents::RunFaces::of(face);
    split_lines(0, bytes)
        .into_iter()
        .filter_map(|(body, _)| {
            let text = String::from_utf8_lossy(body);
            let message = match (face.parse)(&text) {
                Ok(Some(p)) => p.message,
                Ok(None) => return None,
                Err(e) => {
                    tracing::warn!("run record parse skip: {e}");
                    return None;
                }
            };
            let rid = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
                .ok()
                .and_then(|v| faces.response_id(&v));
            Some(match rid {
                Some(rid) => serde_json::json!({ "message": message, "rid": rid }),
                None => serde_json::json!({ "message": message }),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/record_page_tests.rs"]
mod tests;
