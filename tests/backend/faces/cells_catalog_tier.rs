//! 格目录的测试档那一半：整份目录（[`catalog`]）· 冻结格（[`Frozen`]）· 缺格表（[`PENDING`]）。
//! 判据 · 金样 `cells-catalog.golden.json` · `build_id_guard` 的冻结格指纹读它；生产里只有 `view` 投影读成品名与路径（[`super::PRODUCTS`]）。
//! 原先的帧命令 `cells-catalog` 没有调用方，第 8 批删了。

use super::{cells_of, PRODUCTS};

/// 一件成品哪几格冻结。
pub(crate) enum Frozen {
    /// 没有前端照它逐格读到「缺一格就坏」的地步。
    No,
    /// 整件：每一格都冻结（今天的、以后加的）。
    All,
    /// 只这几格（目录里的路径写法）：第二个前端读的那一截；其余的格照常可改。
    Cells(&'static [&'static str]),
}

impl Frozen {
    /// 这一格冻结不冻结。
    pub(crate) fn holds(&self, path: &str) -> bool {
        match self {
            Frozen::No => false,
            Frozen::All => true,
            Frozen::Cells(c) => c.contains(&path),
        }
    }
}

/// 已经判了要补、还没落地的一格。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub(crate) struct Pending {
    pub(crate) product: &'static str,
    pub(crate) path: &'static str,
    pub(crate) kind: &'static str,
}

/// 还缺的格（手机端 10-09 缺格清单）。落地那一刻删掉这里那一行（判据两向钉）。今天一条都不缺；机制留着，下一张缺格清单进这里。
///
/// 已落地 / 不补的（这里不再登记）：
/// - G1 会话灯「后台任务运行中」那一档 ⇒ `activity` 的闭集多了 `backgroundWork`（不是新格），字与语气在 `activity_text` · `activity_tone`。
/// - G2 会话状态的字与语气 ⇒ `session_*.activity_text` · `activity_tone` · `session_state.state_text` …
/// - G3 「在等什么」的字与先答哪个 ⇒ `facts.needs.text` · `facts.needs.rank`。那台 pidfile 的 `waitingFor` 六个原值由适配层翻成
///   [`WaitOn`](crate::agents::WaitOn)，再与记录里没结果的那一步一起在 `facts_query::needs_of` 判成八种 `needs.kind`（映射只在这两处）；
///   字与序跟着种类走，不另挂在 `session_status` 上。
/// - G4 上下文的字 ⇒ `facts.usage.contextText`（判得出上限写百分比、判不出只写用了多少）· `limitText` · `promptTokensText` ·
///   `percent` · `contextTone` · `limitFromText`；上限只有核心一套判定（`facts_query::context_limit`），出口不再按型号猜。
/// - G5 骨架行的时刻：**不补**（10-09 定走法甲）。时刻在骨架上唯一的用处是算「最新的那一支」，手机改吃 `history-branch.off`
///   拿主线之后这一用处没了；要画时刻的地方读正文时记录本身带 `at` · `timeText`。
pub(crate) const PENDING: &[Pending] = &[];

/// 整份目录（纯计算，不读盘）：`{products: [{name, cells: [{path, kind, type, frozen}]}], pending: [{product, path, kind}]}`。
pub(crate) fn catalog() -> serde_json::Value {
    let products: Vec<serde_json::Value> = PRODUCTS
        .iter()
        .map(|p| {
            let cells: Vec<serde_json::Value> = cells_of(p)
                .0
                .into_iter()
                .map(|c| {
                    serde_json::json!({"path": c.path, "kind": c.kind, "type": c.ty, "frozen": p.frozen.holds(&c.path)})
                })
                .collect();
            serde_json::json!({"name": p.name, "cells": cells})
        })
        .collect();
    serde_json::json!({"products": products, "pending": PENDING})
}
