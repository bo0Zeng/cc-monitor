//! **↗ 切到终端**的线上那三型：浮层上那一张（[`FrontFace`]）· 一颗按钮（[`FrontAct`]）· 壳拉前一次交给出口的（[`FrontTold`]）。
//!
//! 只有类型（TS 导出到 `frontend/ui/generated/`）；结局与原因、照它们写字的那一段住 `face_core::front`。出口照摆 [`FrontFace`]，
//! 按钮上的字照 [`FrontAct`] 取，不按结局自己挑。

use crate::cells::{Tone, Words};
use serde::Serialize;

/// 浮层上的一颗按钮（做什么由出口接；字照 kind 取，是骨架上的字）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrontAct {
    /// ［接上终端］。
    Connect,
    /// ［在终端里打开］（接回 tmux）。
    OpenInTerminal,
    /// ［在 cc-monitor 里打开］。
    OpenHere,
    /// ［复制详情］：复制 `detail` 原文。
    Copy { detail: String },
    /// ［更新］那台后端。
    Update,
    /// ［重试］。
    Retry,
    /// ［重新连接］。
    Reconnect,
}

/// ↗ 浮层那一张：标题 · 正文（空 ⇒ 不出）· 灰字办法 · 语气 · 按钮。出口照摆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct FrontFace {
    pub title: Words,
    pub body: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hint: Option<Words>,
    pub tone: Tone,
    pub acts: Vec<FrontAct>,
}

/// 壳拉前一次交给出口的：切过去了（什么都不出，终端到前面就是回执）· 那一张。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrontTold {
    Switched,
    Said(FrontFace),
}
