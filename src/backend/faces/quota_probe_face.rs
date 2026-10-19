//! 帧命令 `quota-probe`：用某个号查一次额度 —— 起那一家的官方客户端报用量（不发模型请求、不花额度）、读成窗口、
//! 记进**同一本**额度账（来源 `usage`，按窗口合并）。同一种事实的第二个来源，不是第二套额度。
//!
//! - 怎么起（程序 · 参数 · 配置目录交给哪一格 · 摘 / 设哪几格环境）与怎么读都住适配层（[`crate::agents::usage_of`]）；
//!   这里只认「这一家能不能查、查回什么窗口」。
//! - 工作目录是这台家里不出会话的那一个（[`crate::common::own_dir::HIDDEN_WORK_DIR`]，没有就建）：起的这一次不进历史页与会话列表。
//! - 读不懂 ⇒ 照实回 `unreadable` ＋ 哪一处读不懂，不猜、不写账。
//! - 动作、不定时：界面「刷新」· skill · AI 按需调。

use crate::accounts::quota::ledger;
use crate::accounts::upstream_select::rotate::account_ok;
use crate::faces::rotation_face::Ctx;
use crate::platform::child::{Child, Deadline};
use crate::platform::fs::Own;
use copy_core::copy_text;

/// 失败带码 ＋ 那一句 ＋ 原话（落账写不进那一形的原话进复制详情，[`Fail`]）。
type Answer = Result<Probed, crate::wire::spec::Fail>;

/// 起官方客户端报一次用量的期限（它要起进程、读登录、问一次上游）。
const PROBE_WITHIN: Deadline = Deadline::secs(30);

/// 这条命令的总期限（分派那一层装；略长于那一次子进程）。
pub(crate) const PROBE_CAP: Deadline = Deadline::secs(32);

/// 报错原话里带官方客户端的输出至多这么多个字。
const SAID_CHARS: usize = 200;

fn bad(detail: &str) -> crate::wire::spec::Fail {
    crate::wire::spec::Fail::new("bad_args", crate::common::contract::malformed(detail))
}

/// `quota-probe` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct QuotaProbeReq {
    /// 哪一家（线上的 kind）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 账号库里的号（配置目录末段；账号 0 是 `0`）。
    pub(crate) account: String,
}

crate::wire::typed::codes! {
    /// `quota-probe` 的码。
    pub(crate) enum ProbeErr {
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
pub(crate) struct QuotaProbe;

impl crate::wire::typed::Command for QuotaProbe {
    const NAME: &'static str = "quota-probe";
    type Req = QuotaProbeReq;
    type Reply = Probed;
    type Err = ProbeErr;
}

impl crate::wire::typed::Handle for QuotaProbe {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: QuotaProbeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Probed, ProbeErr>> + Send + 'static {
        async move {
            answer_probe(&req, &cx.needs)
                .map_err(|f| crate::wire::typed::of_fail(f, ProbeErr::Failed))
        }
    }
}

/// `quota-probe`：`{agent, account}` ⇒ 读到的窗口（并已记进额度账）或「读不懂」。
pub(crate) fn answer_probe(req: &QuotaProbeReq, own: &Own) -> Answer {
    answer_probe_with(
        &Ctx::here(),
        own,
        req,
        crate::accounts::quota::now_unix(),
        None,
    )
}

/// 本体。`path` ＝ 起会话那个 shell 的 `PATH`（判据交一个放着假客户端的目录；生产 `None` ＝ 现问，见 `platform::shell::agent_launch`）。
pub(crate) fn answer_probe_with(
    ctx: &Ctx,
    own: &Own,
    req: &QuotaProbeReq,
    now: u64,
    path: Option<&str>,
) -> Answer {
    let agent = Some(&req.agent)
        .filter(|a| !a.as_str().trim().is_empty())
        .and_then(|a| crate::agents::pick_route(Some(a)).ok())
        .ok_or_else(|| bad("`agent` must be an agent kind such as \"claude\""))?
        .id();
    let account = Some(req.account.as_str())
        .filter(|a| account_ok(a))
        .ok_or_else(|| bad("`account` must be an account id"))?;
    let face = crate::agents::usage_of(agent).ok_or_else(|| {
        (
            "unsupported",
            copy_text(
                "beQuotaProbe.agent.unsupported",
                &[("agent", req.agent.as_str())],
            ),
        )
    })?;
    let lib = ctx.hop.library();
    let found = lib
        .accounts
        .iter()
        .find(|a| a.id == account)
        .ok_or_else(|| {
            (
                "not_found",
                copy_text("beQuotaProbe.account.notFound", &[("account", account)]),
            )
        })?;
    if found.api {
        return Err((
            "unsupported",
            copy_text("beQuotaProbe.account.apiKey", &[("account", account)]),
        )
            .into());
    }
    let book = ctx.hop.quota.path();
    // 家里那个不出会话的工作目录（数据目录底下一层，没有就建）：经写句柄建。
    let no_home = || ("io_failed", copy_text("beQuotaLedger.read.noHome", &[]));
    if book.is_none() {
        return Err(no_home().into());
    }
    let cwd = own
        .path(crate::common::own_dir::HIDDEN_WORK_DIR)
        .map_err(|_| no_home())?;
    own.ensure_dir(&cwd).map_err(|e| {
        crate::wire::spec::Fail::new(
            "io_failed",
            copy_text(
                "beQuotaProbe.workDir.failed",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
        )
        .with_raw(Some(&e.to_string()))
    })?;
    let failed = |why: &str| {
        copy_text(
            "beQuotaProbe.run.failed",
            &[("program", face.program), ("why", why)],
        )
    };
    let at = crate::platform::shell::agent_launch(face.program, path)
        .map_err(|why| crate::wire::spec::Fail::new("failed", failed(&why)))?;
    let mut child = Child::new(at.program)
        .args(at.lead)
        .args(face.args)
        .current_dir(cwd.as_path());
    child = match &found.dir {
        Some(d) => child.env(face.dir_env, d),
        None => child.env_remove(face.dir_env),
    };
    for k in face.env_remove {
        child = child.env_remove(k);
    }
    for (k, v) in face.env_set {
        child = child.env(k, v);
    }
    if let Some(p) = &at.path {
        child = child.env("PATH", p);
    }
    let out = child
        .run(PROBE_WITHIN)
        .map_err(|e| crate::wire::spec::Fail::from(e.into_cmd_said("failed", failed)))?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        let stderr: String = String::from_utf8_lossy(&out.stderr)
            .lines()
            .chain(text.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .chars()
            .take(SAID_CHARS)
            .collect();
        let code = out
            .status
            .code()
            .map_or_else(|| "-".to_string(), |c| c.to_string());
        // 退出码与它说的那一行是原话：进复制详情，不上句子。
        return Err(crate::wire::spec::Fail::new(
            "failed",
            copy_text("beQuotaProbe.run.exit", &[("program", face.program)]),
        )
        .with_raw(Some(&format!("exit {code}: {stderr}"))));
    }
    let shown = book.map(|p| p.display().to_string());
    let probed = |state, reason, windows| Probed {
        agent: req.agent.clone(),
        account: account.to_string(),
        from: ProbeFrom::Usage,
        now,
        state,
        reason,
        windows,
        path: shown.clone(),
    };
    match (face.read)(&text, now) {
        Err(why) => Ok(probed(ProbeState::Unreadable, Some(why), Vec::new())),
        Ok(windows) => {
            ledger::record_probe(&ctx.hop.quota, own, agent, account, windows.clone(), now)
                .map_err(|e| ("io_failed", e))?;
            Ok(probed(ProbeState::Read, None, windows))
        }
    }
}

/// `quota-probe` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Probed {
    /// 哪一家（同入参，线上的 kind）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 账号库里的号（同入参）。
    pub(crate) account: String,
    /// 这份读数从哪来。
    pub(crate) from: ProbeFrom,
    /// 这台此刻的 unix 秒。
    #[cfg_attr(test, ts(type = "number"))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProbeFrom {
    Usage,
}

/// [`Probed::state`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProbeState {
    Read,
    Unreadable,
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_probe_face_tests.rs"]
mod tests;
