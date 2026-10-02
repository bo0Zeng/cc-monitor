//! **线上形状 → 渲染器**：帧命令 `launch-render-cli`（那台机器要跑的那一行 `ccm …`）的入参与映射。
//!
//! 渲不出来是拒（码 `refused` ＋ 理由）：起会话只有这一条路，没有别的路可换。

use super::ccm_invocation::{
    ccm_argv, render_ccm_invocation, Action, CliAccount, CliSpec, Container,
};
use serde::Deserialize;
use std::collections::BTreeSet;

/// `launch-render-cli` 的上线入参（`deny_unknown_fields`：前端多送一个字段 ⇒ 拒，不静默吞）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CliRenderRequest {
    pub action: WireAction,
    pub container: WireContainer,
    pub cwd: Option<String>,
    pub account: WireAccount,
    pub ccm_sid: Option<String>,
    pub model: Option<String>,
    pub launcher: String,
    pub default_launcher: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireAction {
    New,
    Resume { sid: String },
    Attach { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireContainer {
    None,
    Tmux { name: String, send_into: bool },
}

/// 远端只有两态：远端是 ssh 过去，那台机器上的继承态不是 monitor 的环境 ⇒ 没有「继承」那一态。
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireAccount {
    Base,
    /// `name` 说得出 ⇒ `--account`；只有 `configDir` ⇒ `--account-dir`；都没有 ⇒ 拒。
    Account {
        name: Option<String>,
        #[serde(rename = "configDir")]
        config_dir: Option<String>,
    },
}

/// 渲成那一行；拒 ⇒ 理由。能力问这台后端自己（`ccm` 就是它，与 `--ccm-probe` 同一份）。
pub fn render_ccm_launch(req: &CliRenderRequest) -> Result<String, String> {
    let caps: BTreeSet<String> = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    render_ccm_launch_with(req, &caps)
}

/// 同上，能力集由调用方给（夹具对拍用固定的一份，不随这台后端的平台变）。
pub(crate) fn render_ccm_launch_with(
    req: &CliRenderRequest,
    caps: &BTreeSet<String>,
) -> Result<String, String> {
    with_spec(req, false, |spec| render_ccm_invocation(spec, caps)).map_err(|r| r.reason())
}

/// 同一行的 argv 形；`detach` ⇒ 建进 tmux 之后不接进去（这台后端替人在 tmux 里起会话时用 —— 与界面那一行同一份映射、同一个渲染器）。
pub(crate) fn ccm_launch_argv(
    req: &CliRenderRequest,
    caps: &BTreeSet<String>,
    detach: bool,
) -> Result<Vec<String>, String> {
    with_spec(req, detach, |spec| ccm_argv(spec, caps)).map_err(|r| r.reason())
}

/// 上线入参 ⇒ 渲染器那份 spec（唯一的映射）。
fn with_spec<T>(
    req: &CliRenderRequest,
    detach: bool,
    f: impl FnOnce(&CliSpec) -> Result<T, super::ccm_invocation::Refusal>,
) -> Result<T, super::ccm_invocation::Refusal> {
    let action = match &req.action {
        WireAction::New => Action::New,
        WireAction::Resume { sid } => Action::Resume { sid },
        WireAction::Attach { name } => Action::Attach { name },
    };
    let container = match &req.container {
        WireContainer::None => Container::None,
        WireContainer::Tmux { name, send_into } => Container::Tmux {
            name,
            send_into: *send_into,
        },
    };
    let account = match &req.account {
        WireAccount::Base => CliAccount::Base,
        WireAccount::Account { name, config_dir } => CliAccount::Named {
            name: name.as_deref(),
            config_dir: config_dir.as_deref(),
        },
    };
    let spec = CliSpec {
        action,
        container,
        cwd: req.cwd.as_deref(),
        account,
        ccm_sid: req.ccm_sid.as_deref(),
        model: req.model.as_deref(),
        launcher: &req.launcher,
        default_launcher: &req.default_launcher,
        args: &[],
        launch_id: None,
        ccm_path: "ccm",
        detach,
    };
    f(&spec)
}

// `K-R95`：本机拉起载荷里「哪个号」那一格的键名由后端那一份生成给前端（生成物 ＋ 它的判据）。
#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/launch_wire_k_r95_launch_render_facts.rs"]
mod k_r95_launch_render_facts;
