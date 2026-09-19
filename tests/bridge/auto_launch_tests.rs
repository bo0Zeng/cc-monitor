use super::*;
use std::fs;

#[test]
fn default_when_missing() {
    let tmp = std::env::temp_dir().join(format!("ccm-auto-launch-{}.json", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let cfg = load(&tmp);
    assert!(!cfg.auto_launch_enabled);
    assert!(cfg.monitor_exe_path.is_none());
}

#[test]
fn roundtrip_serialize() {
    let cfg = AutoLaunchConfig {
        auto_launch_enabled: true,
        monitor_exe_path: Some(r"C:\foo\monitor.exe".to_string()),
    };
    let s = serde_json::to_string(&cfg).unwrap();
    let parsed: AutoLaunchConfig = serde_json::from_str(&s).unwrap();
    assert!(parsed.auto_launch_enabled);
    assert_eq!(parsed.monitor_exe_path.unwrap(), r"C:\foo\monitor.exe");
}

#[test]
fn save_and_load_roundtrip() {
    let tmp =
        std::env::temp_dir().join(format!("ccm-auto-launch-rt-{}.json", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let cfg = AutoLaunchConfig {
        auto_launch_enabled: true,
        monitor_exe_path: Some(r"C:\bar\monitor.exe".to_string()),
    };
    save(&tmp, &cfg).unwrap();
    let loaded = load(&tmp);
    assert!(loaded.auto_launch_enabled);
    assert_eq!(loaded.monitor_exe_path.unwrap(), r"C:\bar\monitor.exe");
    let _ = fs::remove_file(&tmp);
}
