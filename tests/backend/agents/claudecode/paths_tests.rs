//! # 要求住址：用户裁决
//!
//! 核原文：「`settings.json` 那一形照旧被 claude 压过我们（`viaRelay` 那一格不许再说假话）」。
//! 异源：夹具是本判据现造的设置文件（家目录 / 项目 / 项目本地三处）；被测是适配层那一问 `settings_may_set_base_url`，
//! 它的答案经 `SESSION_ENV_KEYS` 交给 `--session-accounts`（`observe/accounts_query.rs` 的 `viaRelay` 那一格）。

use super::*;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-e2-v146-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("root")).unwrap();
    std::fs::create_dir_all(d.join("proj/.claude")).unwrap();
    d
}

#[test]
fn a_settings_file_that_sets_the_base_url_makes_via_relay_unsayable() {
    let d = tmp("settings");
    let (root, proj) = (d.join("root"), d.join("proj"));
    let ask = || settings_may_set_base_url(None, &root, Some(&proj));
    assert!(!ask(), "一份设置都没有却说「可能被压过」");
    std::fs::write(root.join("settings.json"), r#"{"env":{"OTHER":"x"}}"#).unwrap();
    assert!(!ask(), "设置里没设端点却说「可能被压过」");
    std::fs::write(
        root.join("settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":""}}"#,
    )
    .unwrap();
    assert!(!ask(), "空串端点该当没设");
    std::fs::write(
        root.join("settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    )
    .unwrap();
    assert!(ask(), "家目录设置里设了端点却没认出来");
    std::fs::write(root.join("settings.json"), "{}").unwrap();
    std::fs::write(
        proj.join(".claude/settings.local.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    )
    .unwrap();
    assert!(ask(), "项目本地设置里设了端点却没认出来");
    std::fs::write(proj.join(".claude/settings.local.json"), "{ not json").unwrap();
    assert!(ask(), "读不懂的设置被当成「没设」");
    // 配置根给了就看它那一份，不看默认根。
    let other = d.join("acct");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::remove_file(proj.join(".claude/settings.local.json")).unwrap();
    std::fs::write(
        root.join("settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    )
    .unwrap();
    assert!(
        !settings_may_set_base_url(Some(&other), &root, Some(&proj)),
        "账号配置根给了却去看默认根"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 它管不着的那几类路径（读数表，如实登记）：目录 · subagent 记录 · tasks · settings 今天都不认成会话记录。扩射程要用户拍。
#[test]
fn every_uncovered_shape_is_still_uncovered_today() {
    for p in [
        "/home/u/.claude/projects/-x-proj",
        "/home/u/.claude/projects/-x-proj/<sid>/subagents/agent-ab12.jsonl",
        "/home/u/.claude/tasks/<sid>/17.json",
        "/home/u/.claude/settings.json",
    ] {
        assert!(
            !is_session_record_file(p),
            "{p} 今天被挡住了 —— 扩射程要用户拍"
        );
    }
    assert!(
        is_session_record_file("/home/u/.claude/projects/-x-proj/0000.jsonl"),
        "正控：会话记录没被认出来"
    );
}
