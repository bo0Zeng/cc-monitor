//! 第 7 批换形的壳那一半：本机后端 `prefs-read` 说 `ok` 以后，从 `config.json` 删掉搬走的那几个顶层键，删前原文另存 `config.json.pre-7`。
//! 新旧两种 `config.json` 各演一遍（有那几格 · 没有 · 读不懂）；只删那几个键，别的一格不动；原文只存一次（不拿删过的盖掉原文）。
use super::*;
use serde_json::json;
use std::path::PathBuf;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-prefs-mig-shell-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn old_shape() -> serde_json::Value {
    json!({
        "theme": {"bg": "#010203"},
        "resumeCommand": "ccm",
        "localResumeCommand": "cc",
        "resumeCommandPresets": ["cc"],
        "resumeInTmux": true,
        "contextLimits": {"opus": 1000000},
        "accounts": {"byMachine": {"<local>": {"modelByAccount": {"work": "opus"}}}},
        "claudeDir": "/srv/claude",
        "remote": {"hosts": [{"label": "gpu", "host": "g.lan"}]},
    })
}

/// 旧形状：搬进 `prefs.json` 的那六个键删掉、别的原样；原文逐字存进 `config.json.pre-7`；再跑一次什么都不做、原文那份不被盖。
#[test]
fn the_moved_keys_go_and_the_original_is_kept_once() {
    let d = tmpdir("old");
    let path = d.join("config.json");
    let text = serde_json::to_string_pretty(&old_shape()).unwrap();
    std::fs::write(&path, &text).unwrap();
    assert_eq!(finish_at(&path, &MOVED_KEYS).unwrap(), Finished::Removed(6));
    let left: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        left,
        json!({
            "theme": {"bg": "#010203"},
            "resumeCommandPresets": ["cc"],
            "remote": {"hosts": [{"label": "gpu", "host": "g.lan"}]},
        })
    );
    let backup = d.join(BACKUP_NAME);
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), text);
    assert_eq!(
        finish_at(&path, &MOVED_KEYS).unwrap(),
        Finished::NothingLeft
    );
    assert_eq!(
        std::fs::read_to_string(&backup).unwrap(),
        text,
        "原文那份被盖了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 新形状（那几格已经不在）：不写、不留原文副本。
#[test]
fn a_new_config_is_left_alone() {
    let d = tmpdir("new");
    let path = d.join("config.json");
    std::fs::write(&path, r#"{"theme": {}, "resumeCommandPresets": []}"#).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        finish_at(&path, &MOVED_KEYS).unwrap(),
        Finished::NothingLeft
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert!(!d.join(BACKUP_NAME).exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// 读不懂 · 不在：一个字节不写（读不懂的不当空的删）。
#[test]
fn an_unreadable_or_missing_config_is_not_touched() {
    let d = tmpdir("bad");
    let path = d.join("config.json");
    assert_eq!(
        finish_at(&path, &MOVED_KEYS).unwrap(),
        Finished::NothingLeft
    );
    assert!(!path.exists());
    std::fs::write(&path, "{ not json").unwrap();
    assert!(finish_at(&path, &MOVED_KEYS).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    assert!(!d.join(BACKUP_NAME).exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// 只有本机后端说 `ok` 才删（`absent` ⇒ 本机那一份还没写成；`unreadable` ⇒ 那一份读不懂）。
#[test]
fn only_an_ok_local_prefs_lets_the_keys_go() {
    assert!(local_prefs_ready(&json!({"state": "ok", "prefs": {}})));
    for s in ["absent", "unreadable"] {
        assert!(!local_prefs_ready(&json!({"state": s})), "{s}");
    }
    assert!(!local_prefs_ready(&json!({})));
}

/// 已经有一份原文（上一趟存的）：那几个键又出现了（旧版又写回来）也照删，但那份原文不动 —— 它才是换形之前的那一份。
#[test]
fn an_earlier_original_is_never_overwritten() {
    let d = tmpdir("again");
    let path = d.join("config.json");
    std::fs::write(d.join(BACKUP_NAME), "最早那一份").unwrap();
    std::fs::write(&path, serde_json::to_string(&old_shape()).unwrap()).unwrap();
    assert_eq!(finish_at(&path, &MOVED_KEYS).unwrap(), Finished::Removed(6));
    assert_eq!(
        std::fs::read_to_string(d.join(BACKUP_NAME)).unwrap(),
        "最早那一份"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 机器表那一键（`remote`）另一趟删（本机 `machines-read` 说 `ok` 之后）：两趟谁先都行，原文只存头一趟见到的那一份。
#[test]
fn the_machine_table_goes_on_its_own_turn_and_the_original_stays_the_first_one() {
    let d = tmpdir("machines");
    let path = d.join("config.json");
    let text = serde_json::to_string_pretty(&old_shape()).unwrap();
    std::fs::write(&path, &text).unwrap();
    assert_eq!(
        finish_at(&path, &MOVED_MACHINES).unwrap(),
        Finished::Removed(1)
    );
    let left: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(left.get("remote").is_none() && left.get("resumeCommand").is_some());
    assert_eq!(finish_at(&path, &MOVED_KEYS).unwrap(), Finished::Removed(6));
    assert_eq!(std::fs::read_to_string(d.join(BACKUP_NAME)).unwrap(), text);
    assert_eq!(
        finish_at(&path, &MOVED_MACHINES).unwrap(),
        Finished::NothingLeft
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 「显示分身会话」那个旧开关：本机偏好写成了那一刻一并删掉（不再有人读它，留着会被当成认不出的键）。
#[test]
fn the_old_bg_switch_is_dropped() {
    let d = tmpdir("dropbg");
    let path = d.join("config.json");
    std::fs::write(&path, r#"{"showBgSessions": false, "notifyNeeds": true}"#).unwrap();
    assert_eq!(
        finish_at(&path, &DROPPED_KEYS).unwrap(),
        Finished::Removed(1)
    );
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(v.get("showBgSessions").is_none(), "{v}");
    assert_eq!(v["notifyNeeds"], true);
    assert_eq!(
        finish_at(&path, &DROPPED_KEYS).unwrap(),
        Finished::NothingLeft
    );
    let _ = std::fs::remove_dir_all(&d);
}
