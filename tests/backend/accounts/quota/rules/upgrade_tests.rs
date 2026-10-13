//! 第二版 → 第三版：合成的第二版样本逐字段对期望。

use super::*;
use crate::accounts::quota::rules::script;
use serde_json::{json, Value};

fn shipped(name: &str) -> String {
    std::fs::read_to_string(
        crate::guard_support::repo_root()
            .join("src/shared/rotation/strategies")
            .join(format!("{name}.rhai")),
    )
    .expect("随包那份")
}

fn ship() -> Vec<(String, String)> {
    ["list-order", "reset-first", "most-left"]
        .iter()
        .map(|n| (n.to_string(), shipped(n)))
        .collect()
}

fn row(who: Value, params: Value) -> Value {
    let mut r = who;
    r["params"] = params;
    r
}

fn config(name: &str, strategy: &str, accounts: Value, params: Value, init: Value) -> Value {
    json!({"name": name, "strategy": strategy, "accounts": accounts, "params": params, "acctInit": init,
           "tz": "Asia/Shanghai", "rev": 3, "updatedAt": 1, "savedBy": "settings"})
}

fn session(cfg: &str, copy: Option<Value>) -> Value {
    let mut c = json!({"id": cfg, "rev": 3});
    if let Some(x) = copy {
        c["copy"] = x;
    }
    json!({"agent": "claudecode", "start": "a", "current": "b", "since": 100, "seen": 200, "config": c,
    "manual": {"hot": {"account": "c", "since": 150}},
    "memo": {"x": 1},
    "history": [
      {"at": 110, "from": "a", "to": "b", "leave": {"k": "top"}},
      {"at": 120, "from": "b", "to": "b", "leave": {"k": "held"}, "fromResetsAt": 999},
      {"at": 130, "from": "b", "to": "c", "leave": {"k": "unusable", "why": {"k": "@line"}}},
      {"at": 140, "from": "c", "to": "b", "leave": {"k": "manual"}},
      {"at": 141, "from": "b", "to": "b", "leave": {"k": "overage"}}
    ]})
}

fn v2() -> Value {
    let plain = json!({"line": null, "slots": [], "mode": "normal", "stint": null});
    json!({
      "version": 2,
      "defaultConfig": "c_00000002",
      "configs": {
        "c_00000001": config("默认配置 1", "1", json!([row(json!({"start": true}), plain.clone())]),
                             json!({"back": false, "delay": 0, "standby_wait": 40, "all_out": "go"}), plain.clone()),
        "c_00000002": config("夜里省着用", "2",
            json!([
              row(json!({"start": true}), json!({"line": 90, "slots": [], "mode": "normal"})),
              row(json!({"id": "s"}), json!({"line": 90, "slots": [], "mode": "standby"})),
              row(json!({"rest": true}), json!({"line": 90, "slots": [], "mode": "normal"})),
              row(json!({"id": "f"}), json!({"line": {"5h": 80, "7d": null}, "slots": [{"at": "20:00-23:00", "line": 0}], "mode": "first"}))
            ]),
            json!({"back": true, "all_out": "stop", "standby_wait": 30}),
            json!({"line": 90, "slots": [], "mode": "normal"})),
        "c_000000aa": config("自己写的", "c_mine", json!([row(json!({"start": true}), plain.clone())]),
                             json!({}), plain.clone())
      },
      "sessions": {
        "live1": session("c_00000002", Some(config("夜里省着用", "2", json!([row(json!({"start": true}), json!({"line": 50, "slots": []}))]), json!({"all_out": "go"}), json!({"line": 50, "slots": []})))),
        "dead1": session("c_00000002", Some(config("夜里省着用", "2", json!([row(json!({"start": true}), plain.clone())]), json!({}), plain.clone()))),
        "plain": session("c_00000001", None)
      }
    })
}

fn run(local: Option<&str>) -> Upgraded {
    let ship = ship();
    let rest = vec!["r1".to_string(), "r2".to_string(), "f".to_string()];
    from_v2(
        &v2(),
        &Env {
            shipped: &ship,
            rest: &rest,
            live: &|sid| sid == "live1",
            title: &|sid| (sid == "live1").then(|| "修登录".to_string()),
            local_tz: local,
        },
    )
    .expect("读得懂")
}

fn file<'a>(u: &'a Upgraded, id: &str) -> &'a str {
    &u.files
        .iter()
        .find(|f| f.0 == id)
        .unwrap_or_else(|| {
            panic!(
                "没写 {id}：{:?}",
                u.files.iter().map(|f| &f.0).collect::<Vec<_>>()
            )
        })
        .1
}

fn head(src: &str) -> script::Head {
    script::parse(src).expect("新文件读得懂").head
}

#[test]
fn u1_a_config_equal_to_a_shipped_one_points_to_it() {
    let u = run(Some("Asia/Shanghai"));
    assert!(
        u.files.iter().all(|f| f.0 != "list-order"),
        "随包那份不另写"
    );
    assert_eq!(u.book.sessions["plain"].strategy.id, "list-order");
}

#[test]
fn u2_settings_come_out_in_the_shipped_shape() {
    let u = run(Some("Asia/Shanghai"));
    let id = &u.book.for_new;
    let h = head(file(&u, id));
    assert_eq!(h.name, script::Text::One("夜里省着用".into()));
    assert_eq!(h.tz.as_deref(), Some("Asia/Shanghai"));
    let all = |v: u64| json!([{"from": 0, "to": 1440, "v": v}]);
    assert_eq!(
        h.settings,
        json!({
          "caps": {"5h": all(90), "7d": all(90)},
          "capKind": "hard",
          "accounts": [
            {"id": "f", "back": 1,
             "caps": {"5h": [{"from": 0, "to": 1200, "v": 80}, {"from": 1200, "to": 1380, "v": 0}, {"from": 1380, "to": 1440, "v": 80}],
                      "7d": [{"from": 0, "to": 1200, "v": 100}, {"from": 1200, "to": 1380, "v": 0}, {"from": 1380, "to": 1440, "v": 100}]}},
            {"start": true, "back": 1},
            {"id": "r1", "back": 1},
            {"id": "r2", "back": 1},
            {"id": "s", "back": 1}
          ]
        })
    );
    // 代码照第三版的编号 2 那份（随包第二份）：代码一个字不动。
    let src = file(&u, id);
    let code = |s: &str| s[s.find("---*/").expect("头尾")..].to_string();
    assert_eq!(code(src), code(&shipped("reset-first")));
}

#[test]
fn u3_standby_wait_becomes_the_standby_rows_delay_only_for_the_first_strategy() {
    let mut v = v2();
    v["configs"]["c_00000002"]["strategy"] = json!("1");
    let ship = ship();
    let u = from_v2(
        &v,
        &Env {
            shipped: &ship,
            rest: &[],
            live: &|_| false,
            title: &|_| None,
            local_tz: None,
        },
    )
    .expect("读得懂");
    let h = head(file(&u, &u.book.for_new));
    let rows = h.settings["accounts"].as_array().expect("号列表").clone();
    let s = rows.iter().find(|r| r["id"] == "s").expect("备用号");
    assert_eq!(s["delay"], json!(30));
    assert!(rows
        .iter()
        .filter(|r| r["id"] != "s")
        .all(|r| r.get("delay").is_none()));
}

#[test]
fn u4_live_copies_become_named_files_and_ended_ones_are_dropped() {
    let u = run(Some("Asia/Shanghai"));
    let live = &u.book.sessions["live1"].strategy.id;
    assert_ne!(live, &u.book.for_new);
    assert_eq!(
        head(file(&u, live)).name,
        script::Text::One("夜里省着用 · 修登录".into())
    );
    assert_eq!(
        u.book.sessions["dead1"].strategy.id, u.book.for_new,
        "结束了的会话的副本丢掉，改指它那份配置"
    );
}

#[test]
fn u5_a_users_v3_script_is_set_aside_and_its_config_keeps_the_list_order_code() {
    let u = run(Some("Asia/Shanghai"));
    assert_eq!(u.old_scripts, vec!["c_mine".to_string()]);
    let f = u
        .files
        .iter()
        .find(|f| head(&f.1).name == script::Text::One("自己写的".into()))
        .expect("那份配置写成了文件");
    let code = |s: &str| s[s.find("---*/").expect("头尾")..].to_string();
    assert_eq!(code(&f.1), code(&shipped("list-order")));
}

#[test]
fn u6_a_missing_tz_takes_the_local_one_or_is_marked_unknown() {
    let mut v = v2();
    v["configs"]["c_00000002"]
        .as_object_mut()
        .expect("配置")
        .remove("tz");
    let ship = ship();
    let env = |tz| Env {
        shipped: &ship,
        rest: &[],
        live: &|_| false,
        title: &|_| None,
        local_tz: tz,
    };
    let u = from_v2(&v, &env(Some("Europe/Berlin"))).expect("读得懂");
    assert_eq!(
        head(file(&u, &u.book.for_new)).tz.as_deref(),
        Some("Europe/Berlin")
    );
    let u = from_v2(&v, &env(None)).expect("读得懂");
    let h = head(file(&u, &u.book.for_new));
    assert!(h.tz.is_none() && h.tz_unknown);
}

#[test]
fn u7_session_state_maps_over() {
    let u = run(Some("Asia/Shanghai"));
    let s = &u.book.sessions["plain"];
    assert_eq!(
        (s.start.as_str(), s.current.as_str(), s.since, s.seen),
        ("a", "b", 100, 200)
    );
    assert_eq!(s.temp, vec!["c".to_string()]);
    assert!(s.memo.is_none(), "第三版策略的备忘第五版读不懂，丢");
    assert_eq!(
        s.log,
        vec![
            book::Log {
                at: 110,
                from: "a".into(),
                to: "b".into(),
                cause: book::Cause::Order,
                why: None
            },
            book::Log {
                at: 120,
                from: "b".into(),
                to: "b".into(),
                cause: book::Cause::Hold { until: Some(999) },
                why: None
            },
            book::Log {
                at: 130,
                from: "b".into(),
                to: "c".into(),
                cause: book::Cause::Unusable,
                why: None
            },
            book::Log {
                at: 140,
                from: "c".into(),
                to: "b".into(),
                cause: book::Cause::UseNow,
                why: None
            },
        ]
    );
    assert_eq!(u.book.version, book::VERSION);
}

#[test]
fn u8_the_result_round_trips_and_names_no_shipped_strategy_in_code() {
    let u = run(Some("Asia/Shanghai"));
    let text = serde_json::to_string(&u.book).expect("写得出");
    let back: book::Book = serde_json::from_str(&text).expect("读得回");
    assert_eq!(back, u.book);
    let src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/accounts/quota/rules/upgrade.rs"),
    )
    .expect("源码");
    for n in [
        "list-order",
        "reset-first",
        "most-left",
        "照列表先后",
        "先用快重置的",
        "先用剩得多的",
    ] {
        assert!(!src.contains(n), "升级的代码里点了随包策略的名字 {n}");
    }
}
