//! 住址反空真：每个住址各点名几样盘上真有的东西，指错地方当场红（不数条目数：一个装了别的东西的目录数也能过）。
use super::*;

#[test]
fn every_address_points_at_something_we_can_name() {
    for (name, dir, probes) in [
        (
            "repo_root",
            repo_root(),
            &["package.json", "tsconfig.json", "vite.config.ts"][..],
        ),
        (
            "repo_src_root",
            repo_src_root(),
            &["README.md", "backend", "frontend"][..],
        ),
        ("tests_root", tests_root(), &["backend", "e2e"][..]),
        (
            "backend_src_root",
            backend_src_root(),
            &[
                "lib.rs",
                "main.rs",
                "Cargo.toml",
                "stream/inbound/mod.rs",
                "control/mod.rs",
                "observe/mod.rs",
                "platform/mod.rs",
            ][..],
        ),
        (
            "backend_tests_root",
            backend_tests_root(),
            &["readonly_guard.rs", "no_timer_guard.rs"][..],
        ),
        ("relay_root", relay_root(), &["lib.rs", "Cargo.toml"][..]),
        (
            "comms_tests_root",
            comms_tests_root(),
            &["server_tests.rs"][..],
        ),
        (
            "relay_host_root",
            relay_host_root(),
            &["mod.rs", "key.rs"][..],
        ),
        ("shell_root", shell_root(), &["Cargo.toml", "build.rs"][..]),
        (
            "shell_src_root",
            shell_src_root(),
            &["lib.rs", "main.rs"][..],
        ),
        ("filewin_root", filewin_root(), &["Cargo.toml"][..]),
        ("filewin_src_root", filewin_src_root(), &["lib.rs"][..]),
    ] {
        assert!(dir.is_dir(), "{name}() = {dir:?}，不是目录");
        for probe in probes {
            assert!(
                dir.join(probe).exists(),
                "{name}() = {dir:?} 下没有 {probe} —— 这个住址指错了地方"
            );
        }
    }
    assert_ne!(
        shell_src_root(),
        repo_src_root(),
        "两个 `src` 撞了 —— 某一级爬错"
    );
}

/// 正控：入口 `lib.rs` 在 git 跟踪着的人群里；扩展名过滤真的在过滤。
#[test]
fn tracked_under_lists_the_tracked_entry_and_filters_by_extension() {
    let rs = tracked_under("src/frontend/shell/src", "rs");
    assert!(
        rs.iter().any(|r| r == "src/frontend/shell/src/lib.rs"),
        "git 跟踪着的人群里没有 `lib.rs`：{rs:?}"
    );
    assert!(rs.iter().all(|r| r.ends_with(".rs")), "扩展名过滤失效");
}

/// 测试树里的文件整份是测试段；别处照剥。
#[test]
fn the_production_side_of_a_test_tree_file_is_empty() {
    let attr = format!("#[{}]", "test");
    let src = format!("fn a() {{}}\n{attr}\nfn t() {{}}\n");
    assert_eq!(
        production_side_of(&backend_tests_root().join("readonly_guard.rs"), &src),
        ""
    );
    assert_eq!(
        production_side_of(&comms_tests_root().join("server_tests.rs"), &src),
        ""
    );
    assert!(
        production_side_of(&backend_src_root().join("lib.rs"), "fn a() {}\n").contains("fn a()")
    );
}
