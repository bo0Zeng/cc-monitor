//! 配置编辑层那份账本：升级写回（v1 → v2 → 新格式，一次写回、原文留 `.v1` / `.v2`，会话带着副本）· 没有随包策略（不写任何文件、新会话不轮换）·
//! 新会话载入 `forNew` 那份的副本（读不懂 ⇒ 不轮换）。临时家目录，不碰真文件。

use super::*;
use crate::accounts::quota::rotation::{read_at, Disk, RotationStore};
use crate::accounts::quota::setup::shelf;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotuses-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(shelf::DIR_NAME)).expect("mkdir");
        Self(root)
    }
    fn own(&self) -> Own {
        Own::issue_data_at(&self.0)
    }
    fn file(&self) -> PathBuf {
        self.0.join(rotation::FILE_NAME)
    }
    fn sdir(&self) -> PathBuf {
        self.0.join(shelf::DIR_NAME)
    }
    fn store(&self) -> RotationStore {
        RotationStore::at(Some(self.file()))
    }
    fn setup(&self) -> Setup {
        Setup::of(Arc::new(self.store()))
    }
    fn write(&self, v: &Value) {
        std::fs::write(self.file(), v.to_string()).expect("w");
    }
}

fn env<'a>(live: &'a dyn Fn(&str) -> bool) -> UpgradeEnv<'a> {
    UpgradeEnv {
        live,
        rest: Vec::new(),
        title: &|_| None,
    }
}

fn plain() -> Value {
    json!({"line": null, "slots": [], "mode": "normal", "stint": null})
}

/// 一份第二版：c_1 照列表先后 · c_2 上限 90 · c_3 用户自己的第三版脚本 `mine`；
/// 两个会话：活的那个带副本、走了的那个也带副本。
fn v2() -> Value {
    let row = |who: Value, p: Value| {
        let mut r = who;
        r["params"] = p;
        r
    };
    let line90 = json!({"line": 90, "slots": [], "mode": "normal", "stint": null});
    let params = json!({"back": false, "delay": 0, "standby_wait": 40, "all_out": "go"});
    let cfg = |name: &str, strategy: &str, p: Value| {
        json!({"name": name, "strategy": strategy, "accounts": [row(json!({"start": true}), p.clone())],
               "params": params, "acctInit": p, "tz": "UTC", "rev": 1, "updatedAt": 0, "savedBy": "settings"})
    };
    let copy = cfg("副本", "1", line90.clone());
    json!({
        "version": 2,
        "defaultConfig": "c_2",
        "configs": {"c_1": cfg("默认配置 1", "1", plain()), "c_2": cfg("到 90 就换", "1", line90.clone()),
                    "c_3": cfg("我的", "mine", plain())},
        "sessions": {
            "s-live": {"agent": "claude-code", "start": "a", "current": "a", "since": 1, "seen": 1,
                       "config": {"id": "c_1", "rev": 1, "copy": copy}, "history": []},
            "s-gone": {"agent": "claude-code", "start": "a", "current": "a", "since": 1, "seen": 1,
                       "config": {"id": "c_1", "rev": 1, "copy": copy}, "history": []}
        }
    })
}

/// ★ 第二版：读的一侧在内存里换好来用、写一律拒；升级写回一次：`.v2` 留着、每份配置都写成用户自己的文件、
/// 活会话的副本写成文件（走了的丢）、第三版脚本挪进 `v3/`、新会话照旧不轮换；再升一次什么都不变。
#[test]
fn a_v2_book_is_upgraded_once_with_its_files() {
    let home = Home::new("v2");
    home.write(&v2());
    std::fs::write(home.sdir().join("mine.rhai"), "fn priority(ctx) { #{} }").expect("脚本");
    let store = home.store();
    assert!(
        store.now().sessions.is_empty(),
        "旧版没升级前读的一侧当没有"
    );
    assert!(
        rotation::face_change(&store, &home.own(), |b| b.saw(
            "s",
            "claude-code",
            "a",
            1,
            || None
        ))
        .is_err(),
        "旧版没升级就写了"
    );

    let live = |sid: &str| sid == "s-live";
    let m = migrate_at(&home.own(), &env(&live)).expect("升级");
    assert_eq!(m.from, Some(2));
    assert_eq!(m.moved, ["mine"]);
    assert!(home.sdir().join("v3/mine.rhai").exists() && !home.sdir().join("mine.rhai").exists());
    assert!(
        home.0.join(format!("{}.v2", rotation::FILE_NAME)).exists(),
        "原文没留 .v2"
    );
    let b: Book = match read_at(&home.file()) {
        Disk::Now(b) => b,
        other => panic!("没写成第三版：{other:?}"),
    };
    assert_eq!(b.sessions.len(), 2);
    assert_eq!(
        home.setup().uses.now().for_new,
        "",
        "升级出来的不进任何默认"
    );
    let all = shelf::all(Some(&home.sdir()));
    let copy = |sid: &str| b.sessions[sid].uses.clone().expect("会话没带副本");
    let got = copy("s-live");
    assert_eq!(got.id, "副本 · s-live", "活会话的副本没写成文件");
    assert_eq!(copy("s-gone").id, "默认配置 1", "走了的会话的副本该丢");
    let file = all.iter().find(|s| s.id == got.id).expect("副本那份文件");
    assert_eq!(got.rev, file.rev, "会话载入的就是写出来的那一版");
    assert_eq!(
        got.settings["caps"]["5h"],
        json!([{"from": 0, "to": 1440, "v": 90}]),
        "副本的设置照第二版那份"
    );
    let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    for want in ["默认配置 1", "到 90 就换", "我的"] {
        assert!(ids.contains(&want), "缺 {want}：{ids:?}");
    }
    assert_eq!(ids.len(), 4, "只该有三份配置 ＋ 一份活会话的副本：{ids:?}");
    let c2 = all
        .iter()
        .find(|s| s.id == "到 90 就换")
        .expect("到 90 就换");
    let head = &c2.file.as_ref().expect("读得懂").head;
    assert_eq!(
        head.settings["caps"]["5h"],
        json!([{"from": 0, "to": 1440, "v": 90}])
    );

    let again = migrate_at(&home.own(), &env(&live)).expect("再升一次");
    assert_eq!(again, Migrated::default());
}

/// ★ 第一版：v1 → v2 → v3 一次换完，原文留 `.v1`（不另留 `.v2`）；更早的 `when.threshold.n` 当作每个号的上限（真机上查出：
/// 10-11 那一次升级丢了这一格）。
#[test]
fn a_v1_book_goes_through_v2_to_v3_and_keeps_its_threshold() {
    let home = Home::new("v1");
    home.write(&json!({
        "rules": {"r_1": {"name": "到 80 换", "rotation": {"order": [{"start": true}, "b"], "enabled": ["b"],
                                                           "when": {"threshold": {"n": 80}}, "tz": "UTC"}}},
        "defaultRule": "r_1",
        "sessions": {}
    }));
    let m = migrate_at(&home.own(), &env(&|_| false)).expect("升级");
    assert_eq!(m.from, Some(1));
    assert!(home.0.join(format!("{}.v1", rotation::FILE_NAME)).exists());
    assert!(!home.0.join(format!("{}.v2", rotation::FILE_NAME)).exists());
    let all = shelf::all(Some(&home.sdir()));
    let r1 = all.iter().find(|s| s.id == "到 80 换").expect("写成了文件");
    let s = &r1.file.as_ref().expect("读得懂").head.settings;
    assert_eq!(s["caps"]["5h"], json!([{"from": 0, "to": 1440, "v": 80}]));
    assert_eq!(s["caps"]["7d"], json!([{"from": 0, "to": 1440, "v": 80}]));
    assert_eq!(s["accounts"], json!([{"start": true}, {"id": "b"}]));
}

/// ★ 没有随包策略：没有 `rotation.json` 时升级那一下一个文件都不写、`rotation.json` 也不写；新会话不轮换。
#[test]
fn no_strategy_ships_and_new_sessions_do_not_rotate() {
    let home = Home::new("none");
    let m = migrate_at(&home.own(), &env(&|_| false)).expect("升级");
    assert_eq!(m, Migrated::default());
    assert!(
        shelf::all(Some(&home.sdir())).is_empty(),
        "策略目录里多了文件"
    );
    assert!(!home.file().exists(), "没有旧版也写了 rotation.json");
    assert!(home.setup().for_new().is_none(), "新会话该不轮换");
}

const MINE: &str = "// 我的\n/*--- rotation\n{\"format\": 5, \"settings\": {\"n\": 1}}\n---*/\nfn decide(ctx) { #{ order: [] } }\n";

/// ★ 新会话载入 `forNew` 那份此刻的副本（之后文件再改 · `forNew` 再换都不跟）；那份读不懂 ⇒ 不给（不轮换）。
#[test]
fn a_new_session_loads_a_copy_of_for_new_and_keeps_it() {
    let home = Home::new("fornew");
    std::fs::write(home.sdir().join("mine.rhai"), MINE).expect("策略");
    let setup = home.setup();
    setup
        .change(&home.own(), |b| b.for_new = "mine".into())
        .expect("forNew");
    let mut b = Book::fresh();
    assert!(b.saw("s1", "claude-code", "a", 100, || setup.for_new()));
    let c = b.sessions["s1"].uses.clone().expect("载入了副本");
    assert_eq!((c.id.as_str(), &c.settings), ("mine", &json!({"n": 1})));
    assert_eq!(c.rev, shelf::rev_of(MINE));
    std::fs::write(
        home.sdir().join("mine.rhai"),
        MINE.replace("\"n\": 1", "\"n\": 2"),
    )
    .expect("改");
    b.saw("s1", "claude-code", "b", 200, || {
        panic!("已知的会话不该再问")
    });
    assert_eq!(
        b.sessions["s1"].uses.as_ref(),
        Some(&c),
        "换了开会话的号也该留着副本"
    );
    std::fs::write(home.sdir().join("mine.rhai"), "fn decide(").expect("坏");
    assert!(setup.for_new().is_none(), "读不懂的那份载入不了");
}
