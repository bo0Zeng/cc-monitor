//! 读一份会话记录的那几条帧命令（typed，`stream/inbound/typed.rs`）：按字节分页（`history-read` · `history-page`）·
//! 按行号（`history-lines`）· 一个子运行（`history-run`）· 尾段在哪（`history-tail`）· 主线外（`history-branch`）·
//! 记录还在不在（`history-record`）· 这台上需手动的会话（`sessions-needs`）。
//!
//! 查询本体住 `observe/`（`history_query` · `record_page`），这里只有线上那三个类型和接线；家目录是参数（判据拿夹具喂 `*_at`）。
//! 入方向不许出现 `observe::` 的那条线照旧：本文件住 `faces/`，与 `read_face` 同一层。

use crate::observe::history_query::{self, RecordProbe, TailPlan};
use crate::observe::record_page::{ReadRow, RecordLine, RunRow};
use crate::stream::inbound::typed::{codes, of_pair, Blocking, Command, Cx, NoArgs};
use std::path::Path;

use super::read_face::{
    dress_ask, reader, record_face, too_large, LINES_CAP_BYTES, LINE_CAP_BYTES, READ_PAGE_BYTES,
};

/// 整份回包过 [`LINES_CAP_BYTES`] ⇒ `too_large` 那一句（不截断：截断的清单会被当成完整的用）。
fn over_cap<T: serde::Serialize>(reply: &T) -> Option<String> {
    let size = serde_json::to_vec(reply).map_or(0, |b| b.len());
    (size > LINES_CAP_BYTES).then(|| too_large(size).1)
}

/// 这台的记录树根（生产只这一个值）。
fn home() -> std::path::PathBuf {
    history_query::agent_home()
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

/// 只带一份记录路径的入参（`history-branch`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PathReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
}

/// `history-branch` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Branch {
    /// 回退掉的那几条记录的 `id`（只含进界面的；文件序）；这一家的记录没有链 ⇒ 恒空。
    pub(crate) off: Vec<String>,
    /// 最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
}

codes! {
    /// `history-branch` 的码。
    pub(crate) enum HistoryBranchErr {
        /// 读不动（含围栏外）。
        Failed = "failed",
        /// 整份回包过了上限。
        TooLarge = "too_large",
    }
}

/// `history-branch`：主线外清单（冷读一次；实时那一路是帧 `session_branch`）。
pub(crate) struct HistoryBranchCmd;

impl Command for HistoryBranchCmd {
    const NAME: &'static str = "history-branch";
    type Req = PathReq;
    type Reply = Branch;
    type Err = HistoryBranchErr;
}

impl Blocking for HistoryBranchCmd {
    fn run(req: PathReq, _cx: &Cx) -> Result<Branch, HistoryBranchErr> {
        branch_at(&home(), req)
    }
}

/// [`HistoryBranchCmd`] 的本体，家目录是参数。
pub(crate) fn branch_at(home: &Path, req: PathReq) -> Result<Branch, HistoryBranchErr> {
    let map = history_query::cold_scan(home, &req.path).map_err(HistoryBranchErr::Failed)?;
    let reply = Branch {
        off: map.off.clone(),
        end: map.end,
    };
    match over_cap(&reply) {
        Some(said) => Err(HistoryBranchErr::TooLarge(said)),
        None => Ok(reply),
    }
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
            &|n, sid| dress_ask(n, sid),
        ),
    }
}
