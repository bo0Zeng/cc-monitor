//! **起一个会话的那一行** —— 这台后端出成品，界面只把那一行交给 monitor 开终端跑。
//!
//! 起会话只有 `ccm` 一处：每一条起会话 / 接回 / 换号重启 / 分叉交给终端的都只是一行 `ccm …`
//! （[`ccm_invocation`] 渲），环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。
//! 「起什么」只有一份 [`Launch`]、变成渲染器那份 spec 只有一处映射（本机与远端的三处差别都写在那里）；
//! [`wire`]（`launch-render-cli`）· [`local`]（`launch-local`）只是把各自的入参换成它。
//! 本机远端同一条 `chan.call(origin, …)`：渲的就是那台机器要跑的那一行，能力问它自己。

pub mod ccm_invocation;
pub(crate) mod local;
pub mod wire;

#[cfg(test)]
mod cli_parity;

use crate::control::launch_account::{self as la, Settled};

crate::wire::typed::codes! {
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

/// 起会话那一行要说的「起什么」：`launch-render-cli` · `launch-local` 的入参与 `session-open` 都先变成它，再经 [`spec_with`] 一处变成渲染器那份 spec。
#[derive(Debug, Clone)]
pub(crate) struct Launch {
    /// 哪一家（线上的 kind）。
    pub(crate) agent: String,
    pub(crate) act: Act,
    /// 建进哪个 tmux 会话（接回 ⇒ 接进哪个）；`None` ＝ 直路。
    pub(crate) tmux: Option<String>,
    /// 往那个已有的空 tmux 会话里就地键入（再接进去）。
    pub(crate) send_into: bool,
    pub(crate) cwd: Option<String>,
    /// 身份标记（打在建出来的 tmux 会话上；Windows 上没有 tmux ⇒ 不打）。
    pub(crate) ccm_sid: Option<String>,
    /// 显式指定的模型（压过号带来的那一个）。
    pub(crate) model: Option<String>,
    pub(crate) launcher: String,
    pub(crate) default_launcher: String,
    /// 垫在交给那一家的那一串里的参数（先定的 sid · 权限档 · 第一句）。
    pub(crate) preset_args: Vec<String>,
}

/// 起什么：新起 · resume 某条 · 接回一个已有的 tmux 会话（不起 agent）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Act {
    New,
    Resume(String),
    Attach(String),
}

/// 那一行在哪台的终端里跑：本机（Windows 上是 PowerShell、没有 tmux）· 远端（ssh 过去的 POSIX shell）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Here {
    Local { windows: bool },
    Remote,
}

impl Launch {
    /// 那一家的启动命令与默认启动器：启动命令照这台的偏好，默认启动器问适配层（认不出那一家 ⇒ 空，渲的时候照拒）。
    ///
    /// 〔继承表〕启动命令那一格（照这台，不随主会话）。
    pub(crate) fn launchers(
        agent: &str,
        prefs: &crate::control::machine_prefs::Prefs,
    ) -> (String, String) {
        let default = crate::agents::launch_face_among(crate::agents::REGISTRY, agent)
            .map_or("", |f| f.default_launcher)
            .to_string();
        (prefs.launcher_for(agent, &default), default)
    }
}

/// 这台 ccm 会的那几样（渲染就在这台后端里）。
pub(crate) fn own_caps() -> std::collections::BTreeSet<String> {
    crate::ccm_launcher_with(crate::platform::tmux::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 那一行（号已经判好）：本机 Windows ⇒ PowerShell 方言，别处 POSIX。拒 ⇒ 那一句。
pub(crate) fn line_of(
    l: &Launch,
    account: &Settled,
    here: Here,
    caps: &std::collections::BTreeSet<String>,
    entry: &str,
) -> Result<String, String> {
    spec_with(l, account, here, false, entry, |spec| match here {
        Here::Local { windows: true } => ccm_invocation::render_ccm_invocation_ps(spec, caps),
        _ => ccm_invocation::render_ccm_invocation(spec, caps),
    })
}

/// 同一行的 argv 形；`detach` ⇒ 建进 tmux 之后不接进去（这台后端替人在 tmux 里起会话时用）。
pub(crate) fn argv_of(
    l: &Launch,
    account: &Settled,
    here: Here,
    detach: bool,
    caps: &std::collections::BTreeSet<String>,
    entry: &str,
) -> Result<Vec<String>, String> {
    spec_with(l, account, here, detach, entry, |spec| {
        ccm_invocation::ccm_argv(spec, caps)
    })
}

/// **唯一的映射**：[`Launch`] ＋ 判好的号 ⇒ 渲染器那份 spec。本机与远端只差三处，都写在这里：
/// 不表态的号（本机继承 monitor 的环境 · 远端 ssh 过去的那台不是 monitor 的环境 ⇒ 落账号 0）·
/// Windows 上没有 tmux（一律直路，接回做不到）· 本机直路不带 `--cwd`（终端的工作目录由 monitor 开窗口时给）。
fn spec_with<T>(
    l: &Launch,
    account: &Settled,
    here: Here,
    detach: bool,
    ccm_path: &str,
    f: impl FnOnce(&ccm_invocation::CliSpec) -> Result<T, ccm_invocation::Refusal>,
) -> Result<T, String> {
    use ccm_invocation as ci;
    let windows = here == Here::Local { windows: true };
    if windows && matches!(l.act, Act::Attach(_)) {
        return Err(copy_core::copy_text("rsHistory.attach.windows", &[]));
    }
    let launcher = checked_launcher(&l.launcher)?;
    let tmux = l.tmux.as_deref().filter(|n| !n.is_empty() && !windows);
    let (account, picked_model) = match (account, here) {
        (Settled::Account(a), _) => (
            ci::CliAccount::Named {
                name: a.name.as_str(),
            },
            a.model.as_deref(),
        ),
        (Settled::Unsaid, Here::Local { .. }) => (ci::CliAccount::Inherit, None),
        (Settled::Base | Settled::Unsaid, _) => (ci::CliAccount::Base, None),
    };
    let action = match &l.act {
        Act::New => ci::Action::New,
        Act::Resume(sid) => ci::Action::Resume { sid },
        Act::Attach(name) => ci::Action::Attach { name },
    };
    let local_direct = matches!(here, Here::Local { .. }) && tmux.is_none();
    let args: Vec<&str> = l.preset_args.iter().map(String::as_str).collect();
    let spec = ci::CliSpec {
        agent: &l.agent,
        action,
        container: match tmux {
            Some(name) => ci::Container::Tmux {
                name,
                send_into: l.send_into,
            },
            None => ci::Container::None,
        },
        cwd: l.cwd.as_deref().filter(|c| !c.is_empty() && !local_direct),
        account,
        ccm_sid: l.ccm_sid.as_deref().filter(|_| !windows),
        model: l.model.as_deref().or(picked_model),
        launcher: if launcher.is_empty() {
            &l.default_launcher
        } else {
            launcher
        },
        default_launcher: &l.default_launcher,
        args: &args,
        ccm_path,
        detach,
    };
    f(&spec).map_err(|r| r.reason())
}

/// 启动命令：拼进命令之前过全仓那一张命令片段白名单（`shell_quote_core::launcher_refused_char`）。空 / 纯空白 = 没设。
fn checked_launcher(launcher: &str) -> Result<&str, String> {
    let l = launcher.trim();
    match shell_quote_core::launcher_refused_char(l) {
        Some(c) if !l.is_empty() => Err(copy_core::copy_text(
            "rsHistory.launcher.badChars",
            &[("launcher", &format!("{l:?}")), ("c", &format!("{c:?}"))],
        )),
        _ => Ok(l),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/answers_tests.rs"]
mod tests;

// 假启动器是 POSIX shell 脚本、`ccm` 在那一形上 exec 掉子进程 ⇒ 只在 POSIX 上跑。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/control/launch_render/history_resume_e2e_tests.rs"]
mod history_resume_e2e;
