//! **线上形状 → 渲染器**：帧命令 `launch-render-cli`（那台机器要跑的那一行 `ccm …`）的入参与映射。
//!
//! 渲不出来是拒（码 `refused` ＋ 理由）：起会话只有这一条路，没有别的路可换。

use super::ccm_invocation::{
    ccm_argv, render_ccm_invocation, Action, CliAccount, CliSpec, Container,
};
use crate::control::launch_account::{self as la, AccountAsk, Settled};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// `launch-render-cli` 的上线入参（`deny_unknown_fields`：前端多送一个字段 ⇒ 拒，不静默吞）。
#[derive(Debug, Deserialize)]
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
    /// 这台的模型偏好表（号 → 模型，用户设置的原值）：判出来的号在表里 ⇒ 用那一条。
    #[serde(default)]
    pub models: BTreeMap<String, String>,
    pub launcher: String,
    pub default_launcher: String,
    /// 不上线（后端自己起会话时填）：垫在交给那一家的那一串里的参数（起新会话先定 sid：`--session-id <uuid>`）。
    #[serde(skip)]
    pub preset_args: Vec<String>,
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
    let caps: BTreeSet<String> = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    let entry = own_entry()?;
    render_ccm_launch_with(req, account, &caps, &entry)
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
    with_spec(req, account, false, ccm_path, |spec| {
        render_ccm_invocation(spec, caps)
    })
    .map_err(|r| r.reason())
}

/// 同一行的 argv 形；`detach` ⇒ 建进 tmux 之后不接进去（这台后端替人在 tmux 里起会话时用 —— 与界面那一行同一份映射、同一个渲染器）。
pub(crate) fn ccm_launch_argv(
    req: &CliRenderRequest,
    account: &Settled,
    caps: &BTreeSet<String>,
    ccm_path: &str,
    detach: bool,
) -> Result<Vec<String>, String> {
    with_spec(req, account, detach, ccm_path, |spec| ccm_argv(spec, caps)).map_err(|r| r.reason())
}

/// 上线入参 ＋ 判好的号 ⇒ 渲染器那份 spec（唯一的映射）。
fn with_spec<T>(
    req: &CliRenderRequest,
    account: &Settled,
    detach: bool,
    ccm_path: &str,
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
    // 远端是 ssh 过去，那台的继承态不是 monitor 的环境 ⇒ 不表态也落账号 0。
    let (account, picked_model) = match account {
        Settled::Base | Settled::Unsaid => (CliAccount::Base, None),
        Settled::Account(a) => (
            CliAccount::Named {
                name: a.name.as_str(),
            },
            a.model.as_deref(),
        ),
    };
    let args: Vec<&str> = req.preset_args.iter().map(String::as_str).collect();
    let spec = CliSpec {
        agent: &req.agent,
        action,
        container,
        cwd: req.cwd.as_deref(),
        account,
        ccm_sid: req.ccm_sid.as_deref(),
        model: req.model.as_deref().or(picked_model),
        launcher: &req.launcher,
        default_launcher: &req.default_launcher,
        args: &args,
        ccm_path,
        detach,
    };
    f(&spec)
}
