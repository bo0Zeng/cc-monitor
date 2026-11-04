//! # 要求
//!
//! 「`settings.json` 那一形照旧被 claude 压过我们（`viaRelay` 那一格不许再说假话）」。
//! 异源：夹具是本判据现造的设置文件（家目录 / 项目 / 项目本地三处）；被测是适配层那一问 `settings_may_set_base_url`，
//! 它的答案经 `SESSION_ENV_KEYS` 交给会话账号归属那几行（`observe/accounts_query.rs::session_accounts` 的 `via_relay` 那一格）。

use super::*;

fn tmp(tag: &str) -> PathBuf {
    let d = test_home::scratch(tag);
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

/// 直接敲的 claude 也走中转（设置文件的 `env` 块）：写地址的那一下，随地址一起设的那几条（`DefaultUpstream.relay_env`）也写上 ——
/// 不然直接敲的 claude 走中转时不带分类头，旁路请求（预测下一句那一类）分不出来。别的格原样。
#[test]
fn writing_the_relay_address_also_writes_the_env_that_rides_with_it() {
    let merged = settings_env_merge(
        r#"{"env":{"OTHER":"x"},"theme":"dark"}"#,
        "http://127.0.0.1:1/t/a",
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
    assert_eq!(v["env"][BASE_URL_ENV], "http://127.0.0.1:1/t/a");
    assert_eq!(v["env"]["OTHER"], "x");
    assert_eq!(v["theme"], "dark");
    for (k, val) in super::super::UPSTREAM.relay_env {
        assert_eq!(v["env"][*k], *val, "{k}");
    }
    assert!(!super::super::UPSTREAM.relay_env.is_empty());
}

/// 项目里哪一份设置文件自己写了上游地址（会盖住用户级那一格）：两份都看、本地那一份也算；空串 · 读不懂 · 没写 ⇒ 不算（不猜）。
#[test]
fn a_project_settings_file_that_sets_the_base_url_covers_the_user_one() {
    let d = tmp("covered");
    let proj = d.join("proj");
    let covered = SETTINGS_ENV.covered_in;
    assert_eq!(covered(&proj), None, "一份都没写");
    std::fs::write(
        proj.join(".claude/settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":""}}"#,
    )
    .unwrap();
    assert_eq!(covered(&proj), None, "空串当没写");
    std::fs::write(proj.join(".claude/settings.local.json"), "{ not json").unwrap();
    assert_eq!(covered(&proj), None, "读不懂的不当盖住");
    std::fs::write(
        proj.join(".claude/settings.local.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    )
    .unwrap();
    assert_eq!(
        covered(&proj),
        Some(proj.join(".claude/settings.local.json"))
    );
    let _ = std::fs::remove_dir_all(&d);
}
