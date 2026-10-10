//! **按路径读会话正文、出成品** —— 后端读正文的冷路（旁路快照那一页 ·
//! 查看器整页 · 按偏移 · 按行号 · 子 agent）共用的那一个核。
//!
//! 它只做「通用的流式读取机器」那一半：切行 · 可计行编号（口径只住 `history_query::line_counts`）· 挑哪一家来解释
//! （注册表 `agents::record_face_of`，按那份文件落在谁的根下）· 配排队消息的打字时刻（[`TypedTimes`]，只认适配层给的
//! [`crate::agents::QueueMark`]）· 装成品。**一行在界面里是什么**住适配层（`agents/<名>/` 的 `RecordFace.parse`，出通用记录）。
//!
//! 这里**只读通用记录里正文住的那几个键名**（[`fold_body`]，只在客户端索要折起那一形时），**一家自己的字段一个都不读**。
//!
//! 三种成品（都经一个 [`Reader`]：它带着打字时刻表，也带着要不要折起那一形）：
//! - **行摘要**（[`rows_of`]，给 monitor 旁路快照）：每个可计行 `{end, hash, record?, cwd?}` —— `end` 是这一行（含 `\n`）
//!   之后那个字节的偏移（后端读的是原始字节 ⇒ 永远说得准；没 `\n` 收尾的残尾 ⇒ `null`），`hash` 是这一行正文的摘要
//!   （续传前核「还是不是那一行」用，[`line_hash`]）。`record` 缺 ＝ 不进界面（照占号）。
//! - **记录行**（[`record_lines`]，给界面）：只装进界面的那些，`{session_id, path, seq, cwd, record}`（`JsonlLinePayload`
//!   去掉流机器那两格：`origin` 由问的那一方知道 · `skipped_from` 只属实时流）。
//! - **子运行那一页**（[`run_rows`]）：`{record, rid?}`。
//!
//! 一页从文件中段起读时，调用方把起点之前 [`QUEUE_LOOKBACK_BYTES`] 以内那一段一起交进来（[`Reader::new`]），只为配打字时刻。

use crate::agents::record::TypedTimes;
use crate::agents::RecordFace;
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

/// 一页原始字节 ⇒ 逐行 `(正文, 起点, 末端)`：正文去掉 `\n` 与行尾 `\r`；起点 ＝ 这一行第一个字节的偏移；
/// 末端 ＝ 这一行（含 `\n`）之后那个字节的偏移，残尾 ⇒ `None`。
fn split_lines_full(offset: u64, bytes: &[u8]) -> Vec<(&[u8], u64, Option<u64>)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let (seg, had_nl) = match bytes[at..].iter().position(|&b| b == b'\n') {
            Some(i) => (&bytes[at..at + i], true),
            None => (&bytes[at..], false),
        };
        let consumed = seg.len() + usize::from(had_nl);
        let end = had_nl.then(|| offset + (at + consumed) as u64);
        out.push((
            seg.strip_suffix(b"\r").unwrap_or(seg),
            offset + at as u64,
            end,
        ));
        at += consumed;
    }
    out
}

/// 往回多看多少字节配排队消息的打字时刻：一页从文件中段起读时，`queued` 那条的打字时刻（那一家写在更早的一行上）
/// 可能落在上一页。排队消息与它被插进那一轮的那一条实测相隔两分钟量级 ⇒ 上界固定，配不上照用那条自己的时刻。
pub(crate) const QUEUE_LOOKBACK_BYTES: u64 = 512 * 1024;

/// 一页起点 `offset` 之前那一段（[`QUEUE_LOOKBACK_BYTES`] 以内）⇒ `(它的起点, 原始字节)`；从文件头读 / 读不动 ⇒ 空。
/// `target` 是已过围栏的那份记录。
pub(crate) fn lead_of(target: &Path, offset: u64) -> (u64, Vec<u8>) {
    use std::io::{Read, Seek, SeekFrom};
    let at = offset.saturating_sub(QUEUE_LOOKBACK_BYTES);
    let mut buf = Vec::new();
    let read = std::fs::File::open(target).and_then(|mut f| {
        f.seek(SeekFrom::Start(at))?;
        f.take(offset - at).read_to_end(&mut buf)
    });
    match read {
        Ok(_) => (at, buf),
        Err(_) => (offset, Vec::new()),
    }
}

/// 一页的读法：适配层 ＋ 打字时刻表（往回多看那一段已经喂过）＋ 要不要折起那一形。
pub(crate) struct Reader<'a> {
    face: &'a RecordFace,
    typed: TypedTimes,
    summary_only: bool,
    /// 看的那一台的时区（请求带来的）：成品的钟面按它写（[`crate::agents::record::Record::stamp`]）。
    tz: crate::common::time::Tz,
}

impl<'a> Reader<'a> {
    /// `lead` ＝ 这一页起点之前那一段原始字节（[`QUEUE_LOOKBACK_BYTES`] 以内；从文件头读 ⇒ 空），`lead_at` 是它在文件里的起点。
    /// 只拿来配打字时刻：不出成品、不占号；开头那半截行（从行中间起的）解析不出，自然跳过。
    pub(crate) fn new(
        face: &'a RecordFace,
        lead_at: u64,
        lead: &[u8],
        summary_only: bool,
        tz: crate::common::time::Tz,
    ) -> Self {
        let mut r = Self {
            face,
            typed: TypedTimes::default(),
            summary_only,
            tz,
        };
        for (body, start) in split_starts(lead_at, lead) {
            r.interpret(body, start); // 只为喂打字时刻表：成品不要
        }
        r
    }

    /// 一行交适配层解释：解析不出（连 JSON 都不是）与不进界面同一个结局 —— 照占号、不出成品。
    /// `summary_only` ⇒ 出成品之前先剥正文（[`fold_body`]）：进不进界面、行号怎么占**都不受它影响**。
    fn interpret(&mut self, body: &[u8], start: u64) -> Option<(Value, Option<String>)> {
        match (self.face.parse)(&String::from_utf8_lossy(body), start) {
            Ok(Some(mut t)) => {
                self.typed.pass(&mut t);
                let mut rec = t.record?;
                rec.stamp(&self.tz);
                let mut record = serde_json::to_value(rec).ok()?;
                if self.summary_only {
                    fold_body(&mut record);
                }
                Some((record, t.cwd))
            }
            Ok(None) => None,
            Err(e) => {
                tracing::debug!("record_page: 解析不出的一行（照占号、不出成品）: {e}");
                None
            }
        }
    }
}

/// **剥掉通用记录里正文住的那几格** —— 客户端索要「折起那一行的成品」（`summaryOnly`）时走这一下。
///
/// 剥的是键名，按名字剥、不认记录类别：
/// - `blocks` —— 正文 · 推理 · 工具入参 · 工具结果，**省流量的就是这一格**；
/// - `results.*.patch` / `patchTruncated` —— 改动结果的逐段改动（折起那一行只要「+N −M」那一句）。
///
/// 剥完**剩下的正好是折起那一行要用的**：时刻 `timeText` · 谁说的 `who` · 一行人话 `steps` · 卡型 `cards`
/// · 结果一句 `results`（去掉逐段改动）· 报错 `error` · 型号 `model`。
///
/// **剥 ≠ 置空**：这几格是**删掉**而不是给 `null` / `[]` —— 给个空值等于说「这一条没有正文」，那是假话；
/// 删掉才说得准「这一帧里没有这一格」。要正文的客户端不置这个开关，一切照旧。
fn fold_body(record: &mut Value) {
    let Some(o) = record.as_object_mut() else {
        return;
    };
    o.remove("blocks");
    if let Some(results) = o.get_mut("results").and_then(Value::as_object_mut) {
        for r in results.values_mut().filter_map(Value::as_object_mut) {
            r.remove("patch");
            r.remove("patchTruncated");
        }
    }
}

/// 一页原始字节 ⇒ 逐行 `(正文, 起点)`（[`split_lines`] 的起点那一形）。
fn split_starts(offset: u64, bytes: &[u8]) -> Vec<(&[u8], u64)> {
    split_lines_full(offset, bytes)
        .into_iter()
        .map(|(body, start, _)| (body, start))
        .collect()
}

/// 行摘要的一条（`history-read.rows[]`；成品 `read_row`，格目录里登记、冻结）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct ReadRow {
    /// 这一行（含 `\n`）之后那个字节的偏移；没 `\n` 收尾的残尾 ⇒ `null`。
    pub(crate) end: Option<u64>,
    /// 这一行正文的摘要（[`line_hash`]）。
    pub(crate) hash: u64,
    /// 通用记录（缺 ＝ 不进界面，照占号）。`summaryOnly` 时剥过正文，所以是原样的一团。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) record: Option<Value>,
    /// 那一行记的工作目录（只在有记录时带）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cwd: Option<String>,
}

/// **行摘要**（旁路快照那一页，`history-read`）：页里每个可计行一条，次序同文件。
/// `summary_only`（建 [`Reader`] 时给）⇒ 每条的 `record` 剥掉正文（[`fold_body`]）；`end` / `hash` / `cwd` 与条数一格不变。
pub(crate) fn rows_of(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<ReadRow> {
    split_lines_full(offset, bytes)
        .into_iter()
        .filter(|(body, _, _)| super::history_query::line_counts(body))
        .map(|(body, start, end)| {
            let (record, cwd) = reader
                .interpret(body, start)
                .map_or((None, None), |(r, c)| (Some(r), c));
            ReadRow {
                end,
                hash: line_hash(body),
                record,
                cwd,
            }
        })
        .collect()
}

/// **记录行**：`lines` 里第 k 个可计行的行号是 `seq + k`；只出进界面的那些。`lines` 每条带它的起点字节偏移。回 `(成品, 下一个行号)`。
pub(crate) fn record_lines<'b>(
    reader: &mut Reader<'_>,
    path: &Path,
    seq: u64,
    lines: impl IntoIterator<Item = (&'b [u8], u64)>,
) -> (Vec<Value>, u64) {
    let sid = (reader.face.sid)(path).unwrap_or_default();
    let path_str = path.to_string_lossy();
    let mut next = seq;
    let mut out = Vec::new();
    for (body, start) in lines {
        if !super::history_query::line_counts(body) {
            continue;
        }
        let at = next;
        next += 1;
        if let Some((record, cwd)) = reader.interpret(body, start) {
            out.push(json!({
                "session_id": sid,
                "path": path_str,
                "seq": at,
                "cwd": cwd,
                "record": record,
            }));
        }
    }
    (out, next)
}

/// [`record_lines`] 喂一页原始字节（`history-page`；`offset` 是这一页在文件里的起点）。
pub(crate) fn record_lines_of_page(
    reader: &mut Reader<'_>,
    path: &Path,
    seq: u64,
    offset: u64,
    bytes: &[u8],
) -> (Vec<Value>, u64) {
    record_lines(reader, path, seq, split_starts(offset, bytes))
}

/// 一页子运行记录（`history-run`，`offset` 是这一页在文件里的起点）：出了记录的每一条给 `{record, rid?}`；
/// `rid` 是它的对账键（界面拿它撤那个子运行的活卡）。解析不出 / 不进界面的行跳过。
pub(crate) fn run_rows(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<Value> {
    let faces = crate::agents::RunFaces::of(reader.face);
    split_starts(offset, bytes)
        .into_iter()
        .filter_map(|(body, start)| {
            let (record, _) = reader.interpret(body, start)?;
            let text = String::from_utf8_lossy(body);
            let rid = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
                .ok()
                .and_then(|v| faces.response_id(&v));
            Some(match rid {
                Some(rid) => json!({ "record": record, "rid": rid }),
                None => json!({ "record": record }),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/record_page_tests.rs"]
mod tests;
