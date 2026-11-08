//! 用某个号查一次额度的本体（帧命令 `quota-probe` 的那一半，命令登记 · 入参闸 · 回包形状住 `faces/quota_probe_face.rs`）：
//! 起那一家的官方客户端报用量（不发模型请求、不花额度）、读成窗口、记进**同一本**额度账（来源 `usage`，按窗口合并）。
//!
//! - 怎么起（程序 · 参数 · 配置目录交给哪一格 · 摘 / 设哪几格环境）与怎么读都住适配层（[`crate::agents::usage_of`]）；
//!   这里只认「这一家能不能查、查回什么窗口」。
//! - 工作目录是这台家里不出会话的那一个（[`hidden_work_dir`]，开窗那一发同用）：起的这一次不进历史页与会话列表。
//! - 读不懂 ⇒ 照实回「读不懂 ＋ 哪一处」，不猜、不写账。

use crate::accounts::quota::ledger;
use crate::common::said::Said;
use crate::platform::child::{Child, Deadline};
use crate::platform::fs::{Own, OwnPath};
use copy_core::copy_text;

/// 起官方客户端报一次用量的期限（它要起进程、读登录、问一次上游）。
const PROBE_WITHIN: Deadline = Deadline::secs(30);

/// 报错原话里带官方客户端的输出至多这么多个字（截了接「…」）。
const SAID_CHARS: usize = 200;

/// ★ 家里那个不出会话的工作目录（数据目录底下一层，没有就经写句柄建）：查额度与开窗那一发都在这里起。
/// 家推不出来 · 建不了 ⇒ `io_failed` 那一句。
pub(crate) fn hidden_work_dir(own: &Own) -> Result<OwnPath, Said> {
    let cwd = own
        .path(crate::common::own_dir::HIDDEN_WORK_DIR)
        .map_err(|_| Said::new("io_failed", copy_text("beQuotaLedger.read.noHome", &[])))?;
    own.ensure_dir(&cwd).map_err(|e| {
        crate::wire::spec::fail(
            "io_failed",
            copy_text(
                "beQuotaProbe.workDir.failed",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
        )
        .and_raw(Some(&e.to_string()))
    })?;
    Ok(cwd)
}

/// 查回来的：读得懂（已记进额度账）· 读不懂（哪一处，英文诊断；不写账）。
pub enum Probe {
    Read(Vec<crate::agents::QuotaWindow>),
    Unreadable(String),
}

/// 查哪一个：那一家的路由 · 线上的 kind（只进那一句）· 号 · 此刻 · 起会话那个 shell 的 `PATH`
/// （判据交一个放着假客户端的目录；生产 `None` ＝ 现问，见 `platform::shell::agent_launch`）。
pub struct Ask<'a> {
    pub route: crate::agents::RouteId,
    pub kind: &'a str,
    pub account: &'a str,
    pub now: u64,
    pub path: Option<&'a str>,
}

/// ★ 用 `ask.account` 查一次额度：号照这台的账号库（`lib`）找，读到的记进额度账 `quota`。
pub fn probe(
    lib: &acct_core::AccountBook,
    quota: &ledger::Ledger,
    own: &Own,
    ask: &Ask<'_>,
) -> Result<Probe, Said> {
    let (account, now, path) = (ask.account, ask.now, ask.path);
    let agent = ask.route.id();
    let face = crate::agents::usage_of(ask.route).ok_or_else(|| {
        Said::new(
            "unsupported",
            copy_text("beQuotaProbe.agent.unsupported", &[("agent", ask.kind)]),
        )
    })?;
    let found = lib.by_route(account).ok_or_else(|| {
        Said::new(
            "not_found",
            copy_text("beQuotaProbe.account.notFound", &[("account", account)]),
        )
    })?;
    if found.is_api_key() {
        return Err(Said::new(
            "unsupported",
            copy_text("beQuotaProbe.account.apiKey", &[("account", account)]),
        ));
    }
    if quota.path().is_none() {
        return Err(Said::new(
            "io_failed",
            copy_text("beQuotaLedger.read.noHome", &[]),
        ));
    }
    let cwd = hidden_work_dir(own)?;
    let failed = |why: &str| {
        copy_text(
            "beQuotaProbe.run.failed",
            &[("program", face.program), ("why", why)],
        )
    };
    let at = crate::platform::shell::agent_launch(face.program, path)
        .map_err(|why| crate::wire::spec::fail("failed", failed(&why)))?;
    #[expect(
        clippy::disallowed_methods,
        reason = "起那一家官方客户端报一次某个号的用量（适配层登记的那一形，argv 不过 shell、期限 30 秒）：它照自己的样子用自己的配置目录，后端不写；不许加第二种用途"
    )]
    let mut child = Child::new(at.program)
        .args(at.lead)
        .args(face.args)
        .current_dir(cwd.as_path());
    child = match &found.config_dir {
        Some(d) => child.env(face.dir_env, d),
        None => child.env_remove(face.dir_env),
    };
    for k in face.env_remove {
        child = child.env_remove(k);
    }
    for (k, v) in face.env_set {
        child = child.env(k, v);
    }
    let out = child.run(PROBE_WITHIN).map_err(|e| {
        let (code, said) = e.into_cmd_said("failed", failed);
        crate::wire::spec::fail(code, said)
    })?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let stderr = copy_core::clip(
            copy_core::first_line(&err)
                .or_else(|| copy_core::first_line(&text))
                .unwrap_or(""),
            SAID_CHARS,
        );
        let code = out
            .status
            .code()
            .map_or_else(|| "-".to_string(), |c| c.to_string());
        // 退出码与它说的那一行是原话：进复制详情，不上句子。
        return Err(crate::wire::spec::fail(
            "failed",
            copy_text("beQuotaProbe.run.exit", &[("program", face.program)]),
        )
        .and_raw(Some(&format!("exit {code}: {stderr}"))));
    }
    match (face.read)(&text, now) {
        Err(why) => Ok(Probe::Unreadable(why)),
        Ok(windows) => {
            ledger::record_probe(quota, own, agent, account, windows.clone(), now)
                .map_err(|e| crate::wire::spec::fail("io_failed", e))?;
            Ok(Probe::Read(windows))
        }
    }
}
