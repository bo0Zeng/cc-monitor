//! 轮换的存取（第二版）：内置三份配置 · 往返 · 升级换形（S11）· 只在真换成时记一条 · 会话的副本（S10）· 落盘与重读。

use super::doc::{self, Edit, Set};
use super::*;
use crate::accounts::quota::strategy::{self, Lang};
use serde_json::json;

fn all() -> Vec<std::sync::Arc<strategy::Strategy>> {
    strategy::builtins().to_vec()
}

fn ok(a: &str) -> bool {
    !a.is_empty() && a != "_"
}

/// 没有文件时：内置三份配置、默认指向第一份；每行写全那份策略的每号参数；默认配置 1 ＝ 用满才换 · 都到上限后接着用。
#[test]
fn a_fresh_book_has_the_three_seed_configs() {
    let b = Book::default();
    assert_eq!(b.default_config, "c_00000001");
    let ids: Vec<&str> = b.configs.keys().map(String::as_str).collect();
    assert_eq!(ids, ["c_00000001", "c_00000002", "c_00000003"]);
    let one = &b.configs["c_00000001"].body;
    assert_eq!(
        one.name,
        copy_core::copy_text("beRotation.config.seedName", &[("n", "1")])
    );
    assert_eq!(one.strategy, "1");
    assert_eq!(one.accounts.len(), 1);
    assert_eq!(one.accounts[0].who, Who::Start);
    assert_eq!(one.accounts[0].params["line"], Value::Null);
    assert_eq!(one.params["all_out"], json!("go"));
    let two = &b.configs["c_00000002"].body;
    assert_eq!(
        two.accounts
            .iter()
            .map(|r| r.who.clone())
            .collect::<Vec<_>>(),
        [Who::Start, Who::Rest]
    );
    assert_eq!(two.accounts[1].params["line"], json!(90));
    assert_eq!(two.params["all_out"], json!("stop"));
    let keys: Vec<&String> = two.accounts[0].params.keys().collect();
    assert_eq!(keys, ["line", "mode", "slots"], "每行写全内置 2 的每号参数");
}

/// 第二版往返：写出去带 `version: 2`，读回来一模一样；没有 `version` 才当第一版。
#[test]
fn the_v2_shape_round_trips_and_carries_its_version() {
    let mut b = Book::default();
    b.saw("s", "claude-code", "a", 10);
    let s = b.sessions.get_mut("s").expect("s");
    s.manual.place.push(Place {
        account: "x".into(),
        at: 2,
    });
    s.memo = Some(json!({"delay": {"to": "x", "since": 5}}));
    let v = serde_json::to_value(&b).expect("json");
    assert_eq!(v["version"], json!(2));
    assert_eq!(v["defaultConfig"], json!("c_00000001"));
    let back: Book = serde_json::from_value(v.clone()).expect("读回");
    assert_eq!(back, b);
    assert!(back.from_v1.is_none());
    let mut bad = v;
    bad["version"] = json!(9);
    assert!(
        serde_json::from_value::<Book>(bad).is_err(),
        "认不得的版本 ⇒ 读不懂"
    );
}

fn v1_sample() -> Value {
    json!({
        "rules": {
            "r_00000000": {"name": copy_core::copy_text("beRotation.migrate.oldSeedName", &[]), "rev": 3, "updatedAt": 100, "rotation": {
                "order": [{"start": true}, "team", "lab", "api", "gone"],
                "enabled": ["team", "lab", "api"],
                "atLimit": "stop",
                "cap": {
                    "*": {"5h": 90, "7d": 95},
                    "team": {"*": 99},
                    "lab": {"5h": 99, "7d:opus": 80, "*": [{"at": "17:00-02:00", "n": 80}]},
                    "api": {"*": 0}
                },
                "stint": {"*": {"5h": 10}, "team": {"7d": 5}},
                "preempt": true,
                "fallback": ["api"],
                "wait": 30
            }},
            "r_aaaaaaaa": {"name": "夜里", "rev": 1, "updatedAt": 200, "rotation": {
                "order": [{"start": true}], "enabled": [], "tz": "Asia/Shanghai"
            }}
        },
        "defaultRule": "r_00000000",
        "sessions": {
            "s1": {"agent": "claude-code", "start": "_", "current": "lab", "since": 50, "source": "follow",
                   "blockedAbove": ["team"],
                   "history": [
                       {"at": 1, "from": "_", "to": "team", "why": {"threshold": {"n": 90, "w": "5h"}}, "fromResetsAt": 500},
                       {"at": 2, "from": "team", "to": "lab", "why": "preempt"},
                       {"at": 3, "from": "lab", "to": "team", "why": {"stint": {"w": "5h", "n": 12}}},
                       {"at": 4, "from": "team", "to": "team", "why": {"wait": {"account": "team", "instead": "api"}}, "fromResetsAt": 900},
                       {"at": 5, "from": "team", "to": "team", "why": "toOverage"},
                       {"at": 6, "from": "team", "to": "lab", "why": {"full": {"w": "5h"}}},
                       {"at": 7, "from": "lab", "to": "lab", "why": {"skipped": {"account": "lab", "reason": "needsLogin"}}},
                       {"at": 8, "from": "lab", "to": "lab", "why": {"held": {"n": 90}}},
                       {"at": 9, "from": "lab", "to": "team", "why": "manualHot"},
                       {"at": 10, "from": "api", "to": "lab", "why": "leaveFallback"},
                       {"at": 11, "from": "lab", "to": "team", "why": {"off": {}}}
                   ]},
            "s2": {"agent": "claude-code", "start": "_", "current": "_", "since": 60, "source": {"rule": "r_aaaaaaaa"}},
            "s3": {"agent": "claude-code", "start": "_", "current": "_", "since": 70, "source": "custom",
                   "custom": {"order": [{"start": true}, "team"], "enabled": ["team"]}},
            "s4": {"agent": "claude-code", "start": "_", "current": "_", "since": 80, "source": {"parent": "s2"}},
            "s5": {"agent": "claude-code", "start": "_", "current": "_", "since": 90, "source": {"parent": "nope"}},
            "s6": {"agent": "claude-code", "start": "_", "current": "_", "since": 95, "follow": false}
        }
    })
}

fn row(who: Value, params: Value) -> Value {
    let mut r = who;
    r["params"] = params;
    r
}

/// ★ S11：第一版样本（分窗口线 · 时段 · `7d:<模型>` · 兜底 · 抢回 · 单段 · 先等 · 本会话两份 · 跟随父会话 · 旧原因码各一）⇒ 与期望的第二版逐字段相同。
#[test]
fn a_v1_book_upgrades_field_for_field() {
    let b: Book = serde_json::from_value(v1_sample()).expect("第一版读得懂");
    assert!(b.from_v1.is_some());
    let p = |line: Value, slots: Value, mode: &str, stint: Value| json!({"line": line, "slots": slots, "mode": mode, "stint": stint});
    let want_default = json!({
        "name": copy_core::copy_text("beRotation.config.seedName", &[("n", "1")]),
        "strategy": "1",
        "accounts": [
            row(json!({"start": true}), p(json!({"5h": 90, "7d": 95}), json!([]), "normal", json!(10))),
            row(json!({"id": "team"}), p(json!(99), json!([]), "normal", json!(5))),
            row(json!({"id": "lab"}), p(json!({"5h": 99, "7d": 80}), json!([{"at": "17:00-02:00", "line": 80}]), "normal", json!(10))),
            row(json!({"id": "api"}), p(json!({"5h": 90, "7d": 95}), json!([{"at": "00:00-24:00", "line": 0}]), "standby", json!(10)))
        ],
        "params": {"back": true, "delay": 0, "standby_wait": 30, "all_out": "stop"},
        "acctInit": p(json!({"5h": 90, "7d": 95}), json!([]), "normal", json!(10)),
        "parked": {},
        "tz": "UTC",
        "rev": 3, "updatedAt": 100, "savedBy": "settings"
    });
    assert_eq!(
        serde_json::to_value(&b.configs["r_00000000"]).expect("json"),
        want_default
    );
    let night = &b.configs["r_aaaaaaaa"];
    assert_eq!(night.body.name, "夜里");
    assert_eq!(night.body.tz.as_deref(), Some("Asia/Shanghai"), "时区照留");
    assert_eq!(night.body.params["all_out"], json!("go"), "continue ⇒ go");
    assert_eq!(
        night.body.accounts[0].params["line"],
        Value::Null,
        "没线 ⇒ 用满才换"
    );
    for n in 1..=2 {
        let (id, c) = seed_config(n, 0).expect("种子");
        assert_eq!(b.configs.get(&id), Some(&c), "补建默认配置 {}", n + 1);
    }
    assert_eq!(b.default_config, "r_00000000");
    let open = |sid: &str| serde_json::to_value(&b.sessions[sid].config).expect("json");
    assert_eq!(open("s1"), json!({"id": "r_00000000", "rev": 3}));
    assert_eq!(open("s2"), json!({"id": "r_aaaaaaaa", "rev": 1}));
    assert_eq!(
        open("s4"),
        json!({"id": "r_aaaaaaaa", "rev": 1}),
        "跟随父会话 ⇒ 父此刻那一份"
    );
    assert_eq!(
        open("s5"),
        json!({"id": "r_00000000", "rev": 3}),
        "追不到父 ⇒ 本机默认"
    );
    assert_eq!(
        open("s6"),
        json!({"id": "r_00000000", "rev": 3}),
        "本会话却没有那一份 ⇒ 本机默认"
    );
    let s3 = &b.sessions["s3"].config;
    let copy = s3.copy.as_ref().expect("本会话那一份 ⇒ 副本");
    assert_eq!(copy.name, want_default["name"].as_str().unwrap_or_default());
    assert_eq!(
        copy.accounts
            .iter()
            .map(|r| r.who.clone())
            .collect::<Vec<_>>(),
        [Who::Start, Who::Id("team".into())]
    );
    let h: Vec<Value> = b.sessions["s1"]
        .history
        .iter()
        .map(|r| serde_json::to_value(r).expect("json"))
        .collect();
    let want_h = json!([
        {"at": 1, "from": "_", "to": "team", "leave": {"k": "unusable", "why": {"k": "@line", "w": "5h", "n": 90}}, "fromResetsAt": 500},
        {"at": 2, "from": "team", "to": "lab", "leave": {"k": "top"}, "pick": {"strategy": "1", "why": {"k": "w_order", "n": 3}}},
        {"at": 3, "from": "lab", "to": "team", "leave": {"k": "top"}, "pick": {"strategy": "1", "why": {"k": "w_stint", "n": 12, "m": 12}}},
        {"at": 4, "from": "team", "to": "team", "leave": {"k": "top"}, "pick": {"strategy": "1", "why": {"k": "w_wait", "acct": "team", "at": 900}}, "fromResetsAt": 900},
        {"at": 5, "from": "team", "to": "team", "leave": {"k": "held"}},
        {"at": 6, "from": "team", "to": "lab", "leave": {"k": "unusable", "why": {"k": "@refused"}}},
        {"at": 7, "from": "lab", "to": "lab", "leave": {"k": "unusable", "why": {"k": "@login"}}},
        {"at": 8, "from": "lab", "to": "lab", "leave": {"k": "held"}},
        {"at": 9, "from": "lab", "to": "team", "leave": {"k": "manual"}},
        {"at": 10, "from": "api", "to": "lab", "leave": {"k": "top"}, "pick": {"strategy": "1", "why": {"k": "w_order", "n": 3}}},
        {"at": 11, "from": "lab", "to": "team", "leave": {"k": "unusable", "why": {"k": "@off"}}}
    ]);
    assert_eq!(Value::Array(h), want_h);
    let s1 = serde_json::to_value(&b.sessions["s1"]).expect("json");
    assert!(s1.get("blockedAbove").is_none(), "blockedAbove 丢掉");
    // 再读一次不变。
    let again: Book =
        serde_json::from_value(serde_json::to_value(&b).expect("json")).expect("读回");
    assert_eq!(Book { from_v1: None, ..b }, again);
}

fn sandbox(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-rot2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    d.join(FILE_NAME)
}

/// ★ S11：读到第一版 ⇒ 在锁里换成第二版写回、原文留一份 `rotation.json.v1`；读不懂的不覆盖。
#[test]
fn the_store_rewrites_a_v1_file_once_and_keeps_the_original() {
    let path = sandbox("v1");
    let orig = serde_json::to_string_pretty(&v1_sample()).expect("json");
    std::fs::write(&path, &orig).expect("w");
    let st = RotationStore::at(Some(path.clone()));
    let b = st.now();
    assert_eq!(b.default_config, "r_00000000");
    let disk: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("r")).expect("json");
    assert_eq!(disk["version"], json!(2), "写回了第二版");
    let kept = std::fs::read_to_string(v1_backup_of(&path)).expect("留着 .v1");
    assert_eq!(
        serde_json::from_str::<Value>(&kept).expect("json"),
        v1_sample()
    );
    // 再读一次：已经是第二版，不再动 .v1。
    let again = RotationStore::at(Some(path.clone())).now();
    assert_eq!(again, Book { from_v1: None, ..b });
    // 读不懂的不覆盖。
    std::fs::write(&path, "{\"version\": 9}").expect("w");
    let st = RotationStore::at(Some(path.clone()));
    assert!(st.change(|b| b.default_config = "x".into()).is_err());
    assert_eq!(
        std::fs::read_to_string(&path).expect("r"),
        "{\"version\": 9}"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

fn rec(at: u64, from: &str, to: &str) -> SwitchRecord {
    SwitchRecord {
        at,
        at_text: None,
        from: from.into(),
        to: to.into(),
        leave: Leave::Outranked,
        pick: None,
        from_resets_at: None,
        from_resets_at_text: None,
        text: None,
    }
}

/// ★ 会话已经被别的请求换走了 ⇒ 这一下不再记（同一秒两条一样的记录，10-10 那一幕）；真换成才记、才清基线。
#[test]
fn a_switch_is_recorded_only_when_it_really_happens() {
    let mut b = Book::default();
    b.saw("s", "claude-code", "a", 10);
    b.rebase(
        "s",
        &[(
            "5h".to_string(),
            Base {
                used: 0.3,
                resets_at: None,
            },
        )]
        .into(),
    );
    assert!(b.pin("s", rec(20, "a", "c")));
    assert!(b.sessions["s"].baseline.is_empty());
    // 另一发请求同一秒也判出「a ⇒ c」：会话已经在 c 上 ⇒ 不记。
    assert!(!b.pin("s", rec(20, "a", "c")));
    assert!(!b.pin("s", rec(21, "c", "c")), "原地不算换");
    let h = &b.sessions["s"].history;
    assert_eq!(h.len(), 1);
    assert_eq!(
        (b.sessions["s"].current.as_str(), b.sessions["s"].since),
        ("c", 20)
    );
}

/// 没换成的那几种（停发 · 留在超额）：自上一次真换号以来同一句只记一次。
#[test]
fn a_stuck_record_is_noted_once_until_the_next_switch() {
    let mut b = Book::default();
    b.saw("s", "claude-code", "a", 10);
    let held = SwitchRecord {
        leave: Leave::Held,
        ..rec(30, "a", "a")
    };
    assert!(b.note_stuck("s", held.clone()));
    assert!(!b.note_stuck(
        "s",
        SwitchRecord {
            at: 31,
            ..held.clone()
        }
    ));
    assert!(b.pin("s", rec(40, "a", "c")));
    assert!(b.note_stuck(
        "s",
        SwitchRecord {
            from: "c".into(),
            to: "c".into(),
            ..held
        }
    ));
    assert_eq!(b.sessions["s"].history.len(), 3);
}

/// 新会话打开本机默认那份；换了起它的号 ⇒ 钉号 · 手动调整 · 备忘清掉；本机默认换了，开着的会话不跟着换。
#[test]
fn a_new_session_opens_the_default_and_a_relaunch_clears_its_manual_state() {
    let mut b = Book::default();
    b.saw("s", "claude-code", "a", 10);
    assert_eq!(b.sessions["s"].config.id, "c_00000001");
    {
        let s = b.sessions.get_mut("s").expect("s");
        s.manual.hot = Some(Hot {
            account: "x".into(),
            since: 11,
        });
        s.memo = Some(json!({"k": 1}));
    }
    b.default_config = "c_00000002".into();
    b.saw("s", "claude-code", "b", 20);
    let s = &b.sessions["s"];
    assert_eq!((s.current.as_str(), s.since), ("b", 20));
    assert!(s.manual.is_empty() && s.memo.is_none());
    assert_eq!(s.config.id, "c_00000001", "本机默认换了，开着的不跟");
    b.saw("t", "claude-code", "a", 30);
    assert_eq!(b.sessions["t"].config.id, "c_00000002");
}

/// 清旧会话：没改过副本 · 没手动调整 · 没换过号 · 一周多没被看见 ⇒ 删；改过副本的留着。
#[test]
fn stale_untouched_sessions_are_dropped() {
    let mut b = Book::default();
    b.saw("old", "claude-code", "a", 10);
    b.saw("kept", "claude-code", "a", 10);
    let body = b.configs["c_00000001"].body.clone();
    b.sessions.get_mut("kept").expect("kept").config.copy = Some(ConfigBody {
        name: "x".into(),
        ..body
    });
    b.saw("new", "claude-code", "a", 10 + DROP_AFTER + 1);
    assert!(!b.sessions.contains_key("old"));
    assert!(b.sessions.contains_key("kept"));
}

fn set(acct: Option<&str>, k: &str, v: Value) -> Edit {
    Edit {
        set: vec![Set {
            acct: acct.map(str::to_string),
            k: k.into(),
            v,
        }],
        ..Edit::default()
    }
}

fn two_sessions() -> Book {
    let mut b = Book::default();
    b.default_config = "c_00000002".into();
    b.saw("me", "claude-code", "a", 10);
    b.saw("other", "claude-code", "a", 10);
    b
}

/// ★ S10：改一格 ⇒ 已改、diff 一条；改回 ⇒ 副本没了；改 `rest` 里的号 ⇒ 给它单写一行；`open` 有副本 ⇒ 先问。
#[test]
fn editing_the_copy_marks_it_dirty_and_editing_back_drops_it() {
    let mut b = two_sessions();
    let all = all();
    let e = doc::edit_session(
        &mut b,
        "me",
        &set(Some("@rest"), "line", json!(80)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    assert!(e.dirty);
    assert_eq!(e.diff.len(), 1, "{:?}", e.diff);
    assert_eq!(
        (e.diff[0].acct.as_deref(), e.diff[0].k.as_str()),
        (Some("@rest"), "line")
    );
    assert!(!e.diff[0].text.is_empty());
    assert!(b.sessions["me"].config.copy.is_some());
    assert!(
        b.sessions["other"].config.copy.is_none(),
        "别的会话不受影响"
    );
    let e = doc::edit_session(
        &mut b,
        "me",
        &set(Some("@rest"), "line", json!(90)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    assert!(!e.dirty && e.diff.is_empty());
    assert!(
        b.sessions["me"].config.copy.is_none(),
        "改回原样 ⇒ 副本丢掉"
    );
    // `rest` 底下的一个号 ⇒ 单写一行（抄 rest 那一行的数），放在 rest 后面。
    let e = doc::edit_session(
        &mut b,
        "me",
        &set(Some("lab"), "line", json!(99)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    let copy = b.sessions["me"].config.copy.clone().expect("副本");
    assert_eq!(
        copy.accounts
            .iter()
            .map(|r| r.who.clone())
            .collect::<Vec<_>>(),
        [Who::Start, Who::Rest, Who::Id("lab".into())]
    );
    assert_eq!(copy.accounts[2].params["line"], json!(99));
    assert_eq!(copy.accounts[2].params["mode"], json!("normal"));
    assert!(e
        .diff
        .iter()
        .any(|d| d.k == "@join" && d.acct.as_deref() == Some("lab")));
    // 收不下的格：不改、记一格错。
    let e = doc::edit_session(
        &mut b,
        "me",
        &set(None, "delay", json!(999)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    assert_eq!(e.errors.len(), 1);
    assert_eq!(e.errors[0].cell, "params.delay");
    assert_eq!(e.errors[0].code, "range");
    // 有副本时换一份配置 ⇒ 先问；`discard` ⇒ 打开、手动调整与备忘清掉。
    b.sessions.get_mut("me").expect("me").memo = Some(json!({"a": 1}));
    assert!(
        matches!(doc::open_session(&mut b, "me", "c_00000003", false, &all, Lang::Zh), doc::Opened::Dirty(d) if !d.is_empty())
    );
    assert_eq!(
        doc::open_session(&mut b, "me", "c_00000003", true, &all, Lang::Zh),
        doc::Opened::Opened
    );
    let s = &b.sessions["me"];
    assert_eq!(
        (
            s.config.id.as_str(),
            s.config.copy.is_none(),
            s.memo.is_none()
        ),
        ("c_00000003", true, true)
    );
}

/// ★ S10：`save` 撞 `rev` ⇒ 冲突两边 `diff`；`force` ⇒ 覆盖；`as` ⇒ 新 id、本会话改开它；
/// 别处存了新版 ⇒ 没副本的会话 `rev` 跟上、有副本的出 `newer`。
#[test]
fn saving_the_copy_handles_conflicts_save_as_and_newer_versions() {
    let mut b = two_sessions();
    let all = all();
    doc::edit_session(
        &mut b,
        "me",
        &set(None, "delay", json!(10)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    // 设置窗那边先存了一版。
    let mut theirs = b.configs["c_00000002"].body.clone();
    theirs.params.insert("skip".into(), json!(5));
    doc::write_config(&mut b, "c_00000002", theirs, 50, SAVED_BY_SETTINGS, None);
    assert_eq!(b.sessions["other"].config.rev, 2, "没副本的跟上");
    assert_eq!(
        doc::newer_of(&b, &b.sessions["me"]),
        Some((2, 50, SAVED_BY_SETTINGS.to_string()))
    );
    let r = doc::save_session(&mut b, "me", Some(1), None, false, 60, None, &all, Lang::Zh);
    let doc::Saved::Clash { rev, theirs, mine } = r else {
        panic!("该撞上：{r:?}");
    };
    assert_eq!(rev, 2);
    assert!(!theirs.is_empty() && !mine.is_empty());
    assert!(b.sessions["me"].config.copy.is_some(), "撞上不写");
    let r = doc::save_session(&mut b, "me", Some(1), None, true, 70, None, &all, Lang::Zh);
    assert_eq!(
        r,
        doc::Saved::Saved {
            id: "c_00000002".into(),
            rev: 3
        }
    );
    assert_eq!(b.configs["c_00000002"].body.params["delay"], json!(10));
    assert_eq!(b.configs["c_00000002"].saved_by, "me");
    assert!(b.sessions["me"].config.copy.is_none());
    // 另存为：新 id，本会话改开它，名字不起 ⇒ 配置 N。
    doc::edit_session(
        &mut b,
        "me",
        &set(None, "delay", json!(20)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    let r = doc::save_session(
        &mut b,
        "me",
        None,
        Some(None),
        false,
        80,
        None,
        &all,
        Lang::Zh,
    );
    let doc::Saved::Saved { id, rev } = r else {
        panic!("{r:?}");
    };
    assert_eq!(rev, 1);
    assert_eq!(b.sessions["me"].config.id, id);
    assert_eq!(
        b.configs[&id].body.name,
        copy_core::copy_text("beRotation.config.anonName", &[("n", "1")])
    );
    assert_eq!(
        b.configs["c_00000002"].body.params["delay"],
        json!(10),
        "另存为不动原来那份"
    );
    assert!(doc::revert_session(&mut b, "me"));
}

/// 换策略：旧参数收进 `parked`，同名同类型的照搬；换回来 ⇒ 取回收起的。
#[test]
fn switching_strategy_parks_and_restores_params() {
    let all = all();
    let mut b = two_sessions();
    doc::edit_session(
        &mut b,
        "me",
        &set(None, "grab_min", json!(45)),
        &all,
        &ok,
        Lang::Zh,
    )
    .expect("在");
    let e = Edit {
        strategy: Some("3".into()),
        ..Edit::default()
    };
    doc::edit_session(&mut b, "me", &e, &all, &ok, Lang::Zh).expect("在");
    let copy = b.sessions["me"].config.copy.clone().expect("副本");
    assert_eq!(copy.strategy, "3");
    assert!(copy.parked.contains_key("2"));
    assert_eq!(copy.params["all_out"], json!("stop"), "同名照搬");
    assert!(copy.params.get("grab_min").is_none());
    let back = Edit {
        strategy: Some("2".into()),
        ..Edit::default()
    };
    doc::edit_session(&mut b, "me", &back, &all, &ok, Lang::Zh).expect("在");
    let copy = b.sessions["me"].config.copy.clone().expect("副本");
    assert_eq!(copy.params["grab_min"], json!(45), "取回收起的");
}

// ── 别的进程写了盘 ⇒ 本进程照样推 ─────────────────────────────────────────────

fn drain(rx: &mut tokio::sync::broadcast::Receiver<String>, prefix: &str) -> Vec<String> {
    use tokio::sync::broadcast::error::TryRecvError;
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(s) if s.starts_with(prefix) && !out.contains(&s) => out.push(s),
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(_) => break,
        }
    }
    out.sort();
    out
}

/// 盘上那一份被别处改了 ⇒ 重扫时比对上一份、改到的会话各响一下；本进程自己写的那一下不再响第二次；
/// 配置改了 ⇒ 开着它的会话也响。
#[test]
fn a_write_from_another_process_rings_here_on_rescan() {
    let path = sandbox("a1");
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| {
        b.saw("a1-keep", "claude-code", "a", 1);
        b.saw("a1-flip", "claude-code", "a", 1)
    })
    .expect("write");
    let mut rx = changes().subscribe();
    rescan(&path);
    assert!(drain(&mut rx, "a1-").is_empty(), "本进程写的不重推");
    let mut b = match read_at(&path) {
        Read::Present(b) => b,
        _ => panic!("读不回"),
    };
    b.sessions.get_mut("a1-flip").expect("flip").memo = Some(json!({"x": 1}));
    b.saw("a1-new", "claude-code", "a", 2);
    crate::common::own_state::write_json(&path, &b).expect("write");
    rescan(&path);
    assert_eq!(
        drain(&mut rx, "a1-"),
        vec!["a1-flip".to_string(), "a1-new".to_string()]
    );
    let mut b = match read_at(&path) {
        Read::Present(b) => b,
        _ => panic!("读不回"),
    };
    b.configs.get_mut("c_00000001").expect("1").body.name = "改了".into();
    crate::common::own_state::write_json(&path, &b).expect("write");
    rescan(&path);
    assert_eq!(drain(&mut rx, "a1-").len(), 3, "开着它的会话都响");
    rescan(&path);
    assert!(drain(&mut rx, "a1-").is_empty(), "没变不响");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// 盯着那个目录：别处一写，不用谁来问，本进程就重扫。
#[test]
fn the_watcher_rescans_a_foreign_write_without_being_asked() {
    let path = sandbox("a1w");
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| b.saw("a1w-s", "claude-code", "a", 1))
        .expect("write");
    let _w = watch(&path).expect("watch");
    assert!(
        path.parent()
            .expect("dir")
            .join(super::super::strategy::DIR_NAME)
            .is_dir(),
        "策略目录一起建好、一起盯"
    );
    let mut b = st.now();
    b.sessions.get_mut("a1w-s").expect("s").memo = Some(json!({"y": 2}));
    crate::common::own_state::write_json(&path, &b).expect("write");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let seen_it = || {
        let g = seen().lock().unwrap_or_else(|e| e.into_inner());
        g.get(&path)
            .is_some_and(|(_, b)| b.sessions["a1w-s"].memo.is_some())
    };
    while !seen_it() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(seen_it(), "盯盘线程没重扫到别处写的那一份");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// ★ 读不懂的那份：引擎照上一份读懂的接着挑号（留在内存里），把「读不懂」交出来（一句 ＋ 原话），盘上那份一字不动；
/// 修好了 ⇒ 照新的、状态消掉。
#[test]
fn an_unreadable_file_keeps_the_last_good_book_and_says_so() {
    let path = sandbox("unread");
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| b.default_config = "c_00000003".into())
        .expect("write");
    assert_eq!(st.now().default_config, "c_00000003");
    assert!(st.unreadable().is_none());
    std::fs::write(&path, "{broken").expect("w");
    let b = st.now();
    assert_eq!(b.default_config, "c_00000003", "照上一份读懂的接着用");
    let why = st.unreadable().expect("读不懂要交出来");
    assert!(!why.said.is_empty());
    assert!(why.raw.is_some(), "原话进复制详情");
    assert!(
        st.change(|b| b.default_config = "c_00000001".into())
            .is_err(),
        "不覆盖"
    );
    assert_eq!(std::fs::read_to_string(&path).expect("r"), "{broken");
    let mut good = Book::default();
    good.default_config = "c_00000002".into();
    crate::common::own_state::write_json(&path, &good).expect("w");
    assert_eq!(st.now().default_config, "c_00000002");
    assert!(st.unreadable().is_none());
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
