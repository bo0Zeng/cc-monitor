//! **最近优先 · 几台并成一列**：一列怎么排只有 [`sort_by_recency`] 一条（倒序、稳定）；几台并一列只有 [`merge_by`] 一条
//! （各台的先后不打散，每一步取排头里最近的，同一刻照台的先后），不另判。本机后端每台花片段预算的先后（`search_rules`）· 几台的全文搜索结果合一份（[`crate::search::merge`]）·
//! 本机后端并历史页那一列（`history_browse::lay`）· 手机会话屏（手机核心 `column`）都调这两个；
//! 分叉挂在父会话下那一步（[`nest`]）也只住这里：历史页那一列与手机会话屏都照它摊。

use std::collections::{BTreeMap, BTreeSet};

/// **最近优先**（全仓唯一的排法）：按 `key` 倒序，同值照原先后（稳定）。
pub fn sort_by_recency<T>(items: &mut [T], key: impl Fn(&T) -> i64) {
    items.sort_by_key(|a| std::cmp::Reverse(key(a)));
}

/// 几台各一列（各台自己排好的先后，不重排）⇒ 并成一列：每一步取各台排头里 `key` 最大的那一个，同值照台的先后。只并、不另判。
/// 各台的一列都照 [`sort_by_recency`] 排好时，出来的就是整列照它排的样子；台里另有先后（历史页那几种排法）时照那台的先后，不打散。
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

#[cfg(test)]
#[path = "../../../../tests/common/face-core/column_tests.rs"]
mod tests;
