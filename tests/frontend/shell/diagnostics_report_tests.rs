//! 「复制诊断信息」壳那一半：只交自己的事实（版本号 · 日志文件 · config 原文的顶层键），不判哪个键认不出。

use super::*;

#[test]
fn 入参只带壳的三样事实_顶层键原样交() {
    let cfg = serde_json::json!({"theme": {"bg": "x"}, "zeta": 1, "diagnostics": {}});
    let r = req_of(
        "4.1.1",
        Some("/h/.cc-monitor/logs/monitor.2026-10-06.log".into()),
        Some(&cfg),
    );
    assert_eq!(r.version, "4.1.1");
    assert_eq!(
        r.log_file.as_deref(),
        Some("/h/.cc-monitor/logs/monitor.2026-10-06.log")
    );
    let mut keys = r.config_keys.clone();
    keys.sort();
    assert_eq!(
        keys,
        vec!["diagnostics", "theme", "zeta"],
        "认得的键也要交：判哪个认不出在本机后端"
    );
    let wire = serde_json::to_value(&r).unwrap();
    let mut fields: Vec<&str> = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort();
    assert_eq!(fields, vec!["configKeys", "logFile", "version"]);
}

#[test]
fn 读不到配置或配置不是对象_没有顶层键可交() {
    assert!(req_of("4.1.1", None, None).config_keys.is_empty());
    assert!(req_of("4.1.1", None, Some(&serde_json::json!([1, 2])))
        .config_keys
        .is_empty());
    assert_eq!(req_of("4.1.1", None, None).log_file, None);
}
