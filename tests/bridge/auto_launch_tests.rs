//! # 要求住址：`INVARIANTS §2.1`（`auto-launch.json` 两个字段的类）＋ `设计/01 §5 D3`（诚实的默认 ＝ 不作为）
//!
//! 核原文：`INVARIANTS §2.1` 那一行逐字「`enabled`=真相；`monitor_exe_path`=派生」—— `auto_launch.rs::save` /
//! `auto_launch.rs::load` 往返不丢这两格；`D3` 逐字「诚实的默认 ＝ 恒等 / 不作为 / 沿用调用者已有状态」—— 文件不在 ⇒ 不自启。
//! ⚠ 住址偏弱：这项功能本身（cc 函数在 monitor 没跑时拉起它）在设计篇没有行为节。
//! `roundtrip_serialize` 只测 serde 往返，被 `save_and_load_roundtrip` 整个盖住 ⇒ 列进 `JA1.md` 候选退役。〔JA1 点址 2026-09-24〕

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
    let tmp = std::env::temp_dir().join(format!("ccm-auto-launch-rt-{}.json", std::process::id()));
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
