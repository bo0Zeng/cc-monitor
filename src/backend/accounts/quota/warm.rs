//! ★ 开窗：号的 5 小时窗由第一发触发 —— 用那个号发一句最短的话，让它开始计时。实现只这一处，两个入口：
//! 策略交回的 `warm`（判完由 `upstream_select::turn` 交来 [`kick`]，另起线程发、不等、不挡这一发）·
//! 帧命令 `quota-warm`（quota-warm skill · AI 按需调，等那一发说完再答）。
//!
//! - **怎么发**：本二进制当 ccm 跑 `ccm <适配层那几个参数> -- --account <号> --cwd <家里不出会话的目录> --no-rotation`
//!   （参数住适配层 [`crate::agents::warm_of`]）⇒ 走本机中转（额度账由中转记下）· 这一发不轮换 · 不进历史页与会话列表。
//! - **同一个号 15 分钟内只发一次**（[`AGAIN`]，判与命令共用这一本 [`book`]）：认下就记「发出的时刻」，
//!   发完再记「回来的重置时刻」（额度账里那个号 5 小时窗的重置时刻；没读到 ⇒ 空）。
//! - 收哪几个号（订阅号 · 发得出去 · 5 小时窗没在计时）由判那一侧定（[`crate::accounts::quota::rules::decide::warmable`]）；
//!   命令那一形不判这一条：人或 AI 点名要发就发（15 分钟那一条照样管）。

use crate::platform::child::{Child, Deadline};
use crate::platform::fs::Own;
use std::collections::HashMap;
use std::sync::Mutex;

/// 同一个号两次开窗至少隔这么久。
pub(crate) const AGAIN: u64 = 15 * 60;

/// 那一发的期限（起 ccm、它起那一家的程序、问一次上游）。
const SEND_WITHIN: Deadline = Deadline::secs(120);

/// 命令那一形的总期限（分派那一层装；略长于那一发）。
pub const WARM_CAP: Deadline = Deadline::secs(125);

/// 一个号最近一次开窗：发出的时刻 · 回来的重置时刻（还没回来 · 没读到 ⇒ 空）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) at: u64,
    pub(crate) resets_at: Option<u64>,
}

/// 开窗那一本：(哪一家的路由名, 号) ⇒ 最近一次。
#[derive(Default)]
pub(crate) struct Warm {
    sent: Mutex<HashMap<(String, String), Sent>>,
}

impl Warm {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), Sent>> {
        self.sent.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 认下这几个号此刻开窗：[`AGAIN`] 之内发过的跳过；回认下的（照交的先后、同一个号一次），认下即记发出的时刻。
    pub(crate) fn claim(&self, agent: &str, accts: &[String], now: u64) -> Vec<String> {
        let mut g = self.lock();
        let mut out: Vec<String> = Vec::new();
        for a in accts {
            let key = (agent.to_string(), a.clone());
            if out.contains(a) || g.get(&key).is_some_and(|s| now < s.at + AGAIN) {
                continue;
            }
            g.insert(
                key,
                Sent {
                    at: now,
                    resets_at: None,
                },
            );
            out.push(a.clone());
        }
        out
    }

    /// 那一发回来了：记下额度账里那个号此刻的 5 小时窗重置时刻。
    pub(crate) fn landed(&self, agent: &str, acct: &str, resets_at: Option<u64>) {
        if let Some(s) = self.lock().get_mut(&(agent.to_string(), acct.to_string())) {
            s.resets_at = resets_at;
        }
    }

    /// 那个号最近一次开窗。
    pub(crate) fn sent(&self, agent: &str, acct: &str) -> Option<Sent> {
        self.lock()
            .get(&(agent.to_string(), acct.to_string()))
            .copied()
    }
}

/// 进程里那一本。
pub(crate) fn book() -> &'static Warm {
    static BOOK: std::sync::OnceLock<Warm> = std::sync::OnceLock::new();
    BOOK.get_or_init(Default::default)
}

/// ★ 判完交来的 `warm`（已照事实收过）：认下的各另起一条线程发一句，不等。
pub(crate) fn kick(agent: &str, accts: &[String], own: &Own, now: u64) {
    for a in book().claim(agent, accts, now) {
        let (agent, own) = (agent.to_string(), own.clone());
        let spawned =
            std::thread::Builder::new()
                .name("quota-warm".into())
                .spawn(move || match send(&agent, &a, &own) {
                    Ok(at) => {
                        book().landed(&agent, &a, at);
                        tracing::info!("[warm] {agent}/{a} 开窗一发已回 · 重置 {at:?}");
                    }
                    Err(e) => tracing::warn!("[warm] {agent}/{a} 开窗一发未成：{}", e.logged()),
                });
        if let Err(e) = spawned {
            tracing::warn!("[warm] 开窗线程起不来：{e}");
        }
    }
}

// 发不成的那一句（码 · 句子 · 原话）。
use crate::common::said::Said;

/// 交给 ccm 的那一行（不含程序本身）：`<适配层那几个参数> -- --account <号> --cwd <工作目录> --no-rotation`。
pub(crate) fn line(
    face: &crate::agents::WarmFace,
    account: &str,
    cwd: &std::path::Path,
) -> Vec<std::ffi::OsString> {
    use crate::ccm_line::argv::flag;
    let mut v: Vec<std::ffi::OsString> = face.args.iter().map(Into::into).collect();
    v.extend([flag::END, flag::ACCOUNT, account, flag::CWD].map(Into::into));
    v.push(cwd.into());
    v.push(flag::NO_ROTATION.into());
    v
}

/// ★ 用 `account` 发那一句、等它说完；回额度账里那个号此刻的 5 小时窗重置时刻（没读到 ⇒ 空）。
/// `agent` ＝ 路由名（`claude-code` 一类）。这一家没有开窗那一形 ⇒ 不发；号不在账号库里 ⇒ ccm 自己拒（照它的原话报）。
pub(crate) fn send(agent: &str, account: &str, own: &Own) -> Result<Option<u64>, Said> {
    use copy_core::copy_text;
    let face = crate::agents::route_named(agent)
        .and_then(crate::agents::warm_of)
        .ok_or_else(|| {
            crate::wire::spec::fail(
                "unsupported",
                copy_text("beQuotaWarm.agent.unsupported", &[("agent", agent)]),
            )
        })?;
    let no_home =
        || crate::wire::spec::fail("io_failed", copy_text("beQuotaLedger.read.noHome", &[]));
    let cwd = own
        .path(crate::common::own_dir::HIDDEN_WORK_DIR)
        .map_err(|_| no_home())?;
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
    let me = std::env::current_exe().map_err(|e| {
        crate::wire::spec::fail(
            "failed",
            copy_text(
                "beQuotaWarm.run.failed",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
        )
        .and_raw(Some(&e.to_string()))
    })?;
    #[expect(
        clippy::disallowed_methods,
        reason = "开窗：起本二进制自己当 ccm 跑一发适配层登记的那一句（`--account` 点名、不轮换、期限 120 秒），被起的那一趟照样归本仓的守法管；不许加第二种用途"
    )]
    let mut child = Child::new(me)
        .args(line(&face, account, cwd.as_path()))
        .current_dir(cwd.as_path())
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
    for (k, v) in face.env_set {
        child = child.env(k, v);
    }
    // 后端经 ssh 非登录 shell 起时自己的 PATH 常没有用户级 bin：交起会话那个 shell 的那一份（ccm 照它找那一家的程序）。
    if let Some(p) = crate::platform::shell::session_shell_path() {
        child = child.env("PATH", p);
    }
    let out = child.run(SEND_WITHIN).map_err(|e| {
        let (code, said) = e.into_cmd_said("failed", |why| {
            copy_text("beQuotaWarm.run.failed", &[("why", why)])
        });
        crate::wire::spec::fail(code, said)
    })?;
    if !out.status.success() {
        let code = out
            .status
            .code()
            .map_or_else(|| "-".to_string(), |c| c.to_string());
        // 原话里带 claude 的输出至多这么多个字（截了接「…」）。
        const SAID_CHARS: usize = 200;
        let (err, std_out) = (
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
        );
        let said = copy_core::clip(
            copy_core::first_line(&err)
                .or_else(|| copy_core::first_line(&std_out))
                .unwrap_or(""),
            SAID_CHARS,
        );
        return Err(
            crate::wire::spec::fail("failed", copy_text("beQuotaWarm.run.exit", &[]))
                .and_raw(Some(&format!("exit {code}: {said}"))),
        );
    }
    let quota = super::ledger::Ledger::at(super::ledger::path_from(&|k| std::env::var(k).ok()));
    Ok(resets_5h(&quota, agent, account))
}

/// 额度账里那个号 5 小时窗的重置时刻（窗口名 → 语义位照适配层）。
fn resets_5h(quota: &super::ledger::Ledger, agent: &str, account: &str) -> Option<u64> {
    let slot = crate::agents::route_named(agent).and_then(crate::agents::window_slot_of)?;
    quota
        .entry(agent, account)?
        .reading
        .windows
        .iter()
        .find(|w| slot(&w.name) == Some("5h"))?
        .resets_at
}

/// `quota-warm` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct QuotaWarmReq {
    /// 哪一家（线上的 kind）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 账号库里的号（配置目录末段；账号 0 是 `0`）。
    pub(crate) account: String,
}

/// `quota-warm` 的回包。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Warmed {
    /// 哪一家（同入参）。
    pub(crate) agent: crate::agents::AgentKind,
    /// 号（同入参）。
    pub(crate) account: String,
    /// 这一趟真发了（`false` ＝ 15 分钟内发过，照那一次答）。
    pub(crate) sent: bool,
    /// 那一发发出的时刻（unix 秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) at: u64,
    /// 回来的重置时刻（额度账里那个号的 5 小时窗；没读到 ⇒ `null`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub(crate) resets_at: Option<u64>,
}

crate::wire::typed::codes! {
    /// `quota-warm` 的码。
    pub enum WarmErr {
        /// 那一发没在期限里说完。
        ChildTimedOut = "child_timed_out",
        /// 起不来 · 退出码非零（它自己那句话进原话）。
        NotSent = "failed",
        /// 工作目录建不了。
        IoFailed = "io_failed",
        /// 账号库里没有这个号。
        NotFound = "not_found",
        /// 这一家没有开窗那一形 · 按量号。
        Unsupported = "unsupported",
    }
}

/// `quota-warm`：用某个号发一句、让它的 5 小时窗开始计时（等那一发说完 ⇒ 阻塞档，总期限登记在 `caps.rs`）。
pub struct QuotaWarm;

impl crate::wire::typed::Command for QuotaWarm {
    const NAME: &'static str = "quota-warm";
    type Req = QuotaWarmReq;
    type Reply = Warmed;
    type Err = WarmErr;
}

impl crate::wire::typed::Handle for QuotaWarm {
    type Needs = crate::wire::typed::needs::Own;
    async fn run(
        req: QuotaWarmReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<Warmed, WarmErr> {
        answer(&req, &cx.needs, crate::common::time::now_secs())
            .map_err(|f| crate::wire::typed::of_fail(f, WarmErr::NotSent))
    }
}

/// `quota-warm` 本体：15 分钟内发过 ⇒ 照那一次答、不发；否则发、等它说完。
fn answer(req: &QuotaWarmReq, own: &Own, now: u64) -> Result<Warmed, Said> {
    let route = crate::agents::pick_route(Some(&req.agent)).map_err(|_| {
        crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`agent` must be an agent kind such as \"claude\""),
        )
    })?;
    let agent = route.id();
    if !crate::relay::segment_is_safe(&req.account) {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`account` must be an account id"),
        ));
    }
    let reply = |sent, s: Sent| Warmed {
        agent: req.agent.clone(),
        account: req.account.clone(),
        sent,
        at: s.at,
        resets_at: s.resets_at,
    };
    if book()
        .claim(agent, std::slice::from_ref(&req.account), now)
        .is_empty()
    {
        let s = book().sent(agent, &req.account).unwrap_or(Sent {
            at: now,
            resets_at: None,
        });
        return Ok(reply(false, s));
    }
    let at = send(agent, &req.account, own)?;
    book().landed(agent, &req.account, at);
    Ok(reply(
        true,
        Sent {
            at: now,
            resets_at: at,
        },
    ))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/warm_tests.rs"]
mod tests;
