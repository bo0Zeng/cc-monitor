//! 书签那两条（`bookmarks-read` · `bookmarks-edit`）的线上类型：书签归那台后端（`~/.cc-monitor/bookmarks.json`，
//! 后端 `control/bookmarks.rs` 是唯一的写者），文件窗口与手机照机器名问它，本机与远端同一形。
//!
//! 「此刻这个目录在不在书签里、是第几条」由后端答：两条入参都可带窗口此刻的目录（`at`），回包的 `here` 是它的下标。
//!
//! 目录的归一（[`normalize_dir`]）只此一份：后端存之前归一、比较按归一之后的形；文件窗口地址栏手输那一串、
//! 左栏「家」与「根」判此刻也用它（浏览回包还不带归一后的目录）。

use serde::{Deserialize, Serialize};

/// 目录路径归一：连续的 `/` 折成一个；只剩 `/` 就是根；否则去掉尾巴上那一个 `/`。
///
/// ⚠ 空串照旧是空串、相对路径照旧是相对的（不补前导 `/`）；空串由写的那一方挡掉，不在这里编一个路径出来。
pub fn normalize_dir(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev_slash = false;
    for c in path.chars() {
        if c == '/' {
            if !prev_slash {
                out.push(c);
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    if out == "/" {
        return out;
    }
    if out.ends_with('/') {
        out.pop();
    }
    out
}

/// `bookmarks-read` 的入参（缺 · `null` · `{}` 都认，多一个键拒）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BookmarksReadReq {
    /// 此刻的目录（这台的绝对路径）：带了 ⇒ 回包的 `here` 答它是第几条。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<String>,
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
    /// 此刻的目录：带了 ⇒ 回包的 `here` 答它在改完的那一份里是第几条。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<String>,
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
    /// 入参的 `at` 按归一之后的形在 `dirs` 里的下标；没问 · 不在 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub here: Option<u32>,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/bookmarks_tests.rs"]
mod tests;
