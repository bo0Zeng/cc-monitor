use super::scrub_env_vars;

/// issue #24：清掉存在的、跳过不存在的、不碰未列出的。
/// 用本测试专属的假变量名——cargo test 多线程跑，进程级 env 是共享的，
/// 绝不能在测试里 set/remove 真实的 CLAUDE_* 变量（会干扰并发测试与宿主环境）。
#[test]
fn removes_present_keeps_unlisted_skips_absent() {
    // 正常启动路径：变量全不存在 → 严格 no-op（"零回归"声明的直接对应物）
    assert!(scrub_env_vars(&["CCM_TEST_SCRUB_NOOP"]).is_empty());

    std::env::set_var("CCM_TEST_SCRUB_A", "1");
    std::env::set_var("CCM_TEST_SCRUB_KEEP", "keep");
    let removed = scrub_env_vars(&["CCM_TEST_SCRUB_A", "CCM_TEST_SCRUB_ABSENT"]);
    assert_eq!(removed, vec!["CCM_TEST_SCRUB_A".to_string()]);
    assert!(std::env::var_os("CCM_TEST_SCRUB_A").is_none());
    assert_eq!(
        std::env::var("CCM_TEST_SCRUB_KEEP").as_deref(),
        Ok("keep"),
        "未列出的变量必须原样保留（对应真实场景的 CLAUDE_CONFIG_DIR）"
    );
    std::env::remove_var("CCM_TEST_SCRUB_KEEP");
}
