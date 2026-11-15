//! 书签那两条（`bookmarks-read` · `bookmarks-edit`）的线上类型：书签归那台后端（`~/.cc-monitor/bookmarks.json`，
//! 后端 `control/bookmarks.rs` 是唯一的写者），文件窗口与手机照机器名问它，本机与远端同一形。
//!
//! 「此刻这几个目录在不在书签里、各是第几条」由后端答：两条入参都可带窗口各栏此刻的目录（`at`，几栏几个），
//! 回包的 `here` 按问的那一串各答一个下标（不在的不列）。
//!
//! 目录怎么归一、按什么比，只在后端 `control/bookmarks.rs` 里判；窗口与手机照回包画。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `bookmarks-read` 的入参（缺 · `null` · `{}` 都认，多一个键拒）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BookmarksReadReq {
    /// 各栏此刻的目录（这台的绝对路径）：回包的 `here` 答每一个是第几条。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub at: Vec<String>,
}

/// `bookmarks-edit` 做哪一样（内联进入参那份生成类型，不单出一份）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub enum BookmarkOp {
    /// 每一条：在就删、不在就加到最后（★）。
    Toggle,
    /// 每一条：在就删（×）。
    Remove,
    /// 每一条：不在就加到最后（迁移旧书签用：重来一次不翻转）。
    Add,
}

/// `bookmarks-edit` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BookmarksEditReq {
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub op: BookmarkOp,
    /// 那几个目录（这台的绝对路径；按归一之后的形比、存）。
    pub dirs: Vec<String>,
    /// 各栏此刻的目录：回包的 `here` 答每一个在改完的那一份里是第几条。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub at: Vec<String>,
}

/// 两条的回包：这台此刻的书签（保序、不重样、已归一）。也是盘上那一份的形（盘上不带 `here`）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Bookmarks {
    pub dirs: Vec<String>,
    /// 入参 `at` 里的每一串（原样作键）⇒ 它按归一之后的形在 `dirs` 里的下标；不在的不列，没问 ⇒ 空。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<BTreeMap<String, u32>>")
    )]
    pub here: BTreeMap<String, u32>,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/bookmarks_tests.rs"]
mod tests;
