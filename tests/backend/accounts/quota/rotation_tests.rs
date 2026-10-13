//! `rotation.json` 第三版的存取：升级链（v1 → v2 → v3，一次写回、原文留 `.v1` / `.v2`）· 没有随包策略（不写任何文件、新会话不轮换）·
//! 读不懂不覆盖 · 新会话 · 清旧会话。临时家目录，不碰真文件。

use super::*;
use serde_json::{json, Value};

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotstore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(shelf::DIR_NAME)).expect("mkdir");
        Self(root)
    }
    fn own(&self) -> Own {
        Own::issue_data_at(&self.0)
    }
    fn file(&self) -> PathBuf {
        self.0.join(FILE_NAME)
    }
    fn sdir(&self) -> PathBuf {
        self.0.join(shelf::DIR_NAME)
    }
    fn store(&self) -> RotationStore {
        RotationStore::at(Some(self.file()))
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
    let mem = store.now();
    assert_eq!(mem.version, book::BOOK_VERSION);
    assert_eq!(
        mem.sessions["s-live"].strategy.id, "c_1",
        "内存里的会话照它打开的那份"
    );
    assert!(
        face_change(&store, &home.own(), |b| b.for_new = "x".into()).is_err(),
        "旧版没升级就写了"
    );

    let live = |sid: &str| sid == "s-live";
    let m = migrate_at(&home.own(), &env(&live)).expect("升级");
    assert_eq!(m.from, Some(2));
    assert_eq!(m.moved, ["mine"]);
    assert!(home.sdir().join("v3/mine.rhai").exists() && !home.sdir().join("mine.rhai").exists());
    assert!(
        home.0.join(format!("{FILE_NAME}.v2")).exists(),
        "原文没留 .v2"
    );
    let b = match read_at(&home.file()) {
        Disk::Now(b) => b,
        other => panic!("没写成第三版：{other:?}"),
    };
    assert_eq!(b.for_new, "", "升级出来的不进任何默认");
    assert_eq!(b.sessions.len(), 2);
    assert!(
        b.sessions["s-live"].strategy.id.starts_with("s_"),
        "活会话的副本没写成文件"
    );
    assert_eq!(
        b.sessions["s-gone"].strategy.id, "c_1",
        "走了的会话的副本该丢"
    );
    let all = shelf::all(Some(&home.sdir()));
    let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    for want in ["c_1", "c_2", "c_3"] {
        assert!(ids.contains(&want), "缺 {want}：{ids:?}");
    }
    assert_eq!(ids.len(), 4, "只该有三份配置 ＋ 一份活会话的副本：{ids:?}");
    let c2 = all.iter().find(|s| s.id == "c_2").expect("c_2");
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
    assert!(home.0.join(format!("{FILE_NAME}.v1")).exists());
    assert!(!home.0.join(format!("{FILE_NAME}.v2")).exists());
    let all = shelf::all(Some(&home.sdir()));
    let r1 = all.iter().find(|s| s.id == "r_1").expect("写成了文件");
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
    assert_eq!(
        std::fs::read_dir(home.sdir()).expect("目录").count(),
        0,
        "策略目录里多了文件"
    );
    assert!(!home.file().exists(), "没有旧版也写了 rotation.json");
    let mut b = home.store().now();
    assert_eq!(b.for_new, "");
    assert!(b.saw("s1", "claude-code", "a", 100));
    assert_eq!(b.sessions["s1"].strategy.id, "", "新会话该不轮换");
}

/// ★ 读不懂的那一份：升级不动它、写也不覆盖；读的一侧照上一份读懂的（没有过 ⇒ 新的那一份）接着用、说得出为什么。
#[test]
fn an_unreadable_book_is_never_overwritten() {
    let home = Home::new("unread");
    std::fs::write(home.file(), "{not json").expect("w");
    assert!(migrate_at(&home.own(), &env(&|_| false)).is_err());
    let store = home.store();
    assert!(face_change(&store, &home.own(), |b| b.for_new = "x".into()).is_err());
    assert_eq!(
        std::fs::read_to_string(home.file()).expect("r"),
        "{not json"
    );
    assert_eq!(store.now().for_new, "");
    assert!(store.unreadable().is_some());
    let mut v = json!({"version": 3, "forNew": "mine", "extra": 1});
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(
        matches!(read_at(&home.file()), Disk::Unreadable(_)),
        "多一格该读不懂"
    );
    v.as_object_mut().expect("obj").remove("extra");
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(matches!(read_at(&home.file()), Disk::Now(_)));
}

/// 新会话照 `forNew`；换了开它的号 ⇒ 在用的号回到它、拖出来的顺序 · memo 清掉；没草稿 · 没拖 · 没换过号、七天没见的清掉。
#[test]
fn new_sessions_follow_for_new_and_stale_ones_are_dropped() {
    let mut b = Book::fresh();
    b.for_new = "mine".into();
    assert!(b.saw("s1", "claude-code", "a", 100));
    assert_eq!(b.sessions["s1"].strategy.id, "mine");
    b.sessions.get_mut("s1").expect("s1").temp = vec!["b".into()];
    b.sessions.get_mut("s1").expect("s1").current = "b".into();
    assert!(b.saw("s1", "claude-code", "c", 200));
    let s = &b.sessions["s1"];
    assert_eq!((s.current.as_str(), s.temp.len()), ("c", 0));
    assert!(!b.saw("s1", "claude-code", "c", 300), "同一天再看见不该改");
    b.saw("s2", "claude-code", "a", 300);
    let later = 300 + book::DROP_AFTER + 1;
    b.saw("s3", "claude-code", "a", later);
    assert!(!b.sessions.contains_key("s1") && !b.sessions.contains_key("s2"));
    assert!(b.sessions.contains_key("s3"));
}
