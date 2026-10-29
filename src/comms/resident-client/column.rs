//! **几台的会话清单并成一列**：各台各自排好（最近优先、组按那台的先后），这里只并 —— 同一刻照台的先后（稳定），不另判。
//! 本机后端并历史页那一列（`history_browse::lay`）与手机会话屏（`mobcore::chats_column_json`）都调 [`merge_by`]；
//! 手机那一屏的段也在这里切（[`column`]）：段头是那台写好的字，换了字就起新的一段。

use serde::{Deserialize, Serialize};

/// 各台各自按 `key` 倒序排好的几列 ⇒ 并成一列；同值照台的先后（稳定）。只并、不另判。
pub fn merge_by<'a, T>(lists: &[&'a [T]], key: impl Fn(&T) -> i64) -> Vec<&'a T> {
    let mut at = vec![0usize; lists.len()];
    let mut out = Vec::with_capacity(lists.iter().map(|l| l.len()).sum());
    loop {
        let mut best: Option<usize> = None;
        for (i, l) in lists.iter().enumerate() {
            let Some(x) = l.get(at[i]) else { continue };
            if best.is_none_or(|b| key(x) > key(&lists[b][at[b]])) {
                best = Some(i);
            }
        }
        let Some(b) = best else { return out };
        out.push(&lists[b][at[b]]);
        at[b] += 1;
    }
}

/// 手机会话屏交来的：勾上的那几台，照机器表先后。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ChatsColumnAsk {
    pub hosts: Vec<ColumnHost>,
}

/// 一台：它的行（那台排好的先后）· 它的组的 `order`（那台排好的先后）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnHost {
    pub rows: Vec<ColumnRow>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub groups: Vec<i64>,
}

/// 一行：并的键（那台给的 `at`）· 段头（那台写的；没有 ⇒ `null`）· 已摆进固定 · 组那几段（不再进按天那几段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnRow {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: i64,
    pub section: Option<String>,
    pub placed: bool,
}

/// 并好的：按天那几段 · 按目录时各台的组并成的一列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ChatsColumn {
    pub sections: Vec<ColumnSection>,
    pub groups: Vec<ColumnAt>,
}

/// 按天的一段：段头（那台写的）· 那几行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnSection {
    pub title: Option<String>,
    pub rows: Vec<ColumnAt>,
}

/// 第几台（[`ChatsColumnAsk::hosts`] 里的位置）的第几项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnAt {
    pub host: u32,
    pub at: u32,
}

/// 第几台的那一列 ⇒ 带上位置的那一列（并好之后还认得出是哪台第几项）。
fn placed_in<'a, T: 'a>(lists: impl Iterator<Item = &'a [T]>) -> Vec<Vec<(ColumnAt, &'a T)>> {
    let n = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    lists
        .enumerate()
        .map(|(h, l)| {
            l.iter()
                .enumerate()
                .map(|(i, x)| {
                    (
                        ColumnAt {
                            host: n(h),
                            at: n(i),
                        },
                        x,
                    )
                })
                .collect()
        })
        .collect()
}

/// 手机会话屏那一列：行按 `at` 并、摆进固定 · 组的那几行不进，按段头连着的切成几段；组按 `order` 并。
pub fn column(ask: &ChatsColumnAsk) -> ChatsColumn {
    let rows = placed_in(ask.hosts.iter().map(|h| h.rows.as_slice()));
    let row_lists: Vec<&[(ColumnAt, &ColumnRow)]> = rows.iter().map(Vec::as_slice).collect();
    let mut sections: Vec<ColumnSection> = Vec::new();
    for (at, r) in merge_by(&row_lists, |(_, r)| r.at)
        .into_iter()
        .filter(|(_, r)| !r.placed)
    {
        match sections.last_mut() {
            Some(s) if s.title == r.section => s.rows.push(*at),
            _ => sections.push(ColumnSection {
                title: r.section.clone(),
                rows: vec![*at],
            }),
        }
    }
    let groups = placed_in(ask.hosts.iter().map(|h| h.groups.as_slice()));
    let group_lists: Vec<&[(ColumnAt, &i64)]> = groups.iter().map(Vec::as_slice).collect();
    ChatsColumn {
        sections,
        groups: merge_by(&group_lists, |(_, o)| **o)
            .into_iter()
            .map(|(at, _)| *at)
            .collect(),
    }
}
