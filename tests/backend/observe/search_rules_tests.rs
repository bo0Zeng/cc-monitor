//! 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` 的 `--search` 条（最近优先 · `hitsTruncated`）＋（口径只有一个家）
//!
//! 核原文：`--search` 条逐字「① **行序 = snippet 预算顺序 = 最近优先**（按 jsonl mtime 降序）」，并说「本会话超
//! `PER_SESSION_CAP`(30) 条只列前 30」与「全局 `--limit` 用完」**不是一回事** —— `budget_tells_exhausted_apart_from_session_cap`
//! 与 `sort_by_recency_is_newest_first` 判的正是这两句。其余助手（找词 · 片段 · 截断 · 标题）的单测挂在 `D1` 下。
//! 随通用口径从共享 crate `search-core` 搬来（期望一字未改）；Claude 记录文本那几格在 `agents/claudecode/text_tests.rs`。

use super::*;

#[test]
fn find_ci_ascii_and_cjk() {
    assert_eq!(find_ci("Hello World", "world"), Some((6, 11)));
    assert_eq!(find_ci("HELLO", "hell"), Some((0, 4)));
    assert_eq!(find_ci("abc", "xyz"), None);
    let s = "你好世界 docker 部署";
    let (start, end) = find_ci(s, "docker").unwrap();
    assert_eq!(&s[start..end], "docker");
}

#[test]
fn make_snippet_three_parts() {
    let (before, matched, after) =
        make_snippet("请问怎么用 Docker 部署这个服务到生产环境", "docker");
    assert_eq!(matched, "Docker");
    assert!(before.contains("怎么用"));
    assert!(after.contains("部署"));
}

/// snippet 上下文窗口**就是** `SNIPPET_CTX` —— 期望值从常量取，不写字面量。

/// 改 `SNIPPET_CTX` 一处，这条自动跟上；哪一侧改成自己的数，那一侧就红。

#[test]
fn snippet_context_window_is_exactly_snippet_ctx() {
    let filler = "x".repeat(SNIPPET_CTX * 4);
    let text = format!("{filler}docker{filler}");
    let (before, _m, after) = make_snippet(&text, "docker");
    // 截断了 ⇒ before 前缀一个 `…`、after 后缀一个 `…`，各自内容恰好 SNIPPET_CTX 字符。
    assert_eq!(
        before.chars().count(),
        SNIPPET_CTX + 1,
        "before = … + CTX 字符"
    );
    assert_eq!(
        after.chars().count(),
        SNIPPET_CTX + 1,
        "after = CTX 字符 + …"
    );
}

/// 合并口径（那张差表，两边各取更对的一半）逐格：

/// 五种包装全剥（含 stderr）· 两句样板整行剥（不分大小写、句号可省、只认整行）· 中断标记只在**整条**恰是它时归零。

#[test]
fn truncate_helpers() {
    assert_eq!(truncate_plain("hello", 3), "hel");
    assert_eq!(truncate_plain("你好世界", 2), "你好");
    assert_eq!(truncate_plain("hi", 10), "hi");
    assert_eq!(truncate_excerpt("a\nb", 10), "a b");
    assert_eq!(truncate_excerpt("abcd", 3), "abc…");
}

#[test]
fn clamp_limit_bounds() {
    assert_eq!(clamp_limit(0), LIMIT_MIN);
    assert_eq!(clamp_limit(99_999), LIMIT_MAX);
    assert_eq!(clamp_limit(50), 50);
}

/// 🔴 预算的两个「不给」必须分得开。

#[test]
fn budget_tells_exhausted_apart_from_session_cap() {
    let mut b = SnippetBudget::new(2);
    assert_eq!(b.take(0), SnippetVerdict::Give);
    assert_eq!(b.take(1), SnippetVerdict::Give);
    // 全局预算用完 ⇒ 之后不论哪个会话都不给（整份结果被砍，会话行上 `hitsTruncated` 说出来）。
    assert_eq!(b.take(2), SnippetVerdict::BudgetExhausted);
    assert_eq!(b.take(0), SnippetVerdict::BudgetExhausted);

    // 预算只剩一格、但单会话满了 ⇒ **不是**「结果被砍」，也不花预算：下一个会话照样拿得到那一格。
    let mut c = SnippetBudget::new(1);
    assert_eq!(c.take(PER_SESSION_CAP), SnippetVerdict::SessionCapped);
    assert_eq!(
        c.take(0),
        SnippetVerdict::Give,
        "单会话超 {PER_SESSION_CAP} 条只是「这个会话话多」，没花掉全局预算 —— \
             这两件事合成一个 bool 正是本 crate 要拆开的那个病"
    );
    assert_eq!(c.take(0), SnippetVerdict::BudgetExhausted);
}

/// 最近优先：原地降序，稳定到「同 key 保持原相对序」。

#[test]
fn sort_by_recency_is_newest_first() {
    let mut v = vec![("a", 10i64), ("b", 300), ("c", 200)];
    sort_by_recency(&mut v, |x| x.1);
    assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), ["b", "c", "a"]);
}

#[test]
fn session_title_picks_in_order() {
    assert_eq!(session_title(Some("AI"), "首句", "abcdefghij"), "AI");
    assert_eq!(session_title(Some("  "), "首句", "abcdefghij"), "首句");
    assert_eq!(session_title(None, "", "abcdefghij"), "abcdefgh");
}

// ═══ 搜索口径只有一个家（原 `tests/frontend/shell/search_kou_jing_guard.rs` 的 ①② 两道，随家搬进后端）══════
//
// 要求：「一个判定只有一个家」。家从共享 crate `search-core` 拆成两份：通用口径 = 本文件的被测
// `observe/search_rules.rs`，Claude 记录文本 = 适配层 `agents/claudecode/text.rs`；搜索那一侧（`observe/search_query.rs`）不许自己再有一份，
// 且必须真的调到家里那一份。monitor 那一侧零处（③）仍住 `tests/frontend/shell/search_kou_jing_guard.rs`。

/// 通用口径的助手（收口前两侧各写一遍的那 12 个里的 8 个）。
const RULE_HELPERS: &[&str] = &[
    "make_snippet",
    "find_ci",
    "tail_chars",
    "head_chars",
    "collapse_ws",
    "collapse_ws_keep_ellipsis",
    "truncate_plain",
    "truncate_excerpt",
];

/// Claude 记录文本的助手（那 12 个里的另 4 个）。
const TEXT_HELPERS: &[&str] = &[
    "extract_text_blocks",
    "extract_tool_text",
    "stringify_json",
    "user_text",
];

/// 口径常量 —— **它们就是口径本身**，任何一处再写一遍就是第二份口径。
const CONSTS: &[&str] = &[
    "MAIN_CAP",
    "TOOL_CAP",
    "SNIPPET_CTX",
    "PER_SESSION_CAP",
    "DEFAULT_LIMIT",
];

/// 搜索那一侧必须真的调到的东西（不只是「import 了」）。
const MUST_CALL: &[&str] = &[
    "search_rules::make_snippet",
    "search_rules::sort_by_recency",
    "crate::agents::human_speech",
    "search_rules::session_title",
];

#[test]
fn the_search_kou_jing_has_exactly_one_home() {
    let rules =
        guard_core::production_code(include_str!("../../../src/backend/observe/search_rules.rs"));
    let text = guard_core::production_code(include_str!(
        "../../../src/backend/agents/claudecode/text.rs"
    ));
    let search =
        guard_core::production_code(include_str!("../../../src/backend/observe/search_query.rs"));
    let defs = |code: &str, h: &str| code.matches(&format!("fn {h}(")).count();
    // ① 家里恰各一份（否则下面 ② 退化成「哪里都没有」，零命中地绿）。
    for h in RULE_HELPERS {
        assert_eq!(
            defs(&rules, h),
            1,
            "`search_rules.rs` 里 `{h}` 不是恰好一份"
        );
        assert_eq!(defs(&text, h), 0, "`text.rs` 里长出了通用助手 `{h}`");
    }
    for h in TEXT_HELPERS {
        assert_eq!(defs(&text, h), 1, "`text.rs` 里 `{h}` 不是恰好一份");
        assert_eq!(
            defs(&rules, h),
            0,
            "`search_rules.rs` 里长出了记录文本助手 `{h}`"
        );
    }
    for c in CONSTS {
        assert_eq!(
            rules.matches(&format!("const {c}: usize")).count(),
            1,
            "`search_rules.rs` 里口径常量 `{c}` 不是恰好一份"
        );
    }
    assert!(
        rules.contains("struct SnippetBudget") && rules.contains("enum SnippetVerdict"),
        "snippet 预算（含「预算用完」vs「单会话满」这一拆）必须住口径的家"
    );
    assert_eq!(
        defs(&rules, "sort_by_recency<T>"),
        1,
        "预算顺序（最近优先）必须住口径的家"
    );
    // ② 搜索那一侧：不许自己再有一份，且必须真的调家里那一份。
    let redefined: Vec<&str> = RULE_HELPERS
        .iter()
        .chain(TEXT_HELPERS)
        .copied()
        .filter(|h| defs(&search, h) > 0)
        .collect();
    assert!(
        redefined.is_empty(),
        "`search_query.rs` 的生产段里又长出了这些助手的定义：{redefined:?}（`K-R100` 收口前的形状：两份实现、零判据对拍）"
    );
    let reconst: Vec<&str> = CONSTS
        .iter()
        .copied()
        .filter(|c| search.contains(&format!("const {c}")))
        .collect();
    assert!(
        reconst.is_empty(),
        "`search_query.rs` 的生产段里重新定义了口径常量：{reconst:?}"
    );
    for call in MUST_CALL {
        assert!(
            search.contains(call),
            "`search_query.rs` 不再调 `{call}` —— 要么自己算了一遍（第二份口径），要么口径搬家了而本条没跟"
        );
    }
}

/// 〔perfC #5〕大小写不敏感的「包含」：答案与「整段转小写再找」逐个相同（Unicode 多字符展开 · 希腊词尾 Σ · CJK · 空串），
/// 而且每问不分配（每问要对常驻索引里每一条跑一遍，每条转小写那一份分配就是每问的大头）：
/// 先各跑一趟让本线程那块复用的缓冲长到够大，之后再跑一个字节都不分配。
#[test]
fn contains_lc_agrees_with_lowercasing_and_allocates_nothing() {
    let hays = [
        "",
        "Hello World",
        "游标分页 Cursor 改造",
        "ΟΔΟΣ ΟΔΟΣ.",
        "ΣΑΣ",
        "İstanbul İ",
        "ẞtraße STRASSE",
        "abcabcabd",
        "ﬁle ﬃ",
        "Ǆ ǅ ǆ",
        "mixed CASE with 中文 and ÀÉÎ",
    ];
    let needles = [
        "",
        "a",
        "hello",
        "world",
        "o w",
        "cursor",
        "游标",
        "改造",
        "οδος",
        "οδοσ",
        "σας",
        "ας",
        "ς",
        "i̇stanbul",
        "istanbul",
        "i̇",
        "ß",
        "straße",
        "strasse",
        "abd",
        "abcabd",
        "ﬁ",
        "ǆ",
        "àéî",
        "中文 and",
    ];
    for h in hays {
        for n in needles {
            assert_eq!(
                contains_lc(h, n),
                h.to_lowercase().contains(n),
                "{h:?} 里找 {n:?}：与整段转小写再找不一样"
            );
        }
    }
    let long_ascii = "Some Long ASCII text with Cursor pagination ".repeat(2000);
    let long_cjk = "把分页逻辑改成游标分页，顺带补测试。Cursor ".repeat(2000);
    contains_lc(&long_cjk, "热身");
    let base = crate::alloc_probe::reset_peak();
    let a = contains_lc(&long_ascii, "cursor pagination");
    let b = contains_lc(&long_cjk, "游标分页，顺带");
    let c = contains_lc(&long_cjk, "没有这一句");
    let grew = crate::alloc_probe::peak_since(base);
    assert!(a && b && !c);
    assert_eq!(grew, 0, "大小写不敏感的包含分配了 {grew} 字节");
}
