//! 「终端」视图那张名单的线上形（`terminals-board-here` 每台一份 · `terminals-board` 各台合成一份）：
//! 名单与状态全在后端写好，出口照摆（按 `key` 认行 · 按 `tags` 筛 · 按 `find` 找）。
//! 监控板那一对（`monitor-board-here` 每台一份 · `monitor-board` 各台合成）也住这里：分组 · 组内先后 · 组头 · 汇总那一句后端写好。

use crate::cells::Words;
use crate::frame::SessionDot;
use crate::session::Ability;
use crate::Machine;
use serde::{Deserialize, Serialize};

/// 筛选那四格（闭集）：全部 · 有会话的 · 没有会话的 · 后台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum BoardTag {
    All,
    Session,
    Bare,
    Background,
}

/// 名单里的一行（一个终端）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BoardRow {
    /// 这一行在整张名单里的键（不透明、跨几次问稳定）：出口按它认行。
    pub key: String,
    /// 终端句柄（`terminals-list` 那一格；抓屏 · 订画面 · 送字时交回）。
    pub terminal: String,
    /// 终端的名字（`terminals-list` 那一行的 `name`：tmux 是会话名，托管是 `ccm --attach <名>` 认的那个名）。
    pub name: String,
    /// 里面跑着的会话。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sid: Option<String>,
    pub dot: SessionDot,
    /// 点的名字（读屏 · 悬停）。
    pub dot_name: Words,
    pub title: Words,
    /// 单独终端窗口的系统标题（标题 · 终端 · 那台）：各台合成那一步写（那台叫什么合成时才知道），这台那一份是空的。
    pub window_title: Words,
    /// 标题照命令原文、等宽画（没有会话的终端）。
    pub mono: bool,
    /// 标题旁的徽标（在等人 ⇒ 那一态的短名）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub badge: Option<Words>,
    /// 第二行（号或「没有会话」· 目录）。
    pub meta: Words,
    /// 第二行尾的小签（后台 · 终端窗口 ×n · 已退出）。
    pub chips: Vec<Words>,
    /// 行尾那一格（现在 · 1m · 2h）。
    pub at_text: Words,
    /// 它算进哪几格筛选（不含 `all`）。
    pub tags: Vec<BoardTag>,
    /// 找的时候比对的那一串（标题 · 目录 · 终端名，已小写）。
    pub find: String,
    /// 能不能结束。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(
            type = "true | { no: \"not_yours\" | \"not_managed\" | \"other_windows\", said: string }"
        )
    )]
    pub can_end: Ability,
}

/// `terminals-board-here` 的回包：这台的那几行（排好了）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BoardHere {
    pub rows: Vec<BoardRow>,
    /// `false` ＝ 名单里有读不懂的行。
    pub complete: bool,
}

/// 一台没答：那一句（＋ 上次答的那份名单是几点的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BoardDown {
    pub said: Words,
}

/// 一台一组。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BoardGroup {
    pub machine: Machine,
    pub name: Words,
    /// 组头那一句（n 个终端 · m 个后台）。
    pub sub: Words,
    /// 这台没答 ⇒ 那一句；`rows` 是上次答的那份（点一律 unknown）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub down: Option<BoardDown>,
    /// `false` ＝ 那台名单里有读不懂的行。
    pub complete: bool,
    pub rows: Vec<BoardRow>,
}

/// 筛选的一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BoardFilter {
    pub tag: BoardTag,
    pub label: Words,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub count: u32,
}

/// `terminals-board` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TerminalsBoard {
    pub filters: Vec<BoardFilter>,
    pub groups: Vec<BoardGroup>,
}

/// `monitor-board-here` 每台一行：这台那一条会话此刻的序 · 活不活 · 几个 agent 在跑。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MonitorHereRow {
    pub sid: String,
    /// 这台发出去的那张脸上的序（`face.order`）。
    pub order: u8,
    /// 那张脸的一生是「活」。
    pub live: bool,
    /// 这台运行簿里这条会话此刻在跑的子 agent 数。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub running: u32,
}

/// `monitor-board-here` 的回包：问到的那几条里这台认得的（不认得的不进）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MonitorHere {
    pub rows: Vec<MonitorHereRow>,
}

/// 监控板的一组（一台）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MonitorGroup {
    pub machine: Machine,
    /// 组头那一句（那台的名字 ＋ 几条）。
    pub title: Words,
    /// 组内的先后（排好了）。
    pub sids: Vec<String>,
    /// 这台没答 ⇒ 那台离线那一句（`sids` 照请求先后）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub down: Option<Words>,
}

/// `monitor-board` 的回包：顶上那一句 ＋ 各台一组（排好了）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MonitorBoard {
    /// 几台 · 几条活会话 · 几个 agent 在跑；一组都没有 ⇒ 空。
    pub summary: Words,
    pub groups: Vec<MonitorGroup>,
}

/// 「结束所选」那一批里的一项：哪台名单里的哪个终端（句柄）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalsEndItem {
    pub machine: Machine,
    /// 名单里的那个句柄（`terminals-list` · 名单那一行的 `terminal`）。
    pub terminal: String,
}

/// `terminals-end`：结束跨几台的一批终端（按名单句柄；有没有会话都一样）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalsEndReq {
    /// 那几项（1–64 个，照选中的先后）。
    pub items: Vec<TerminalsEndItem>,
    /// 自报的前端（「哪个前端的会话」那一维，同 `terminal-end`）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
}

/// 一个终端的结局上线那一形：每台那一跳与本机合好的回包同一形。可缺的格一律出 `null`，不省键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalOutcome {
    /// 哪台（每台那一跳答的是它自己 ⇒ 本机那一个；本机合好时换成那一项的那台）。
    pub machine: Machine,
    pub terminal: String,
    pub outcome: crate::batch::SessionState,
    /// 没成的原因码；做成了 ⇒ `null`。
    pub why: Option<String>,
    /// 那台写好的那一句；做成了 ⇒ `null`。
    pub said: Option<String>,
    /// 没成那一项的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
    pub copy_detail: String,
    /// 那台名单那一行的标题（远端的套机器名）。
    pub title: String,
}

/// `terminals-end` 的回包：逐条结局与入参同序，汇总句 · 逐条行 · 复制详情都写好（同 `SessionsBatchReply` 那四格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalsEndReply {
    pub results: Vec<TerminalOutcome>,
    pub said: String,
    pub failed: bool,
    pub lines: Vec<String>,
    pub copy_detail: String,
}
