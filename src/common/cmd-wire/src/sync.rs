//! 带键集合的同步形：客户端带着手上那份的记号来问，后端答「没变 / 只给变了的 / 整份」。
//! 比在后端（`backend/common/views.rs`）；这里只放线上形与客户端那一步机械的 [`Hand::apply`]（Rust 消费方共用；手机那一份照同一份金样）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 手上那份的记号：不透明，客户端只存、原样交回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "SyncHeld", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(transparent)]
pub struct Held(pub String);

/// 只要一窗：排好的第 `from` 项起、至多 `limit` 项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "SyncRange", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub from: u32,
    pub limit: u32,
}

/// 这一问的「头」：整份有几项（不止这一窗）＋ 业务自己的那一格（分组 · 截没截 · 面包屑 …）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "SyncHead", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Head<H> {
    pub total: u32,
    pub info: H,
}

/// 一次答：`same` ⇒ 什么都没变（`order` `head` 为 null、`up` 空）；`whole` ⇒ 手上那份作废、`up` 就是这一窗的全部；
/// 否则 `up` 是变了 / 新进来的那几项，`order`（这一窗的键，先后照排）· `head` 变了才带。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "SyncPatch", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Patch<H, T> {
    pub held: Held,
    pub whole: bool,
    pub same: bool,
    pub order: Option<Vec<String>>,
    pub up: BTreeMap<String, T>,
    pub head: Option<Head<H>>,
}

/// 客户端手上那一份：记号 ＋ 这一窗的先后 ＋ 各项 ＋ 头。空的那一份 ＝ 没问过（不带记号问 ⇒ 整份）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hand<H, T> {
    pub held: Option<Held>,
    pub order: Vec<String>,
    pub items: BTreeMap<String, T>,
    pub head: Option<Head<H>>,
}

impl<H, T> Default for Hand<H, T> {
    fn default() -> Self {
        Hand {
            held: None,
            order: Vec::new(),
            items: BTreeMap::new(),
            head: None,
        }
    }
}

impl<H, T> Hand<H, T> {
    /// 照答换，没有判断：`same` ⇒ 只换记号；`whole` ⇒ 先清空；带了 `order` / `head` 就换；`up` 盖上；只留键在 `order` 里的。
    pub fn apply(&mut self, p: Patch<H, T>) {
        self.held = Some(p.held);
        if p.same {
            return;
        }
        if p.whole {
            self.items.clear();
            self.order.clear();
            self.head = None;
        }
        if let Some(order) = p.order {
            self.order = order;
        }
        if let Some(head) = p.head {
            self.head = Some(head);
        }
        self.items.extend(p.up);
        let keep: std::collections::HashSet<&String> = self.order.iter().collect();
        self.items.retain(|k, _| keep.contains(k));
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/sync_tests.rs"]
mod tests;
