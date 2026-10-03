//! 中转看见的请求标记：带过一次就算带过 · 没看见过 ⇒ 说不出 · 只记到上界那么多个会话（丢最早的）。

use super::*;

const WIDE: (&str, &str) = ("anthropic-beta", "context-1m");

#[test]
fn once_seen_with_the_item_it_stays_seen_and_unseen_sessions_say_nothing() {
    assert_eq!(seen("rm-never", WIDE), None, "中转没看见过 ⇒ 说不出");
    note("rm-std", &[(WIDE, false)]);
    assert_eq!(seen("rm-std", WIDE), Some(false));
    note("rm-wide", &[(WIDE, false)]);
    note("rm-wide", &[(WIDE, true)]);
    note("rm-wide", &[(WIDE, false)]); // 同一会话里不带它的小请求不把它抹掉
    assert_eq!(seen("rm-wide", WIDE), Some(true));
    note("", &[(WIDE, true)]);
    assert_eq!(seen("", WIDE), None, "没有流标签的请求不记");
}

#[test]
fn the_table_keeps_at_most_the_bound_and_drops_the_oldest() {
    // 一张本地的表（不碰进程那一张：别的判据正并发地往里记）。
    let mut t = Table::default();
    for i in 0..4 {
        t.note(3, &format!("s{i}"), &[(WIDE, i % 2 == 0)]);
    }
    assert_eq!(t.by.len(), 3);
    assert_eq!(t.order, ["s1", "s2", "s3"]);
    assert_eq!(t.seen("s0", WIDE), None, "最早记下的那个丢了");
    assert_eq!(t.seen("s2", WIDE), Some(true));
    assert_eq!(t.seen("s3", WIDE), Some(false));
}
