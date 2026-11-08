//! 帧命令 `quota-probe`：用某个号查一次额度 —— 命令登记 · 入参闸 · 回包形状住这里；
//! 起官方客户端、读成窗口、记进额度账那一段是 [`crate::accounts::quota::probe`]。
//! - 读不懂 ⇒ 照实回 `unreadable` ＋ 哪一处读不懂，不猜、不写账。
//! - 动作、不定时：界面「刷新」· skill · AI 按需调。

use crate::accounts::quota::probe::{probe, Ask, Probe};
use crate::faces::rotation_face::Ctx;
use crate::platform::child::Deadline;
use crate::platform::fs::Own;
use relay_route_core::account_ok;

/// 失败带码 ＋ 那一句 ＋ 原话（落账写不进那一形的原话进复制详情，[`Said`]）。
type Answer = Result<Probed, crate::common::said::Said>;

/// 这条命令的总期限（分派那一层装；略长于查额度那一次子进程的 30 秒）。
pub const PROBE_CAP: Deadline = Deadline::secs(32);

fn bad(detail: &str) -> crate::common::said::Said {
    crate::wire::spec::fail("bad_args", crate::common::contract::malformed(detail))
}

/// `quota-probe` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct QuotaProbeReq {
    /// 哪一家（线上的 kind）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 账号库里的号（配置目录末段；账号 0 是 `0`）。
    pub(crate) account: String,
}

crate::wire::typed::codes! {
    /// `quota-probe` 的码。
    pub enum ProbeErr {
        /// 官方客户端没在期限里说完。
        ChildTimedOut = "child_timed_out",
        /// 起不来 · 退出码非零（它自己那句话进原话）。
        Failed = "failed",
        /// 工作目录建不了 · 额度账写不进。
        IoFailed = "io_failed",
        /// 账号库里没有这个号。
        NotFound = "not_found",
        /// 这一家查不了 · 按量号没有额度可查。
        Unsupported = "unsupported",
    }
}

/// `quota-probe`：用某个号查一次额度（起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`；落账经写句柄）。
pub struct QuotaProbe;

impl crate::wire::typed::Command for QuotaProbe {
    const NAME: &'static str = "quota-probe";
    type Req = QuotaProbeReq;
    type Reply = Probed;
    type Err = ProbeErr;
}

impl crate::wire::typed::Handle for QuotaProbe {
    type Needs = crate::wire::typed::needs::Own;
    async fn run(
        req: QuotaProbeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<Probed, ProbeErr> {
        answer_probe(&req, &cx.needs).map_err(|f| crate::wire::typed::of_fail(f, ProbeErr::Failed))
    }
}

/// `quota-probe`：`{agent, account}` ⇒ 读到的窗口（并已记进额度账）或「读不懂」。
pub(crate) fn answer_probe(req: &QuotaProbeReq, own: &Own) -> Answer {
    answer_probe_with(
        &Ctx::here(),
        own,
        req,
        crate::common::time::now_secs(),
        None,
    )
}

/// 入参闸 ＋ 本体（[`probe`]）＋ 回包。`path` 原样交给本体（判据交一个放着假客户端的目录；生产 `None` ＝ 现问）。
pub(crate) fn answer_probe_with(
    ctx: &Ctx,
    own: &Own,
    req: &QuotaProbeReq,
    now: u64,
    path: Option<&str>,
) -> Answer {
    let route = Some(&req.agent)
        .filter(|a| !a.as_str().trim().is_empty())
        .and_then(|a| crate::agents::pick_route(Some(a)).ok())
        .ok_or_else(|| bad("`agent` must be an agent kind such as \"claude\""))?;
    let account = Some(req.account.as_str())
        .filter(|a| account_ok(a))
        .ok_or_else(|| bad("`account` must be an account id"))?;
    let ask = Ask {
        route,
        kind: req.agent.as_str(),
        account,
        now,
        path,
    };
    let got = probe(&ctx.hop.book(), &ctx.hop.quota, own, &ask)?;
    let (state, reason, windows) = match got {
        Probe::Read(windows) => (ProbeState::Read, None, windows),
        Probe::Unreadable(why) => (ProbeState::Unreadable, Some(why), Vec::new()),
    };
    Ok(Probed {
        agent: req.agent.clone(),
        account: account.to_string(),
        from: ProbeFrom::Usage,
        now,
        state,
        reason,
        windows,
        path: ctx.hop.quota.path().map(|p| p.display().to_string()),
    })
}

/// `quota-probe` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Probed {
    /// 哪一家（同入参，线上的 kind）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 账号库里的号（同入参）。
    pub(crate) account: String,
    /// 这份读数从哪来。
    pub(crate) from: ProbeFrom,
    /// 这台此刻的 unix 秒。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) now: u64,
    /// 读得懂（已记进额度账）· 输出对不上（不猜、不写账）。
    pub(crate) state: ProbeState,
    /// 只在 `unreadable` 时有：哪一处读不懂（英文诊断）；`read` ⇒ `null`。
    pub(crate) reason: Option<String>,
    /// 读到的窗口（形状同 `reading.windows`）；`unreadable` ⇒ `[]`。
    pub(crate) windows: Vec<crate::agents::QuotaWindow>,
    /// 额度账那份文件的绝对路径（家推不出来时 `null`）。
    pub(crate) path: Option<String>,
}

/// [`Probed::from`]：恒是官方客户端报的用量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProbeFrom {
    Usage,
}

/// [`Probed::state`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProbeState {
    Read,
    Unreadable,
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_probe_face_tests.rs"]
mod tests;
