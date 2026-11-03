//! 组那一族的线上形（`groups-here` 一台那一片 · `groups-read` 几台合好的那一份）。
//! 「几台合一份」那一步连组头那几句住 `face_core::groups::merge`（本机后端与手机嵌的 Rust 核心调同一份）；这里只留类型。

use crate::cells::Words;
use crate::frame::{SessionDot, SessionFace};
use crate::listing::HistoryMachineRead;
use crate::Machine;
use serde::{Deserialize, Serialize};

// ───────────────────────── 一台那一片：groups-here ─────────────────────────

/// 这一片里的一组：id · 名字 · 几时建的 · 这个名字几时起的（毫秒）。进组时整份抄进组员那一台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HereGroup {
    pub id: String,
    pub name: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub made: i64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub named: i64,
}

/// 复活一个标签要的那几格（重启后那条会话没有活进程，照它造灰标签）：那台照自己那份清单那一行给。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MemberFacts {
    /// 显示的标题（改过的 ＞ 标题 ＞ 第一句 ＞ 清单那一句「没名字」）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub title: Words,
    /// 项目目录（清单里没有那一条 ⇒ 缺）。
    pub cwd: Option<String>,
    /// 记录文件（resume 照它；清单里没有 ⇒ 缺）。
    pub record: Option<String>,
    /// 上次跑在哪个号下（没记 ⇒ 缺）。
    pub account: Option<String>,
    /// 后台会话。
    pub bg: bool,
    /// 最后活动（毫秒；清单里没有 ⇒ 缺）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub at: Option<i64>,
}

/// 那台一条带组或固定的会话：在哪几组 · 固没固定 · 那台此刻给它画的样子 · 复活那几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HereMember {
    pub sid: String,
    pub groups: Vec<String>,
    pub pinned: bool,
    pub face: SessionFace,
    pub facts: MemberFacts,
}

/// `groups-here` 的回包：那台那一片。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupsHere {
    pub groups: Vec<HereGroup>,
    pub members: Vec<HereMember>,
}

// ───────────────────────── 几台并成一份：groups-read ─────────────────────────

/// 一组的名字此刻从哪来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum GroupKnown {
    /// 名字取自这一问答了的那一片。
    Now,
    /// 名字取自那台上一次答的那一片（那台这一问没答；`at` ＝ 那一份几时读到的，写好的字）。
    Before {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
        at: Words,
    },
    /// 哪一片都没有这一组的记录（只有组员指着它）：名字缺。
    Unknown,
}

/// 组里一条会话：哪台 · 哪条 · 固没固定 · 那张脸（那台这一问没答 ⇒ 看不见的那一张）· 复活那几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupMember {
    #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
    pub origin: Machine,
    pub sid: String,
    pub pinned: bool,
    pub face: SessionFace,
    pub facts: MemberFacts,
}

/// 组头那一颗点：几条 ＋ 那颗点的名字（照组员那张脸上的字）。只出有的那几样，序：需手动 · 在跑 · 后台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupCount {
    pub dot: SessionDot,
    pub n: u32,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub word: Words,
    /// 组头那一句（「需手动 2」那样，出口照摆）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: Words,
}

/// 并好的一组。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupRead {
    pub id: String,
    /// 名字：各片里 `named` 最新的那一份（哪一片都没有记录 ⇒ 缺）。
    pub name: Option<String>,
    /// 叫它什么（出口照摆）：有名字 ⇒ 名字；没有 ⇒ 照实说不知道。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub title: Words,
    /// 各片里最早的那个建组时刻（没有记录 ⇒ 缺）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub made: Option<i64>,
    /// 取到的那个名字几时起的（没有记录 ⇒ 缺）：进组时同名字一起抄进组员那一台。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub named: Option<i64>,
    pub known: GroupKnown,
    /// 组员：照问的台序、再照 sid。
    pub members: Vec<GroupMember>,
    pub counts: Vec<GroupCount>,
}

/// 一台这一问的结局（同历史页那一列那一格：答了 · 上次所知 · 读不到）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupsMachine {
    #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
    pub origin: Machine,
    pub read: HistoryMachineRead,
}

/// `groups-read` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupsRead {
    /// 各组：照建的先后、再照 id。
    pub groups: Vec<GroupRead>,
    /// 各台固定的会话（不论在不在组里），同组员那一形。
    pub pinned: Vec<GroupMember>,
    /// 问的每一台这一问的结局（照问的先后）。
    pub machines: Vec<GroupsMachine>,
}
