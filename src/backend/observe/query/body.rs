//! **按路径读会话正文、出成品** —— 后端读正文的冷路（旁路快照那一页 ·
//! 查看器整页 · 按偏移 · 按行号 · 子 agent）共用的那一个核。
//!
//! 它只做「通用的流式读取机器」那一半：切行 · 可计行编号（口径只住 `lines::counts`）· 挑哪一家来解释
//! （注册表 `agents::record_face_of`，按那份文件落在谁的根下）· 配排队消息的打字时刻（[`TypedTimes`]，只认适配层给的
//! [`crate::agents::QueueMark`]）· 装成品。**一行在界面里是什么**住适配层（`agents/<名>/` 的 `RecordFace.parse`，出通用记录）。
//!
//! 这里**一家自己的字段一个都不读**；出口要哪几格由登记处照它交的声明统一投影（`faces/project.rs`），不在这里。
//!
//! 三种成品（都经一个 [`Reader`]：它带着打字时刻表）：
//! - **行摘要**（[`rows_of`]，给 monitor 旁路快照）：每个可计行 `{end, hash, record?, cwd?}` —— `end` 是这一行（含 `\n`）
//!   之后那个字节的偏移（后端读的是原始字节 ⇒ 永远说得准；没 `\n` 收尾的残尾 ⇒ `null`），`hash` 是这一行正文的摘要
//!   （续传前核「还是不是那一行」用，[`line_hash`]）。`record` 缺 ＝ 不进界面（照占号）。
//! - **记录行**（[`record_lines`]，给界面）：只装进界面的那些，`{session_id, path, seq, cwd, record}`（`JsonlLinePayload`
//!   去掉流机器那两格：`origin` 由问的那一方知道 · `skipped_from` 只属实时流）。
//! - **子运行那一页**（[`run_rows`]）：`{record, rid?}`。
//!
//! 一页从文件中段起读时，调用方把起点之前 [`QUEUE_LOOKBACK_BYTES`] 以内那一段一起交进来（[`Reader::new`]），只为配打字时刻。

use super::locate::validate_session_path;
use crate::agents::record::RecordExt as _;
use crate::agents::record::TypedTimes;
use crate::agents::RecordFace;
use crate::platform::fs::Ro;
/// 行摘要 · 记录行的线上形住 `cmd-wire`（手机按同一份解）；记录那一格后端照原样的一团写（缺省 `Value`）。
pub(crate) use cmd_wire::record::{ReadRow, RecordLine};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

/// 一行正文（不含 `\n`、去掉行尾 `\r`）的摘要：FNV-1a 64。
///
/// 选它而不选 `DefaultHasher`：摘要要**跨进程**比（快照那一刻的后端与续传那一刻的后端可以是两个进程、两个版本），
/// 而 `DefaultHasher` 不承诺跨 Rust 版本稳定。不是安全用途（只防「断线期间文件被改写」那种非恶意的变化）。
pub(crate) fn line_hash(body: &[u8]) -> u64 {
    cmd_wire::fnv::fnv1a64(body)
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
    let read = Ro::open(target).and_then(|mut f| {
        f.seek(SeekFrom::Start(at))?;
        f.take(offset - at).read_to_end(&mut buf)
    });
    match read {
        Ok(_) => (at, buf),
        Err(_) => (offset, Vec::new()),
    }
}

/// 一页的读法：适配层 ＋ 打字时刻表（往回多看那一段已经喂过）。要哪几格不归这里管：出口交的声明由登记处统一投影（`faces/project.rs`）。
pub(crate) struct Reader<'a> {
    face: &'a RecordFace,
    typed: TypedTimes,
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
        tz: crate::common::time::Tz,
    ) -> Self {
        let mut r = Self {
            face,
            typed: TypedTimes::default(),
            tz,
        };
        for (body, start) in split_starts(lead_at, lead) {
            r.interpret(body, start); // 只为喂打字时刻表：成品不要
        }
        r
    }

    /// 一行交适配层解释：解析不出（连 JSON 都不是）与不进界面同一个结局 —— 照占号、不出成品。
    fn interpret(&mut self, body: &[u8], start: u64) -> Option<(Value, Option<String>)> {
        match (self.face.parse)(&crate::agents::Line::new(
            &String::from_utf8_lossy(body),
            start,
        )) {
            Ok(Some(mut t)) => {
                self.typed.pass(&mut t);
                let mut rec = t.record?;
                rec.stamp(&self.tz);
                let record = serde_json::to_value(rec).ok()?;
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

/// 一页原始字节 ⇒ 逐行 `(正文, 起点)`（[`split_lines`] 的起点那一形）。
fn split_starts(offset: u64, bytes: &[u8]) -> Vec<(&[u8], u64)> {
    split_lines_full(offset, bytes)
        .into_iter()
        .map(|(body, start, _)| (body, start))
        .collect()
}

/// **行摘要**（旁路快照那一页，`history-read`）：页里每个可计行一条，次序同文件。
/// 相邻的认不出那几条并进前一条（[`fold_unread`]）：被并掉的那一行照占号、不再带记录。
pub(crate) fn rows_of(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<ReadRow> {
    let mut out: Vec<ReadRow> = Vec::new();
    let mut last: Option<usize> = None;
    for (body, start, end) in split_lines_full(offset, bytes)
        .into_iter()
        .filter(|(body, _, _)| super::super::lines::counts(body))
    {
        let (mut record, mut cwd) = reader
            .interpret(body, start)
            .map_or((None, None), |(r, c)| (Some(r), c));
        if let (Some(r), Some(i)) = (&record, last) {
            if let Some(prev) = out[i].record.as_mut() {
                if fold_unread(prev, r) {
                    (record, cwd) = (None, None);
                }
            }
        }
        if record.is_some() {
            last = Some(out.len());
        }
        out.push(ReadRow {
            end,
            hash: line_hash(body),
            record,
            cwd,
        });
    }
    out
}

/// **相邻的认不出那几条并成一条**（适配层每行给一条 `unread`，并是出成品这一遍的事）：`into` 与 `next` 都是 `unread`、
/// 原因与类型都一样 ⇒ `count` 加上、`text` 照新的数重写（[`crate::agents::record::unread_text`]），回 `true`；否则不动、回 `false`。
/// 只看通用记录的格，不认哪一家。
pub(crate) fn fold_unread(into: &mut Value, next: &Value) -> bool {
    use crate::agents::record::{unread_text, UnreadWhy};
    let same = |k: &str| into.get(k) == next.get(k);
    if into.get("t").and_then(Value::as_str) != Some("unread")
        || !same("t")
        || !same("why")
        || !same("type")
    {
        return false;
    }
    let why = match into.get("why").and_then(Value::as_str) {
        Some("unknownType") => UnreadWhy::UnknownType,
        Some("parseFailed") => UnreadWhy::ParseFailed,
        _ => return false,
    };
    let n = |v: &Value| v.get("count").and_then(Value::as_u64).unwrap_or(1);
    let count = u32::try_from(n(into) + n(next)).unwrap_or(u32::MAX);
    let kind = into.get("type").and_then(Value::as_str).map(str::to_string);
    into["count"] = json!(count);
    into["text"] = json!(unread_text(why, kind.as_deref(), count).0);
    true
}

/// 子运行那一页的一条（`history-run.rows[]`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RunRow {
    /// 通用记录（同 [`RecordLine::record`]）。
    #[cfg_attr(test, ts(as = "crate::agents::record::Record"))]
    pub(crate) record: Value,
    /// 它的对账键（界面拿它撤那个子运行的活卡）；没有 ⇒ 这一格不出现。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) rid: Option<String>,
}

/// **记录行**：`lines` 里第 k 个可计行的行号是 `seq + k`；只出进界面的那些；相邻的认不出那几条并进前一条（[`fold_unread`]，行号是前一条的）。`lines` 每条带它的起点字节偏移。回 `(成品, 下一个行号)`。
pub(crate) fn record_lines<'b>(
    reader: &mut Reader<'_>,
    path: &Path,
    seq: u64,
    lines: impl IntoIterator<Item = (&'b [u8], u64, Option<u64>)>,
) -> (Vec<RecordLine>, u64) {
    let sid = (reader.face.sid)(path).unwrap_or_default();
    let path_str = path.to_string_lossy();
    let mut next = seq;
    let mut out: Vec<RecordLine> = Vec::new();
    for (body, start, end) in lines {
        if !super::super::lines::counts(body) {
            continue;
        }
        let at = next;
        next += 1;
        if let Some((record, cwd)) = reader.interpret(body, start) {
            if let Some(prev) = out.last_mut() {
                if fold_unread(&mut prev.record, &record) {
                    prev.end = end;
                    continue;
                }
            }
            out.push(RecordLine {
                session_id: sid.clone(),
                path: path_str.to_string(),
                seq: at,
                end,
                cwd,
                record,
            });
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
) -> (Vec<RecordLine>, u64) {
    record_lines(reader, path, seq, split_lines_full(offset, bytes))
}

/// 一页子运行记录（`history-run`，`offset` 是这一页在文件里的起点）：出了记录的每一条给 `{record, rid?}`；
/// `rid` 是它的对账键（界面拿它撤那个子运行的活卡）。解析不出 / 不进界面的行跳过；相邻的认不出那几条并成一条（[`fold_unread`]）。
pub(crate) fn run_rows(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<RunRow> {
    let faces = crate::agents::RunFaces::of(reader.face);
    let mut out: Vec<RunRow> = Vec::new();
    for (body, start) in split_starts(offset, bytes) {
        let Some((record, _)) = reader.interpret(body, start) else {
            continue;
        };
        if let Some(prev) = out.last_mut() {
            if fold_unread(&mut prev.record, &record) {
                continue;
            }
        }
        let rid = super::super::lines::parse(body).and_then(|v| faces.response_id(&v));
        out.push(RunRow { record, rid });
    }
    out
}

/// 帧面按字节区间分页读一份会话的**一页**（`history-read`）。
///
/// # 为什么要分页（`C1` · 2026-09-24）
///
/// 一次性子命令把整份会话**流**回去（`io::copy`），连接关了就是 EOF；帧面是一问一答，
/// 一帧应答要整个进内存、整个过线 —— 而本仓见过 270 MB 的会话，monitor 那头单帧上限
/// 64 MiB。⇒ 调用方给 `[offset, until)`，本函数回**不超过 `page` 字节、切在行尾**的一页，
/// 外加续点 `next`；调用方循环到 `eof`。
///
/// # 切法
///
/// - 这一页之后区间里还有字节 ⇒ 切在**最后一个 `\n` 之后**，不把半行交出去；
/// - 一页里一个 `\n` 都没有（单行比 `page` 还长）⇒ 往后续读到那一行的 `\n`，
///   但**不超过 `line_cap`** —— 超了回 `oversized_line`（不许截半行冒充整行）；
/// - 区间到头（`until` 或读时的文件长度）⇒ 余下的全给，含 torn 残尾（残尾怎么处置由调用方定）。
///
/// 错误是 `(code, message)`：`refused`（围栏拒）· `failed`（读失败）· `oversized_line`
/// （不叫 `line_too_long`：那是入方向信封的**协议级** code，命令级不许撞名）。
pub(crate) fn read_page(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<ReadPage, (&'static str, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let mut f = Ro::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    let len = f
        .metadata()
        .map_err(|e| ("failed", format!("stat failed: {e}")))?
        .len();
    let limit = until.map_or(len, |u| u.min(len));
    if offset >= limit {
        return Ok(ReadPage {
            bytes: Vec::new(),
            next: offset,
            eof: true,
        });
    }
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| ("failed", format!("seek failed: {e}")))?;
    let mut src = f.take(limit - offset);
    let mut bytes = Vec::with_capacity(page.min((limit - offset) as usize));
    (&mut src)
        .take(page as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ("failed", format!("read failed: {e}")))?;
    let reached_limit = offset + bytes.len() as u64 >= limit;
    if !reached_limit {
        match bytes.iter().rposition(|&b| b == b'\n') {
            Some(i) => bytes.truncate(i + 1),
            None => {
                // 单行比一页还长：续读到它的行尾，但不超过 `line_cap`。
                let mut more = [0u8; 64 * 1024];
                loop {
                    let k = src
                        .read(&mut more)
                        .map_err(|e| ("failed", format!("read failed: {e}")))?;
                    if k == 0 {
                        break;
                    }
                    let done = match more[..k].iter().position(|&b| b == b'\n') {
                        Some(i) => {
                            bytes.extend_from_slice(&more[..=i]);
                            true
                        }
                        None => {
                            bytes.extend_from_slice(&more[..k]);
                            false
                        }
                    };
                    // ⚠ 上限在**每一次**续读之后判，包括找到行尾的那一次 ——
                    //   只在「没找到」那一支判的话，行尾恰好落在这一块里的超长行会被整行交出去。
                    if bytes.len() > line_cap {
                        return Err((
                            "oversized_line",
                            copy_text(
                                "beHistoryQuery.readPage.lineTooLong",
                                &[
                                    ("lineCap", &line_cap.to_string()),
                                    ("offset", &offset.to_string()),
                                ],
                            ),
                        ));
                    }
                    if done {
                        break;
                    }
                }
            }
        }
    }
    let next = offset + bytes.len() as u64;
    Ok(ReadPage {
        bytes,
        next,
        eof: next >= limit,
    })
}

/// [`read_page`] 的一页。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadPage {
    /// 原始字节（调用方负责解码 —— 帧面那层用 `from_utf8_lossy`）。
    pub bytes: Vec<u8>,
    /// 下一页从这里起。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 帧面按**行号**取回一段（`history-lines`）：第 `[from, until)` 个可计行的原文。
///
/// # 为什么要有它
///
/// 按字节取正文（[`read_page`]）要调用方先知道字节边界 —— 那只有骨架索引给得出。
/// 没接骨架的会话（后台 tab · 老后端 · seq 对不上的）丢掉的正文就无处可回 ⇒ monitor 的重放缓冲
/// 不敢设上界（级 3）。本函数是**不依赖索引**的那条取回路：调用方只说行号。
///
/// # 口径
///
/// - 「第几行」只有一个家：[`super::super::lines::counts`]（BOM 与全空白不占号）＋ 没 `\n` 收尾的残尾不计 ——
///   与 [`tail_plan`] 的 `total` · 冷开扫描图的行号 · 实时 `line` 帧的 `seq` 同一个空间。
/// - 只交**可计行**（空行在号外，交出去调用方也不会给它编号）；第 k 条就是第 `from + k` 行。
/// - `until` 缺 ＝ 到最后一个完整行为止。
///
/// # 一帧装得下
///
/// 交出的原文累计达到 `page` 字节就停（至少交一行）；单行超过 `line_cap` ⇒ `oversized_line`
/// （同 [`read_page`]：不截半行）。停在中途时 `eof == false`、`next < until`，调用方接着要。
///
/// # 代价（写清楚）
///
/// **O(第 `from` 行之前的字节)**：后端零状态，每次从文件头数。本机最大一份会话（111 MB / 4.2 万行）
/// 热缓存从头数到尾 ≈ 22 ms；取回只在用户往上翻过了前端手里最老那一条时才发生，一次一批。
/// 真撞上「弱设备上超大会话翻得卡」的读数时再加锚，不先加。
///
/// 错误同 [`read_page`]：`refused` · `failed` · `oversized_line`。
pub(crate) fn read_lines(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let f = Ro::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    read_lines_from(std::io::BufReader::new(f), from, until, page, line_cap)
}

/// [`read_lines`] 的内核：读 `r`（从文件头起）。**纯 I/O 泛型**，单测直接喂字节。
pub(crate) fn read_lines_from<R: std::io::BufRead>(
    r: R,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let until = until.unwrap_or(u64::MAX);
    let mut lines: Vec<String> = Vec::new();
    let mut starts: Vec<u64> = Vec::new();
    let mut ends: Vec<u64> = Vec::new();
    let mut bytes: usize = 0;
    let mut n: u64 = 0; // 下一个可计行的行号
    let mut end: u64 = 0; // 最后一个数到的可计行之后
    let mut rows = super::super::lines::Lines::new(r, 0);
    let eof = loop {
        if n >= until {
            break false;
        }
        let Some(row) = rows
            .next()
            .map_err(|e| ("failed", format!("scan failed: {e}")))?
        else {
            break true; // 文件到头；torn 残尾不计（同 `tail_plan`）
        };
        let (start, pos, body) = (row.at, row.at + row.whole.len() as u64, row.body());
        if !super::super::lines::counts(body) {
            continue;
        }
        let at = n;
        n += 1;
        end = pos;
        if at < from {
            continue;
        }
        if body.len() > line_cap {
            return Err((
                "oversized_line",
                copy_text(
                    "beHistoryQuery.readLinesFrom.lineTooLong",
                    &[("at", &at.to_string()), ("lineCap", &line_cap.to_string())],
                ),
            ));
        }
        bytes += body.len();
        lines.push(String::from_utf8_lossy(body).into_owned());
        starts.push(start);
        ends.push(pos);
        if bytes >= page {
            break false;
        }
    };
    let next = from + lines.len() as u64;
    Ok(LinesPage {
        from,
        lines,
        starts,
        ends,
        next,
        end,
        eof,
    })
}

/// [`read_lines`] 的一段。不变量：`next == from + lines.len()`。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LinesPage {
    /// 第一条的行号（＝ 请求里的 `from`）。
    pub from: u64,
    /// 可计行的原文（不含行尾 `\n`；`\r` 与 BOM 原样留着，调用方剥）。
    pub lines: Vec<String>,
    /// 每一条在文件里的起点字节偏移（与 `lines` 逐条对齐）。
    pub starts: Vec<u64>,
    /// 每一条（含 `\n`）之后那个字节（与 `lines` 逐条对齐）。
    pub ends: Vec<u64>,
    /// 下一段从这一行起。
    pub next: u64,
    /// 最后一个数到的可计行之后那个字节（交出的 · 跳过的都算；残尾不算）。
    pub end: u64,
    /// 读到了最后一个完整行之后（后面没有了）。
    pub eof: bool,
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/query/body_tests.rs"]
mod tests;
