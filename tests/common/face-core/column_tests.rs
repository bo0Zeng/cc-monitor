//! 最近优先只有一条规则：一列按键倒序、同值不动（[`sort_by_recency`]）；几台并一列 ＝ 照台的先后拼起来、再照它排（[`merge_by`]）。
//! 本机后端历史页 · 全文搜索几台合一份 · 手机会话屏都经这两个，规则不另写。
use super::*;

/// 一列：最近的在前；同值的照原先后（稳定）。
#[test]
fn sort_by_recency_is_newest_first_and_stable() {
    let mut v = vec![("a", 1), ("b", 3), ("c", 1), ("d", 2), ("e", 3)];
    sort_by_recency(&mut v, |x| x.1);
    let names: Vec<&str> = v.iter().map(|x| x.0).collect();
    assert_eq!(names, ["b", "e", "d", "a", "c"]);
}

/// 各台都照 [`sort_by_recency`] 排好 ⇒ 并出来的就是整列照它排（同一刻照台的先后）：两条规则说的是同一件事。
#[test]
fn merge_of_sorted_lists_is_the_sort_of_their_concatenation() {
    let mut here = vec![("h1", 5), ("h2", 9), ("h3", 2), ("h4", 7)];
    let mut there = vec![("t1", 9), ("t2", 4), ("t3", 7)];
    sort_by_recency(&mut here, |x| x.1);
    sort_by_recency(&mut there, |x| x.1);
    let got: Vec<&str> = merge_by(&[&here[..], &there[..]], |x| x.1)
        .into_iter()
        .map(|x| x.0)
        .collect();
    let mut want: Vec<(&str, i64)> = here.iter().chain(there.iter()).copied().collect();
    sort_by_recency(&mut want, |x| x.1);
    let want: Vec<&str> = want.iter().map(|x| x.0).collect();
    assert_eq!(got, want);
    assert_eq!(got, ["h2", "t1", "h4", "t3", "h1", "t2", "h3"]);
}

/// 台里另有先后（历史页按别的排法交来的一列）⇒ 那台的先后不打散：每一步只在各台排头里取。
#[test]
fn merge_by_never_reorders_within_one_host() {
    let here = [("h1", 1), ("h2", 9)];
    let there = [("t1", 5)];
    let got: Vec<&str> = merge_by(&[&here[..], &there[..]], |x| x.1)
        .into_iter()
        .map(|x| x.0)
        .collect();
    assert_eq!(got, ["t1", "h1", "h2"]);
}

/// 各台已排好时：同一刻照台的先后（这台在前），空的那台不占位。
#[test]
fn merge_by_keeps_host_order_on_ties() {
    let a = [("a1", 3), ("a2", 1)];
    let b: [(&str, i64); 0] = [];
    let c = [("c1", 3), ("c2", 2)];
    let got: Vec<&str> = merge_by(&[&a[..], &b[..], &c[..]], |x| x.1)
        .into_iter()
        .map(|x| x.0)
        .collect();
    assert_eq!(got, ["a1", "c1", "c2", "a2"]);
}
