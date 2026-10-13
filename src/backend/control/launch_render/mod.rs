//! **起一个会话的那一行** —— 这台后端出成品，界面只把那一行交给 monitor 开终端跑。
//!
//! 起会话只有 `ccm` 一处：每一条起会话 / 接回 / 换号重启 / 分叉交给终端的都只是一行 `ccm …`
//! （[`ccm_invocation`] 渲），环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。
//! [`wire`]：`launch-render-cli` 的线上形状（远端，以及本机就地 resume）。[`local`]：本机起会话（`launch-local`）。
//! 本机远端同一条 `chan.call(origin, …)`：渲的就是那台机器要跑的那一行，能力问它自己。

pub mod ccm_invocation;
pub(crate) mod local;
pub mod wire;

#[cfg(test)]
mod cli_parity;

use crate::control::launch_account::{self as la, Settled};

crate::stream::inbound::typed::codes! {
    /// `launch-render-cli` · `launch-local` 的码（两条同一份）。
    pub(crate) enum LaunchRenderErr {
        /// 渲不出来（坏输入 · 目录不在 · 那一家认不得）：那一句说为什么。
        Refused = "refused",
        /// 要的号选不了：`data` 带要的号与替代。
        AccountUnavailable = "account_unavailable" => la::AccountUnavailable,
    }
}

/// 选不了号 ⇒ 命令级码 `account_unavailable`，`data` 是那一形。
pub(crate) fn unavailable(u: la::AccountUnavailable) -> LaunchRenderErr {
    let said = la::unavailable_said(&u);
    LaunchRenderErr::AccountUnavailable(said, u)
}

/// `launch-render-cli` · `launch-local` 的应答：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::Rendered;

/// 应答里的 `account`：实际用的号（账号库里的那一形；别的 ⇒ `None`）。
pub(crate) fn launched(account: &Settled) -> Option<la::LaunchedAccount> {
    match account {
        Settled::Account(a) => Some(a.clone()),
        _ => None,
    }
}

/// `launch-render-cli`：那台机器要跑的那一行 `ccm …`；渲不出来 ⇒ `refused` ＋ 理由；要的号选不了 ⇒ `account_unavailable`。
pub(crate) fn answer_cli(
    req: &wire::CliRenderRequest,
    facts: &la::Facts,
) -> Result<Rendered, LaunchRenderErr> {
    let account = wire::settle(req, facts).map_err(unavailable)?;
    let cmd = wire::render_ccm_launch(req, &account).map_err(LaunchRenderErr::Refused)?;
    Ok(Rendered {
        cmd,
        account: launched(&account),
    })
}

/// `launch-local`：本机起会话那一行；坏输入 / 目录不在 ⇒ `refused`；要的号选不了 ⇒ `account_unavailable`。
pub(crate) fn answer_local(
    req: &local::LocalLaunchRequest,
    facts: &la::Facts,
) -> Result<Rendered, LaunchRenderErr> {
    let account = local::settle(req, facts).map_err(unavailable)?;
    let cmd =
        local::plan(req, &account, &local::Facts::PRODUCTION).map_err(LaunchRenderErr::Refused)?;
    Ok(Rendered {
        cmd,
        account: launched(&account),
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/answers_tests.rs"]
mod tests;

// 假启动器是 POSIX shell 脚本、`ccm` 在那一形上 exec 掉子进程 ⇒ 只在 POSIX 上跑。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/control/launch_render/history_resume_e2e_tests.rs"]
mod history_resume_e2e;
