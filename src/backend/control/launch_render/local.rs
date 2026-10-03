//! **本机起会话**（新起 / resume / 接回）那一行 —— 本机后端出成品；monitor 只剩「开一个终端窗口跑这一行」
//! （`open_local_terminal`）。
//!
//! 一行 = `ccm …`（[`super::ccm_invocation`]）：POSIX 上有会话名 ⇒ 建进 tmux（`--ccm-tmux=`）；Windows / 没名字 ⇒ 直路。
//! 起 agent 的那几格带 `--ccm-launch-id <token>`（交回调用方回填 sid）。环境与中转地址由 `ccm` 自己定。
//! 事实（平台 · 目录在不在）由 [`Facts`] 给，[`plan`] 是纯的 —— 判据喂确定的事实驱动生产那一条。

use super::ccm_invocation as ci;
use copy_core::copy_text;
use serde::Deserialize;
use std::collections::BTreeSet;

/// `launch-local` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LocalLaunchRequest {
    /// 这个会话是哪一家（线上的 kind）。必填：没说就不知道该怎么起，不落默认那一家。
    pub(crate) agent: String,
    pub(crate) action: LocalAction,
    /// 只用来核「新起」那一格的目录在不在；终端的工作目录由 monitor 开窗口时给。
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    /// 自定义启动命令（空 / 纯空白 = 没设）。
    #[serde(default)]
    pub(crate) launcher: Option<String>,
    /// 三态：缺席 = 调用方没表态（继承环境）· `base` = 显式账号 0 · `named` = 具名账号。
    #[serde(default)]
    pub(crate) account: Option<LaunchAccount>,
    /// 要建进 tmux 时的会话名（界面铸名口铸的；这里不铸 —— 撞名避让只有一个家）。
    #[serde(default)]
    pub(crate) tmux_name: Option<String>,
    /// 这一家 agent 的默认启动器（等于它就不吐 `--launcher`）。
    pub(crate) default_launcher: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum LocalAction {
    New,
    Resume {
        sid: String,
    },
    /// 接回一个已有的 tmux 会话（名字走 `tmuxName`）：不起 agent ⇒ 不要账号、身份。
    Attach,
}

/// 「这次拉起用哪个号」—— 形状与界面 `LOCAL_LAUNCH_ACCOUNT_WIRE`（生成物）同一份。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LaunchAccount {
    Base,
    Named {
        #[serde(rename = "configDir")]
        config_dir: String,
        /// 说得出的那个名字；`None` ⇒ `--account-dir <目录>`。
        #[serde(default)]
        name: Option<String>,
    },
}

/// [`plan`] 要的两样事实。生产那一份是 [`Facts::PRODUCTION`]。
#[derive(Clone, Copy)]
pub(crate) struct Facts {
    /// 本地终端是 PowerShell（Windows）还是 POSIX shell。Windows 上没有 tmux ⇒ 一律直路。
    pub(crate) windows: bool,
    pub(crate) is_dir: fn(&str) -> bool,
}

impl Facts {
    pub(crate) const PRODUCTION: Facts = Facts {
        windows: crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        is_dir: dir_exists,
    };
}

fn dir_exists(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

/// 一次拉起的成品。`launch_id` = 交给 `ccm` 放进 agent 进程环境的那个身份 token（接回那一格不起 agent ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Planned {
    pub(crate) cmd: String,
    pub(crate) launch_id: Option<String>,
}

/// 整条计划。拒 ⇒ `Err(那一句)`。
pub(crate) fn plan(req: &LocalLaunchRequest, facts: &Facts) -> Result<Planned, String> {
    plan_with(req, facts, false, |spec, caps| {
        ci::render_ccm_invocation(spec, caps)
    })
    .map(|(cmd, launch_id)| Planned { cmd, launch_id })
}

/// 本机那一行的 argv 形（同 [`plan`]：同一份计划、同一个渲染器），`detach` ⇒ 建进 tmux 之后不接进去。
/// 这台后端替人在 tmux 里起会话时用（tab 栏在 tmux 里起）；名字是调用方铸好的。
pub(crate) fn plan_argv(
    req: &LocalLaunchRequest,
    facts: &Facts,
    detach: bool,
) -> Result<Vec<String>, String> {
    plan_with(req, facts, detach, |spec, caps| ci::ccm_argv(spec, caps)).map(|(argv, _)| argv)
}

/// 整条计划，渲成什么形由 `out` 定（一行字 · argv）。回 `(成品, 身份 token)`。
fn plan_with<T>(
    req: &LocalLaunchRequest,
    facts: &Facts,
    detach: bool,
    out: impl Fn(&ci::CliSpec, &BTreeSet<String>) -> Result<T, ci::Refusal>,
) -> Result<(T, Option<String>), String> {
    let name = req.tmux_name.as_deref().filter(|n| !n.is_empty());
    if let LocalAction::Attach = req.action {
        if facts.windows {
            return Err(copy_text("rsHistory.attach.windows", &[]));
        }
        let Some(name) = name else {
            return Err(copy_text("rsHistory.launch.noSessionName", &[]));
        };
        let cmd = with_spec(
            req,
            ci::Action::Attach { name },
            Some(name),
            None,
            detach,
            &out,
        )?;
        return Ok((cmd, None));
    }
    if let (LocalAction::New, Some(dir)) = (&req.action, req.cwd.as_deref()) {
        if !dir.is_empty() && !(facts.is_dir)(dir) {
            return Err(copy_text("rsHistory.newSession.noDir", &[("cwd", dir)]));
        }
    }
    let (action, sid) = match &req.action {
        LocalAction::Resume { sid } => (ci::Action::Resume { sid }, Some(sid.as_str())),
        _ => (ci::Action::New, None),
    };
    let token = identity_token(sid)?;
    // Windows 上没有 tmux ⇒ 直路（ccm 在那个 PowerShell 窗口里起 agent、等它退）。
    // §36（只绑 Windows）：Windows 这一行里不渲清嵌套会话变量的那一段 —— 清它们是 `ccm` 在最终 exec 那一处做的。
    let container = if facts.windows { None } else { name };
    let cmd = with_spec(req, action, container, Some(&token), detach, &out)?;
    Ok((cmd, Some(token)))
}

/// 本机那一形的入参 ⇒ 渲染器那份 spec（唯一的映射）。
fn with_spec<T>(
    req: &LocalLaunchRequest,
    action: ci::Action,
    tmux: Option<&str>,
    launch_id: Option<&str>,
    detach: bool,
    f: impl Fn(&ci::CliSpec, &BTreeSet<String>) -> Result<T, ci::Refusal>,
) -> Result<T, String> {
    let launcher = checked_launcher(req.launcher.as_deref())?;
    // 账号三态逐态对：`base` ⇒ `--base`；具名 ⇒ `--account <名>` / `--account-dir <目录>`；缺席 ⇒ 继承（不吐）。
    let account = match req.account.as_ref() {
        Some(LaunchAccount::Base) => ci::CliAccount::Base,
        Some(LaunchAccount::Named { name, config_dir }) => ci::CliAccount::Named {
            name: name.as_deref(),
            config_dir: Some(config_dir.as_str()),
        },
        None => ci::CliAccount::Inherit,
    };
    let sid = match action {
        ci::Action::Resume { sid } => Some(sid),
        _ => None,
    };
    let spec = ci::CliSpec {
        agent: &req.agent,
        action,
        container: match tmux {
            Some(name) => ci::Container::Tmux {
                name,
                send_into: false,
            },
            None => ci::Container::None,
        },
        cwd: None,
        account,
        // 身份标记打在建出来的 tmux 会话上（resume 才说得出 sid）。
        ccm_sid: sid.filter(|_| tmux.is_some()),
        model: None,
        launcher: launcher.as_deref().unwrap_or(&req.default_launcher),
        default_launcher: &req.default_launcher,
        args: &[],
        launch_id,
        ccm_path: "ccm",
        detach,
    };
    // 能力是这台 ccm 自己的（渲染就在这台后端里）。
    let caps: BTreeSet<String> = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    f(&spec, &caps).map_err(|r| r.reason())
}

/// 身份 token：resume 用那个 sid（过得了段闸时），否则现铸一个 UUID v4 形的 nonce。
fn identity_token(sid: Option<&str>) -> Result<String, String> {
    if let Some(s) = sid.filter(|s| relay_route_core::segment_is_safe(s)) {
        return Ok(s.to_string());
    }
    // 随机数取 OS 那一份（经 `rustls` 带进来的 `ring`，不新增依赖）。
    let mut b = [0u8; 16];
    if rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut b)
        .is_err()
    {
        return Err(copy_text("beLaunchRender.token.noRandom", &[]));
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

/// 自定义启动命令：拼进命令之前过全仓那一张命令片段白名单（`shell_quote_core::launcher_refused_char`）。空 / 纯空白 = 没设。
fn checked_launcher(launcher: Option<&str>) -> Result<Option<String>, String> {
    let Some(l) = launcher.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    if let Some(c) = shell_quote_core::launcher_refused_char(l) {
        return Err(copy_text(
            "rsHistory.launcher.badChars",
            &[("launcher", &format!("{l:?}")), ("c", &format!("{c:?}"))],
        ));
    }
    Ok(Some(l.to_string()))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/local_tests.rs"]
mod tests;
