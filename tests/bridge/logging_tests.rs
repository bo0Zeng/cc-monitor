use super::*;

#[test]
fn diagnostics_default_is_user_friendly() {
    let d = DiagnosticsConfig::default();
    assert!(d.log_enabled, "log 默认开（issue #4 要求）");
    assert!(
        d.error_toast,
        "error toast 默认开（用户看不见 ERROR 是 v1.7 事故根因）"
    );
    assert_eq!(d.log_level, "info");
    assert_eq!(d.max_files, 3);
}

#[test]
fn diagnostics_legacy_config_missing_field_uses_defaults() {
    // 老用户的 config.json 没有 diagnostics 字段；新版本读应回退到 default
    let raw = r#"{"claudeDir":"C:\\Users\\foo\\.claude","theme":{}}"#;
    let v: serde_json::Value = serde_json::from_str(raw).unwrap();
    let d = v
        .get("diagnostics")
        .cloned()
        .and_then(|d| serde_json::from_value::<DiagnosticsConfig>(d).ok())
        .unwrap_or_default();
    assert_eq!(d, DiagnosticsConfig::default());
}

#[test]
fn diagnostics_partial_field_serde_default_other() {
    // 只有 log_level 一个字段的 config 也能 deserialize（其他字段拿 default）
    let raw = r#"{"log_level":"debug"}"#;
    let d: DiagnosticsConfig = serde_json::from_str(raw).unwrap();
    assert_eq!(d.log_level, "debug");
    assert!(d.log_enabled);
    assert!(d.error_toast);
    assert_eq!(d.max_files, 3);
}

#[test]
fn build_env_filter_accepts_valid_levels() {
    for lv in ["trace", "debug", "info", "warn", "error", "off"] {
        assert!(build_env_filter(lv).is_some(), "level {lv} should parse");
    }
}

#[test]
fn build_env_filter_rejects_garbage() {
    assert!(build_env_filter("nonsense=42=42=").is_none());
}

#[test]
fn write_then_read_diagnostics_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let cfg = DiagnosticsConfig {
        log_enabled: true,
        log_level: "warn".to_string(),
        error_toast: false,
        max_files: 7,
    };
    write_diagnostics_to_config(&tmp, &cfg).unwrap();

    let back = read_diagnostics_from_config(&tmp);
    assert_eq!(back, cfg);

    // 不破坏 config.json 已有字段（手写 theme/claudeDir 后再写 diagnostics）
    let cfg_path = tmp.join("config.json");
    let raw = std::fs::read_to_string(&cfg_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(v.get("diagnostics").is_some());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn write_diagnostics_preserves_other_fields() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // 先写 config.json 含 theme + claudeDir（模拟老用户）。
    // 用 r##"..."## 加一层 `#` 是因为 JSON 内含 "#000000"，普通 r#"..."# 会被
    // "# 提前终止（raw string 的结束分隔符是 `"` 后跟匹配数量的 `#`）
    let original = r##"{"claudeDir":"C:\\foo","theme":{"bg":"#000000"}}"##;
    std::fs::write(tmp.join("config.json"), original).unwrap();

    // 再写 diagnostics
    write_diagnostics_to_config(&tmp, &DiagnosticsConfig::default()).unwrap();

    // 老字段必须还在
    let raw = std::fs::read_to_string(tmp.join("config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v.get("claudeDir").and_then(|x| x.as_str()), Some("C:\\foo"));
    assert!(v.get("theme").is_some());
    assert!(v.get("diagnostics").is_some());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn find_latest_log_picks_newest_mtime() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(tmp.join("monitor.2026-01-01.log"), "old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(tmp.join("monitor.2026-05-25.log"), "new").unwrap();
    // 非 log 文件不应被选中
    std::fs::write(tmp.join("notes.txt"), "noise").unwrap();

    let latest = find_latest_log_file(&tmp).unwrap();
    assert!(latest.to_string_lossy().contains("monitor.2026-05-25"));

    let _ = std::fs::remove_dir_all(&tmp);
}
