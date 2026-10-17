//! **起一个会话的那一行** —— 这台后端出成品，界面只把那一行交给 monitor 开终端跑。
//!
//! 起会话只有 `ccm` 一处：每一条起会话 / 接回 / 换号重启 / 分叉交给终端的都只是一行 `ccm …`
//! （[`ccm_invocation`] 渲），环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。
//! 「起什么」只有一份 [`Launch`]（`session-open` 一族拼它），变成渲染器那份 spec 只有一处映射（本机与远端的三处差别都写在那里）。

pub mod ccm_invocation;

use crate::control::launch_account::Settled;

/// 这台的平台与目录在不在（本机那一形要的事实；判据喂确定的值）。生产那一份是 [`Host::PRODUCTION`]。
#[derive(Clone, Copy)]
pub(crate) struct Host {
    /// 本地终端是 PowerShell（Windows）还是 POSIX shell。Windows 上没有 tmux ⇒ 一律直路。
    pub(crate) windows: bool,
    pub(crate) is_dir: fn(&str) -> bool,
    /// 这台 `ccm` 的入口（绝对路径）：那一行直接叫它，不靠终端 `PATH`。说不出 ⇒ `None`（拒）。
    pub(crate) entry: fn() -> Option<String>,
}

impl Host {
    pub(crate) const PRODUCTION: Host = Host {
        windows: crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        is_dir: |p| std::path::Path::new(p).is_dir(),
        entry: crate::platform::paths::installed_ccm_entry,
    };
}

/// 起会话那一行要说的「起什么」：经 [`spec_with`] 一处变成渲染器那份 spec。
#[derive(Debug, Clone)]
pub(crate) struct Launch {
    /// 哪一家（线上的 kind）。
    pub(crate) agent: String,
    pub(crate) act: Act,
    /// 建进哪个 tmux 会话（接回 ⇒ 接进哪个）；`None` ＝ 直路。
    pub(crate) tmux: Option<String>,
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
            Some(name) => ci::Container::Tmux { name },
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

// 假启动器是 POSIX shell 脚本、`ccm` 在那一形上 exec 掉子进程 ⇒ 只在 POSIX 上跑。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/control/launch_render/history_resume_e2e_tests.rs"]
mod history_resume_e2e;
