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
use copy_core::copy_text;

/// `history-read` · `history-tail` 一页（一条应答）交出去的字节数；源字节照这个数一块一块读。
///
/// 取 1 MiB：一页就是一帧应答，它在后端写出方向上与实时 `line` 帧**排同一条队** ——
/// 页越大，实时行被挡住的时间越长；页越小，往返越多。1 MiB 在 1 Mbps 的弱网上约 8 秒一页。
/// 量的是交出去的、不是读过的：不进界面的大行（请求体那一类）只占一个号，按源字节封顶会让尾段白白多出几页往返。
/// 一页最多扫多少源字节见 `history_face::read_rows`（[`LINE_CAP_BYTES`]）。
pub(crate) const READ_PAGE_BYTES: usize = 1 << 20;

/// 单行比一页还长时，最多续读到多长（超过 ⇒ `oversized_line`）。
///
/// monitor 那头单帧上限 64 MiB（`stream_source::BACKEND_FRAME_LINE_CAP`），而字节进 JSON 串
/// 要转义（引号、反斜杠）—— 留一半余量。
pub(crate) const LINE_CAP_BYTES: usize = 32 << 20;

/// 按行那六条整份输出的上限（同上，留一半余量）。
pub(crate) const LINES_CAP_BYTES: usize = 32 << 20;

/// `backend-log` 一次最多回多少字节（尾部）。
pub(crate) const LOG_TAIL_BYTES: u64 = 256 << 10;

/// 本进程的 stderr 此刻落在哪份文件（`main.rs` 装上之后交进来；没装 ⇒ 空）。
static BACKEND_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// `main.rs` 装上 stderr 诊断文件之后调一次（诊断文件的门在那里，本模块只读它）。
pub fn note_backend_log(path: std::path::PathBuf) {
    let _ = BACKEND_LOG.set(path);
}

/// 那份文件的尾部 ⇒ `{path, size_text, text, truncated}`；没装 ⇒ `path: null`。截断时从截点后第一个换行起（不给半行）。
pub(crate) fn log_tail(
    path: Option<&std::path::Path>,
    max: u64,
) -> Result<LogTail, crate::common::said::Said> {
    use std::io::{Read, Seek, SeekFrom};
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
    let mut buf = Vec::new();
    f.seek(SeekFrom::Start(from))
        .and_then(|_| f.take(max).read_to_end(&mut buf))
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LogTail {
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct BackendLogReq {
    /// 只回尾部这么多字节；缺 ⇒ 封顶 256 KiB（给多了也照封顶）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) max_bytes: Option<u64>,
}

crate::wire::typed::codes! {
    /// `backend-log` 的码：那份文件读不了。
    pub(crate) enum BackendLogErr {
        Failed = "failed",
    }
}

/// `backend-log`：这台后端的 stderr 诊断文件尾部（读一份文件 ⇒ 阻塞档）。
pub(crate) struct BackendLog;

impl crate::wire::typed::Command for BackendLog {
    const NAME: &'static str = "backend-log";
    type Req = BackendLogReq;
    type Reply = LogTail;
    type Err = BackendLogErr;
}

impl crate::wire::typed::Handle for BackendLog {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BackendLogReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<LogTail, BackendLogErr>> + Send + 'static {
        async move {
            let max = req
                .max_bytes
                .map_or(LOG_TAIL_BYTES, |n| n.min(LOG_TAIL_BYTES));
            log_tail(BACKEND_LOG.get().map(|p| p.as_path()), max)
                .map_err(crate::wire::typed::of_fail_with(BackendLogErr::Failed))
        }
    }
}

crate::wire::typed::codes! {
    /// `drift-report` 没有别的码。
    pub(crate) enum DriftReportErr {}
}

/// `drift-report`：这台后端的漂移账（看不懂的记录类型），各家的面并在一起、注册序（纯内存读一把锁，同 `forward-list`）。
pub(crate) struct DriftReportCmd;

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

/// `drift-merge` 的一台：那台的漂移账（`drift-report` 的回包原样）；问不到 ⇒ `null`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct DriftOn {
    /// 那台的键（本机 `<local>`，别的台是机器表里的名字）。
    pub(crate) origin: String,
    pub(crate) report: Option<crate::agents::drift::DriftReport>,
}

/// `drift-merge` 的回包：本机在前，其余照机器表的顺序。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct DriftMerge {
    pub(crate) machines: Vec<DriftOn>,
}

crate::wire::typed::codes! {
    /// `drift-merge` 没有别的码。
    pub(crate) enum DriftMergeErr {}
}

/// `drift-merge`：本机与机器表里每台的漂移账（诊断信息要的那一份）。界面只问本机后端这一条：别的台那一份由这台经它的常驻流问
/// `drift-report`（`control/feeds.rs::call`），同时问、并好再答（[`drift_hub`]）。只读；读机器表进阻塞池。
pub(crate) struct DriftMergeCmd;

impl crate::wire::typed::Command for DriftMergeCmd {
    const NAME: &'static str = "drift-merge";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = DriftMerge;
    type Err = DriftMergeErr;
}

impl crate::wire::typed::Handle for DriftMergeCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<DriftMerge, DriftMergeErr>> + Send + 'static {
        let until = cx.until;
        async move {
            let names = tokio::task::spawn_blocking(|| {
                crate::control::machines::answer_read_in(
                    crate::platform::paths::home_dir().as_deref(),
                )
                .hosts
                .iter()
                .map(|h| h.name().to_string())
                .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            let there = move |m: String| async move {
                crate::control::feeds::call::<DriftReportCmd>(
                    &m,
                    &crate::wire::typed::NoArgs {},
                    until,
                )
                .await
                .map_err(|f| tracing::warn!("drift-merge [{m}]: {}", f.said))
                .ok()
            };
            Ok(drift_hub(names, crate::agents::drift_report(), there).await)
        }
    }
}

/// [`DriftMergeCmd`] 并好再答的那一层（这台那一份 · 怎么问到那台由调用方给，判据喂假的）：本机在前，
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
    pub(crate) enum AccountsListErr {
        TooLarge = "too_large",
    }
}

/// `accounts-list`：账号清单，并上这台机器自己那份 apikey 表（读 manifest ＋ 每个号 stat 一下 ⇒ 阻塞档）。
pub(crate) struct AccountsListCmd;

impl crate::wire::typed::Command for AccountsListCmd {
    crate::wire::typed::by_call!(cmd_wire::accounts::AccountsListReq);
    type Err = AccountsListErr;
}

impl crate::wire::typed::Handle for AccountsListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: crate::observe::accounts_query::AccountsListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::observe::accounts_query::AccountsList, AccountsListErr>,
    > + Send
           + 'static {
        async move {
            let (kind, _) = crate::agents::pick_kind(Some(req.agent.as_str()))
                .map_err(AccountsListErr::BadArgs)?;
            let agent = crate::agents::pick_route(Some(&req.agent))
                .map_err(AccountsListErr::BadArgs)?
                .id();
            let rows = crate::accounts::upstream_select::file_face::machine_rows();
            let mut v = crate::observe::accounts_query::list_product(
                &rows,
                agent,
                crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
                crate::relay::our_relay_listening(relay_route_core::PORT),
            );
            let quota = crate::faces::rotation_face::quota_read_with(
                &crate::faces::rotation_face::Ctx::here(),
                crate::accounts::quota::now_unix(),
                &cx.tz,
            );
            with_quota(&mut v, &quota, kind);
            crate::observe::accounts_query::with_sign_in(
                &mut v,
                crate::agents::accounts_face(kind).is_some_and(|f| f.sign_in.is_some()),
            );
            let size = serde_json::to_string(&v).map_or(0, |t| t.len());
            if size > LINES_CAP_BYTES {
                return Err(AccountsListErr::TooLarge(too_large(size).said));
            }
            Ok(v)
        }
    }
}

/// 每个号带上它「5h 那一格」（同 `quota-read` 那一格，唯一一处：账读不出 ⇒ 顶上那一格；没出过数 · 按量号 ⇒ 没有）。
pub(crate) fn with_quota(
    v: &mut crate::observe::accounts_query::AccountsList,
    q: &crate::faces::quota_read::QuotaRead,
    kind: &str,
) {
    for r in &mut v.accounts {
        r.quota = q.five_hour.clone().or_else(|| {
            q.accounts
                .iter()
                .find(|a| a.head.agent.as_str() == kind && a.head.account == r.scanned.name)
                .and_then(|a| a.five_hour.clone())
        });
    }
}

/// `machine-interrupts` 的入参。
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct MachineInterruptsReq {
    /// 停的是哪台：别的台 ⇒ 这台后端经那台的常驻流问它自己那一份（会话三格）、再数自己账上通往它的转发；缺 · 本机 ⇒ 只答这台。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) machine: Option<String>,
    /// `true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) app_exit: Option<bool>,
    /// 带 `appExit` 时一并问的那几台（各照它自己的退出行为答，答到的加起来；问不到的不算）。问到那几台时不带它（不往下扇）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) machines: Option<Vec<cmd_wire::Machine>>,
}

/// `machine-interrupts` 的回包：会话三格 ＋ 转发（[`crate::observe::accounts_query::Interrupts`] 那几格）；
/// 问的是别的台而那台问不到 ⇒ `relayedSessions` 是 `null`（说不清，不当成 0），另两格 0，转发照这台账上的数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachineInterruptsReply {
    pub(crate) relayed_sessions: Option<u32>,
    pub(crate) relayed_maybe: u32,
    pub(crate) live_streams: u32,
    pub(crate) forwards: u32,
}

impl From<crate::observe::accounts_query::Interrupts> for MachineInterruptsReply {
    fn from(i: crate::observe::accounts_query::Interrupts) -> Self {
        MachineInterruptsReply {
            relayed_sessions: Some(i.relayed_sessions),
            relayed_maybe: i.relayed_maybe,
            live_streams: i.live_streams,
            forwards: i.forwards,
        }
    }
}

crate::wire::typed::codes! {
    /// `machine-interrupts` 没有别的码。
    pub(crate) enum MachineInterruptsErr {}
}

/// `machine-interrupts`：停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前会打断什么（`observe/accounts_query.rs::machine_product`）：
/// 那台的活会话经不经本机中转 · 活着的几个 · 这台账上通往那台的转发。界面只问本机后端这一条：别的台那一份由这台经它的常驻流问到
/// （`control/feeds.rs::call`）、并好再答（[`interrupts_hub`]）。只读；这台自己那一份读 pidfile ⇒ 进阻塞池。
pub(crate) struct MachineInterrupts;

impl crate::wire::typed::Command for MachineInterrupts {
    const NAME: &'static str = "machine-interrupts";
    type Req = MachineInterruptsReq;
    type Reply = MachineInterruptsReply;
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
                let home = crate::history::listing::agent_home();
                tokio::task::spawn_blocking(move || interrupts_at(&home, &r))
                    .await
                    .unwrap_or(MachineInterruptsReply {
                        relayed_sessions: None,
                        relayed_maybe: 0,
                        live_streams: 0,
                        forwards: 0,
                    })
            };
            let there = move |m: String, r: MachineInterruptsReq| async move {
                crate::control::feeds::call::<MachineInterrupts>(&m, &r, until)
                    .await
                    .map_err(|f| tracing::warn!("machine-interrupts [{m}]: {}", f.said))
                    .ok()
            };
            Ok(interrupts_hub(req, here, there).await)
        }
    }
}

/// [`MachineInterrupts`] 并好再答的那一层（这台自己怎么答 · 怎么问到那台由调用方给，判据喂假的）：
/// - `machine` 是别的台 ⇒ 那台那一份（不带 `machine`）的会话三格 ＋ 这台那一份（带 `machine`）的转发，两边同时问；那台问不到 ⇒ 会话那一格 `null`。
/// - `appExit` ⇒ 这台那一份 ＋ `machines` 里各台（只带 `appExit`）答到的加起来；问不到的那台不算。
/// - 别的（缺 `machine` · 本机）⇒ 只答这台。
pub(crate) async fn interrupts_hub<H, HF, T, TF>(
    req: MachineInterruptsReq,
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
        let mut names: Vec<String> = Vec::new();
        for m in req.machines.iter().flatten() {
            if let Some(n) = m.host_name() {
                if !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
        }
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
            });
    }
    let remote = req
        .machine
        .as_deref()
        .filter(|m| cmd_wire::Machine::named(*m).is_remote())
        .map(str::to_string);
    let Some(name) = remote else {
        return here(MachineInterruptsReq {
            machine: None,
            app_exit: None,
            machines: None,
        })
        .await;
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
        },
    }
}

/// 这台自己那一份（[`interrupts_hub`] 的 `here`），家目录是参数（判据拿夹具喂它）。
pub(crate) fn interrupts_at(
    home: &std::path::Path,
    req: &MachineInterruptsReq,
) -> MachineInterruptsReply {
    use crate::observe::accounts_query;
    let lines = accounts_query::session_accounts_here(home);
    if req.app_exit == Some(true) {
        let kill = crate::control::exit_policy::read_now().kill_on_exit();
        return accounts_query::app_exit_product(
            kill,
            &lines,
            crate::dial::forwards::running_all(),
        )
        .into();
    }
    let forwards = req
        .machine
        .as_deref()
        .map_or(0, crate::dial::forwards::running_to_machine);
    accounts_query::machine_product(&lines, forwards).into()
}

/// 过围栏 ⇒ 这份记录是哪一家的（注册表按它落在谁的根下认），解释交那一家。
pub(crate) fn record_face(
    home: &std::path::Path,
    path: &str,
) -> Result<(std::path::PathBuf, crate::agents::RecordFace), crate::common::said::Said> {
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
    tz: &crate::Tz,
) -> crate::observe::query::body::Reader<'a> {
    crate::observe::query::body::Reader::new(face, tz.clone())
}

pub(crate) fn too_large(size: usize) -> crate::common::said::Said {
    too_large_at(size, LINES_CAP_BYTES)
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
/// 没有挂着的、而答题口没接上（`off`，[`answer_off_of`]）⇒「只能在终端里答」那一句换成它为什么。
/// 会话事实 · 过程那一轮 · 需手动清单三处都经这里（同一条会话三处说的是同一句）。
pub(crate) fn dress_ask(
    needs: Option<crate::observe::recognize::facts::Needs>,
    sid: &str,
    off: Option<&crate::common::cells::Words>,
) -> Option<crate::observe::recognize::facts::Needs> {
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let mut needs = crate::observe::recognize::facts::dress_ask_with(
        needs,
        crate::control::asks::pending(sid).into_iter().next(),
        now,
    );
    if let (Some(n), Some(off)) = (needs.as_mut(), off) {
        if n.ask.is_none() {
            n.manual_text = Some(off.clone());
        }
    }
    needs
}

/// 答题口没接上 ⇒ 写好的那一句（会话事实 `answerOff`）；接上了 / 判不了 ⇒ `None`。按持着它的进程现查（`writers` 是它们的 pid）：
/// ccm 起它时挂不上答题 hook ⇒ 起的那个进程环境里带着那个变量（值说原因）；换了启动器那一支（ccm 的便条说插件是经环境交下去的）、
/// 持着它的进程环境里没有我们那个插件目录 ⇒ 启动器没把环境传下去；身上看不出 ccm 挂过 ⇒ 别处起的（`agent` 那一家有答题口才这么说）。
pub(crate) fn answer_off_of(
    writers: &[u32],
    agent: Option<&str>,
) -> Option<crate::common::cells::Words> {
    use crate::agents::asks::HookOff;
    use crate::platform::proc::EnvRead;
    writers.iter().find_map(|pid| {
        match crate::platform::proc::proc_env_var(*pid, crate::agents::asks::ANSWER_HOOK_OFF_ENV) {
            EnvRead::Value(v) => Some(crate::agents::asks::hook_off_words(&v)),
            EnvRead::Unset => {
                let off = if plugin_lost(*pid) {
                    Some(HookOff::EnvLost)
                } else if started_elsewhere(*pid, agent) {
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

/// ccm 给这个进程留的便条说答题插件经环境交下去了，它的环境里却没有我们那个目录（启动器把环境清掉了）。
/// 持着会话的这个进程是不是别处起的（判法住 `agents::asks::started_elsewhere`；这里只把那几样从进程身上读出来）。
fn started_elsewhere(pid: u32, agent: Option<&str>) -> bool {
    let has_ask_face = agent.and_then(crate::agents::ask_face).is_some();
    let noted = crate::platform::paths::data_home().is_some_and(|data| {
        let started = crate::platform::proc::start_epoch_from_ticks(
            crate::platform::proc::proc_starttime(pid),
        );
        crate::control::launch_account::note_of(&data, pid, started).is_some()
    });
    let plugin_here = crate::platform::paths::home_dir().is_some_and(|home| {
        let ours = crate::agents::asks::plugin_dir_under(&home.to_string_lossy());
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

fn plugin_lost(pid: u32) -> bool {
    let (Some(data), Some(home)) = (
        crate::platform::paths::data_home(),
        crate::platform::paths::home_dir(),
    ) else {
        return false;
    };
    let started =
        crate::platform::proc::start_epoch_from_ticks(crate::platform::proc::proc_starttime(pid));
    if !crate::control::launch_account::note_of(&data, pid, started).is_some_and(|n| n.plugin) {
        return false;
    }
    let ours = crate::agents::asks::plugin_dir_under(&home.to_string_lossy());
    crate::agents::plugin_envs().into_iter().all(|var| {
        !matches!(
            crate::platform::proc::proc_env_var(pid, var),
            crate::platform::proc::EnvRead::Value(v) if crate::agents::asks::dirs_have(&v, &ours)
        )
    })
}
