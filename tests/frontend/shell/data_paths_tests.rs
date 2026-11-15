//! 数据位置那几行由本机后端 `data-report` 答（`src/backend/footprint/data.rs::monitor_rows`，判据在 `tests/backend/footprint/data_tests.rs`）：
//! 壳这一侧不再列、不再 stat，只留取家目录那一处。

/// ★ 壳里没有第二份数据位置：命令 · 类型 · 枚举在壳生产源码里零命中（注释与测试段不算）。带正控。
#[test]
fn the_shell_lists_no_data_paths_of_its_own() {
    let gone = [
        "get_data_paths",
        "DataPathInfo",
        "DataPathsResponse",
        "monitor_entries",
        "LOGS_DIR_LABEL",
    ];
    let hits_in = |src: &str| -> Vec<&str> {
        let prod = guard_core::production_code(src);
        gone.iter()
            .copied()
            .filter(|g| guard_core::contains_word(&prod, g))
            .collect()
    };
    assert_eq!(
        hits_in("// get_data_paths 注释不算\npub async fn get_data_paths() {}\n"),
        vec!["get_data_paths"],
        "正控没逮到 —— 尺子瞎了"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(
        files.len() > 50,
        "壳源码只扫到 {} 份 —— 遍历坏了",
        files.len()
    );
    let hits: Vec<String> = files
        .iter()
        .flat_map(|(p, src)| {
            hits_in(src)
                .into_iter()
                .map(move |g| format!("{}：{g}", p.display()))
        })
        .collect();
    assert_eq!(hits, Vec::<String>::new());
}
