//! 第 7 批的一次换形（`config.json` 那几格 ⇒ 各台 `prefs.json`）：新旧两种 `config.json` 各演一遍（有那几格 · 没有 · 读不懂 · 不在），
//! 远端那几份记成待交、连上那台再交（那台还没有才交，交成 / 那台已有都删掉那一条，问不到留着）。家目录都在临时目录里。
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

fn kind() -> &'static str {
    crate::agents::default_kind()
}

/// 旧形状：通用页一格 · 本机一格 · 机器表每台一格 · tmux · 上限表 · 每号模型（本机 · 一台远端），外加几处坏值。
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

fn prefs(v: serde_json::Value) -> Prefs {
    serde_json::from_value(v).unwrap()
}

/// 旧形状 ⇒ 本机那一份（远端每台那几格不搬：不为旧配置留兼容）；本机那一格优先、空 ⇒ 通用页那一格；坏值那一条丢掉（不拖垮整份）。
#[test]
fn an_old_config_moves_only_the_local_prefs() {
    let p = plan(&old_shape());
    let k = kind();
    assert_eq!(
        p.local,
        prefs(
            json!({"launchers": {k: "cc"}, "resumeWhere": "tmux", "models": {"work": "opus"}, "contextLimits": {"opus": 1000000}})
        )
    );
}

/// 旧开关读成三选：开 ⇒ tmux；关 / 没有 ⇒ 托管（今天关着的行为）。
#[test]
fn the_old_resume_switch_reads_as_one_of_the_three() {
    use cmd_wire::session::ResumeWhere;
    for (cfg, want) in [
        (json!({"resumeInTmux": true}), ResumeWhere::Tmux),
        (json!({"resumeInTmux": false}), ResumeWhere::Managed),
        (json!({}), ResumeWhere::Managed),
    ] {
        assert_eq!(plan(&cfg).local.resume_where, want, "{cfg}");
    }
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
    assert!(!raw.contains("resumeInTmux"), "旧键留下了：{raw}");
    assert_eq!(reshape_prefs_in(&own), Reshaped::Nothing);
    assert_eq!(std::fs::read_to_string(&at).unwrap(), raw, "第 2 版又被写了");
    // 关着 ⇒ 托管。
    write_prefs_raw(&home, r#"{"version":1,"resumeInTmux":false}"#);
    assert_eq!(reshape_prefs_in(&own), Reshaped::Done);
    assert_eq!(
        crate::common::machine_prefs::read_at(&at).prefs().resume_where,
        ResumeWhere::Managed
    );
    // 读不懂（第 1 版里多一格 · 不是 JSON）⇒ 一个字节不动。
    for bad in [r#"{"version":1,"resumeInTmux":true,"more":1}"#, "{not json"] {
        write_prefs_raw(&home, bad);
        assert!(matches!(reshape_prefs_in(&own), Reshaped::Unreadable(_)), "{bad}");
        assert_eq!(std::fs::read_to_string(&at).unwrap(), bad);
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 新形状（那几格已经不在）⇒ 本机缺省那一份。
#[test]
fn a_new_config_moves_nothing() {
    let p = plan(
        &json!({"theme": {}, "resumeCommandPresets": ["cc"], "remote": {"hosts": [{"label": "gpu", "host": "g"}]}}),
    );
    assert_eq!(p.local, Prefs::default());
}

fn write_config(home: &Path, text: &str) {
    std::fs::write(home.join(".cc-monitor").join(CONFIG_NAME), text).unwrap();
}

/// 旧形状：写出本机 `prefs.json`（远端那几格不记），`config.json` 一个字节不动；再起一次 ⇒ 已经有了、不再写。
#[test]
fn the_resident_migrates_an_old_config_once_and_leaves_config_json_alone() {
    let home = scratch("old");
    let own = Own::issue_at(&home);
    let text = serde_json::to_string_pretty(&old_shape()).unwrap();
    write_config(&home, &text);
    assert_eq!(migrate_in(&own), Migrated::Done);
    let local = crate::common::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL));
    assert_eq!(
        local,
        crate::common::machine_prefs::Loaded::Present(plan(&old_shape()).local)
    );
    assert!(
        !home.join(relay_route_core::PREFS_HANDOFF_REL).exists(),
        "远端那几格又记进了待交"
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".cc-monitor").join(CONFIG_NAME)).unwrap(),
        text,
        "config.json 被动了"
    );
    // 再起一次：prefs.json 已在 ⇒ 不读 config.json、不写。
    write_config(&home, "{\"resumeCommand\": \"other\"}");
    assert_eq!(migrate_in(&own), Migrated::AlreadyThere);
    assert_eq!(
        local,
        crate::common::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL))
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 新形状：照样写出本机那一份（缺省），没有待交那一份。
#[test]
fn a_new_config_still_marks_the_move_done() {
    let home = scratch("new");
    let own = Own::issue_at(&home);
    write_config(&home, "{\"theme\": {}}");
    assert_eq!(migrate_in(&own), Migrated::Done);
    assert_eq!(
        crate::common::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL)),
        crate::common::machine_prefs::Loaded::Present(Prefs::default())
    );
    assert!(!home.join(relay_route_core::PREFS_HANDOFF_REL).exists());
    let _ = std::fs::remove_dir_all(&home);
}

/// 读不懂 · 不在 ⇒ 一个字节不写（读不懂的不当空的搬）。
#[test]
fn an_unreadable_or_missing_config_writes_nothing() {
    let home = scratch("bad");
    let own = Own::issue_at(&home);
    assert_eq!(migrate_in(&own), Migrated::NoConfig);
    write_config(&home, "{ not json");
    assert!(matches!(migrate_in(&own), Migrated::Unreadable(_)));
    assert!(!home.join(relay_route_core::PREFS_REL).exists());
    assert!(!home.join(relay_route_core::PREFS_HANDOFF_REL).exists());
    let _ = std::fs::remove_dir_all(&home);
}

/// 机器表：旧形状每台带过去（宽读、`resumeCommand` 那几格不带），`config.json` 一个字节不动；已有 ⇒ 不再搬。
#[test]
fn the_machine_table_moves_once_without_the_old_resume_cells() {
    let home = scratch("machines-old");
    let own = Own::issue_at(&home);
    let mut cfg = old_shape();
    cfg["remote"]["hosts"][0]["addresses"] = json!("a.lan\n\n b.lan ");
    cfg["remote"]["hosts"][0]["connect"] = json!(false);
    cfg["remote"]["hosts"][1]["port"] = json!("2222");
    cfg["remote"]["hosts"][1]["daemonless"] = json!(true);
    let text = serde_json::to_string_pretty(&cfg).unwrap();
    write_config(&home, &text);
    assert_eq!(migrate_machines_in(&own), MachinesMoved::Done(3));
    let at = home.join(relay_route_core::MACHINES_REL);
    let raw = std::fs::read_to_string(&at).unwrap();
    let disk: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for h in disk["hosts"].as_array().unwrap() {
        let keys = h.as_object().unwrap();
        assert!(
            !keys.contains_key("resumeCommand") && !keys.contains_key("daemonless"),
            "旧格带过去了：{h}"
        );
    }
    let crate::control::machines::Loaded::Present(hosts) = crate::control::machines::read_at(&at)
    else {
        panic!("没写成")
    };
    assert_eq!(
        hosts.iter().map(|h| h.name()).collect::<Vec<_>>(),
        ["gpu", "pi.lan", "evil"]
    );
    assert_eq!(hosts[0].addresses, ["a.lan", "b.lan"]);
    assert!(!hosts[0].connect && hosts[1].connect);
    assert_eq!(hosts[1].port, 22, "类型不对的格当缺");
    assert_eq!(
        std::fs::read_to_string(home.join(".cc-monitor").join(CONFIG_NAME)).unwrap(),
        text
    );
    write_config(&home, "{\"remote\": {\"hosts\": []}}");
    assert_eq!(migrate_machines_in(&own), MachinesMoved::AlreadyThere);
    assert_eq!(std::fs::read_to_string(&at).unwrap(), raw);
    let _ = std::fs::remove_dir_all(&home);
}

/// 新形状（没有 `remote`）⇒ 写出空表；`remote` 认不出 · 读不懂 · 不在 ⇒ 一个字节不写。
#[test]
fn a_new_or_unrecognized_machine_table() {
    let home = scratch("machines-new");
    let own = Own::issue_at(&home);
    let at = home.join(relay_route_core::MACHINES_REL);
    assert_eq!(migrate_machines_in(&own), MachinesMoved::NoConfig);
    write_config(&home, "{ not json");
    assert!(matches!(
        migrate_machines_in(&own),
        MachinesMoved::Unreadable(_)
    ));
    write_config(&home, "{\"remote\": {\"host\": \"old-single.lan\"}}");
    assert!(matches!(
        migrate_machines_in(&own),
        MachinesMoved::Unreadable(_)
    ));
    assert!(!at.exists());
    write_config(&home, "{\"theme\": {}}");
    assert_eq!(migrate_machines_in(&own), MachinesMoved::Done(0));
    assert_eq!(
        crate::control::machines::read_at(&at),
        crate::control::machines::Loaded::Present(Vec::new())
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// `claudeDir` 并进本机那一份（只认绝对路径）；远端那几份不带。
#[test]
fn the_agent_home_moves_into_the_local_prefs_only() {
    let mut cfg = old_shape();
    cfg["claudeDir"] = json!("/srv/claude");
    let p = plan(&cfg);
    assert_eq!(
        p.local
            .agent_homes
            .get(crate::agents::default_kind())
            .map(String::as_str),
        Some("/srv/claude")
    );
    cfg["claudeDir"] = json!("relative/dir");
    assert_eq!(plan(&cfg).local.agent_homes.len(), 0);
}
