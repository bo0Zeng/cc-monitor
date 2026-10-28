//! 「终端」视图那张名单的线上形（`terminals-board-here` 每台一份 · `terminals-board` 各台合成一份）：
//! 名单与状态全在后端写好，出口照摆（按 `key` 认行 · 按 `tags` 筛 · 按 `find` 找）。

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
