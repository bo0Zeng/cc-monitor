//! **搜索那一族助手的口径只许有一份**〔`K-R85`〕。
//!
//! # ✅ 它服务哪条要求：**`设计/01 §5 D1`**〔`P20` 第二刀 2026-09-22〕
//!
//! `D1` =「**一个判定只有一个家**」。本族的人群是收口前在两侧**各写一遍**的那 12 个助手，
//! 而下面那条注释已经写清人群的取法（「按『口径可能住在哪』取，不是按『当初动过哪几行』取」）
//! —— 那正是 `D1` 在判据层的形状。核对过程住 `设计/99 §4.11.5`。

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
        "预算顺序必须住 core：收口前 monitor 按 `updated_at desc`、backend 按 readdir，\
             `--limit 50` 下两侧给出的 3 个会话**只重合 1 个**"
    );

    // ── ② 搜索的那一侧：不许自己再有一份，且必须真的调 core。
    //   〔LOC1b · 第四波 4D〕从前是两侧（monitor `search.rs` 的内存索引 ＋ 后端 `--search`）；本机搜索改问本机后端之后
    //   monitor 那一侧删了 ⇒ 只剩后端这一侧，另一半的「一个家」由下面 ③ 钉：monitor 生产树里零处再搜。
    for (name, raw) in [(
        "backend observe/search_query.rs",
        include_str!("../../src/backend/observe/search_query.rs"),
    )] {
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

    // ── ③〔LOC1b · 第四波 4D〕monitor 那一侧**不再搜**：生产树里零处调 `search_core::`、零处定义那 12 个助手。
    //   要求住址：`设计/00 §2.5 ①` 逐字「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」·
    //   `90 §4 F`「搜索收口到 search-core ＋ 后端」。本机搜索今天经通道问本机后端（`src/views/history-search.ts`）。
    //   正控：同一个识别器在后端那一份上认得出调用（否则零命中是空真）。
    //   〔RENDER2 · J10 乙（主会话 09-27 裁）〕唯一的例外：`messages.rs` 解析 user 记录时调注入噪声那一条规则给前端出成品 ——
    //   只许那一个文件、只许这两个名字（两向：用到的名字集合 == 放行集合）。
    const RECORD_RULE: &[&str] = &[
        "search_core::user_text(",
        "search_core::extract_text_blocks(",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stray: Vec<String> = Vec::new();
    let mut record_rule_used: Vec<&str> = Vec::new();
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        let mut prod = guard_core::production_code(&raw);
        if path.file_name() == Some(std::ffi::OsStr::new("messages.rs")) {
            for name in RECORD_RULE {
                if prod.contains(name) {
                    record_rule_used.push(name);
                }
                prod = prod.replace(name, "");
            }
        }
        let calls = prod.contains("search_core::");
        let defs: Vec<&str> = HELPERS
            .iter()
            .copied()
            .filter(|h| prod.contains(&format!("fn {h}(")))
            .collect();
        if calls || !defs.is_empty() {
            stray.push(format!(
                "{} （调 core：{calls} · 定义：{defs:?}）",
                path.display()
            ));
        }
    }
    assert!(
        stray.is_empty(),
        "monitor 生产树里又有人在搜会话了：{stray:?}\n\
         本机搜索只许经通道问本机后端 —— 在 monitor 里再建一份就是 LOC1b 删掉的那个第二读者。"
    );
    assert_eq!(
        record_rule_used,
        RECORD_RULE.to_vec(),
        "`messages.rs` 不再经 search-core 给 user 记录出成品（或放行表多了一项没人用）"
    );
    let backend =
        guard_core::production_code(include_str!("../../src/backend/observe/search_query.rs"));
    assert!(
        backend.contains("search_core::"),
        "识别器正控：后端那一份认不出 `search_core::` —— 本条空转"
    );
}
