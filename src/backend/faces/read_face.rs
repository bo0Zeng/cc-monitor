//! **只读查询的帧面宿主** —— 层 1 那一搬。
//!
//! # 它补的是哪一格
//!
//! 「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那 8 条今天全是**一次性子命令**：monitor 每问一次就
//! 新开一条 TCP+SSH+鉴权，exec 一次本二进制，读完 stdout 就断。
//! 而同一台机器上**已经有一条**长连接（流模式那条），入方向早就能一问一答
//! （`stream/inbound/`）—— 缺的只是这 8 条没登记上去。
//!
//! # 为什么住顶层，而不是 `observe/`
//!
//! 与 `files/` 同一个理由：`stream/inbound/` 不许出现 `observe::`
//! （`inbound_structure_guards::inbound_never_reaches_into_the_observe_layer`），
//! 而查询的本体就住 `observe/`。⇒ 本文件是那道线**之外**的一层薄宿主：
//! 解帧面的 `args`、把输出装进应答、给错误一个 code。**查询本身一行不在这里** ——
//! 每条都调 CLI 那一臂**同一个函数**（只是把 `out` 从 stdout 换成内存），
//! 「一份代码两种宿主」在这一族上的样子。
//!
//! # 应答的两种形状
//!
//! - **按行**（六条）：`{"lines": [...]}` —— 与 CLI 那条 stdout 逐行、trim 过、剔空行后一致；
//!   整份输出超过 [`LINES_CAP_BYTES`] ⇒ `too_large`（不截断：截断的清单会被当成完整的用）。
//! - **按字节分页**（`history-read`）：一份会话可以是几百 MB，一帧装不下
//!   ⇒ 调用方给 `[offset, until)`，回一页（整行，按交出去的字节封顶）＋ 续点，调用方循环到 `eof`。
//!   `history-tail` 只回那张「尾段在哪」的图，正文仍走 `history-read`。
//!
//! # 它**不**做什么
//!
//! 不拨号、不起进程、不写盘 —— 全是既有读函数的换壳。`readonly_guard` 那条写盘禁令照旧管它。

use crate::platform::fs::Ro;
use cmd_wire::machine::{MachineInterruptsReply, MachineInterruptsReq};
use copy_core::{copy_text, Discard, Why};

/// `history-read` · `history-tail` 一页（一条应答）交出去的字节数；源字节照这个数一块一块读。
///
/// 取 1 MiB：一页就是一帧应答，它在后端写出方向上与实时 `line` 帧**排同一条队** ——
/// 页越大，实时行被挡住的时间越长；页越小，往返越多。1 MiB 在 1 Mbps 的弱网上约 8 秒一页。
/// 量的是交出去的、不是读过的：不进界面的大行（请求体那一类）只占一个号，按源字节封顶会让尾段白白多出几页往返。
/// 一页最多扫多少源字节见 `history_face::read_rows`（[`LINE_CAP_BYTES`]）。
pub(crate) const READ_PAGE_BYTES: cap_core::Cap = cap_core::Cap {
    bytes: 1 << 20,
    over: cap_core::Over::Chunk,
};

/// 单行比一页还长时，最多续读到多长（超过 ⇒ `oversized_line`）。
///
/// monitor 那头单帧上限 64 MiB（`stream_source::BACKEND_FRAME_LINE_CAP`），而字节进 JSON 串
/// 要转义（引号、反斜杠）—— 留一半余量。
pub(crate) const LINE_CAP_BYTES: cap_core::Cap = cap_core::Cap {
    bytes: 32 << 20,
    over: cap_core::Over::Reject,
};

/// 按行那六条整份输出的上限（同上，留一半余量）。
pub(crate) const LINES_CAP_BYTES: cap_core::Cap = cap_core::Cap {
    bytes: 32 << 20,
    over: cap_core::Over::Reject,
};

/// `backend-log` 一次最多回多少字节（尾部）。
pub(crate) const LOG_TAIL_BYTES: cap_core::Cap = cap_core::Cap {
    bytes: 256 << 10,
    over: cap_core::Over::TruncateAndSay,
};

/// 本进程的 stderr 此刻落在哪份文件（进程入口装上之后交进来；没装 ⇒ 空）。
static BACKEND_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// 进程入口（`stream/entry.rs`）装上 stderr 诊断文件之后调一次（诊断文件的门在那里，本模块只读它）。
pub fn note_backend_log(path: std::path::PathBuf) {
    BACKEND_LOG.set(path).discard(Why::SetOnce);
}

/// 那份文件的尾部 ⇒ `{path, size_text, text, truncated}`；没装 ⇒ `path: null`。截断时从截点后第一个换行起（不给半行）。
pub(crate) fn log_tail(
    path: Option<&std::path::Path>,
    max: u64,
) -> Result<LogTail, crate::common::said::Said> {
    use std::io::{Seek, SeekFrom};
    let Some(path) = path else {
        return Ok(LogTail {
            path: None,
            size_text: None,
            text: String::new(),
            truncated: false,
        });
    };
    let mut f =
        Ro::open(path).map_err(|e| crate::common::said::Said::new("failed", e.to_string()))?;
    let size = f
        .metadata()
        .map_err(|e| crate::common::said::Said::new("failed", e.to_string()))?
        .len();
    let from = size.saturating_sub(max);
    let buf = f
        .seek(SeekFrom::Start(from))
        .and_then(|_| cap_core::read_window(&mut f, max))
        .map_err(|e| crate::common::said::Said::new("failed", e.to_string()))?;
    let body = if from > 0 {
        buf.iter()
            .position(|&b| b == b'\n')
            .map_or(&buf[..0], |k| &buf[k + 1..])
    } else {
        &buf[..]
    };
    Ok(LogTail {
        path: Some(path.display().to_string()),
        size_text: Some(crate::common::cells::Words(copy_core::size_text(size))),
        text: String::from_utf8_lossy(body).into_owned(),
        truncated: from > 0,
    })
}

/// `backend-log` 的应答：这台后端 stderr 诊断文件的尾部。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LogTail {
    /// 本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null`。
    pub(crate) path: Option<String>,
    /// 那份文件的总大小，写好的字（`copy_core::size_text`）；没装 ⇒ `null`。
    pub(crate) size_text: Option<crate::common::cells::Words>,
    /// 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行。
    pub(crate) text: String,
    /// 前面还有没回的字节。
    pub(crate) truncated: bool,
}

/// `backend-log` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BackendLogReq {
    /// 只回尾部这么多字节；缺 ⇒ 封顶 256 KiB（给多了也照封顶）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable, type = "number"))]
    pub(crate) max_bytes: Option<u64>,
}

crate::wire::typed::codes! {
    /// `backend-log` 的码：那份文件读不了。
    pub enum BackendLogErr {
        Failed = "failed",
    }
}

/// `backend-log`：这台后端的 stderr 诊断文件尾部（读一份文件 ⇒ 阻塞档）。
pub struct BackendLog;

impl crate::wire::typed::Command for BackendLog {
    const NAME: &'static str = "backend-log";
    type Req = BackendLogReq;
    type Reply = LogTail;
    type Err = BackendLogErr;
}

impl crate::wire::typed::Handle for BackendLog {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: BackendLogReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<LogTail, BackendLogErr> {
        let max = req
            .max_bytes
            .map_or(LOG_TAIL_BYTES.bytes, |n| n.min(LOG_TAIL_BYTES.bytes));
        log_tail(BACKEND_LOG.get().map(|p| p.as_path()), max)
            .map_err(crate::wire::typed::of_fail_with(BackendLogErr::Failed))
    }
}

crate::wire::typed::codes! {
    /// `drift-report` 没有别的码。
    pub enum DriftReportErr {}
}

/// `drift-report`：这台后端的漂移账（看不懂的记录类型），各家的面并在一起、注册序（纯内存读一把锁，同 `forward-list`）。
pub struct DriftReportCmd;

impl crate::wire::typed::Command for DriftReportCmd {
    const NAME: &'static str = "drift-report";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = crate::agents::drift::DriftReport;
    type Err = DriftReportErr;
}

impl crate::wire::typed::Handle for DriftReportCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<crate::agents::drift::DriftReport, DriftReportErr>>
           + Send
           + 'static {
        std::future::ready(Ok(crate::agents::drift_report()))
    }
}

/// 各台漂移账并好的一台：那台的漂移账（`drift-report` 的回包原样）；问不到 ⇒ 没有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DriftOn {
    /// 那台的键（本机 `<local>`，别的台是机器表里的名字）。
    pub(crate) origin: String,
    pub(crate) report: Option<crate::agents::drift::DriftReport>,
}

/// 各台漂移账并好的那一份：本机在前，其余照机器表的顺序（只有诊断信息用，`diagnostics_face.rs`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DriftMerge {
    pub(crate) machines: Vec<DriftOn>,
}

/// 这一端机器表里各台的名字（机器表的顺序；读盘进阻塞池，读不到 ⇒ 空）。
pub(crate) async fn machine_names() -> Vec<String> {
    tokio::task::spawn_blocking(|| {
        crate::control::machines::answer_read_in(crate::platform::paths::home_dir().as_deref())
            .hosts
            .iter()
            .map(|h| h.name().to_string())
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default()
}

/// 各台的漂移账并成一份（诊断信息那一段用；这台那一份 · 怎么问到那台由调用方给，判据喂假的）：本机在前，
/// `names`（机器表的顺序）里每台各问一次、同时问；表里写着本机 · 写了两遍的只算一次；问不到 ⇒ 那一行 `null`。
pub(crate) async fn drift_hub<T, TF>(
    names: Vec<String>,
    mine: crate::agents::drift::DriftReport,
    there: T,
) -> DriftMerge
where
    T: Fn(String) -> TF,
    TF: std::future::Future<Output = Option<crate::agents::drift::DriftReport>> + Send + 'static,
{
    let mut order: Vec<String> = Vec::new();
    for n in names {
        if !cmd_wire::Machine::named(&n).is_remote() || order.contains(&n) {
            continue;
        }
        order.push(n);
    }
    let mut asks = tokio::task::JoinSet::new();
    for (i, n) in order.iter().enumerate() {
        let ask = there(n.clone());
        asks.spawn(async move { (i, ask.await) });
    }
    let mut got: Vec<Option<crate::agents::drift::DriftReport>> = vec![None; order.len()];
    while let Some(done) = asks.join_next().await {
        if let Ok((i, r)) = done {
            got[i] = r;
        }
    }
    let mut machines = vec![DriftOn {
        origin: cmd_wire::machine::LOCAL.to_string(),
        report: Some(mine),
    }];
    machines.extend(
        order
            .into_iter()
            .zip(got)
            .map(|(origin, report)| DriftOn { origin, report }),
    );
    DriftMerge { machines }
}

crate::wire::typed::codes! {
    /// `accounts-list` 的码：整份超过一帧装得下的。
    pub enum AccountsListErr {
        TooLarge = "too_large",
    }
}

/// `accounts-list`：账号清单，并上这台机器自己那份 apikey 表（读 manifest ＋ 每个号 stat 一下 ⇒ 阻塞档）。
pub struct AccountsListCmd;

impl crate::wire::typed::Command for AccountsListCmd {
    crate::wire::typed::by_call!(cmd_wire::accounts::AccountsListReq);
    type Err = AccountsListErr;
}

impl crate::wire::typed::Handle for AccountsListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: cmd_wire::accounts::AccountsListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<crate::accounts::manage::library::AccountsList, AccountsListErr> {
        let (kind, _) =
            crate::agents::pick_kind(Some(req.agent.as_str())).map_err(AccountsListErr::BadArgs)?;
        let agent = crate::agents::pick_route(Some(&req.agent))
            .map_err(AccountsListErr::BadArgs)?
            .id();
        let mut v = crate::faces::launch_account_face::list_of(agent);
        let now = crate::common::time::now_secs();
        let quota = crate::faces::rotation_face::quota_read_with(
            &crate::faces::rotation_face::Ctx::here(),
            now,
            &cx.tz,
        );
        with_quota(
            &mut v,
            &quota,
            kind,
            &crate::common::time::TextClock::new(now, &cx.tz),
        );
        // 每一行照它自己那一家的登录形（不拿请求那一家套整张表）。
        crate::accounts::manage::library::with_sign_in(&mut v, |k| {
            k.and_then(crate::agents::accounts_face)
                .and_then(|f| f.sign_in)
        });
        let size = serde_json::to_string(&v).map_or(0, |t| t.len());
        if size > LINES_CAP_BYTES.as_len() {
            return Err(AccountsListErr::TooLarge(too_large(size).said));
        }
        Ok(v)
    }
}

/// ★ 每个号带上它的额度格与额度几行（同一份 `quota-read` 成品里取，按 kind ＋ 号名对；唯一一处，两端照摆）：
/// 紧凑那一格：账读不出 ⇒ 顶上那一句（`5h 读不到`）；出过数 ⇒ [`slot_cells`]；按量号没出过数 ⇒ 用量那一格；别的 ⇒ 空。
/// 几行（设置页）：账读不出 ⇒ 一行顶上那一句；账上有它 ⇒ 那一号那一段照 [`crate::faces::quota_rows::account_line_rows`]；别的 ⇒ 空。
pub(crate) fn with_quota(
    v: &mut crate::accounts::manage::library::AccountsList,
    q: &crate::faces::quota_read::QuotaRead,
    kind: &str,
    clock: &crate::common::time::TextClock,
) {
    use crate::faces::quota_rows::account_line_rows;
    for r in &mut v.accounts {
        let name = r.scanned.name.as_str();
        let seen = q
            .accounts
            .iter()
            .find(|a| a.head.agent.as_str() == kind && a.head.account == name);
        let unseen = q
            .unseen
            .iter()
            .find(|u| u.head.agent.as_str() == kind && u.head.account == name);
        (r.slots, r.quota) = if let Some(head) = crate::faces::quota_rows::head_five_hour(q.state) {
            let tone = crate::common::cells::Tone::Plain;
            let line = vec![cmd_wire::quota::Cell {
                text: head.clone(),
                tone,
            }];
            (vec![cell(cmd_wire::quota::SLOT_5H, head, tone)], vec![line])
        } else if let Some(a) = seen {
            (
                slot_cells(&a.head.show, clock),
                account_line_rows(&a.rows, a.head.show.login, a.head.show.stale),
            )
        } else if let Some(u) = unseen {
            let slots = (u.head.kind == cmd_wire::quota::Kind::Api)
                .then(|| cell("", u.head.usage.text.clone(), u.head.usage.tone))
                .into_iter()
                .collect();
            (slots, account_line_rows(&u.rows, u.head.login, false))
        } else {
            (Vec::new(), Vec::new())
        };
    }
}

/// 一格只有字与语气的（账读不出 · 按量号）。
fn cell(
    slot: &str,
    text: crate::common::cells::Words,
    tone: crate::common::cells::Tone,
) -> cmd_wire::quota::SlotShow {
    cmd_wire::quota::SlotShow {
        slot: slot.to_string(),
        pct: None,
        resets_at: None,
        full: false,
        text,
        tone,
        per_pct_text: None,
    }
}

/// ★ 出过数的一个号 ⇒ 紧凑那一格：按量号 ⇒ 用量那一格；订阅号 ⇒ 头一个语义位那一格，字是整格
/// （位名 · `slot_words` 写的值 · 重置照 `quota_rows::slot_reset`：5h 恒带、别的位只在它是卡人的那一格又被拒时带），语气照 `slot_words`，数旧 ⇒ plain 画 warn。
fn slot_cells(
    show: &cmd_wire::quota::QuotaShow,
    clock: &crate::common::time::TextClock,
) -> Vec<cmd_wire::quota::SlotShow> {
    use crate::common::cells::Words;
    use cmd_wire::quota::{Kind, QuotaState, SLOT_5H};
    if show.kind == Kind::Api {
        return vec![cell("", show.usage.text.clone(), show.usage.tone)];
    }
    show.slots
        .iter()
        .take(1)
        .map(|s| {
            let here = show.usage.slot.as_deref() == Some(s.slot.as_str());
            let label = crate::accounts::quota::name_words::slot_text(&s.slot);
            let reset = crate::faces::quota_rows::slot_reset(s.resets_at, clock)
                .filter(|_| s.slot == SLOT_5H || (here && show.state == QuotaState::Refused));
            let text = match reset {
                Some(r) => copy_text(
                    "acct.usage.wvr",
                    &[("w", &label), ("v", &s.text.0), ("r", &r.0)],
                ),
                None => copy_text("acct.usage.wv", &[("w", &label), ("v", &s.text.0)]),
            };
            let tone = crate::faces::quota_rows::stale_tone(s.tone, show.stale);
            cmd_wire::quota::SlotShow {
                text: Words(text),
                tone,
                ..s.clone()
            }
        })
        .collect()
}

/// 账上那几格 ⇒ 线上那一形。
fn reply_of(i: crate::accounts::manage::library::Interrupts) -> MachineInterruptsReply {
    MachineInterruptsReply {
        relayed_sessions: Some(i.relayed_sessions),
        relayed_maybe: i.relayed_maybe,
        live_streams: i.live_streams,
        forwards: i.forwards,
        rows: Vec::new(),
    }
}

crate::wire::typed::codes! {
    /// `machine-interrupts` 没有别的码。
    pub enum MachineInterruptsErr {}
}

/// `machine-interrupts`：停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前会打断什么（`accounts/manage/library.rs::machine_product`）：
/// 那台的活会话经不经本机中转 · 活着的几个 · 这台账上通往那台的转发。界面只问本机后端这一条：别的台那一份由这台经它的常驻流问到
/// （`control/feeds.rs::call`）、并好再答（[`interrupts_hub`]），再排成确认框里那两行（[`rows_of`]）。只读；这台自己那一份读 pidfile ⇒ 进阻塞池。
pub struct MachineInterrupts;

impl crate::wire::typed::Command for MachineInterrupts {
    crate::wire::typed::by_call!(MachineInterruptsReq);
    type Err = MachineInterruptsErr;
}

impl crate::wire::typed::Handle for MachineInterrupts {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineInterruptsReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineInterruptsReply, MachineInterruptsErr>>
           + Send
           + 'static {
        let until = cx.until;
        async move {
            let here = |r: MachineInterruptsReq| async move {
                let home = crate::agents::local_homes();
                tokio::task::spawn_blocking(move || interrupts_at(&home, &r))
                    .await
                    .unwrap_or(MachineInterruptsReply {
                        relayed_sessions: None,
                        relayed_maybe: 0,
                        live_streams: 0,
                        forwards: 0,
                        rows: Vec::new(),
                    })
            };
            let there = move |m: String, r: MachineInterruptsReq| async move {
                crate::control::feeds::call::<MachineInterrupts>(&m, &r, until)
                    .await
                    .map_err(|f| tracing::warn!("machine-interrupts [{m}]: {}", f.said))
                    .ok()
            };
            let fleet = crate::dial::remote_ask::REACH.asked();
            let mut got = interrupts_hub(req.clone(), &fleet, here, there).await;
            got.rows = rows_of(&req, &got);
            Ok(got)
        }
    }
}

/// [`MachineInterrupts`] 并好再答的那一层（这台自己怎么答 · 怎么问到那台由调用方给，判据喂假的）：
/// - `machine` 是别的台 ⇒ 那台那一份（不带 `machine`）的会话三格 ＋ 这台那一份（带 `machine`）的转发，两边同时问；那台问不到 ⇒ 会话那一格 `null`。
/// - `appExit` ⇒ 这台那一份 ＋（带 `fleet` 时）`fleet` 里各台远端（只带 `appExit`）答到的加起来；问不到的那台不算。
/// - 别的（缺 `machine` · 本机）⇒ 只答这台。
pub(crate) async fn interrupts_hub<H, HF, T, TF>(
    req: MachineInterruptsReq,
    fleet: &[cmd_wire::Machine],
    here: H,
    there: T,
) -> MachineInterruptsReply
where
    H: Fn(MachineInterruptsReq) -> HF,
    HF: std::future::Future<Output = MachineInterruptsReply>,
    T: Fn(String, MachineInterruptsReq) -> TF,
    TF: std::future::Future<Output = Option<MachineInterruptsReply>> + Send + 'static,
{
    if req.app_exit == Some(true) {
        let own = MachineInterruptsReq {
            app_exit: Some(true),
            ..Default::default()
        };
        let names: Vec<String> = if req.fleet == Some(true) {
            fleet
                .iter()
                .filter_map(|m| m.host_name().map(str::to_string))
                .collect()
        } else {
            Vec::new()
        };
        let mut asks = tokio::task::JoinSet::new();
        for n in names {
            asks.spawn(there(n, own.clone()));
        }
        let mine = here(own.clone()).await;
        let mut theirs = Vec::new();
        while let Some(got) = asks.join_next().await {
            theirs.push(got.ok().flatten());
        }
        return theirs
            .into_iter()
            .flatten()
            .fold(mine, |a, b| MachineInterruptsReply {
                relayed_sessions: Some(
                    a.relayed_sessions.unwrap_or(0) + b.relayed_sessions.unwrap_or(0),
                ),
                relayed_maybe: a.relayed_maybe + b.relayed_maybe,
                live_streams: a.live_streams + b.live_streams,
                forwards: a.forwards + b.forwards,
                rows: Vec::new(),
            });
    }
    let remote = req
        .machine
        .as_deref()
        .filter(|m| cmd_wire::Machine::named(*m).is_remote())
        .map(str::to_string);
    let Some(name) = remote else {
        return here(MachineInterruptsReq::default()).await;
    };
    let counted = MachineInterruptsReq {
        machine: Some(name.clone()),
        ..Default::default()
    };
    let (mine, theirs) = tokio::join!(here(counted), there(name, MachineInterruptsReq::default()));
    match theirs {
        Some(t) => MachineInterruptsReply {
            forwards: mine.forwards,
            ..t
        },
        None => MachineInterruptsReply {
            relayed_sessions: None,
            relayed_maybe: 0,
            live_streams: 0,
            forwards: mine.forwards,
            rows: Vec::new(),
        },
    }
}

/// 并好的那几格 ⇒ 确认框里「中断 · 保留」两行（[`MachineInterruptsReply::rows`]，字只在这里写）；一项都没有 ⇒ 空。
/// 退出 / 重启 cc-monitor（`appExit`）那一问另数文件窗口（入参 `files`，缺 ⇒ 说数不出），实时那一项说「各台」；
/// 停的是一台 ⇒ 实时那一项说那台（本机叫本机）。重启 · 更新 ⇒ 中断几秒，停 · 卸载 ⇒ 中断到再启动。
pub(crate) fn rows_of(
    req: &MachineInterruptsReq,
    r: &MachineInterruptsReply,
) -> Vec<cmd_wire::machine::InterruptRow> {
    use cmd_wire::machine::{InterruptAct, InterruptRow};
    let brief = matches!(req.act, Some(InterruptAct::Restart | InterruptAct::Update));
    let quitting = req.app_exit == Some(true);
    let n = |v: u32| v.to_string();
    let mut items = Vec::new();
    if quitting {
        match req.files {
            None => items.push(copy_text("interrupts.item.filewinUnknown", &[])),
            Some(f) => {
                if f.unsaved > 0 {
                    items.push(copy_text(
                        "interrupts.item.unsaved",
                        &[("n", &n(f.unsaved))],
                    ));
                }
                if f.running > 0 {
                    items.push(copy_text(
                        "interrupts.item.filewinRunning",
                        &[("n", &n(f.running))],
                    ));
                }
            }
        }
    }
    match r.relayed_sessions {
        None => items.push(copy_text("interrupts.item.relayedUnknown", &[])),
        Some(0) => {}
        Some(s) => items.push(copy_text("interrupts.item.relayed", &[("n", &n(s))])),
    }
    if r.relayed_maybe > 0 {
        items.push(copy_text(
            "interrupts.item.relayedMaybe",
            &[("n", &n(r.relayed_maybe))],
        ));
    }
    if r.live_streams > 0 {
        let machine = if quitting {
            copy_text("restartNow.dialog.everywhere", &[])
        } else {
            copy_core::machine_word(&cmd_wire::Machine::named(
                req.machine.as_deref().unwrap_or(cmd_wire::LOCAL),
            ))
        };
        items.push(if brief {
            copy_text("interrupts.item.liveBrief", &[("machine", &machine)])
        } else {
            copy_text("interrupts.item.liveGone", &[("machine", &machine)])
        });
    }
    if r.forwards > 0 {
        items.push(copy_text(
            "interrupts.item.forwards",
            &[("n", &n(r.forwards))],
        ));
    }
    if items.is_empty() {
        return Vec::new();
    }
    let label = if brief {
        copy_text("interrupts.row.brief", &[])
    } else {
        copy_text("interrupts.row.untilStart", &[])
    };
    vec![
        InterruptRow { label, items },
        InterruptRow {
            label: copy_text("kit.interrupts.keep", &[]),
            items: vec![copy_text("interrupts.item.sessionsRun", &[])],
        },
    ]
}

/// 这台自己那一份（[`interrupts_hub`] 的 `here`），家目录是参数（判据拿夹具喂它）。
pub(crate) fn interrupts_at(
    home: impl crate::agents::IntoHomes,
    req: &MachineInterruptsReq,
) -> MachineInterruptsReply {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    let home: &[crate::agents::AgentHome] = &home_v;
    use crate::accounts::manage::library;
    let lines = library::session_accounts_here(home);
    if req.app_exit == Some(true) {
        let kill = crate::control::exit_policy::read_now().kill_on_exit();
        return reply_of(library::app_exit_product(
            kill,
            &lines,
            crate::dial::forwards::running_all(),
        ));
    }
    let forwards = req
        .machine
        .as_deref()
        .map_or(0, crate::dial::forwards::running_to_machine);
    reply_of(library::machine_product(&lines, forwards))
}

/// 过围栏 ⇒ 这份记录是哪一家的（注册表按它落在谁的根下认），解释交那一家。
pub(crate) fn record_face(
    home: impl crate::agents::IntoHomes,
    path: &str,
) -> Result<(std::path::PathBuf, crate::agents::RecordFace), crate::common::said::Said> {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    let home: &[crate::agents::AgentHome] = &home_v;
    let target = crate::observe::query::locate::session_path_at(home, path)
        .map_err(|e| crate::common::said::Said::new("refused", e))?;
    let face = crate::agents::record_face_of(&target).ok_or_else(|| {
        crate::common::said::Said::new(
            "failed",
            crate::common::contract::malformed("no adapter reads this record"),
        )
    })?;
    Ok((target, face))
}

/// 读那一份记录的读法：适配层 ＋ 看的那一台的时区。
pub(crate) fn reader<'a>(
    face: &'a crate::agents::RecordFace,
    tz: &crate::common::time::Tz,
) -> crate::observe::query::body::Reader<'a> {
    crate::observe::query::body::Reader::new(face, tz.clone())
}

pub(crate) fn too_large(size: usize) -> crate::common::said::Said {
    too_large_at(size, LINES_CAP_BYTES.as_len())
}

/// 同 [`too_large`]，上限是参数（全文搜索那条按它判，判据拿小上限验）。
pub(crate) fn too_large_at(size: usize, cap: usize) -> crate::common::said::Said {
    crate::common::said::Said::new(
        "too_large",
        copy_text(
            "beReadFace.tooLarge.say",
            &[("size", &size.to_string()), ("cap", &cap.to_string())],
        ),
    )
}

// 按命令名问本族 · 本族点名的那几条：本宿主的判据与主 crate 里引到命令表的那几条读。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/faces/read_face_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/faces/read_face_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/faces/interrupts_hub_tests.rs"]
mod interrupts_hub_tests;

#[cfg(test)]
#[path = "../../../tests/backend/faces/drift_merge_tests.rs"]
mod drift_merge_tests;

/// 会话的 `needs` 配上常驻进程里挂着的待答请求（`control::asks` 那一份登记表；配法住 `recognize::facts::dress_ask_with`）；
/// 一条都没挂着 ⇒「只能在终端里答」那一句换成它为什么：登记表记着这一家不给经钩子答这一种（[`crate::control::asks::refused`]，码 `NoAnswer`）
/// 先说它；否则答题口没接上（`off`，[`answer_off_of`]）说那个原因。
/// 会话事实 · 过程那一轮 · 需手动清单三处都经这里（同一条会话三处说的是同一句）。
pub fn dress_ask(
    needs: Option<crate::observe::recognize::facts::Needs>,
    sid: &str,
    off: Option<&crate::common::cells::Words>,
) -> Option<crate::observe::recognize::facts::Needs> {
    let now = crate::common::time::now_ms();
    let hung = crate::control::asks::pending(sid).into_iter().next();
    // 挂着一条（界面答不了的那一种也是挂着的：那一句「在终端里答」由它写好）⇒ 不换原因；只有一条都没挂着才换。
    let none_hung = hung.is_none();
    let mut needs = crate::observe::recognize::facts::dress_ask_with(needs, hung, now);
    if let Some(n) = needs.as_mut().filter(|n| none_hung && n.ask().is_none()) {
        let refused = crate::control::asks::refused(sid)
            .map(|o| crate::agents::asks::hook_off_words(o.env_value()));
        if let Some(why) = refused.or_else(|| off.cloned()) {
            n.manual(why);
        }
    }
    needs
}

/// 答题口没接上 ⇒ 写好的那一句（后端内部的 `answerOff`，经 `needs.manualText` 出）；接上了 / 判不了 ⇒ `None`。按持着它的进程现查（`writers` 是它们的 pid）：
/// ccm 起它时挂不上答题 hook ⇒ 起的那个进程环境里带着那个变量（值说原因）；换了启动器那一支（ccm 的便条说插件是经环境交下去的）、
/// 持着它的进程环境里没有我们那个插件目录 ⇒ 启动器没把环境传下去；身上看不出 ccm 挂过 ⇒ 别处起的（`agent` 那一家有答题口才这么说）。
pub(crate) fn answer_off_of(
    writers: &[u32],
    agent: Option<&str>,
) -> Option<crate::common::cells::Words> {
    answer_off_among(writers, agent, &crate::host::managed::hop_of_pid)
}

/// [`answer_off_of`] 的可喂夹具那一半：`hop_of` ＝ 持着会话的进程住的托管终端里记的那一跳（便条照会话找只经
/// [`crate::control::launch_account::session_note`]）。
pub(crate) fn answer_off_among(
    writers: &[u32],
    agent: Option<&str>,
    hop_of: &dyn Fn(u32) -> Option<crate::host::managed::Hop>,
) -> Option<crate::common::cells::Words> {
    use crate::agents::asks::HookOff;
    use crate::platform::proc::EnvRead;
    writers.iter().find_map(|pid| {
        match crate::platform::proc::proc_env_var(*pid, crate::agents::asks::ANSWER_HOOK_OFF_ENV) {
            EnvRead::Value(v) => Some(crate::agents::asks::hook_off_words(&v)),
            EnvRead::Unset => {
                let note = crate::platform::paths::data_home().and_then(|data| {
                    crate::control::launch_account::session_note(&data, *pid, hop_of)
                });
                let off = if plugin_lost(*pid, note.as_ref()) {
                    Some(HookOff::EnvLost)
                } else if started_elsewhere(*pid, agent, note.is_some()) {
                    Some(HookOff::Elsewhere)
                } else {
                    None
                };
                off.map(|o| crate::agents::asks::hook_off_words(o.env_value()))
            }
            EnvRead::Unreadable => None,
        }
    })
}

/// 持着会话的这个进程是不是别处起的（判法住 `agents::asks::started_elsewhere`；这里只把那几样从进程身上读出来）。
/// `noted` ＝ ccm 给它留过起会话便条。
fn started_elsewhere(pid: u32, agent: Option<&str>, noted: bool) -> bool {
    let has_ask_face = agent.and_then(crate::agents::ask_face).is_some();
    let plugin_here = ours_plugin_dir().is_some_and(|ours| {
        crate::agents::plugin_envs().into_iter().any(|var| {
            matches!(
                crate::platform::proc::proc_env_var(pid, var),
                crate::platform::proc::EnvRead::Value(v) if crate::agents::asks::dirs_have(&v, &ours)
            )
        })
    });
    crate::agents::asks::started_elsewhere(
        has_ask_face,
        crate::platform::proc::proc_cmdline(pid).as_deref(),
        plugin_here,
        noted,
    )
}

/// 这台我们那个答题插件目录在环境变量里那一项的写法（判只在 `agents::asks::plugin_dir_in_env`）。
fn ours_plugin_dir() -> Option<String> {
    crate::platform::paths::home_dir().and_then(|h| crate::agents::asks::plugin_dir_in_env(&h))
}

/// ccm 给这个进程留的便条（`note`）说答题插件经环境交下去了，它的环境里却没有我们那个目录（启动器把环境清掉了）。
fn plugin_lost(pid: u32, note: Option<&crate::control::launch_account::Note>) -> bool {
    let Some(ours) = ours_plugin_dir() else {
        return false;
    };
    if !note.is_some_and(|n| n.plugin) {
        return false;
    }
    crate::agents::plugin_envs().into_iter().all(|var| {
        !matches!(
            crate::platform::proc::proc_env_var(pid, var),
            crate::platform::proc::EnvRead::Value(v) if crate::agents::asks::dirs_have(&v, &ours)
        )
    })
}
