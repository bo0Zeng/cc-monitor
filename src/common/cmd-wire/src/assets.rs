//! 资产目录同步那一条（`assets-sync`，本机常驻后端答）的入参与回包：壳连上一台时照它交那台的名字，界面经通道问同一条。

use serde::{Deserialize, Serialize};

/// `assets-sync` 的入参：缺 ⇒ 对机器表里「连这台」的每一台各一趟；给 `origin` ⇒ 先对那一台（机器表里要有）。
/// 怎么够到那台只看本机后端的机器表：线上不交拨号（带 `dial` ⇒ `bad_args`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AssetsSyncReq {
    /// 那台的名字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub origin: Option<crate::Machine>,
}

/// 一趟对一台的结局（`synced[]` 的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AssetsSyncRow {
    /// 那台的名字。
    pub origin: crate::Machine,
    /// 那台目录的 `self`（没拉成 ⇒ `null`）。
    pub peer: Option<String>,
    /// 本机目录因这一趟变了没有。
    pub changed: bool,
    /// 推过去几台快照。
    pub pushed: usize,
    /// 那一趟哪里没办成（`null` ＝ 全办成了）。
    pub error: Option<String>,
}

/// 「连这台」的那几台的一行：`machine` = 对面的资产目录 id，没拉过 ⇒ `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ReachRow {
    /// 那台的名字。
    pub origin: crate::Machine,
    /// 对面的资产目录 id。
    pub machine: Option<String>,
}

/// `assets-sync` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AssetsSynced {
    /// 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>`。
    #[serde(rename = "self")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "self"))]
    pub self_id: String,
    pub synced: Vec<AssetsSyncRow>,
    /// 「连这台」的那几台 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）。
    pub reach: Vec<ReachRow>,
}
