//! 中转看见的请求标记：带过一次就算带过 · 没看见过 ⇒ 说不出 · 只记到上界那么多个会话（丢最早的）。
//! 认不认「扩展上下文」那一项由适配层在头的值里找（逗号列表里有一项以它登记的前缀开头，大小写不论）。

use super::*;

const BETA: &str = "anthropic-beta";

#[test]
fn once_seen_with_the_item_it_stays_seen_and_unseen_sessions_say_nothing() {
    assert_eq!(seen("rm-never"), None, "中转没看见过 ⇒ 说不出");
    note("rm-std", &[(BETA, "interleaved-thinking-2025-05-14")]);
    assert_eq!(seen("rm-std"), Some(false));
    note::<&str>("rm-bare", &[]);
    assert_eq!(seen("rm-bare"), Some(false), "看见过、没带那个头 ⇒ 没带");
    note("rm-wide", &[(BETA, "x-1")]);
    note(
        "rm-wide",
        &[(
            BETA,
            "interleaved-thinking-2025-05-14, Context-1M-2025-08-07",
        )],
    );
    note("rm-wide", &[(BETA, "x-1")]); // 同一会话里不带它的小请求不把它抹掉
    assert_eq!(seen("rm-wide"), Some(true));
    note("", &[(BETA, "context-1m-2025-08-07")]);
    assert_eq!(seen(""), None, "没有流标签的请求不记");
}

#[test]
fn the_item_is_found_only_in_the_header_the_adapter_names() {
    note("rm-elsewhere", &[("x-other", "context-1m-2025-08-07")]);
    assert_eq!(
        seen("rm-elsewhere"),
        Some(false),
        "别的头里出现同样的字样不算"
    );
    note("rm-inside", &[(BETA, "no-context-1m")]);
    assert_eq!(seen("rm-inside"), Some(false), "要整项以前缀开头，不是包含");
    note("rm-case", &[("Anthropic-Beta", "context-1m-2025-08-07")]);
    assert_eq!(seen("rm-case"), Some(true), "头名不分大小写");
}

#[test]
fn the_table_keeps_at_most_the_bound_and_drops_the_oldest() {
    // 一张本地的表（不碰进程那一张：别的判据正并发地往里记）。
    let mut t = Table::default();
    for i in 0..4 {
        t.note(3, &format!("s{i}"), i % 2 == 0);
    }
    assert_eq!(t.by.len(), 3);
    assert_eq!(t.order, ["s1", "s2", "s3"]);
    assert_eq!(t.seen("s0"), None, "最早记下的那个丢了");
    assert_eq!(t.seen("s2"), Some(true));
    assert_eq!(t.seen("s3"), Some(false));
}
