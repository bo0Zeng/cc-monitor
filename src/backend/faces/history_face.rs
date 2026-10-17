//! 读一份会话记录的那几条帧命令（typed，`wire/typed.rs`）：按字节分页（`history-read` · `history-page`）·
//! 按行号（`history-lines`）· 一个子运行（`history-run`）· 尾段在哪（`history-tail`）· 主线外（`history-branch`）·
//! 记录还在不在（`history-record`）· 这台上需手动的会话（`sessions-needs`）· 会话事实（`history-facts`）· 骨架索引等几条扫描。
//!
//! 查询本体住 `observe/query/`，这里只有线上那三个类型和接线；家目录是参数（判据拿夹具喂 `*_at`）。
//! 入方向不许出现 `observe::` 的那条线照旧：本文件住 `faces/`，与 `read_face` 同一层。

use crate::observe::query::body::{RecordLine, RunRow};
use crate::observe::query::branch::{branch_page, Branch, BranchReq};
use crate::observe::query::index::{index_page, HistoryIndex, HistoryIndexReq};
use crate::observe::query::inputs::{user_inputs_page, UserInputs, UserInputsReq};
use crate::observe::query::locate::{self, RecordProbe};
use crate::observe::query::tail::TailPlan;
use crate::observe::query::turns::{HistoryTurns, HistoryTurnsReq};
#[cfg(test)]
use crate::observe::query::ENVELOPE_BYTES;
use crate::observe::query::{body, tail};
use crate::wire::typed::{codes, of_pair, Command, NoArgs};
use crate::wire::SessionFaceExt as _;
pub(crate) use cmd_wire::needs::{AnsweredRow, NeedsList};
/// 这两条的回包住 `cmd-wire`（手机按同一份解，记录那一格按完整记录）；后端写的那一份记录是原样的一团（缺省 `Value`）。
pub(crate) use cmd_wire::record::{HistoryLines, HistoryRead};
pub(crate) use cmd_wire::session::{Held, HistoryTail, TailHow};
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

impl crate::wire::typed::Handle for HistoryReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryRead, HistoryReadErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            read_at(&home(), req, &cx.tz)
        }
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
    let page = body::read_page(
        home,
        &req.path,
        offset,
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    Ok(HistoryRead {
        rows: crate::observe::query::body::rows_of(
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

impl crate::wire::typed::Handle for HistoryPageCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryPageReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryPage, HistoryPageErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            page_at(&home(), req, &cx.tz)
        }
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
    let page = body::read_page(
        home,
        &req.path,
        offset,
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    let (lines, next_seq) = crate::observe::query::body::record_lines_of_page(
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

impl crate::wire::typed::Handle for HistoryLinesCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryLinesReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryLines, HistoryLinesErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            lines_at(&home(), req, &cx.tz)
        }
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
    let page = body::read_lines(
        home,
        &req.path,
        req.from.unwrap_or(0),
        req.until,
        READ_PAGE_BYTES,
        LINE_CAP_BYTES,
    )
    .map_err(&lower)?;
    let first = page.starts.first().copied().unwrap_or(0);
    let (lines, _) = crate::observe::query::body::record_lines(
        &mut reader(&face, &target, first, tz),
        &target,
        page.from,
        page.lines
            .iter()
            .zip(page.starts.iter().copied())
            .zip(page.ends.iter().copied())
            .map(|((l, at), end)| (l.as_bytes(), at, Some(end))),
    );
    Ok(HistoryLines {
        from: page.from,
        next: page.next,
        eof: page.eof,
        end: page.end,
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

impl crate::wire::typed::Handle for HistoryRunCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryRunReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryRun, HistoryRunErr>> + Send + 'static {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            run_at(&home(), req, &cx.tz)
        }
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
        locate::run_source(home, &req.parent, req.run.as_deref(), req.tool.as_deref())
            .map_err(&lower)?;
    let path = source.to_string_lossy().into_owned();
    let (target, face) = record_face(home, &path).map_err(&lower)?;
    let page = body::read_page(home, &path, from, None, READ_PAGE_BYTES, LINE_CAP_BYTES)
        .map_err(&lower)?;
    let rows = crate::observe::query::body::run_rows(
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

impl crate::wire::typed::Handle for HistoryBranchCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BranchReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Branch, HistoryBranchErr>> + Send + 'static {
        async move { branch_at(&home(), req) }
    }
}

/// [`HistoryBranchCmd`] 的本体，家目录是参数。
pub(crate) fn branch_at(home: &Path, req: BranchReq) -> Result<Branch, HistoryBranchErr> {
    branch_page(home, req, REPLY_PAGE_BYTES).map_err(HistoryBranchErr::Failed)
}

// ───────────────────────── history-tail ─────────────────────────

/// `history-tail` 的入参：要最新几行 ＋ 手上那一份（上一次回的 `held`，之后实时行推过的）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryTailReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 要最新几行（也是「缺得比这还多就不读那一截」的那条线）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) n: u64,
    /// 上一次回的那一份（缺 ⇒ 手上什么都没有）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) held: Option<Held>,
}

codes! {
    /// `history-tail` 的码。
    pub(crate) enum HistoryTailErr {
        /// 读不动（含围栏外 · 一行比续读上限还长）。
        Failed = "failed",
    }
}

/// `history-tail`：一份会话的尾段在哪、手上那一份续不续得上，连同那一段的第一页正文一问交回（再往后走 `history-read` 分页）。
pub(crate) struct HistoryTailCmd;

impl Command for HistoryTailCmd {
    const NAME: &'static str = "history-tail";
    type Req = HistoryTailReq;
    type Reply = HistoryTail;
    type Err = HistoryTailErr;
}

impl crate::wire::typed::Handle for HistoryTailCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryTailReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryTail, HistoryTailErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move { tail_at(&home(), req, &cx.tz) }
    }
}

/// 手上那一份 × 这一次的图 ⇒ 怎么接。`holds` 核记号（只在要核时调）。**纯函数**。
///
/// 没交 · 换了路径 ⇒ 尾段；比这一次的图长（行数 / 字节任一越过）⇒ 变短了；记号那一段不再是那一段 ⇒ 被改写；
/// 缺的比 `n` 还多 ⇒ 不读那一截；其余从 `held.next` 接着读。记号是空串（那一刻一行都没有 · 那一行太长）⇒ 不核。
pub(crate) fn how_of(
    held: Option<&Held>,
    path: &str,
    plan: &TailPlan,
    n: u64,
    holds: impl FnOnce(&str) -> bool,
) -> TailHow {
    let Some(h) = held.filter(|h| h.path == path) else {
        return TailHow::Tail;
    };
    if h.next > plan.total || h.next_byte > plan.end {
        return TailHow::Truncated;
    }
    if !h.mark.is_empty() && !holds(&h.mark) {
        return TailHow::Rewritten;
    }
    if plan.total - h.next > n {
        return TailHow::Handoff;
    }
    TailHow::Resume
}

/// [`HistoryTailCmd`] 的本体，家目录是参数。
pub(crate) fn tail_at(
    home: &Path,
    req: HistoryTailReq,
    tz: &crate::Tz,
) -> Result<HistoryTail, HistoryTailErr> {
    let failed = HistoryTailErr::Failed;
    let path = req.path;
    let tail = tail::tail_now(home, &path, usize::try_from(req.n).unwrap_or(usize::MAX))
        .map_err(failed)?;
    let plan = tail.plan;
    let mark = |start: u64, end: u64| {
        tail::mark_at(home, &path, start, end, LINE_CAP_BYTES).map_err(failed)
    };
    let how = how_of(req.held.as_ref(), &path, &plan, req.n, |m| {
        tail::mark_span(m)
            .and_then(|(s, e)| mark(s, e).ok().flatten())
            .is_some_and(|now| now == m)
    });
    let (from, at) = match (how, req.held.as_ref()) {
        (TailHow::Resume, Some(h)) => (h.next, h.next_byte),
        (TailHow::Handoff, _) => (plan.total, plan.end),
        _ => (plan.tail_from, plan.split_at),
    };
    let page = read_at(
        home,
        HistoryReadReq {
            path: path.clone(),
            offset: Some(at),
            until: Some(plan.end),
        },
        tz,
    )
    .map_err(|e| failed(crate::wire::typed::Codes::into_fail(e).message))?;
    let mark = match tail.last {
        Some(last) => mark(last, plan.end)?.unwrap_or_default(),
        None => String::new(),
    };
    Ok(HistoryTail {
        held: Held {
            path,
            next: plan.total,
            next_byte: plan.end,
            mark,
        },
        plan,
        how,
        from,
        rows: page.rows,
        next: page.next,
        eof: page.eof,
    })
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

impl crate::wire::typed::Handle for HistoryRecordCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryRecordReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RecordProbe, HistoryRecordErr>> + Send + 'static
    {
        async move { record_at(&home(), req) }
    }
}

/// [`HistoryRecordCmd`] 的本体，家目录是参数。
pub(crate) fn record_at(
    home: &Path,
    req: HistoryRecordReq,
) -> Result<RecordProbe, HistoryRecordErr> {
    locate::record_for(home, req.config_dir.as_deref(), &req.sid).map_err(HistoryRecordErr::BadArgs)
}

// ───────────────────────── sessions-needs ─────────────────────────

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

impl crate::wire::typed::Handle for SessionsNeedsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<NeedsList, SessionsNeedsErr>> + Send + 'static
    {
        async move { Ok(needs_at(&home())) }
    }
}

/// [`SessionsNeedsCmd`] 的本体，家目录是参数。
pub(crate) fn needs_at(home: &Path) -> NeedsList {
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    needs_with(
        home,
        &crate::control::session_hidden::Here::read_now(),
        &annotations,
    )
}

/// [`needs_at`]；隐藏要的这台那几样 · 注解由调用方给（判据喂夹具）。
pub(crate) fn needs_with(
    home: &Path,
    here: &crate::control::session_hidden::Here,
    annotations: &crate::history::history_annotations::Table,
) -> NeedsList {
    NeedsList {
        waiting: crate::observe::accounts_query::live_needs(
            home,
            &crate::control::asks::sids(),
            &|n, sid| {
                let path = locate::session_record(home, sid)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned());
                let off = path.as_deref().and_then(|path| {
                    answer_off_of(
                        &locate::facts_holders(home, path).unwrap_or_default(),
                        locate::facts_agent(home, path).as_deref(),
                    )
                });
                dress_ask(n, sid, off.as_ref())
            },
            &|sid, bg| {
                crate::history::session_hidden::hidden_here(here, annotations, sid, bg, &|p| {
                    crate::history::session_hidden::title_here(p, annotations)
                })
            },
        ),
        // 这台家里的会话才列（登记表是进程里一份）。
        answered: crate::control::asks::answered_rows()
            .into_iter()
            .filter(|r| locate::session_record(home, &r.sid).is_ok())
            .collect(),
    }
}

// ───────────────────────── sessions-needs-merge ─────────────────────────

/// `sessions-needs-merge` 的入参：各台答回来的需手动清单，按台。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct NeedsMergeReq {
    pub(crate) machines: Vec<crate::observe::recognize::facts::NeedsMachine>,
}

codes! {
    /// `sessions-needs-merge` 自己没有码（只有入参形状不对的 `bad_args`）。
    pub(crate) enum NeedsMergeErr {}
}

/// `sessions-needs-merge`：把各台的需手动清单合成一个先后（桌面 `Ctrl+J` 走它；纯计算，不读盘 ⇒ 不进阻塞档）。
pub(crate) struct NeedsMergeCmd;

impl Command for NeedsMergeCmd {
    const NAME: &'static str = "sessions-needs-merge";
    type Req = NeedsMergeReq;
    type Reply = crate::observe::recognize::facts::NeedsMerged;
    type Err = NeedsMergeErr;
}

impl crate::wire::typed::Handle for NeedsMergeCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: NeedsMergeReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::observe::recognize::facts::NeedsMerged, NeedsMergeErr>,
    > + Send
           + 'static {
        std::future::ready(Ok(crate::observe::recognize::facts::merge_needs(req.machines)))
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

// ───────────────────────── history-index ─────────────────────────

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

impl crate::wire::typed::Handle for HistoryIndexCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryIndexReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryIndex, HistoryIndexErr>> + Send + 'static
    {
        async move { index_at(&home(), req) }
    }
}

/// [`HistoryIndexCmd`] 的本体，家目录是参数。
pub(crate) fn index_at(home: &Path, req: HistoryIndexReq) -> Result<HistoryIndex, HistoryIndexErr> {
    index_page(home, req, REPLY_PAGE_BYTES).map_err(HistoryIndexErr::Failed)
}

// ───────────────────────── history-user-inputs ─────────────────────────

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

impl crate::wire::typed::Handle for UserInputsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: UserInputsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<UserInputs, UserInputsErr>> + Send + 'static {
        async move { user_inputs_at(&home(), req) }
    }
}

/// [`UserInputsCmd`] 的本体，家目录是参数。
pub(crate) fn user_inputs_at(home: &Path, req: UserInputsReq) -> Result<UserInputs, UserInputsErr> {
    user_inputs_page(home, req, REPLY_PAGE_BYTES).map_err(UserInputsErr::Failed)
}

// ───────────────────────── history-turns ─────────────────────────

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

impl crate::wire::typed::Handle for HistoryTurnsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryTurnsReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryTurns, HistoryTurnsErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            turns_at(&home(), req, &cx.tz)
        }
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

/// [`turns_at`] 的本体，一页的上限是参数：从扫描图上切（[`crate::observe::query::turns::turns_page`]），
/// 还没收尾的最后一轮按这台此刻的会话事实拼进过程行（判定同 `history-facts`）；整份过上限 ⇒ `too_large`。
pub(crate) fn turns_page(
    home: &Path,
    req: HistoryTurnsReq,
    tz: &crate::Tz,
    cap: usize,
) -> Result<HistoryTurns, HistoryTurnsErr> {
    let path = req.path.clone();
    let reply = crate::observe::query::turns::turns_page(home, req, tz, cap, &|last| {
        dress_open_turn(home, &path, last)
    })
    .map_err(HistoryTurnsErr::Failed)?;
    match over_cap(&reply) {
        Some(said) => Err(HistoryTurnsErr::TooLarge(said)),
        None => Ok(reply),
    }
}

/// 还没收尾那一轮的「现在在做哪一步 / 在等什么」：此刻谁持着它 · 答题口接没接上 · pidfile 说在等什么。
fn dress_open_turn(home: &Path, path: &str, last: &mut crate::observe::recognize::turns::TurnRow) {
    let sid = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let holders = locate::facts_holders(home, path).unwrap_or_default();
    let off = answer_off_of(&holders, locate::facts_agent(home, path).as_deref());
    let needs = dress_ask(
        crate::observe::recognize::facts::needs_of(
            crate::agents::record_kind_of(Path::new(path)).unwrap_or_default(),
            &last.pending,
            crate::observe::accounts_query::session_wait(home, sid).as_ref(),
        ),
        sid,
        off.as_ref(),
    );
    crate::observe::recognize::turns::dress_live(last, needs.as_ref(), !holders.is_empty());
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

impl crate::wire::typed::Handle for HistoryFindCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryFindReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistoryFind, HistoryFindErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            find_at(&home(), req, &cx.tz)
        }
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
    let target = locate::session_path_at(home, &req.path).map_err(HistoryFindErr::Failed)?;
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
            let r = locate::open_session_at(home, &req.path, 0).map_err(HistoryFindErr::Failed)?;
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
    /// jsonl 路径（围栏同 `history-read`）。接着上一次算由后端按路径留着（`observe::record_scan`），调用方什么都不带。
    pub(crate) path: String,
}

codes! {
    /// `history-facts` 的码。
    pub(crate) enum HistoryFactsErr {
        /// 读不动（围栏外 · 打不开）。
        Failed = "failed",
        /// 整份回包过上限（不截断）。
        TooLarge = "too_large",
    }
}

/// `history-facts`：会话事实（分叉血缘 · 改动文件集 · 最新 usage 与上下文上限 · 项目目录 · 此刻在写它的进程 · 没结果的调用 · 最后一句 · 需手动）。
pub(crate) struct HistoryFactsCmd;

impl Command for HistoryFactsCmd {
    const NAME: &'static str = "history-facts";
    type Req = HistoryFactsReq;
    type Reply = crate::observe::recognize::facts::SessionFacts;
    type Err = HistoryFactsErr;
}

impl crate::wire::typed::Handle for HistoryFactsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryFactsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::observe::recognize::facts::SessionFacts, HistoryFactsErr>,
    > + Send
           + 'static {
        async move {
            // 上下文上限表住这台的偏好（`prefs.json`），要用的那一刻现读。
            let limits = crate::control::machine_prefs::read_now().prefs().limits();
            facts_at(&home(), req, &limits)
        }
    }
}

/// [`HistoryFactsCmd`] 的本体，家目录是参数。
pub(crate) fn facts_at(
    home: &Path,
    req: HistoryFactsReq,
    limits: &crate::observe::recognize::facts::ContextLimits,
) -> Result<crate::observe::recognize::facts::SessionFacts, HistoryFactsErr> {
    use crate::observe::{accounts_query, recognize::facts};
    let path = req.path.as_str();
    // 中转看见过这个会话的请求 ⇒ 它带没带扩展上下文那一项（会话 id ＝ 记录文件名，中转的流标签就是它）。
    let sid = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let relay = crate::agents::context_marks()
        .into_iter()
        .find_map(|m| crate::observe::relay_marks::seen(sid, m));
    let mut facts = locate::facts(home, path, limits, relay).map_err(HistoryFactsErr::Failed)?;
    if facts.project_dir.is_none() {
        facts.project_dir = locate::facts_project_dir(home, path);
    }
    if facts.agent.is_none() {
        facts.agent = locate::facts_agent(home, path);
    }
    // 此刻谁持着它：判活只问这一处（照那一家声明的方式判；判不了 ⇒ 没有人、且「状态不明」说是判不了）。
    let holders = locate::facts_holders(home, path);
    facts.writers = holders.clone().unwrap_or_default();
    // 权限档的字：那一家的权限档那一处给（同 `session-permission` 回包的 `modeText`）。
    facts.permission_mode_text = facts
        .agent
        .as_deref()
        .zip(facts.permission_mode.as_deref())
        .and_then(|(kind, mode)| crate::agents::permission_mode_text(kind, mode))
        .map(crate::common::cells::Words);
    // 模型的字：那一家的模型那一处给（同起会话的那张表的系列名）。
    facts.model_text = facts
        .agent
        .as_deref()
        .zip(facts.usage.as_ref().and_then(|u| u.model.as_deref()))
        .and_then(|(kind, model)| crate::agents::model_text(kind, model))
        .map(crate::common::cells::Words);
    // 需手动：那台 pidfile 此刻说在等 ⇒ 配上记录里没结果的那一步判种类（不累加）。
    // 答题口没接上 ⇒ 那一句（[`answer_off_of`]，每次按持着它的进程现查）；配上挂着的待答请求（`needs.ask`：可以怎么答；
    // 挡着终端的那种 pidfile 不说在等，照它补出一份），没有挂着的 ⇒「只能在终端里答」换成它为什么（[`dress_ask`]）。
    facts.answer_off = answer_off_of(&facts.writers, facts.agent.as_deref());
    // 没人答时自动答：注解里那一格 · 上一条替人答过的那一句（判定与记录都住 `control::asks`）。
    facts.auto_answer =
        crate::history::history_annotations::auto_answer_of(sid).map(|a| a.after_min);
    facts.auto_answered = crate::control::asks::answered(sid);
    facts.needs = dress_ask(
        facts::needs_of(
            crate::agents::record_kind_of(Path::new(path)).unwrap_or_default(),
            &facts.pending,
            accounts_query::session_wait(home, sid).as_ref(),
        ),
        sid,
        facts.answer_off.as_ref(),
    );
    // 后台任务运行中：那台 pidfile 此刻说是这一态 ⇒ 配上记录里还没收场的后台命令写那一句（不累加）。
    facts.background = accounts_query::session_background(home, sid).map(|born| {
        let now = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        facts::background_of(&facts.bg_tasks, born, now)
    });
    // 每一步还没结果时的样子：在等你 · 在跑 · 状态不明（界面只读这一格）。判不了活 ⇒ 状态不明说的是判不了。
    facts::settle_pending(&mut facts, holders.is_some());
    // 状态一句会走的那一截：照 pidfile 此刻那一态 ＋ 上面这几格写（不活 ⇒ 不带）。
    facts.line = facts::line_of(
        crate::history::live::live_activity(home)
            .get(sid)
            .copied()
            .flatten(),
        &facts,
        crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
    );
    // 会话血缘：由谁启动 · 起了哪几个（不累加，每次现查；子会话隐不隐藏照同一处判）。
    let (parent, children) = kin_of(home, sid, &crate::control::session_hidden::Here::read_now());
    facts.started_by = parent;
    facts.children = children;
    match over_cap(&facts) {
        Some(said) => Err(HistoryFactsErr::TooLarge(said)),
        None => Ok(facts),
    }
}

/// 这条会话的主会话与子会话（`here` ＝ 这台的会话血缘 · 启动台账 · 偏好；注解现读）：会话事实那两格。
pub(crate) fn kin_of(
    home: &Path,
    sid: &str,
    here: &crate::control::session_hidden::Here,
) -> (
    Option<crate::observe::recognize::facts::Kin>,
    Vec<crate::observe::recognize::facts::Kin>,
) {
    use crate::history::session_hidden::title_here;
    use crate::observe::recognize::facts::Kin;
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    let live = crate::history::live::live_activity(home);
    let bg = crate::observe::accounts_query::live_background(home);
    let kin = |k: &str| Kin {
        sid: k.to_string(),
        title: title_here(k, &annotations),
        face: crate::wire::SessionFace::of(crate::wire::Life::of_row(Some(
            crate::history::live::life_of(&live, k),
        ))),
        hidden: crate::history::session_hidden::hidden_here(
            here,
            &annotations,
            k,
            bg.contains(k),
            &|p| title_here(p, &annotations),
        ),
    };
    let parent = here.parent_of(sid).map(kin);
    let children = here.lineage.children_of(sid).into_iter().map(kin).collect();
    (parent, children)
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/history_paging_tests.rs"]
mod paging_tests;

#[cfg(test)]
#[path = "../../../tests/backend/faces/history_bytes_tests.rs"]
mod bytes_tests;
