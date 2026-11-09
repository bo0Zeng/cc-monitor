//! 装 · 卸远端后端那两条（`deploy-install` · `deploy-uninstall`，本机常驻后端答）的入参与回包。
//! 入参只有那台叫什么：带着哪几个槽是本机后端自己的事实（落点旁边的槽目录），那一趟整趟在后端；名字与回包挂在入参上（[`crate::call`] 那张表）。

use serde::{Deserialize, Serialize};

/// `deploy-install` 的入参：那台的名字（怎么够到它照本机后端的机器表组；带着哪几个槽照本机后端的槽目录）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct DeployInstallReq {
    /// 那台的名字（非空；机器表按它查 · 状态成品按它记）。
    pub machine: String,
}

/// `deploy-uninstall` 的入参：同 [`DeployInstallReq`]（卸也传一份暂存件上去、由它删落点那一份）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct DeployUninstallReq {
    pub machine: String,
}

/// 这一趟做了什么：部署判定（`deploy_contract::DeployAction`）那三种的标签，值域与它一处（`DeployAction::tag`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum ActionTag {
    /// 已是这一版。
    Skip,
    /// 放上了这一版。
    Deploy,
    /// 那台不比这一版旧 ⇒ 一个字节没写。
    Keep,
}

/// `deploy-install` 的回包：这一趟做了什么 · 那一句 · 落点上现在是哪一版 · 那台是什么 OS。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Installed {
    pub action: ActionTag,
    /// 这一趟做了什么那一句（`deploy::Plan::said`）。
    pub said: String,
    /// 落点上现在那一份自报的身份（`keep` ⇒ 那台上那一份的；否则这一版的）。
    pub build: String,
    /// 那台的 OS（表 A 的词）。
    pub os: String,
}

/// `deploy-uninstall` 的回包：删没删 · 那一句（`deploy` crate 写的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Uninstalled {
    /// 删了落点那一份 ⇒ `true`；本来就不在 ⇒ `false`。
    pub removed: bool,
    pub said: String,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/deploy_tests.rs"]
mod tests;
