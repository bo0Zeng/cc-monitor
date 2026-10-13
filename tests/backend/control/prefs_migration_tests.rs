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

/// 旧形状 ⇒ 本机一份、远端每台一份；本机那一格优先、空 ⇒ 通用页那一格；坏值那一条丢掉（不拖垮整份）。
#[test]
fn an_old_config_splits_into_the_local_prefs_and_one_per_remote() {
    let p = plan(&old_shape());
    let k = kind();
    assert_eq!(
        p.local,
        prefs(
            json!({"launchers": {k: "cc"}, "resumeInTmux": true, "models": {"work": "opus"}, "contextLimits": {"opus": 1000000}})
        )
    );
    assert_eq!(
        p.remote,
        BTreeMap::from([
            (
                "gpu".to_string(),
                prefs(
                    json!({"launchers": {k: "cct"}, "resumeInTmux": true, "models": {"home": "sonnet"}, "contextLimits": {"opus": 1000000}})
                )
            ),
            (
                "pi.lan".to_string(),
                prefs(
                    json!({"launchers": {k: "ccm"}, "resumeInTmux": true, "contextLimits": {"opus": 1000000}})
                )
            ),
            (
                "evil".to_string(),
                prefs(json!({"resumeInTmux": true, "contextLimits": {"opus": 1000000}}))
            ),
        ])
    );
}

/// 新形状（那几格已经不在）⇒ 本机缺省那一份、远端一份都不记。
#[test]
fn a_new_config_moves_nothing() {
    let p = plan(
        &json!({"theme": {}, "resumeCommandPresets": ["cc"], "remote": {"hosts": [{"label": "gpu", "host": "g"}]}}),
    );
    assert_eq!(p.local, Prefs::default());
    assert!(p.remote.is_empty(), "{:?}", p.remote);
}

fn write_config(home: &Path, text: &str) {
    std::fs::write(home.join(".cc-monitor").join(CONFIG_NAME), text).unwrap();
}

/// 旧形状：写出本机 `prefs.json` ＋ 待交那一份，`config.json` 一个字节不动；再起一次 ⇒ 已经有了、不再写。
#[test]
fn the_resident_migrates_an_old_config_once_and_leaves_config_json_alone() {
    let home = scratch("old");
    let own = Own::issue_at(&home);
    let text = serde_json::to_string_pretty(&old_shape()).unwrap();
    write_config(&home, &text);
    assert_eq!(migrate_in(&own), Migrated::Done { pending: 3 });
    let local = crate::control::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL));
    assert_eq!(
        local,
        crate::control::machine_prefs::Loaded::Present(plan(&old_shape()).local)
    );
    assert_eq!(read_pending(&own).len(), 3);
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
        crate::control::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL))
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 新形状：照样写出本机那一份（缺省），没有待交那一份。
#[test]
fn a_new_config_still_marks_the_move_done() {
    let home = scratch("new");
    let own = Own::issue_at(&home);
    write_config(&home, "{\"theme\": {}}");
    assert_eq!(migrate_in(&own), Migrated::Done { pending: 0 });
    assert_eq!(
        crate::control::machine_prefs::read_at(&home.join(relay_route_core::PREFS_REL)),
        crate::control::machine_prefs::Loaded::Present(Prefs::default())
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

/// 一台远端那一问的替身：`prefs-read` 答哪一态（`None` ⇒ 问不到），记下交过的 `prefs-set`。
struct Peer {
    state: Option<&'static str>,
    sets: std::sync::Mutex<Vec<serde_json::Value>>,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Peer {
    fn new(state: Option<&'static str>) -> Peer {
        Peer {
            state,
            sets: Default::default(),
            asked: Default::default(),
        }
    }
    fn ask(&self, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, Said> {
        self.asked.lock().unwrap().push(cmd.to_string());
        let Some(state) = self.state else {
            return Err(Said {
                code: Some("unreachable".into()),
                message: "够不到".into(),
            });
        };
        match cmd {
            "prefs-read" => Ok(json!({"state": state, "prefs": {}})),
            "prefs-set" => {
                self.sets.lock().unwrap().push(args);
                Ok(json!({"state": "ok"}))
            }
            _ => panic!("问了不该问的：{cmd}"),
        }
    }
}

fn pending_home(tag: &str) -> (PathBuf, Own) {
    let home = scratch(tag);
    let own = Own::issue_at(&home);
    write_config(&home, &serde_json::to_string(&old_shape()).unwrap());
    assert_eq!(migrate_in(&own), Migrated::Done { pending: 3 });
    (home, own)
}

fn gpu() -> cmd_wire::Machine {
    cmd_wire::Machine("gpu".to_string())
}

fn run(fut: impl std::future::Future<Output = HandedOver>) -> HandedOver {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(fut)
}

/// 那台还没有 ⇒ 交一次 `prefs-set`（整份那一台的），交成了删掉那一条；别的台不动。
#[test]
fn a_machine_without_prefs_gets_its_share_once() {
    let (home, own) = pending_home("give");
    let peer = Peer::new(Some("absent"));
    let got = run(hand_over(&own, &gpu(), |c, a| {
        std::future::ready(peer.ask(c, a))
    }));
    assert_eq!(got, HandedOver::Given);
    let want = serde_json::to_value(&plan(&old_shape()).remote["gpu"]).unwrap();
    assert_eq!(*peer.sets.lock().unwrap(), vec![want]);
    let left = read_pending(&own);
    assert!(!left.contains_key("gpu"));
    assert_eq!(left.len(), 2);
    // 再连一次：没有那一条了 ⇒ 一句都不问。
    let peer = Peer::new(Some("absent"));
    assert_eq!(
        run(hand_over(&own, &gpu(), |c, a| std::future::ready(
            peer.ask(c, a)
        ))),
        HandedOver::Nothing
    );
    assert!(peer.asked.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

/// 那台已经有了（读得懂 · 读不懂都算有）⇒ 不盖，删掉那一条。
#[test]
fn a_machine_that_already_has_prefs_is_not_overwritten() {
    for state in ["ok", "unreadable"] {
        let (home, own) = pending_home(state);
        let peer = Peer::new(Some(state));
        assert_eq!(
            run(hand_over(&own, &gpu(), |c, a| std::future::ready(
                peer.ask(c, a)
            ))),
            HandedOver::AlreadyThere
        );
        assert!(peer.sets.lock().unwrap().is_empty(), "{state}：盖了那台的");
        assert!(!read_pending(&own).contains_key("gpu"));
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// 问不到（不通 · 那台太旧不认这条）⇒ 留着那一条，下次连上再交；不出声。
#[test]
fn an_unreachable_machine_keeps_its_share_pending() {
    let (home, own) = pending_home("keep");
    let peer = Peer::new(None);
    assert!(matches!(
        run(hand_over(&own, &gpu(), |c, a| std::future::ready(
            peer.ask(c, a)
        ))),
        HandedOver::Kept(_)
    ));
    assert!(read_pending(&own).contains_key("gpu"));
    let _ = std::fs::remove_dir_all(&home);
}
