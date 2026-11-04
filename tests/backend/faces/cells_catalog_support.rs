//! 格目录的测试档那一半：整份目录（[`catalog`]）· 冻结格（[`FROZEN`]）。
//! 判据（`cells_catalog_tests`）· 金样 `cells-catalog.golden.json` · 主 crate `build_id_guard` 的冻结格指纹读它（经 `test-support`）；
//! 生产里只有 `view` 投影读成品名与路径（[`PRODUCTS`]）。

use super::*;

/// 一件成品哪几格冻结。
pub enum Frozen {
    /// 没有前端照它逐格读到「缺一格就坏」的地步。
    No,
    /// 整件：每一格都冻结（今天的、以后加的）。
    All,
    /// 只这几格（目录里的路径写法）：第二个前端读的那一截；其余的格照常可改。
    Cells(&'static [&'static str]),
}

impl Frozen {
    /// 这一格冻结不冻结。
    pub fn holds(&self, path: &str) -> bool {
        match self {
            Frozen::No => false,
            Frozen::All => true,
            Frozen::Cells(c) => c.contains(&path),
        }
    }
}

/// 整份目录（纯计算，不读盘）：`{products: [{name, cells: [{path, kind, type, frozen}]}]}`。
pub fn catalog() -> serde_json::Value {
    let products: Vec<serde_json::Value> = PRODUCTS
        .iter()
        .map(|p| {
            let cells: Vec<serde_json::Value> = cells_of(p)
                .0
                .into_iter()
                .map(|c| {
                    serde_json::json!({"path": c.path, "kind": c.kind, "type": c.ty, "frozen": frozen_of(p.name).holds(&c.path)})
                })
                .collect();
            serde_json::json!({"name": p.name, "cells": cells})
        })
        .collect();
    serde_json::json!({"products": products})
}

/// 每件成品哪几格冻结（[`PRODUCTS`] 每件恰好一行，判据钉）：两个前端（桌面界面 · 手机）都照它读的那几格 —— 删 / 改名 / 换类型 ⇒ 判据红
/// （对的是落盘的格目录金样：金样里标了冻结的格、目录里没了或换了样，重写金样也不放行）。
pub const FROZEN: &[(&str, Frozen)] = &[
    ("record", Frozen::All),
    ("facts", Frozen::No),
    ("needs_row", Frozen::No),
    ("index_row", Frozen::No),
    ("read_row", Frozen::All),
    (
        "session_added",
        Frozen::Cells(&[
            "sid",
            "path",
            "cwd",
            "name",
            "agent_kind",
            "attachable",
            "face.dot",
            "face.life.activity",
            "face.can.stop.can",
            "face.settled",
            "face.name",
            "face.tone",
            "face.can.model.can",
            "face.can.model.why",
            "face.can.permission.can",
            "face.can.permission.why",
            "background",
        ]),
    ),
    (
        "session_status",
        Frozen::Cells(&[
            "sid",
            "waiting_for",
            "face.dot",
            "face.life.activity",
            "face.can.stop.can",
            "face.settled",
            "face.name",
            "face.tone",
            "face.can.model.can",
            "face.can.model.why",
            "face.can.permission.can",
            "face.can.permission.why",
        ]),
    ),
    (
        "session_left",
        Frozen::Cells(&[
            "sid",
            "face.dot",
            "face.life.by",
            "face.settled",
            "face.name",
            "face.tone",
            "face.can.model.can",
            "face.can.model.why",
            "face.can.permission.can",
            "face.can.permission.why",
        ]),
    ),
    (
        "quota_read",
        Frozen::Cells(&[
            "state",
            "text",
            "detail",
            "names.accounts.*",
            "names.slots.*",
            "accounts[].account",
            "accounts[].rows[][].text",
            "accounts[].rows[][].tone",
            "accounts[].fiveHour",
            "accounts[].warm.act",
            "accounts[].warm.at",
            "accounts[].warm.text",
            "unseen[].account",
            "unseen[].rows[][].text",
            "unseen[].rows[][].tone",
            "unseen[].fiveHour",
            "unseen[].warm.act",
            "unseen[].warm.at",
            "unseen[].warm.text",
        ]),
    ),
    (
        "history_list",
        Frozen::Cells(&[
            "up.*.sessionId",
            "up.*.label",
            "up.*.atText",
            "up.*.sectionText",
            "up.*.projectPath",
            "up.*.jsonlPath",
            "up.*.agent",
            "up.*.agentTag",
            "up.*.face.dot",
            "up.*.face.life.activity",
            "up.*.face.settled",
            "up.*.face.name",
            "up.*.face.tone",
            "up.*.face.can.model.can",
            "up.*.face.can.model.why",
            "up.*.face.can.permission.can",
            "up.*.face.can.permission.why",
            "up.*.starred",
            "up.*.customTitle",
            "up.*.hidden.by",
            "up.*.hidden.said",
            "held",
            "whole",
            "same",
            "order[]",
            "head.total",
            "head.info.notice",
        ]),
    ),
    // 组（`groups-read`）：手机会话列表按组分段照这几格摆（段头 · 组员 · 固定 · 那台没读全那一句）。
    (
        "groups_read",
        Frozen::Cells(&[
            "groups[].id",
            "groups[].title",
            "groups[].counts[].text",
            "groups[].members[].sid",
            "groups[].members[].facts.title",
            "groups[].members[].facts.cwd",
            "groups[].members[].face.dot",
            "groups[].members[].face.name",
            "groups[].members[].face.tone",
            "pinned[].sid",
            "pinned[].facts.title",
            "pinned[].face.dot",
            "machines[].read.is",
            "machines[].read.why",
        ]),
    ),
];

/// 那件成品的冻结格（[`FROZEN`] 里没登 ⇒ 不冻结；登没登全由判据管）。
pub fn frozen_of(name: &str) -> &'static Frozen {
    FROZEN
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(&Frozen::No, |(_, f)| f)
}
