/// 收口前在两侧**各写一遍**的那 12 个助手（`K-R85` 逐条实测同名同形）。
/// ⚠ 人群按「口径可能住在哪」取，不是按「当初动过哪几行」取。
const HELPERS: &[&str] = &[
    "extract_text_blocks",
    "extract_tool_text",
    "stringify_json",
    "clean_user_text",
    "make_snippet",
    "find_ci",
    "tail_chars",
    "head_chars",
    "collapse_ws",
    "collapse_ws_keep_ellipsis",
    "truncate_plain",
    "truncate_excerpt",
];

/// 口径常量 —— **它们就是口径本身**，两侧任何一处再写一遍就是第二份口径。
const CONSTS: &[&str] = &[
    "MAIN_CAP",
    "TOOL_CAP",
    "SNIPPET_CTX",
    "PER_SESSION_CAP",
    "DEFAULT_LIMIT",
];

/// 两侧都必须真的调到的东西（不只是「import 了」）。
const MUST_CALL: &[&str] = &[
    "search_core::make_snippet",
    "search_core::sort_by_recency",
    "search_core::clean_user_text",
    "search_core::session_title",
];

#[test]
fn the_search_kou_jing_has_exactly_one_home() {
    // ── ① core 确实持有口径，否则下面两条退化成「哪里都没有」，零命中地绿。
    let core = guard_core::production_code(include_str!(
        "../../src/bridge/crates/search-core/src/lib.rs"
    ));
    let missing: Vec<&str> = HELPERS
        .iter()
        .copied()
        .filter(|h| !core.contains(&format!("pub fn {h}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "`search-core` 生产段里找不到这些助手：{missing:?}\n             口径搬走了还是抽取坏了？本条此刻无效。"
    );
    let missing_c: Vec<&str> = CONSTS
        .iter()
        .copied()
        .filter(|c| !core.contains(&format!("pub const {c}: usize")))
        .collect();
    assert!(
        missing_c.is_empty(),
        "`search-core` 生产段里找不到这些口径常量：{missing_c:?}"
    );
    assert!(
        core.contains("pub struct SnippetBudget") && core.contains("pub enum SnippetVerdict"),
        "snippet 预算（含「预算用完」vs「单会话满」这一拆）必须住 core —— \
             它一旦回到两侧，`hits: []` 又会变成一个装两件事的值"
    );
    assert!(
        core.contains("pub fn sort_by_recency"),
        "预算顺序必须住 core：收口前 monitor 按 `updated_at desc`、daemon 按 readdir，\
             `--limit 50` 下两侧给出的 3 个会话**只重合 1 个**"
    );

    // ── ② 两侧：不许自己再有一份，且必须真的调 core。
    for (name, raw) in [
        (
            "monitor src/search.rs",
            include_str!("../../src/bridge/src/search.rs"),
        ),
        (
            "daemon observe/search_query.rs",
            include_str!("../../src/backend/observe/search_query.rs"),
        ),
    ] {
        let prod = guard_core::production_code(raw);
        let redefined: Vec<&str> = HELPERS
            .iter()
            .copied()
            .filter(|h| prod.contains(&format!("fn {h}(")))
            .collect();
        assert!(
            redefined.is_empty(),
            "{name} 的生产段里又长出了这些助手的定义：{redefined:?}\n                 🔴 那就是 `K-R100` 收口前的形状：两份实现、逐字相同、**零判据对拍**。\n                 「今天没漂」不是保障 —— 下一次谁改一侧，另一侧静默留在原地，\n                 而搜索结果不一致**不会报错**（本地一份 snippet、远端另一份，谁都不抛异常）。"
        );
        let reconst: Vec<&str> = CONSTS
            .iter()
            .copied()
            .filter(|c| prod.contains(&format!("const {c}")))
            .collect();
        assert!(
            reconst.is_empty(),
            "{name} 的生产段里重新定义了口径常量：{reconst:?}\n                 那些数**就是口径**，只许住 `search-core`。在这里写一个同样的字面量，\n                 两边漂开时**搜索结果会静默不一致**。"
        );
        for call in MUST_CALL {
            assert!(
                prod.contains(call),
                "{name} 不再调 `{call}` —— 它要么自己算了一遍（第二份口径），\n                     要么口径搬家了而本条没跟。"
            );
        }
    }
}
