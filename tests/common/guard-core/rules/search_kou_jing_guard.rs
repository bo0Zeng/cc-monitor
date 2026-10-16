//! **搜索那一族助手 monitor 这一侧零处**。
//!
//! # ✅ 它服务哪条要求：「一个判定只有一个家」·「本机与远端走同一条代码路径」
//!
//! 口径的家恰一份、后端搜索那一侧只调它，由后端 `tests/backend/history/search_rules_tests.rs::the_search_kou_jing_has_exactly_one_home` 钉。
//! 这里只剩 ③：monitor 生产树里零处定义那 12 个助手 —— 本机搜索只许经通道问本机后端。

use super::{none, rule};

/// 收口前在两侧**各写一遍**的那 12 个助手（`K-R85` 逐条实测同名同形）。
const HELPERS: &[&str] = &[
    "extract_text_blocks",
    "extract_tool_text",
    "stringify_json",
    "user_text",
    "make_snippet",
    "find_ci",
    "tail_chars",
    "head_chars",
    "collapse_ws",
    "collapse_ws_keep_ellipsis",
    "truncate_plain",
    "truncate_excerpt",
];

fn defs(prod: &str) -> Vec<&'static str> {
    HELPERS
        .iter()
        .copied()
        .filter(|h| prod.contains(&format!("fn {h}(")))
        .collect()
}

#[test]
fn the_monitor_grows_no_search_helpers() {
    rule(|repo| {
        let stray: Vec<String> = repo
            .crate_tree("src/frontend/shell", &["rs"])
            .into_iter()
            .filter_map(|f| {
                let found = defs(f.prod());
                (!found.is_empty()).then(|| format!("{} （定义：{found:?}）", f.rel))
            })
            .collect();
        none(
            "monitor 生产树里又有人在搜会话了（本机搜索只许经通道问本机后端 —— 在 monitor 里再建一份就是 LOC1b 删掉的那个第二读者）",
            stray,
            defs("pub(crate) fn make_snippet(t: &str) {}").len(),
        );
    });
}
