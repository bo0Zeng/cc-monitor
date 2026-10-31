//! **几台的会话清单并成一列**：各台各自排好（最近优先、组按那台的先后），这里只并 —— 同一刻照台的先后（稳定），不另判。
//! 本机后端并历史页那一列（`history_browse::lay`）与手机会话屏（`mobcore::chats_column_json`）都调 [`merge_by`]；
//! 分叉挂在父会话下那一步（[`nest`]）也只住这里：历史页那一列与手机会话屏都照它摊。
//! 手机那一屏的段也在这里切（[`column`]）：段头是那台写好的字，换了字就起新的一段；挂在父下的分叉跟着父，不另起段。

use std::collections::{BTreeMap, BTreeSet};

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

/// 一行在分叉树里的样子：`child` ＝ 挂在展开了的父会话下；`forks` ＝ 它下面挂着几个分叉（`forks_open` 展没展开）；
/// `orphan` ＝ 它是分叉、父会话不在这一列里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nest {
    pub child: bool,
    pub forks: u32,
    pub forks_open: bool,
    pub orphan: bool,
}

/// 并好的一列 ⇒ 分叉树摊开的一列：父只认这一列里有的（`id` · `parent` 的键里带着哪台，同一台才相等）；
/// 父在 ⇒ 不自成一行，跟在父后面、父展开了（`open`）才出（分叉的分叉同样）；父不在 ⇒ 自成一行、记孤儿。先后照这一列。
pub fn nest<'a, T, I: Ord>(
    rows: &[&'a T],
    id: impl Fn(&T) -> I,
    parent: impl Fn(&T) -> Option<I>,
    open: &BTreeSet<I>,
) -> Vec<(&'a T, Nest)> {
    let present: BTreeSet<I> = rows.iter().map(|r| id(r)).collect();
    let mut kids: BTreeMap<I, Vec<&'a T>> = BTreeMap::new();
    let mut tops: Vec<(&'a T, bool)> = Vec::new();
    for r in rows {
        match parent(r) {
            Some(p) if present.contains(&p) => kids.entry(p).or_default().push(r),
            p => tops.push((r, p.is_some())),
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    for (r, orphan) in tops {
        push_nested(&mut out, r, false, orphan, &kids, &id, open);
    }
    out
}

/// 一行 ＋ 展开了的话它下面的分叉（递归）。
fn push_nested<'a, T, I: Ord>(
    out: &mut Vec<(&'a T, Nest)>,
    r: &'a T,
    child: bool,
    orphan: bool,
    kids: &BTreeMap<I, Vec<&'a T>>,
    id: &impl Fn(&T) -> I,
    open: &BTreeSet<I>,
) {
    let me = id(r);
    let mine = kids.get(&me).map_or(&[][..], Vec::as_slice);
    let forks_open = open.contains(&me);
    out.push((
        r,
        Nest {
            child,
            forks: u32::try_from(mine.len()).unwrap_or(u32::MAX),
            forks_open,
            orphan,
        },
    ));
    if forks_open {
        for c in mine {
            push_nested(out, c, true, false, kids, id, open);
        }
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
    /// 展开了分叉的那几行。
    pub forks: Vec<ColumnFork>,
}

/// 第几台（[`ChatsColumnAsk::hosts`] 里的位置）上的一个会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnFork {
    pub host: u32,
    pub sid: String,
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

/// 一行：并的键（那台给的 `at`）· 段头（那台写的；没有 ⇒ `null`）· 已摆进固定 · 组那几段（不再进按天那几段，挂在它下面的分叉也就没了父）·
/// 会话 id · 它分叉自哪个会话（那台写的；不是分叉 ⇒ `null`）。
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
    pub sid: String,
    pub parent: Option<String>,
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
    pub rows: Vec<ColumnLine>,
}

/// 段里的一行：第几台的第几项 ＋ 它在分叉树里的样子（[`Nest`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnLine {
    pub host: u32,
    pub at: u32,
    pub child: bool,
    pub forks: u32,
    #[serde(rename = "forksOpen")]
    pub forks_open: bool,
    pub orphan: bool,
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

/// 手机会话屏那一列：行按 `at` 并、摆进固定 · 组的那几行不进；分叉挂父（[`nest`]，同一台）；
/// 顶层的行按段头连着的切成几段，挂在父下的跟着父；组按 `order` 并。
pub fn column(ask: &ChatsColumnAsk) -> ChatsColumn {
    let rows = placed_in(ask.hosts.iter().map(|h| h.rows.as_slice()));
    let row_lists: Vec<&[(ColumnAt, &ColumnRow)]> = rows.iter().map(Vec::as_slice).collect();
    let merged: Vec<&(ColumnAt, &ColumnRow)> = merge_by(&row_lists, |(_, r)| r.at)
        .into_iter()
        .filter(|(_, r)| !r.placed)
        .collect();
    let open: BTreeSet<(u32, String)> = ask.forks.iter().map(|f| (f.host, f.sid.clone())).collect();
    let mut sections: Vec<ColumnSection> = Vec::new();
    for ((at, r), n) in nest(
        &merged,
        |(at, r)| (at.host, r.sid.clone()),
        |(at, r)| r.parent.clone().map(|p| (at.host, p)),
        &open,
    ) {
        let line = ColumnLine {
            host: at.host,
            at: at.at,
            child: n.child,
            forks: n.forks,
            forks_open: n.forks_open,
            orphan: n.orphan,
        };
        match sections.last_mut() {
            Some(s) if n.child || s.title == r.section => s.rows.push(line),
            _ => sections.push(ColumnSection {
                title: r.section.clone(),
                rows: vec![line],
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
