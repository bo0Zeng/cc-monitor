//! **线上形状 → 渲染器**：帧命令 `launch-render-cli`（那台机器要跑的那一行 `ccm …`）的入参与映射。
//!
//! 渲不出来是拒（码 `refused` ＋ 理由）：起会话只有这一条路，没有别的路可换。

use crate::control::launch_account::{self as la, AccountAsk, Settled};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// `launch-render-cli` 的上线入参（`deny_unknown_fields`：前端多送一个字段 ⇒ 拒，不静默吞）。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CliRenderRequest {
    /// 这个会话是哪一家（线上的 kind）。必填：没说就不知道该怎么起，不落默认那一家。
    pub agent: String,
    pub action: WireAction,
    pub container: WireContainer,
    pub cwd: Option<String>,
    pub account: AccountAsk,
    pub ccm_sid: Option<String>,
    /// 显式指定的模型（压过下面那张表）。
    pub model: Option<String>,
    /// 不上线：这台的模型偏好表（号 → 模型，[`Self::with_prefs`] 从这台的偏好填）：判出来的号在表里 ⇒ 用那一条。
    #[serde(skip)]
    pub models: BTreeMap<String, String>,
    /// 不上线：启动命令（[`Self::with_prefs`] 从这台的偏好填，没设过 ⇒ 那一家的默认）。
    #[serde(skip)]
    pub launcher: String,
    /// 不上线：那一家的默认启动器（适配层那一份）。
    #[serde(skip)]
    pub default_launcher: String,
    /// 不上线（后端自己起会话时填）：垫在交给那一家的那一串里的参数（起新会话先定 sid：`--session-id <uuid>`）。
    #[serde(skip)]
    pub preset_args: Vec<String>,
}

impl CliRenderRequest {
    /// 填上不上线的那三格：启动命令 · 模型表照这台的偏好，默认启动器问适配层（[`super::Launch::launchers`]）。
    pub(crate) fn with_prefs(mut self, prefs: &crate::control::machine_prefs::Prefs) -> Self {
        (self.launcher, self.default_launcher) = super::Launch::launchers(&self.agent, prefs);
        self.models = prefs.models.clone();
        self
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireAction {
    New,
    Resume { sid: String },
    Attach { name: String },
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireContainer {
    None,
    Tmux { name: String, send_into: bool },
}

/// 这一趟用哪个号（[`la::settle`]）。选不了 ⇒ `Err`。
pub(crate) fn settle(
    req: &CliRenderRequest,
    facts: &la::Facts,
) -> Result<Settled, la::AccountUnavailable> {
    let sid = match &req.action {
        WireAction::Resume { sid } => Some(sid.as_str()),
        _ => None,
    };
    la::settle(&req.account, sid, &req.models, facts)
}

/// 渲成那一行（号已经判好）；拒 ⇒ 理由。能力问这台后端自己（`ccm` 就是它，与 `--ccm-probe` 同一份）。
pub(crate) fn render_ccm_launch(
    req: &CliRenderRequest,
    account: &Settled,
) -> Result<String, String> {
    render_ccm_launch_with(req, account, &super::own_caps(), &own_entry()?)
}

/// 这台 `ccm` 的入口（绝对路径）：那一行直接叫它 —— 远端交互 shell 的 `PATH` 上未必有 `ccm`（没装接入块就没有）。
pub(crate) fn own_entry() -> Result<String, String> {
    crate::platform::paths::installed_ccm_entry()
        .ok_or_else(|| copy_core::copy_text("beLaunchRender.entry.noHome", &[]))
}

/// 同上，能力集与入口由调用方给（夹具对拍用固定的一份，不随这台后端的平台与家目录变）。
pub(crate) fn render_ccm_launch_with(
    req: &CliRenderRequest,
    account: &Settled,
    caps: &BTreeSet<String>,
    ccm_path: &str,
) -> Result<String, String> {
    super::line_of(
        &launch_of(req),
        account,
        super::Here::Remote,
        caps,
        ccm_path,
    )
}

/// 同一行的 argv 形；`detach` ⇒ 建进 tmux 之后不接进去。
pub(crate) fn ccm_launch_argv(
    req: &CliRenderRequest,
    account: &Settled,
    caps: &BTreeSet<String>,
    ccm_path: &str,
    detach: bool,
) -> Result<Vec<String>, String> {
    super::argv_of(
        &launch_of(req),
        account,
        super::Here::Remote,
        detach,
        caps,
        ccm_path,
    )
}

/// 上线入参 ⇒ [`super::Launch`]。
fn launch_of(req: &CliRenderRequest) -> super::Launch {
    let (tmux, send_into) = match &req.container {
        WireContainer::None => (None, false),
        WireContainer::Tmux { name, send_into } => (Some(name.clone()), *send_into),
    };
    super::Launch {
        agent: req.agent.clone(),
        act: match &req.action {
            WireAction::New => super::Act::New,
            WireAction::Resume { sid } => super::Act::Resume(sid.clone()),
            WireAction::Attach { name } => super::Act::Attach(name.clone()),
        },
        tmux,
        send_into,
        cwd: req.cwd.clone(),
        ccm_sid: req.ccm_sid.clone(),
        model: req.model.clone(),
        launcher: req.launcher.clone(),
        default_launcher: req.default_launcher.clone(),
        preset_args: req.preset_args.clone(),
    }
}
