//! **↗ 切到终端**的线上那几型：浮层上那一张（[`FrontFace`]）· 一颗按钮（[`FrontAct`]）· 壳拉前一次交给出口的（[`FrontTold`]）·
//! 壳窗口那一跳没成的结局码（[`FrontCode`]）与本机后端照它写好的那一张（[`MissedFace`]，随 `session-window` 的回包来）。
//!
//! 只有类型（TS 导出到 `frontend/ui/generated/`）；照原因与结局码写字的那一段住 `face_core::front`（只本机后端调）。出口照摆 [`FrontFace`]，
//! 按钮上的字照 [`FrontAct`] 取，不按结局自己挑。

use crate::cells::{Tone, Words};
use serde::{Deserialize, Serialize};

/// 浮层上的一颗按钮（做什么由出口接；字照 kind 取，是骨架上的字）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// 壳窗口那一跳没成的结局码（闭集）：壳只判出是哪一种，那一张由本机后端照它写好、随 `session-window` 的回包一起来（[`MissedFace`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "kebab-case")]
pub enum FrontCode {
    /// 认得的那个窗口已经不在了。
    Gone,
    /// 句柄或进程号已被别的进程复用 ⇒ 认不准。
    Unclear,
    /// 找到了，系统不许抢前台（它在任务栏闪），或答应了前台却没换。
    Refused,
    /// 整条链上哪一级都没有窗口：真在后台。
    NoWindow,
    /// Wayland 会话：别的程序的窗口看不见、也切不过去。
    DesktopWontSwitch,
    /// 这台系统没有「按句柄找 / 验 / 拉前窗口」这一族。
    Unsupported,
    /// 分不清是哪个窗口：本会话的 shell 没登记（Linux）。
    SeveralUnregistered,
    /// 分不清：借本会话的控制台被系统拒了（Windows）。
    SeveralConsole,
    /// 分不清：终端是以管理员身份运行的（Windows）。
    SeveralElevated,
    /// 分不清：终端程序不提供控制台（Windows）。
    SeveralForeign,
    /// 窗口到前面了、标签页没切成：标签名不随程序标题变。
    TabTitleIgnored,
    /// 窗口到前面了、标签页没切成：读不到 / 点不了标签页。
    TabNoUia,
    /// 窗口到前面了、标签页没切成：终端以管理员身份运行。
    TabElevated,
    /// 窗口到前面了、标签页没切成：读不到本会话的控制台。
    TabNoConsole,
    /// 窗口那一跳没在期限内回（系统调用卡住）。
    TimedOut,
}

impl FrontCode {
    /// 全部的码（后端每个写一张）。
    pub const ALL: [FrontCode; 15] = [
        FrontCode::Gone,
        FrontCode::Unclear,
        FrontCode::Refused,
        FrontCode::NoWindow,
        FrontCode::DesktopWontSwitch,
        FrontCode::Unsupported,
        FrontCode::SeveralUnregistered,
        FrontCode::SeveralConsole,
        FrontCode::SeveralElevated,
        FrontCode::SeveralForeign,
        FrontCode::TabTitleIgnored,
        FrontCode::TabNoUia,
        FrontCode::TabElevated,
        FrontCode::TabNoConsole,
        FrontCode::TimedOut,
    ];
}

/// 一个结局码 ⇒ 本机后端写好的那一张。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MissedFace {
    pub code: FrontCode,
    pub face: FrontFace,
}
