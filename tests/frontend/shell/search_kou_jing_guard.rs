//! **搜索那一族助手 monitor 这一侧零处**〔`K-R85` · LOC1b〕。
//!
//! # ✅ 它服务哪条要求：**`设计/01 §5 D1`**「一个判定只有一个家」·〔LOC1b〕`设计/00 §2.5 ①`「本机与远端走同一条代码路径」
//!
//! 〔P1〕从前本文件还钉「口径的家恰一份 · 后端搜索那一侧只调它」（①②）：家从共享 crate `search-core` 拆进后端之后，
//! 那两道随家搬进后端 `tests/backend/observe/search_rules_tests.rs::the_search_kou_jing_has_exactly_one_home`（不再跨两半读源码）。
//! 这里只剩 ③：monitor 生产树里零处定义那 12 个助手 —— 本机搜索只许经通道问本机后端。

/// 收口前在两侧**各写一遍**的那 12 个助手（`K-R85` 逐条实测同名同形）。
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

#[test]
fn the_monitor_grows_no_search_helpers() {
    let defs = |prod: &str| -> Vec<&'static str> {
        HELPERS
            .iter()
            .copied()
            .filter(|h| prod.contains(&format!("fn {h}(")))
            .collect()
    };
    // 正控：识别器在一段合成源码上认得出定义（否则零命中是空真）。
    assert_eq!(
        defs("pub(crate) fn make_snippet(t: &str) {}"),
        ["make_snippet"]
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stray: Vec<String> = Vec::new();
    let mut scanned = 0;
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        scanned += 1;
        let found = defs(&guard_core::production_code(&raw));
        if !found.is_empty() {
            stray.push(format!("{} （定义：{found:?}）", path.display()));
        }
    }
    assert!(scanned > 50, "只扫到 {scanned} 份 —— 遍历坏了");
    assert!(
        stray.is_empty(),
        "monitor 生产树里又有人在搜会话了：{stray:?}\n\
         本机搜索只许经通道问本机后端 —— 在 monitor 里再建一份就是 LOC1b 删掉的那个第二读者。"
    );
}
