//! 读一份会话记录的那几条帧命令（typed，`wire/typed.rs`）：按字节分页（`history-read` · `history-page`）·
//! 按行号（`history-lines`）· 一个子运行（`history-run`）· 尾段在哪（`history-tail`）· 主线外（`history-branch`）·
//! 记录还在不在（`history-record`）· 这台上需手动的会话（`sessions-needs`）· 会话事实（`history-facts`）· 骨架索引等几条扫描。
//!
//! 查询本体住 `observe/`（`history_query` · `record_page`），这里只有线上那三个类型和接线；家目录是参数（判据拿夹具喂 `*_at`）。
//! 入方向不许出现 `observe::` 的那条线照旧：本文件住 `faces/`，与 `read_face` 同一层。

use crate::observe::history_query::{self, RecordProbe, TailPlan};
use crate::observe::record_page::{ReadRow, RecordLine, RunRow};
use crate::wire::typed::{codes, of_pair, Blocking, Command, Cx, NoArgs};
use std::path::Path;

use super::read_face::{
    answer_off_of, dress_ask, reader, record_face, too_large, LINES_CAP_BYTES, LINE_CAP_BYTES,
    READ_PAGE_BYTES,
};
use crate::wire::REPLY_PAGE_BYTES;

/// 整份回包过 [`LINES_CAP_BYTES`] ⇒ `too_large` 那一句（不截断：截断的清单会被当成完整的用）。
fn over_cap<T: serde::Serialize>(reply: &T) -> Option<String> {
    let size = serde_json::to_vec(reply).map_or(0, |b| b.len());
    (size > LINES_CAP_BYTES).then(|| too_large(size).1)
}

/// 这台的记录树根（生产只这一个值）。
fn home() -> std::path::PathBuf {
    crate::history::listing::agent_home()
}

/// 按字节分页那几条的码：围栏外 ⇒ `refused`；一行比续读上限还长 ⇒ `oversized_line`；读不动 ⇒ `failed`。
macro_rules! page_codes {
    ($(#[$m:meta])* $name:ident) => {
        codes! {
            $(#[$m])*
            pub(crate) enum $name {
                /// 读不动（打不开 · 没有一家认得这份记录）。
                Failed = "failed",
                /// 一行比续读上限还长（不截断）。
                OversizedLine = "oversized_line",
                /// 路径在记录树的围栏外。
                Refused = "refused",
            }
        }
    };
}

// ───────────────────────── history-read ─────────────────────────

/// `history-read` 的入参：读 `[offset, until)` 那一页（切在行尾）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryReadReq {
    /// jsonl 路径（围栏：记录树里）。
    pub(crate) path: String,
    /// 从这个字节起（缺 ⇒ 0）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) offset: Option<u64>,
    /// 可选右端（半开区间）；缺 ⇒ 读到文件尾。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) until: Option<u64>,
}

/// `history-read` 的回包（带 `view` 时 `rows[]` 那几格可缺）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryRead {
    /// 这一页里每个可计行一条（空白 / 纯 BOM 行不占）。
    pub(crate) rows: Vec<ReadRow>,
    /// 下一页从这里起（＝ `offset` ＋ 这一页的原始字节数）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) next: u64,
    /// 区间到头了（`until` 或读时的文件长度）。
    pub(crate) eof: bool,
}

page_codes!(
    /// `history-read` 的码。
    HistoryReadErr
);

/// `history-read`：按字节分页读一份会话，出行摘要。
pub(crate) struct HistoryReadCmd;

impl Command for HistoryReadCmd {
    const NAME: &'static str = "history-read";
    type Req = HistoryReadReq;
    type Reply = HistoryRead;
    type Err = HistoryReadErr;
}

impl Blocking for HistoryReadCmd {
    fn run(req: HistoryReadReq, cx: &Cx) -> Result<HistoryRead, HistoryReadErr> {
        read_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryReadCmd`] 的本体，家目录是参数。
pub(crate) fn read_at(
    home: &Path,
    req: HistoryReadReq,
    tz: &crate::Tz,
) -> Result<HistoryRead, HistoryReadErr> {
    let lower = of_pair(HistoryReadErr::Failed);
    let offset = req.offset.unwrap_or(0);
    let (target, face) = record_face(home, &req.path).map_err(&lower)?;
    let page = history_query::read_page(
        home,
        &req.path,
        offset,
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    Ok(HistoryRead {
        rows: crate::observe::record_page::rows_of(
            &mut reader(&face, &target, offset, tz),
            offset,
            &page.bytes,
        ),
        next: page.next,
        eof: page.eof,
    })
}

// ───────────────────────── history-page ─────────────────────────

/// `history-page` 的入参：同 `history-read`，另带 `offset` 那一行的行号。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryPageReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从这个字节起（缺 ⇒ 0）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) offset: Option<u64>,
    /// 可选右端（半开区间）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) until: Option<u64>,
    /// `offset` 那一行的行号（缺 ⇒ 0）；续页交上一页的 `nextSeq`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) seq: Option<u64>,
}

/// `history-page` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryPage {
    /// 只装进界面的记录行（第 k 个可计行 ＝ `seq + k`）。
    pub(crate) lines: Vec<RecordLine>,
    /// 同 `history-read`。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) next: u64,
    /// 下一页第一行的行号。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) next_seq: u64,
    /// 同 `history-read`。
    pub(crate) eof: bool,
}

page_codes!(
    /// `history-page` 的码。
    HistoryPageErr
);

/// `history-page`：按字节分页读，出记录行。
pub(crate) struct HistoryPageCmd;

impl Command for HistoryPageCmd {
    const NAME: &'static str = "history-page";
    type Req = HistoryPageReq;
    type Reply = HistoryPage;
    type Err = HistoryPageErr;
}

impl Blocking for HistoryPageCmd {
    fn run(req: HistoryPageReq, cx: &Cx) -> Result<HistoryPage, HistoryPageErr> {
        page_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryPageCmd`] 的本体，家目录是参数。
pub(crate) fn page_at(
    home: &Path,
    req: HistoryPageReq,
    tz: &crate::Tz,
) -> Result<HistoryPage, HistoryPageErr> {
    let lower = of_pair(HistoryPageErr::Failed);
    let offset = req.offset.unwrap_or(0);
    let (target, face) = record_face(home, &req.path).map_err(&lower)?;
    let page = history_query::read_page(
        home,
        &req.path,
        offset,
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    let (lines, next_seq) = crate::observe::record_page::record_lines_of_page(
        &mut reader(&face, &target, offset, tz),
        &target,
        req.seq.unwrap_or(0),
        offset,
        &page.bytes,
    );
    Ok(HistoryPage {
        lines,
        next: page.next,
        next_seq,
        eof: page.eof,
    })
}

// ───────────────────────── history-lines ─────────────────────────

/// `history-lines` 的入参：按行号取 `[from, until)`。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryLinesReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 第一行的行号（缺 ⇒ 0）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选右端（半开区间）；缺 ⇒ 到最后一个完整行为止。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) until: Option<u64>,
}

/// `history-lines` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryLines {
    /// 第一行的行号。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) next: u64,
    /// 读到了最后一个完整行之后。
    pub(crate) eof: bool,
    /// `[from, next)` 里进界面的那些（不进界面的照占号、不出现）。
    pub(crate) lines: Vec<RecordLine>,
}

page_codes!(
    /// `history-lines` 的码。
    HistoryLinesErr
);

/// `history-lines`：按行号取回一段。
pub(crate) struct HistoryLinesCmd;

impl Command for HistoryLinesCmd {
    const NAME: &'static str = "history-lines";
    type Req = HistoryLinesReq;
    type Reply = HistoryLines;
    type Err = HistoryLinesErr;
}

impl Blocking for HistoryLinesCmd {
    fn run(req: HistoryLinesReq, cx: &Cx) -> Result<HistoryLines, HistoryLinesErr> {
        lines_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryLinesCmd`] 的本体，家目录是参数。
pub(crate) fn lines_at(
    home: &Path,
    req: HistoryLinesReq,
    tz: &crate::Tz,
) -> Result<HistoryLines, HistoryLinesErr> {
    let lower = of_pair(HistoryLinesErr::Failed);
    let (target, face) = record_face(home, &req.path).map_err(&lower)?;
    let page = history_query::read_lines(
        home,
        &req.path,
        req.from.unwrap_or(0),
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    let first = page.starts.first().copied().unwrap_or(0);
    let (lines, _) = crate::observe::record_page::record_lines(
        &mut reader(&face, &target, first, tz),
        &target,
        page.from,
        page.lines
            .iter()
            .map(|l| l.as_bytes())
            .zip(page.starts.iter().copied()),
    );
    Ok(HistoryLines {
        from: page.from,
        next: page.next,
        eof: page.eof,
        lines,
    })
}

// ───────────────────────── history-run ─────────────────────────

/// `history-run` 的入参：父记录 ＋（子运行 ‖ 派出它的那次工具调用），从 `from` 起一页。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryRunReq {
    /// 父会话的记录路径（围栏照旧；越界 ⇒ `path_refused`）。
    pub(crate) parent: String,
    /// 子运行标识（运行表里那一格）；与 `tool` 至少给一个，都给以它为准。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) run: Option<String>,
    /// 派出它的那次工具调用的 id。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) tool: Option<String>,
    /// 从这个字节起读（缺 ⇒ 0）；续读拿上一次的 `end`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
}

/// `history-run` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryRun {
    /// 读的是哪个子运行。
    pub(crate) run: String,
    /// 那份子运行记录（不透明，给查看器整份打开用）。
    pub(crate) path: String,
    /// 这一页里每一条进界面的记录。
    pub(crate) rows: Vec<RunRow>,
    /// 读到哪了（最后一个整行之后）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 这一页没读到头（再从 `end` 读）。
    pub(crate) more: bool,
}

codes! {
    /// `history-run` 的码。
    pub(crate) enum HistoryRunErr {
        /// 读不动。
        Failed = "failed",
        /// 还没对上是哪一个子运行（前台子运行跑完才写明）。
        NotFound = "not_found",
        /// 父记录在围栏外。
        PathRefused = "path_refused",
        /// 子运行记录在围栏外。
        Refused = "refused",
        /// 整份回包过了上限。
        TooLarge = "too_large",
    }
}

/// `history-run`：一个子运行的记录（按运行读，不认任何一家的目录 / 字段）。
pub(crate) struct HistoryRunCmd;

impl Command for HistoryRunCmd {
    const NAME: &'static str = "history-run";
    type Req = HistoryRunReq;
    type Reply = HistoryRun;
    type Err = HistoryRunErr;
}

impl Blocking for HistoryRunCmd {
    fn run(req: HistoryRunReq, cx: &Cx) -> Result<HistoryRun, HistoryRunErr> {
        run_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryRunCmd`] 的本体，家目录是参数。
pub(crate) fn run_at(
    home: &Path,
    req: HistoryRunReq,
    tz: &crate::Tz,
) -> Result<HistoryRun, HistoryRunErr> {
    let lower = of_pair(HistoryRunErr::Failed);
    let from = req.from.unwrap_or(0);
    let (source, run) =
        history_query::run_source(home, &req.parent, req.run.as_deref(), req.tool.as_deref())
            .map_err(&lower)?;
    let path = source.to_string_lossy().into_owned();
    let (target, face) = record_face(home, &path).map_err(&lower)?;
    let page = history_query::read_page(home, &path, from, None, READ_PAGE_BYTES, LINE_CAP_BYTES)
        .map_err(&lower)?;
    let rows = crate::observe::record_page::run_rows(
        &mut reader(&face, &target, from, tz),
        from,
        &page.bytes,
    );
    let reply = HistoryRun {
        run,
        path,
        rows,
        end: page.next,
        more: !page.eof,
    };
    match over_cap(&reply) {
        Some(said) => Err(HistoryRunErr::TooLarge(said)),
        None => Ok(reply),
    }
}

// ───────────────────────── history-branch ─────────────────────────

/// `history-branch` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从清单第几条起（缺 ⇒ 0；下一页传 `next`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-branch` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Branch {
    /// 回退掉的那几条记录的 `id`（只含进界面的；文件序），这一页是清单里从 `from` 起的那几条；这一家的记录没有链 ⇒ 恒空。
    pub(crate) off: Vec<String>,
    /// 最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从清单第几条起；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
}

codes! {
    /// `history-branch` 的码。
    pub(crate) enum HistoryBranchErr {
        /// 读不动（含围栏外）。
        Failed = "failed",
    }
}

/// `history-branch`：主线外清单（冷读一次；实时那一路是帧 `session_branch`）。
pub(crate) struct HistoryBranchCmd;

impl Command for HistoryBranchCmd {
    const NAME: &'static str = "history-branch";
    type Req = BranchReq;
    type Reply = Branch;
    type Err = HistoryBranchErr;
}

impl Blocking for HistoryBranchCmd {
    fn run(req: BranchReq, _cx: &Cx) -> Result<Branch, HistoryBranchErr> {
        branch_at(&home(), req)
    }
}

/// [`HistoryBranchCmd`] 的本体，家目录是参数。
pub(crate) fn branch_at(home: &Path, req: BranchReq) -> Result<Branch, HistoryBranchErr> {
    branch_page(home, req, REPLY_PAGE_BYTES)
}

/// [`branch_at`] 的本体，一页的上限是参数。清单出自扫描图（同一张），按条切页。
pub(crate) fn branch_page(
    home: &Path,
    req: BranchReq,
    cap: usize,
) -> Result<Branch, HistoryBranchErr> {
    let map = history_query::cold_scan(home, &req.path).map_err(HistoryBranchErr::Failed)?;
    let from = usize::try_from(req.from.unwrap_or(0)).unwrap_or(usize::MAX);
    let fixed = jlen(&Branch {
        off: Vec::new(),
        end: u64::MAX,
        next: Some(u64::MAX),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let rest = map.off.get(from..).unwrap_or_default();
    let n = rest
        .iter()
        .take_while(|id| pager.admit(jlen(*id) + 1))
        .count();
    Ok(Branch {
        off: rest[..n].to_vec(),
        end: map.end,
        next: (n < rest.len()).then(|| (from + n) as u64),
    })
}

// ───────────────────────── history-tail ─────────────────────────

/// `history-tail` 的入参：要最新几行。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryTailReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 要最新几行。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) n: u64,
}

codes! {
    /// `history-tail` 的码。
    pub(crate) enum HistoryTailErr {
        /// 读不动（含围栏外）。
        Failed = "failed",
    }
}

/// `history-tail`：一份会话的尾段从哪个字节起（正文另走 `history-read` 分页拉）。
pub(crate) struct HistoryTailCmd;

impl Command for HistoryTailCmd {
    const NAME: &'static str = "history-tail";
    type Req = HistoryTailReq;
    type Reply = TailPlan;
    type Err = HistoryTailErr;
}

impl Blocking for HistoryTailCmd {
    fn run(req: HistoryTailReq, _cx: &Cx) -> Result<TailPlan, HistoryTailErr> {
        tail_at(&home(), req)
    }
}

/// [`HistoryTailCmd`] 的本体，家目录是参数。
pub(crate) fn tail_at(home: &Path, req: HistoryTailReq) -> Result<TailPlan, HistoryTailErr> {
    history_query::tail_now(
        home,
        &req.path,
        usize::try_from(req.n).unwrap_or(usize::MAX),
    )
    .map_err(HistoryTailErr::Failed)
}

// ───────────────────────── history-record ─────────────────────────

/// `history-record` 的入参（resume 之前问；只收 sid）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct HistoryRecordReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 这次 resume 要用的账号配置目录（缺 / `null` ⇒ 这台的家目录）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) config_dir: Option<String>,
}

codes! {
    /// `history-record` 自己没有别的码（sid 形状不对 · 账号根不认 ⇒ `bad_args`）。
    pub(crate) enum HistoryRecordErr {}
}

/// `history-record`：这条会话的记录还在不在。
pub(crate) struct HistoryRecordCmd;

impl Command for HistoryRecordCmd {
    const NAME: &'static str = "history-record";
    type Req = HistoryRecordReq;
    type Reply = RecordProbe;
    type Err = HistoryRecordErr;
}

impl Blocking for HistoryRecordCmd {
    fn run(req: HistoryRecordReq, _cx: &Cx) -> Result<RecordProbe, HistoryRecordErr> {
        record_at(&home(), req)
    }
}

/// [`HistoryRecordCmd`] 的本体，家目录是参数。
pub(crate) fn record_at(
    home: &Path,
    req: HistoryRecordReq,
) -> Result<RecordProbe, HistoryRecordErr> {
    history_query::record_for(home, req.config_dir.as_deref(), &req.sid)
        .map_err(HistoryRecordErr::BadArgs)
}

// ───────────────────────── sessions-needs ─────────────────────────

/// `sessions-needs` 的回包：这台上需手动的会话，先答的在前。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct NeedsList {
    /// 此刻活着、那台说在等人的会话（`needs.rank` 先答的在前，同一档等得久的在前）；一个都没有 ⇒ 空。
    pub(crate) waiting: Vec<crate::observe::accounts_query::NeedsRow>,
}

codes! {
    /// `sessions-needs` 自己没有别的码。
    pub(crate) enum SessionsNeedsErr {}
}

/// `sessions-needs`：这台上需手动的会话清单（一次问一台）。
pub(crate) struct SessionsNeedsCmd;

impl Command for SessionsNeedsCmd {
    const NAME: &'static str = "sessions-needs";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = NeedsList;
    type Err = SessionsNeedsErr;
}

impl Blocking for SessionsNeedsCmd {
    fn run(_req: NoArgs, _cx: &Cx) -> Result<NeedsList, SessionsNeedsErr> {
        Ok(needs_at(&home()))
    }
}

/// [`SessionsNeedsCmd`] 的本体，家目录是参数。
pub(crate) fn needs_at(home: &Path) -> NeedsList {
    NeedsList {
        waiting: crate::observe::accounts_query::live_needs(
            home,
            &crate::control::asks::sids(),
            &|n, sid| {
                let path = history_query::session_record(home, sid)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned());
                let off = path.as_deref().and_then(|path| {
                    answer_off_of(
                        &history_query::facts_holders(home, path).unwrap_or_default(),
                        history_query::facts_agent(home, path).as_deref(),
                    )
                });
                dress_ask(n, sid, off.as_ref())
            },
        ),
    }
}

// ───────────────────────── 出成品那几条的收集器 ─────────────────────────

/// 逐条收成品，整份（按序列化字节计）过 [`LINES_CAP_BYTES`] ⇒ 当场停下扫描、回 `too_large`
/// （不截断：截断的清单会被当成完整的用）—— 同全文搜索那一份上限（`search_face::search_at`）。
pub(crate) struct Capped<T> {
    pub(crate) rows: Vec<T>,
    seen: usize,
    over: bool,
}

impl<T> Default for Capped<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            seen: 0,
            over: false,
        }
    }
}

impl<T: serde::Serialize + Clone> Capped<T> {
    pub(crate) fn push(&mut self, row: &T) -> std::io::Result<()> {
        self.seen += serde_json::to_vec(row).map_or(0, |b| b.len()) + 1;
        if self.seen > LINES_CAP_BYTES {
            self.over = true;
            return Err(std::io::Error::other("reply cap reached"));
        }
        self.rows.push(row.clone());
        Ok(())
    }

    /// 扫描的结局：超了上限的那一次停下，原因是「超了」（`Err(Ok(那一句))`），不是扫描函数报的那句 I/O 错（`Err(Err(那一句))`）。
    pub(crate) fn finish<R>(&self, res: std::io::Result<R>) -> Result<R, Result<String, String>> {
        if self.over {
            return Err(Ok(too_large(self.seen).1));
        }
        res.map_err(|e| Err(format!("stream failed: {e}")))
    }
}

/// 读成品那几条的码：读不动 ⇒ `failed`；整份装不下一帧 ⇒ `too_large`。
macro_rules! scan_codes {
    ($(#[$m:meta])* $name:ident) => {
        codes! {
            $(#[$m])*
            pub(crate) enum $name {
                /// 读不动（路径在围栏外 · 打不开 · 续点越过文件尾 / 不在行边界）。
                Failed = "failed",
                /// 整份装不下一帧（不截断）。
                TooLarge = "too_large",
            }
        }
        impl From<Result<String, String>> for $name {
            fn from(r: Result<String, String>) -> Self {
                match r {
                    Ok(said) => $name::TooLarge(said),
                    Err(said) => $name::Failed(said),
                }
            }
        }
    };
}

// ───────────────────────── 分页（index · user-inputs · turns · branch）─────────────────────────

/// 一条应答那一行里信封那一截的余量（`{"kind":"reply","id":…,"ok":true,"data":` 与收尾；`id` 是客户端起的短号）。
pub(crate) const ENVELOPE_BYTES: usize = 256;

/// **一页的装法**（四条分页命令共用这一份）：先占住回包里不随条目变的那一截（空回包按最长的续点序列化）＋ 信封，
/// 再一项一项加它序列化后的字节数（上界）；装不下 · 到了 `limit` 项 ⇒ 停，下一项就是下一页的头。
/// ⇒ 一条应答一行 ≤ 上限。一页至少装一项：一项单独就比上限大 ⇒ 这一页只有它（不切一项）。
pub(crate) struct Pager {
    room: usize,
    used: usize,
    limit: usize,
    taken: usize,
}

impl Pager {
    pub(crate) fn new(cap: usize, fixed: usize, limit: Option<u64>) -> Self {
        Pager {
            room: cap.saturating_sub(fixed + ENVELOPE_BYTES),
            used: 0,
            limit: limit.map_or(usize::MAX, |n| {
                usize::try_from(n.max(1)).unwrap_or(usize::MAX)
            }),
            taken: 0,
        }
    }

    /// 这一项（`cost` 字节）装得下 ⇒ 记上、回 `true`；装不下 ⇒ `false`（这一页到此为止）。
    pub(crate) fn admit(&mut self, cost: usize) -> bool {
        if self.taken >= self.limit || (self.taken > 0 && self.used + cost > self.room) {
            return false;
        }
        self.used += cost;
        self.taken += 1;
        true
    }

    /// 最后装进去的那一项换了个样子（还没收尾那一轮按此刻的会话事实补了字）：照新的字节数改账；超了 ⇒ `false`（调用方把它挪到下一页）。
    fn redo_last(&mut self, old: usize, new: usize) -> bool {
        self.used = self.used - old + new;
        self.taken <= 1 || self.used <= self.room
    }
}

/// 一个值序列化后几个字节。
fn jlen<T: serde::Serialize + ?Sized>(v: &T) -> usize {
    serde_json::to_vec(v).map_or(0, |b| b.len())
}

/// 一个数写成十进制几个字。
fn digits(n: u64) -> usize {
    n.checked_ilog10().map_or(1, |d| d as usize + 1)
}

/// 扫文件那一处装满一页时停下扫描用的那一句（不是读错）。
fn page_full() -> std::io::Error {
    std::io::Error::other("page full")
}

// ───────────────────────── history-index ─────────────────────────

/// `history-index` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryIndexReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从哪个字节起（缺 ⇒ 0；续传带上次的 `end`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) offset: Option<u64>,
    /// 可选：只收起点 `< until` 的行。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) until: Option<u64>,
    /// 可选：一页最多几行（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// 一张稀疏小表的行号列：每项与上一项的行号差（第一项与 −1 比；连着 ＝ 0）。
fn gap(last: &mut i64, i: usize) -> u64 {
    let i = i as i64;
    let d = (i - *last - 1) as u64;
    *last = i;
    d
}

/// 骨架索引的正文那五个数（五个全 0 的行不进表）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexBody {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) ch: Vec<u32>,
    pub(crate) cj: Vec<u32>,
    pub(crate) pl: Vec<u32>,
    pub(crate) cb: Vec<u32>,
    pub(crate) cl: Vec<u32>,
}

/// 骨架索引的来源那一列（只有带来源的行）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexSpeaker {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) v: Vec<String>,
}

/// 骨架索引里用户输入的摘要与时刻（没有时刻 ⇒ 空串）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexInputs {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) x: Vec<String>,
    pub(crate) ts: Vec<String>,
}

/// **骨架索引按列排**（17 MB 的会话一份索引六千多行，逐行对象里键名占一大半）：
///
/// - 每行都有的几格各一列：`o`（与上一行末尾的字节差，第一行与 `from` 比；连着 ＝ 0）· `n` · `t`（`classes` 里的号，
///   从 1 起，0 ＝ 不进界面）· `fd`（0 ＝ 没有）· `u`（没有 ⇒ `null`）。
/// - 稀疏的几格各一张小表（`body` · `sp` · `inputs`），`at` 是与上一项的行号差。
/// - `sc` 不上帧面（CLI 那一臂照旧逐行出 [`crate::observe::history_query::IndexRow`]）。
///
/// 界面读法只在 `session-reads.ts::unpackIndex` 一处（装回逐行，下游照旧），跨语言金样钉两侧。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryIndex {
    /// 起点字节。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页最后一行的末字节 ＝ 下一次续传该带的 `offset`（最后一页 ⇒ 最后一个完整行的末字节）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一行的起点）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 行数（每一列都这么长）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) count: usize,
    /// 本份里出现过的记录类（`t` 列的号从 1 起指它）。
    pub(crate) classes: Vec<crate::agents::record::RecordClass>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) o: Vec<u64>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) n: Vec<u64>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) t: Vec<usize>,
    pub(crate) fd: Vec<u32>,
    pub(crate) u: Vec<Option<String>>,
    pub(crate) body: IndexBody,
    pub(crate) sp: IndexSpeaker,
    pub(crate) inputs: IndexInputs,
}

/// 逐行的骨架 ⇒ 按列排的那一份（见 [`HistoryIndex`]）。
pub(crate) fn pack_index(
    from: u64,
    end: u64,
    rows: &[crate::observe::history_query::IndexRow],
) -> HistoryIndex {
    let mut ix = HistoryIndex {
        from,
        end,
        next: None,
        count: rows.len(),
        classes: Vec::new(),
        o: Vec::new(),
        n: Vec::new(),
        t: Vec::new(),
        fd: Vec::new(),
        u: Vec::new(),
        body: IndexBody::default(),
        sp: IndexSpeaker::default(),
        inputs: IndexInputs::default(),
    };
    let (mut lb, mut ls, mut li) = (-1i64, -1i64, -1i64);
    let mut prev_end = from;
    for (i, r) in rows.iter().enumerate() {
        ix.o.push(r.o.saturating_sub(prev_end));
        ix.n.push(r.n);
        prev_end = r.o + r.n;
        ix.t.push(match r.t {
            Some(c) => {
                let k = ix.classes.iter().position(|x| *x == c).unwrap_or_else(|| {
                    ix.classes.push(c);
                    ix.classes.len() - 1
                });
                k + 1
            }
            None => 0,
        });
        ix.fd.push(r.fd);
        ix.u.push(r.u.clone());
        if [r.ch, r.cj, r.pl, r.cb, r.cl].iter().any(|n| *n != 0) {
            let b = &mut ix.body;
            b.at.push(gap(&mut lb, i));
            b.ch.push(r.ch);
            b.cj.push(r.cj);
            b.pl.push(r.pl);
            b.cb.push(r.cb);
            b.cl.push(r.cl);
        }
        if let Some(v) = r.sp {
            ix.sp.at.push(gap(&mut ls, i));
            ix.sp.v.push(v.to_string());
        }
        if let Some(x) = &r.x {
            ix.inputs.at.push(gap(&mut li, i));
            ix.inputs.x.push(x.clone());
            ix.inputs.ts.push(r.ts.clone().unwrap_or_default());
        }
    }
    ix
}

scan_codes!(
    /// `history-index` 的码。
    HistoryIndexErr
);

/// `history-index`：会话骨架索引。
pub(crate) struct HistoryIndexCmd;

impl Command for HistoryIndexCmd {
    const NAME: &'static str = "history-index";
    type Req = HistoryIndexReq;
    type Reply = HistoryIndex;
    type Err = HistoryIndexErr;
}

impl Blocking for HistoryIndexCmd {
    fn run(req: HistoryIndexReq, _cx: &Cx) -> Result<HistoryIndex, HistoryIndexErr> {
        index_at(&home(), req)
    }
}

/// [`HistoryIndexCmd`] 的本体，家目录是参数。
pub(crate) fn index_at(home: &Path, req: HistoryIndexReq) -> Result<HistoryIndex, HistoryIndexErr> {
    index_page(home, req, REPLY_PAGE_BYTES)
}

/// 一行骨架索引在按列排的那一份里至多占几个字节（`i` ＝ 它在这一页里排第几；行号差 ≤ `i`，偏移差 ≤ 偏移本身）。
/// 记录类第一次出现那一行另算它在 `classes` 里那一项。
fn index_row_cost(
    r: &crate::observe::history_query::IndexRow,
    i: usize,
    seen: &mut Vec<crate::agents::record::RecordClass>,
) -> usize {
    let at = digits(i as u64) + 1;
    let mut n = digits(r.o) + digits(r.n) + digits(r.fd.into()) + jlen(&r.u) + 3 + 5;
    n += match r.t {
        Some(c) if !seen.contains(&c) => {
            seen.push(c);
            jlen(&c) + 1 + digits(seen.len() as u64)
        }
        Some(_) => digits(seen.len() as u64),
        None => 1,
    };
    let body = [r.ch, r.cj, r.pl, r.cb, r.cl];
    if body.iter().any(|v| *v != 0) {
        n += at + body.iter().map(|v| digits((*v).into()) + 1).sum::<usize>();
    }
    if let Some(v) = r.sp {
        n += at + jlen(v) + 1;
    }
    if let Some(x) = &r.x {
        n += at + jlen(x) + 1 + jlen(r.ts.as_deref().unwrap_or("")) + 1;
    }
    n
}

/// [`index_at`] 的本体，一页的上限是参数（判据拿小上限喂）。
///
/// 行从哪来：从 0 起（没有 `until`）⇒ 冷开扫描图；从中间起 ⇒ 扫描图留着且没变就从它切（分页的后几页不重扫），
/// 否则从文件那一处扫、装满一页当场停（只读这一页那一截）。
pub(crate) fn index_page(
    home: &Path,
    req: HistoryIndexReq,
    cap: usize,
) -> Result<HistoryIndex, HistoryIndexErr> {
    let offset = req.offset.unwrap_or(0);
    let fixed = jlen(&pack_index(u64::MAX, u64::MAX, &[])) + digits(u64::MAX);
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut seen = Vec::new();
    let mut rows: Vec<crate::observe::history_query::IndexRow> = Vec::new();
    let mut next = None;
    let map = if req.until.is_some() {
        None
    } else if offset == 0 {
        Some(history_query::cold_scan(home, &req.path).map_err(HistoryIndexErr::Failed)?)
    } else {
        history_query::held_scan(home, &req.path)
            .map_err(HistoryIndexErr::Failed)?
            .filter(|m| offset <= m.end)
    };
    let end = match map {
        Some(map) => {
            let start = map.index.partition_point(|r| r.o < offset);
            for r in &map.index[start..] {
                if !pager.admit(index_row_cost(r, rows.len(), &mut seen)) {
                    next = Some(r.o);
                    break;
                }
                rows.push(r.clone());
            }
            map.end
        }
        None => {
            let r = history_query::open_session_at(home, &req.path, offset)
                .map_err(HistoryIndexErr::Failed)?;
            let (target, _) =
                record_face(home, &req.path).map_err(|(_, said)| HistoryIndexErr::Failed(said))?;
            let kind = crate::agents::record_kind_of(&target).unwrap_or_default();
            let scanned = history_query::scan_session_index(kind, r, offset, req.until, |row| {
                if !pager.admit(index_row_cost(row, rows.len(), &mut seen)) {
                    next = Some(row.o);
                    return Err(page_full());
                }
                rows.push(row.clone());
                Ok(())
            });
            match scanned {
                Ok((_, end)) => end,
                Err(_) if next.is_some() => 0,
                Err(e) => return Err(HistoryIndexErr::Failed(format!("stream failed: {e}"))),
            }
        }
    };
    let mut ix = pack_index(offset, next.unwrap_or(end), &rows);
    ix.next = next;
    Ok(ix)
}

// ───────────────────────── history-user-inputs ─────────────────────────

/// `history-user-inputs` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct UserInputsReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 起点（缺 ⇒ 0；增量传上次的 `end`，下一页传 `next`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-user-inputs` 的回包：「人说过的话」清单。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UserInputs {
    /// 这份清单从哪个字节起（＝ 请求的 `from`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页管到哪个字节 ＝ 下一次增量该带的 `from`（最后一页 ⇒ 最后一个完整行的末字节）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一条那一行的起点）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 每条用户输入，对话序。
    pub(crate) entries: Vec<crate::observe::user_inputs::UserInputRow>,
}

scan_codes!(
    /// `history-user-inputs` 的码。
    UserInputsErr
);

/// `history-user-inputs`：「人说过的话」清单。
pub(crate) struct UserInputsCmd;

impl Command for UserInputsCmd {
    const NAME: &'static str = "history-user-inputs";
    type Req = UserInputsReq;
    type Reply = UserInputs;
    type Err = UserInputsErr;
}

impl Blocking for UserInputsCmd {
    fn run(req: UserInputsReq, _cx: &Cx) -> Result<UserInputs, UserInputsErr> {
        user_inputs_at(&home(), req)
    }
}

/// [`UserInputsCmd`] 的本体，家目录是参数。
pub(crate) fn user_inputs_at(home: &Path, req: UserInputsReq) -> Result<UserInputs, UserInputsErr> {
    user_inputs_page(home, req, REPLY_PAGE_BYTES)
}

/// [`user_inputs_at`] 的本体，一页的上限是参数。来源同 [`index_page`]：从 0 ⇒ 扫描图；从中间 ⇒ 图留着没变就切，否则从文件扫、满了就停。
pub(crate) fn user_inputs_page(
    home: &Path,
    req: UserInputsReq,
    cap: usize,
) -> Result<UserInputs, UserInputsErr> {
    let from = req.from.unwrap_or(0);
    let fixed = jlen(&UserInputs {
        from: u64::MAX,
        end: u64::MAX,
        next: Some(u64::MAX),
        entries: Vec::new(),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut rows = Vec::new();
    let mut next = None;
    let map = if from == 0 {
        Some(history_query::cold_scan(home, &req.path).map_err(UserInputsErr::Failed)?)
    } else {
        history_query::held_scan(home, &req.path)
            .map_err(UserInputsErr::Failed)?
            .filter(|m| from <= m.end)
    };
    let end = match map {
        Some(map) => {
            let start = map.inputs.partition_point(|r| r.at < from);
            for r in &map.inputs[start..] {
                if !pager.admit(jlen(r) + 1) {
                    next = Some(r.at);
                    break;
                }
                rows.push(r.clone());
            }
            map.end
        }
        None => {
            let r = history_query::open_user_inputs_at(home, &req.path, from)
                .map_err(UserInputsErr::Failed)?;
            let scanned = crate::observe::user_inputs::scan_user_inputs(r, from, |row| {
                if !pager.admit(jlen(row) + 1) {
                    next = Some(row.at);
                    return Err(page_full());
                }
                rows.push(row.clone());
                Ok(())
            });
            match scanned {
                Ok((_, end)) => end,
                Err(_) if next.is_some() => 0,
                Err(e) => return Err(UserInputsErr::Failed(format!("stream failed: {e}"))),
            }
        }
    };
    Ok(UserInputs {
        from,
        end: next.unwrap_or(end),
        next,
        entries: rows,
    })
}

// ───────────────────────── history-turns ─────────────────────────

/// `history-turns` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryTurnsReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从这个字节起扫（缺 ⇒ 0；续取传还在跑的那一轮的 `at`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几轮（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-turns` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryTurns {
    /// 起点字节（＝ 请求的 `from`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页管到哪个字节（有下一页 ⇒ ＝ `next`；最后一页 ⇒ 最后一个完整行的末字节，残尾不计）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一轮的 `at`）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 这一段里的每一轮，文件序；`startText` · `endText` 是看的那一台钟上的 `HH:MM`（界面照抄；解不出 ⇒ 空串）。
    pub(crate) turns: Vec<crate::observe::turns::TurnRow>,
}

scan_codes!(
    /// `history-turns` 的码。
    HistoryTurnsErr
);

/// `history-turns`：每轮的摘要。
pub(crate) struct HistoryTurnsCmd;

impl Command for HistoryTurnsCmd {
    const NAME: &'static str = "history-turns";
    type Req = HistoryTurnsReq;
    type Reply = HistoryTurns;
    type Err = HistoryTurnsErr;
}

impl Blocking for HistoryTurnsCmd {
    fn run(req: HistoryTurnsReq, cx: &Cx) -> Result<HistoryTurns, HistoryTurnsErr> {
        turns_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryTurnsCmd`] 的本体，家目录是参数。还在跑的最后一轮下次从它的 `at` 再取。
pub(crate) fn turns_at(
    home: &Path,
    req: HistoryTurnsReq,
    tz: &crate::Tz,
) -> Result<HistoryTurns, HistoryTurnsErr> {
    turns_page(home, req, tz, REPLY_PAGE_BYTES)
}

/// [`turns_at`] 的本体，一页的上限是参数。来源同 [`index_page`]。每一轮先按看的那一台的钟写好再算字节；
/// 还没收尾的最后一轮只在最后一页、按这台此刻的会话事实拼进过程行（拼完超了这一页 ⇒ 挪到下一页）。
pub(crate) fn turns_page(
    home: &Path,
    req: HistoryTurnsReq,
    tz: &crate::Tz,
    cap: usize,
) -> Result<HistoryTurns, HistoryTurnsErr> {
    let path = req.path.as_str();
    let from = req.from.unwrap_or(0);
    let fixed = jlen(&HistoryTurns {
        from: u64::MAX,
        end: u64::MAX,
        next: Some(u64::MAX),
        turns: Vec::new(),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut turns: Vec<crate::observe::turns::TurnRow> = Vec::new();
    let mut costs: Vec<usize> = Vec::new();
    let mut next = None;
    let mut take = |row: &crate::observe::turns::TurnRow, next: &mut Option<u64>| -> bool {
        let mut t = row.clone();
        t.stamp(tz);
        let c = jlen(&t) + 1;
        if !pager.admit(c) {
            *next = Some(t.at);
            return false;
        }
        turns.push(t);
        costs.push(c);
        true
    };
    let map = if from == 0 {
        Some(history_query::cold_scan(home, path).map_err(HistoryTurnsErr::Failed)?)
    } else {
        history_query::held_scan(home, path)
            .map_err(HistoryTurnsErr::Failed)?
            .filter(|m| from <= m.end)
    };
    let end = match map {
        Some(map) => {
            let start = map.turns.partition_point(|t| t.at < from);
            for t in &map.turns[start..] {
                if !take(t, &mut next) {
                    break;
                }
            }
            map.end
        }
        None => {
            let r = history_query::open_user_inputs_at(home, path, from)
                .map_err(HistoryTurnsErr::Failed)?;
            let scanned = crate::observe::turns::scan_turns(r, from, |row| {
                if take(row, &mut next) {
                    Ok(())
                } else {
                    Err(page_full())
                }
            });
            match scanned {
                Ok((_, end)) => end,
                Err(_) if next.is_some() => 0,
                Err(e) => return Err(HistoryTurnsErr::Failed(format!("stream failed: {e}"))),
            }
        }
    };
    drop(take);
    // 还没收尾的最后一轮（只会在最后一页）：「现在在做哪一步 / 在等什么」按这台此刻的会话事实拼进过程行（判定同 `history-facts`）。
    if next.is_none() {
        if let Some(last) = turns.last_mut().filter(|t| !t.done) {
            let sid = Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let holders = history_query::facts_holders(home, path).unwrap_or_default();
            let off = answer_off_of(&holders, history_query::facts_agent(home, path).as_deref());
            let needs = dress_ask(
                crate::observe::facts_query::needs_of(
                    &last.pending,
                    crate::observe::accounts_query::session_wait(home, sid).as_ref(),
                ),
                sid,
                off.as_ref(),
            );
            let live = !holders.is_empty();
            crate::observe::turns::dress_live(last, needs.as_ref(), live);
            let old = costs.last().copied().unwrap_or(0);
            if !pager.redo_last(old, jlen(&*last) + 1) {
                next = turns.pop().map(|t| t.at);
            }
        }
    }
    let reply = HistoryTurns {
        from,
        end: next.unwrap_or(end),
        next,
        turns,
    };
    match over_cap(&reply) {
        Some(said) => Err(HistoryTurnsErr::TooLarge(said)),
        None => Ok(reply),
    }
}

// ───────────────────────── history-find ─────────────────────────

/// `history-find` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryFindReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 查询串（原样；以 `--` 起头也照样是查询，不是选项）。
    pub(crate) query: String,
    /// 工具结果也搜（缺 ⇒ 不搜）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) include_tools: Option<bool>,
    /// 只留谁说的，同 `history-search` 同名同义（`user` 时工具内容不搜）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) scope: Option<super::search_face::SearchScope>,
    /// 最多列几条（缺 ⇒ 500，封顶 2000）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
    /// 跳过前几条（续下一页；缺 ⇒ 0）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) skip: Option<u64>,
}

/// `history-find` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryFind {
    /// 全量命中数（≥ 条数；大于 ⇒ 被上限砍过）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) total: u64,
    /// 这一页的命中，文件序；只搜流里画的那几张卡（自动写的应答不搜）。
    pub(crate) hits: Vec<crate::observe::search_query::FindHit>,
}

scan_codes!(
    /// `history-find` 的码。
    HistoryFindErr
);

/// `history-find`：会话内查找。
pub(crate) struct HistoryFindCmd;

impl Command for HistoryFindCmd {
    const NAME: &'static str = "history-find";
    type Req = HistoryFindReq;
    type Reply = HistoryFind;
    type Err = HistoryFindErr;
}

impl Blocking for HistoryFindCmd {
    fn run(req: HistoryFindReq, cx: &Cx) -> Result<HistoryFind, HistoryFindErr> {
        find_at(&home(), req, &cx.tz)
    }
}

/// [`HistoryFindCmd`] 的本体，家目录是参数。先走常驻索引（不再每次从头扫）；不归它管 ⇒ 现扫。
pub(crate) fn find_at(
    home: &Path,
    req: HistoryFindReq,
    tz: &crate::Tz,
) -> Result<HistoryFind, HistoryFindErr> {
    use crate::observe::search_query::{self, FindPage, FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT};
    let limit = req.limit.map_or(FIND_DEFAULT_LIMIT, |n| {
        usize::try_from(n).unwrap_or(usize::MAX).min(FIND_MAX_LIMIT)
    });
    let page = FindPage {
        skip: usize::try_from(req.skip.unwrap_or(0)).unwrap_or(usize::MAX),
        limit,
    };
    let scope = req.scope.map(super::search_face::SearchScope::word);
    let include_tools = req.include_tools.unwrap_or(false);
    let target = history_query::session_path_at(home, &req.path).map_err(HistoryFindErr::Failed)?;
    let mut hits = Capped::default();
    let scanned = match search_query::find_indexed(
        home,
        &target,
        &req.query,
        include_tools,
        scope,
        page,
        |h| hits.push(&search_query::with_hit_text(h, tz)),
    ) {
        Some(r) => r,
        None => {
            let r = history_query::open_session_at(home, &req.path, 0)
                .map_err(HistoryFindErr::Failed)?;
            search_query::scan_session_find(r, &req.query, include_tools, scope, page, |h| {
                hits.push(&search_query::with_hit_text(h, tz))
            })
        }
    };
    let (_, total) = hits.finish(scanned)?;
    Ok(HistoryFind {
        total,
        hits: hits.rows,
    })
}

// ───────────────────────── history-facts ─────────────────────────

/// `history-facts` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryFactsReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 可缺：续传令牌 ＝ 上一次应答里的 `{end, rev}`。缺 / `null` ⇒ 从字节 0 扫；给了 ⇒ 从那台续算表里那一份（[`facts_cache`]）
    /// 接着扫、累加在它上面；表里没有 ⇒ `stale_prior`（调用方不带令牌从 0 要）。带不带 `view` 都是这两格（投影不去它们）。
    ///
    /// [`facts_cache`]: crate::observe::facts_cache
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) prior: Option<crate::observe::facts_cache::FactsToken>,
}

codes! {
    /// `history-facts` 的码。
    pub(crate) enum HistoryFactsErr {
        /// 读不动（围栏外 · 打不开 · 续点越过文件尾 / 不在行边界：调用方从 0 重要一份）。
        Failed = "failed",
        /// 整份回包过上限（不截断）。
        TooLarge = "too_large",
        /// 续传令牌对不上这台续算表里的任何一份（挤掉了 · 后端重启过 · 一次性进程）：调用方不带令牌从 0 要一份。
        StalePrior = "stale_prior",
    }
}

/// `history-facts`：会话事实（分叉血缘 · 改动文件集 · 最新 usage 与上下文上限 · 项目目录 · 此刻在写它的进程 · 没结果的调用 · 最后一句 · 需手动）。
pub(crate) struct HistoryFactsCmd;

impl Command for HistoryFactsCmd {
    const NAME: &'static str = "history-facts";
    type Req = HistoryFactsReq;
    type Reply = crate::observe::facts_query::SessionFacts;
    type Err = HistoryFactsErr;
}

impl Blocking for HistoryFactsCmd {
    fn run(
        req: HistoryFactsReq,
        _cx: &Cx,
    ) -> Result<crate::observe::facts_query::SessionFacts, HistoryFactsErr> {
        // 上下文上限表住这台的偏好（`prefs.json`），要用的那一刻现读。
        let limits = crate::control::machine_prefs::read_now().prefs().limits();
        facts_at(&home(), req, &limits)
    }
}

/// [`HistoryFactsCmd`] 的本体，家目录是参数。
pub(crate) fn facts_at(
    home: &Path,
    req: HistoryFactsReq,
    limits: &crate::observe::facts_query::ContextLimits,
) -> Result<crate::observe::facts_query::SessionFacts, HistoryFactsErr> {
    use crate::observe::{accounts_query, facts_cache, facts_query};
    let path = req.path.as_str();
    let prior = match req.prior {
        None => None,
        Some(token) => Some(facts_cache::find(path, token).ok_or_else(|| {
            HistoryFactsErr::StalePrior(format!(
                "resume token {}@{} is not in this backend's table",
                token.rev, token.end
            ))
        })?),
    };
    // 中转看见过这个会话的请求 ⇒ 它带没带扩展上下文那一项（会话 id ＝ 记录文件名，中转的流标签就是它）。
    let sid = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let relay = crate::agents::context_marks()
        .into_iter()
        .find_map(|m| crate::observe::relay_marks::seen(sid, m));
    let mut facts = match prior {
        // 冷开（没有续点）⇒ 共用那张扫描图；有续点 ⇒ 从它的 `end` 接着扫、累加在它上面。
        None => history_query::cold_scan(home, path)
            .map_err(HistoryFactsErr::Failed)?
            .facts(limits, relay),
        Some(prior) => {
            let r = history_query::open_facts_at(home, path, prior.end)
                .map_err(HistoryFactsErr::Failed)?;
            facts_query::scan_facts(r, prior, limits, relay)
                .map_err(|e| HistoryFactsErr::Failed(format!("stream failed: {e}")))?
        }
    };
    if facts.project_dir.is_none() {
        facts.project_dir = history_query::facts_project_dir(home, path);
    }
    if facts.agent.is_none() {
        facts.agent = history_query::facts_agent(home, path);
    }
    // 记进续算表、铸新 rev（项目目录 · 哪一家读到即锁定、跟着进表；下面那几格每次现查，不进表）；从哪一份续来的那一份同时摘掉。
    facts_cache::keep(path, &mut facts, req.prior.map(|t| t.rev));
    // 此刻谁持着它：判活只问这一处（照那一家声明的方式判；判不了 ⇒ 没有人、且「状态不明」说是判不了）。
    let holders = history_query::facts_holders(home, path);
    facts.writers = holders.clone().unwrap_or_default();
    // 需手动：那台 pidfile 此刻说在等 ⇒ 配上记录里没结果的那一步判种类（不累加，`prior` 里那一份不用）。
    // 答题口没接上 ⇒ 那一句（[`answer_off_of`]，每次按持着它的进程现查）；配上挂着的待答请求（`needs.ask`：可以怎么答；
    // 挡着终端的那种 pidfile 不说在等，照它补出一份），没有挂着的 ⇒「只能在终端里答」换成它为什么（[`dress_ask`]）。
    facts.answer_off = answer_off_of(&facts.writers, facts.agent.as_deref());
    facts.needs = dress_ask(
        facts_query::needs_of(
            &facts.pending,
            accounts_query::session_wait(home, sid).as_ref(),
        ),
        sid,
        facts.answer_off.as_ref(),
    );
    // 后台任务运行中：那台 pidfile 此刻说是这一态 ⇒ 配上记录里还没收场的后台命令写那一句（不累加，`prior` 里那一份不用）。
    facts.background = accounts_query::session_background(home, sid).map(|born| {
        let now = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        facts_query::background_of(&facts.bg_tasks, born, now)
    });
    // 每一步还没结果时的样子：在等你 · 在跑 · 状态不明（界面只读这一格）。判不了活 ⇒ 状态不明说的是判不了。
    facts_query::settle_pending(&mut facts, holders.is_some());
    match over_cap(&facts) {
        Some(said) => Err(HistoryFactsErr::TooLarge(said)),
        None => Ok(facts),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/history_paging_tests.rs"]
mod paging_tests;
