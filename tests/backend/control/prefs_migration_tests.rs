//! 常驻那一位起来时那一次：`prefs.json` 第 1 版 ⇒ 第 2 版；`config.json` 里的旧偏好不再搬。家目录都在临时目录里。
use super::*;
use crate::platform::Own;
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-prefs-mig-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".cc-monitor")).unwrap();
    d
}

/// 旧 `config.json` 的形状：通用页一格 · 本机一格 · 机器表每台一格 · tmux · 上限表 · 每号模型（本机 · 一台远端）。
fn old_shape() -> serde_json::Value {
    json!({
        "theme": {"bg": "#010203"},
        "resumeCommand": "ccm",
        "localResumeCommand": " cc ",
        "resumeCommandPresets": ["cc", "cct"],
        "resumeInTmux": true,
        "contextLimits": {"opus": 1000000, "bad": -1, "": 5, "str": "9"},
        "accounts": {"byMachine": {
            "<local>": {"modelByAccount": {"work": "opus", "bad": "rm -rf /"}},
            "gpu": {"modelByAccount": {"home": "sonnet"}},
        }},
        "remote": {"hosts": [
            {"label": "gpu", "host": "g.lan", "resumeCommand": "cct"},
            {"label": "", "host": "pi.lan", "resumeCommand": ""},
            {"label": "evil", "host": "e.lan", "resumeCommand": "cc; rm -rf /"},
        ]},
    })
}

fn write_prefs_raw(home: &Path, text: &str) -> PathBuf {
    let at = home.join(relay_route_core::PREFS_REL);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(&at, text).unwrap();
    at
}

/// 盘上第 1 版的 `prefs.json`（旧开关）⇒ 常驻那一位起来时换成第 2 版：开关读成三选、别的几格原样、旧键不留；第 2 版不再动；读不懂的不动。
#[test]
fn a_first_version_prefs_file_is_reshaped_once() {
    use cmd_wire::session::ResumeWhere;
    let home = scratch("prefs-v1");
    let own = Own::issue_at(&home);
    let at = write_prefs_raw(
        &home,
        r#"{"version":1,"launchers":{"claude":"cc"},"resumeInTmux":true,"models":{"work":"opus"},"contextLimits":{},"childTab":"show"}"#,
    );
    assert_eq!(reshape_prefs_in(&own), Reshaped::Done);
    let crate::common::machine_prefs::Loaded::Present(p) =
        crate::common::machine_prefs::read_at(&at)
    else {
        panic!("换完读不懂：{}", std::fs::read_to_string(&at).unwrap())
    };
    assert_eq!(p.resume_where, ResumeWhere::Tmux);
    assert_eq!(p.launchers.get("claude").map(String::as_str), Some("cc"));
    assert_eq!(p.models.get("work").map(String::as_str), Some("opus"));
    assert_eq!(p.child_tab, cmd_wire::session::Tab::Show);
    let raw = std::fs::read_to_string(&at).unwrap();
    let keys: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(keys.get("resumeInTmux").is_none(), "旧键留下了：{raw}");
    assert_eq!(keys["version"], 2);
    assert_eq!(reshape_prefs_in(&own), Reshaped::Nothing);
    assert_eq!(
        std::fs::read_to_string(&at).unwrap(),
        raw,
        "第 2 版又被写了"
    );
    // 关着 ⇒ 托管。
    write_prefs_raw(&home, r#"{"version":1,"resumeInTmux":false}"#);
    assert_eq!(reshape_prefs_in(&own), Reshaped::Done);
    assert_eq!(
        crate::common::machine_prefs::read_at(&at)
            .prefs()
            .resume_where,
        ResumeWhere::Managed
    );
    // 读不懂（第 1 版里多一格 · 不是 JSON）⇒ 一个字节不动。
    for bad in [r#"{"version":1,"resumeInTmux":true,"more":1}"#, "{not json"] {
        write_prefs_raw(&home, bad);
        assert!(
            matches!(reshape_prefs_in(&own), Reshaped::Unreadable(_)),
            "{bad}"
        );
        assert_eq!(std::fs::read_to_string(&at).unwrap(), bad);
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 起来那一下：`config.json` 还带第 7 批那几格（连 `remote` · `claudeDir`）、`prefs.json` · `machines.json` 都不在 ⇒ 两份都不写，`config.json` 一个字节不动。
#[test]
fn an_old_config_is_not_moved_at_startup() {
    let home = scratch("startup-old");
    let own = Own::issue_at(&home);
    let mut cfg = old_shape();
    cfg["claudeDir"] = json!("/srv/claude");
    let text = serde_json::to_string_pretty(&cfg).unwrap();
    std::fs::write(home.join(".cc-monitor/config.json"), &text).unwrap();
    run_in(&own);
    assert!(
        !home.join(relay_route_core::PREFS_REL).exists(),
        "又从 config.json 搬了 prefs.json"
    );
    assert!(
        !home.join(relay_route_core::MACHINES_REL).exists(),
        "又从 config.json 搬了机器表"
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".cc-monitor/config.json")).unwrap(),
        text,
        "config.json 被动了"
    );
    let _ = std::fs::remove_dir_all(&home);
}
