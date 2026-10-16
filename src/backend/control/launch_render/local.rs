//! **本机起会话**（新起 / resume / 接回）那一行 —— 本机后端出成品；monitor 只剩「开一个终端窗口跑这一行」
//! （`open_local_terminal`）。
//!
//! 一行 = `ccm …`（[`super::ccm_invocation`]）：POSIX 上有会话名 ⇒ 建进 tmux（`--ccm-tmux=`）；Windows / 没名字 ⇒ 直路。
//! 环境与中转地址由 `ccm` 自己定。
//! 事实（平台 · 目录在不在）由 [`Facts`] 给，[`plan`] 是纯的 —— 判据喂确定的事实驱动生产那一条。

use crate::control::launch_account::{self as la, AccountAsk, Settled};
use copy_core::copy_text;
use serde::Deserialize;

/// `launch-local` 的入参。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LocalLaunchRequest {
    /// 这个会话是哪一家（线上的 kind）。必填：没说就不知道该怎么起，不落默认那一家。
    pub(crate) agent: String,
    pub(crate) action: LocalAction,
    /// 只用来核「新起」那一格的目录在不在；终端的工作目录由 monitor 开窗口时给。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) cwd: Option<String>,
    /// 不上线：自定义启动命令（[`Self::with_prefs`] 从这台的偏好填；没设 / 等于默认 ⇒ `None`）。
    #[serde(skip)]
    pub(crate) launcher: Option<String>,
    /// 缺席 = 不表态（继承环境；接回那一形）· 跟随 · 账号 0 · 用户点名（[`AccountAsk`]）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) account: Option<AccountAsk>,
    /// 要建进 tmux 时的会话名（界面铸名口铸的；这里不铸 —— 撞名避让只有一个家）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) tmux_name: Option<String>,
    /// 不上线：这一家 agent 的默认启动器（适配层那一份；等于它就不吐 `--launcher`）。
    #[serde(skip)]
    pub(crate) default_launcher: String,
    /// 不上线（后端自己起会话时填）：垫在交给那一家的那一串里的参数（起新会话先定 sid：`--session-id <uuid>`）。
    #[serde(skip)]
    pub(crate) preset_args: Vec<String>,
    /// 不上线（后端自己起会话时填）：这一趟用的模型（交给那一家的 `--model`）；`None` ＝ 跟随。
    #[serde(skip)]
    pub(crate) model: Option<String>,
}

impl LocalLaunchRequest {
    /// 填上不上线的那两格：启动命令照这台的偏好，默认启动器问适配层。
    pub(crate) fn with_prefs(mut self, prefs: &crate::control::machine_prefs::Prefs) -> Self {
        let (l, d) = super::Launch::launchers(&self.agent, prefs);
        self.launcher = Some(l).filter(|l| l != &d);
        self.default_launcher = d;
        self
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum LocalAction {
    New,
    Resume {
        sid: String,
    },
    /// 接回一个已有的 tmux 会话（名字走 `tmuxName`）：不起 agent ⇒ 不要账号、身份。
    Attach,
}

/// [`plan`] 要的两样事实。生产那一份是 [`Facts::PRODUCTION`]。
#[derive(Clone, Copy)]
pub(crate) struct Facts {
    /// 本地终端是 PowerShell（Windows）还是 POSIX shell。Windows 上没有 tmux ⇒ 一律直路。
    pub(crate) windows: bool,
    pub(crate) is_dir: fn(&str) -> bool,
    /// 这台 `ccm` 的入口（绝对路径）：那一行直接叫它，不靠终端 `PATH`。说不出 ⇒ `None`（拒）。
    pub(crate) entry: fn() -> Option<String>,
}

impl Facts {
    pub(crate) const PRODUCTION: Facts = Facts {
        windows: crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        is_dir: dir_exists,
        entry: crate::platform::paths::installed_ccm_entry,
    };
}

fn dir_exists(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

/// 这一趟用哪个号（[`la::settle`]；缺席 ⇒ 不表态）。选不了 ⇒ `Err`。
pub(crate) fn settle(
    req: &LocalLaunchRequest,
    facts: &la::Facts,
) -> Result<Settled, la::AccountUnavailable> {
    let sid = match &req.action {
        LocalAction::Resume { sid } => Some(sid.as_str()),
        _ => None,
    };
    match &req.account {
        None => Ok(Settled::Unsaid),
        Some(a) => la::settle(a, sid, &Default::default(), facts),
    }
}

/// 整条计划（号已经判好）。拒 ⇒ `Err(那一句)`。
pub(crate) fn plan(
    req: &LocalLaunchRequest,
    account: &Settled,
    facts: &Facts,
) -> Result<String, String> {
    let (l, entry) = launch_of(req, facts)?;
    super::line_of(&l, account, here(facts), &super::own_caps(), &entry)
}

/// 本机那一行的 argv 形（同 [`plan`]），`detach` ⇒ 建进 tmux 之后不接进去。名字是调用方铸好的。
pub(crate) fn plan_argv(
    req: &LocalLaunchRequest,
    account: &Settled,
    facts: &Facts,
    detach: bool,
) -> Result<Vec<String>, String> {
    let (l, entry) = launch_of(req, facts)?;
    super::argv_of(&l, account, here(facts), detach, &super::own_caps(), &entry)
}

fn here(facts: &Facts) -> super::Here {
    super::Here::Local {
        windows: facts.windows,
    }
}

/// 本机那一形的入参 ⇒ [`super::Launch`]（接回要名字；新起核目录在不在）＋ 这台 `ccm` 的入口。
fn launch_of(req: &LocalLaunchRequest, facts: &Facts) -> Result<(super::Launch, String), String> {
    let name = req.tmux_name.as_deref().filter(|n| !n.is_empty());
    let entry = (facts.entry)().ok_or_else(|| copy_text("beLaunchRender.entry.noHome", &[]))?;
    let act = match &req.action {
        LocalAction::Attach if facts.windows => {
            return Err(copy_text("rsHistory.attach.windows", &[]))
        }
        LocalAction::Attach => match name {
            Some(n) => super::Act::Attach(n.to_string()),
            None => return Err(copy_text("rsHistory.launch.noSessionName", &[])),
        },
        LocalAction::Resume { sid } => super::Act::Resume(sid.clone()),
        LocalAction::New => {
            if let Some(dir) = req.cwd.as_deref().filter(|d| !d.is_empty()) {
                if !(facts.is_dir)(dir) {
                    return Err(copy_text("rsHistory.newSession.noDir", &[("cwd", dir)]));
                }
            }
            super::Act::New
        }
    };
    // 身份标记打在建出来的 tmux 会话上（resume 才说得出 sid）。
    let ccm_sid = match (&act, name) {
        (super::Act::Resume(sid), Some(_)) => Some(sid.clone()),
        _ => None,
    };
    let l = super::Launch {
        agent: req.agent.clone(),
        act,
        tmux: name.map(str::to_string),
        send_into: false,
        cwd: req.cwd.clone(),
        ccm_sid,
        model: req.model.clone(),
        launcher: req.launcher.clone().unwrap_or_default(),
        default_launcher: req.default_launcher.clone(),
        preset_args: req.preset_args.clone(),
    };
    Ok((l, entry))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/local_tests.rs"]
mod tests;
