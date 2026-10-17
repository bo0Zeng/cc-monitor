//! 第二版 → 第三版：合成的第二版样本逐字段对期望。

use super::*;
use crate::accounts::quota::setup::script;
use serde_json::{json, Value};

/// 升级材料（第二版的编号 1 · 2 · 3）。
fn material(n: usize) -> &'static str {
    MATERIALS[n - 1]
}

fn code(s: &str) -> String {
    s[s.find("---*/").expect("头尾")..].to_string()
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
    let rest = vec!["r1".to_string(), "r2".to_string(), "f".to_string()];
    from_v2(
        &v2(),
        &Env {
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

/// ★ 每份配置都写成用户自己的策略文件（不跟谁比、不指过去）；新会话照旧不轮换。
#[test]
fn u1_every_config_becomes_a_users_own_file_and_new_sessions_do_not_rotate() {
    let u = run(Some("Asia/Shanghai"));
    for id in ["默认配置 1", "夜里省着用", "自己写的"] {
        file(&u, id);
    }
    assert_eq!(used(&u, "plain"), "默认配置 1", "文件名照配置名（＝ 标题）");
}

/// 那个会话载入的那份（id）。
fn used(u: &Upgraded, sid: &str) -> String {
    u.book.sessions[sid]
        .uses
        .as_ref()
        .expect("会话带着副本")
        .id
        .clone()
}

/// ★ 设置一格不丢：线 · 时段 ⇒ 分段上限；都到上限就停 ⇒ 硬上限；优先 ⇒ 号列表最前、备用 ⇒ 耗尽后启用（不出排）；
/// 不强制切换、权重容差 0（第二版里同一组不抢）。
#[test]
fn u2_settings_come_out_without_rows() {
    let u = run(Some("Asia/Shanghai"));
    let h = head(file(&u, "夜里省着用"));
    assert_eq!(h.tz.as_deref(), Some("Asia/Shanghai"));
    let all = |v: u64| json!([{"from": 0, "to": 1440, "v": v}]);
    assert_eq!(
        h.settings,
        json!({
          "caps": {"5h": all(90), "7d": all(90)},
          "capKind": "hard",
          "close": 0,
          "force": null,
          "accounts": [
            {"id": "f",
             "caps": {"5h": [{"from": 0, "to": 1200, "v": 80}, {"from": 1200, "to": 1380, "v": 0}, {"from": 1380, "to": 1440, "v": 80}],
                      "7d": [{"from": 0, "to": 1200, "v": 100}, {"from": 1200, "to": 1380, "v": 0}, {"from": 1380, "to": 1440, "v": 100}]}},
            {"start": true},
            {"id": "r1"},
            {"id": "r2"},
            {"id": "s", "fallback": true}
          ]
        })
    );
    // 编号 2 ⇒ 第二份材料的代码，一个字不动；照声明校验收得下。
    let src = file(&u, "夜里省着用");
    assert_eq!(code(src), code(material(2)));
    let about = script::parse(src).expect("读").about;
    assert!(
        about.contains("权重") && !about.contains("90"),
        "说明是材料开头的注释、不写值：{about}"
    );
    assert_eq!(super::super::form::check(&h.form, &h.settings), []);
}

/// 编号 1 的「用备用号前先等」⇒ 耗尽后启用那几号的启用延迟；整份的延迟 ⇒ 没另设的耗尽后启用号（启用延迟只对它们生效）。
#[test]
fn u3_waits_become_start_delays_of_fallbacks() {
    let delays = |strategy: &str| {
        let mut v = v2();
        v["configs"]["c_00000002"]["strategy"] = json!(strategy);
        v["configs"]["c_00000002"]["params"]["delay"] = json!(5);
        let u = from_v2(
            &v,
            &Env {
                rest: &[],
                live: &|_| false,
                title: &|_| None,
                local_tz: None,
            },
        )
        .expect("读得懂");
        let src = file(&u, "夜里省着用").to_string();
        let rows = head(&src).settings["accounts"]
            .as_array()
            .expect("号列表")
            .clone();
        let delay = |id: &str| {
            rows.iter()
                .find(|r| r["id"] == id || (id == "start" && r["start"] == true))
                .expect("那一号")
                .get("delay")
                .cloned()
        };
        ((delay("s"), delay("start"), delay("f")), src)
    };
    let ((s, start, f), src) = delays("1");
    assert_eq!(s, Some(json!(30)), "编号 1：备用号照「先等」");
    assert_eq!((start, f), (None, None), "不是耗尽后启用的号没有启用延迟");
    assert_eq!(code(&src), code(material(1)));
    let ((s, _, _), src) = delays("2");
    assert_eq!(s, Some(json!(5)), "别的编号：备用号照整份的延迟");
    assert_eq!(code(&src), code(material(2)));
}

#[test]
fn u4_live_copies_become_named_files_and_ended_ones_are_dropped() {
    let u = run(Some("Asia/Shanghai"));
    assert_eq!(used(&u, "live1"), "夜里省着用 · 修登录");
    assert_eq!(
        used(&u, "dead1"),
        "夜里省着用",
        "结束了的会话的副本丢掉，改指它那份配置"
    );
}

#[test]
fn u5_a_users_v3_script_is_set_aside_and_its_config_takes_the_first_material() {
    let u = run(Some("Asia/Shanghai"));
    assert_eq!(u.old_scripts, vec!["c_mine".to_string()]);
    let f = u
        .files
        .iter()
        .find(|f| f.0 == "自己写的")
        .expect("那份配置写成了文件");
    assert_eq!(code(&f.1), code(material(1)));
}

#[test]
fn u6_a_missing_tz_takes_the_local_one_or_is_marked_unknown() {
    let mut v = v2();
    v["configs"]["c_00000002"]
        .as_object_mut()
        .expect("配置")
        .remove("tz");
    let env = |tz| Env {
        rest: &[],
        live: &|_| false,
        title: &|_| None,
        local_tz: tz,
    };
    let u = from_v2(&v, &env(Some("Europe/Berlin"))).expect("读得懂");
    assert_eq!(
        head(file(&u, "夜里省着用")).tz.as_deref(),
        Some("Europe/Berlin")
    );
    let u = from_v2(&v, &env(None)).expect("读得懂");
    let h = head(file(&u, "夜里省着用"));
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
    assert_eq!(u.book.version, book::BOOK_VERSION);
}

#[test]
fn u8_the_result_round_trips() {
    let u = run(Some("Asia/Shanghai"));
    let text = serde_json::to_string(&u.book).expect("写得出");
    let back: book::Book = serde_json::from_str(&text).expect("读得回");
    assert_eq!(back, u.book);
}

/// 用户那三个会话的样子（号名是合成的）：七个号、先用快要重置的，总体 90%，一个号单独 99%，一个号 17:00–02:00 到 80%、
/// 其余时候 99%，都到上限就停。两个会话带着副本、一个照配置。
fn ours() -> Value {
    let p = |line: u64| json!({"line": line, "slots": [], "mode": "normal"});
    let mut rows: Vec<Value> = (1..=5)
        .map(|i| row(json!({"id": format!("p{i}")}), p(90)))
        .collect();
    rows.push(row(
        json!({"id": "p6"}),
        json!({"line": 99, "slots": [{"at": "17:00-02:00", "line": 80}], "mode": "normal"}),
    ));
    rows.push(row(json!({"id": "p7"}), p(99)));
    let cfg = config(
        "七个号",
        "2",
        Value::Array(rows),
        json!({"back": false, "all_out": "stop"}),
        p(90),
    );
    json!({
      "version": 2,
      "defaultConfig": "c_7",
      "configs": {"c_7": cfg.clone()},
      "sessions": {"o1": session("c_7", Some(cfg.clone())), "o2": session("c_7", Some(cfg)), "o3": session("c_7", None)}
    })
}

/// ★ 用户那三个会话升级后设置照搬：总体 90% · 单独 99% · 分时段 80% / 99% · 硬上限 · 不强制切换；代码是「离重置近的权重高」那份。
/// 照它判：在用的号到上限 ⇒ 换到最快重置的；别的号快重置也不换掉没满的在用号；全到上限 ⇒ 停发。
#[test]
fn u9_our_three_sessions_keep_every_setting() {
    let u = from_v2(
        &ours(),
        &Env {
            rest: &[],
            live: &|_| true,
            title: &|sid| Some(format!("会话 {sid}")),
            local_tz: None,
        },
    )
    .expect("读得懂");
    let all = |v: u64| json!([{"from": 0, "to": 1440, "v": v}]);
    let night = json!([{"from": 0, "to": 120, "v": 80}, {"from": 120, "to": 1020, "v": 99}, {"from": 1020, "to": 1440, "v": 80}]);
    let want = json!({
      "caps": {"5h": all(90), "7d": all(90)},
      "capKind": "hard",
      "close": 0,
      "force": null,
      "accounts": [
        {"id": "p1"}, {"id": "p2"}, {"id": "p3"}, {"id": "p4"}, {"id": "p5"},
        {"id": "p6", "caps": {"5h": night, "7d": night}},
        {"id": "p7", "caps": {"5h": all(99), "7d": all(99)}}
      ]
    });
    let mut ids = Vec::new();
    for sid in ["o1", "o2", "o3"] {
        let id = used(&u, sid);
        let src = file(&u, &id);
        let h = head(src);
        assert_eq!(h.settings, want, "{sid} 的设置没照搬");
        assert_eq!(
            code(src),
            code(material(2)),
            "{sid} 的代码不是离重置近的权重高那份"
        );
        ids.push(id);
    }
    assert_eq!(
        ids,
        ["七个号 · 会话 o1", "七个号 · 会话 o2", "七个号"],
        "两个副本各写一份，文件名照「配置名 · 会话标题」"
    );

    use crate::accounts::quota::rules::decide::{turn, Pick, Sigma};
    use crate::accounts::quota::rules::Acct;
    let f = script::parse(file(&u, &ids[0])).expect("读得懂");
    let fp = super::super::adopted_of_file("", "", &f, None);
    let t: u64 = 1_800_000_000 / 86_400 * 86_400 + 12 * 3600;
    let a = |id: &str, used: f64, resets_in: Option<u64>| Acct {
        id: id.into(),
        api: false,
        used: [used, 0.0],
        resets: [resets_in.map(|m| t + m * 60), None],
        refused_until: None,
        barred: None,
        rate: [None, None],
        per_pct: [None, None],
    };
    let sig = |c: &str| Sigma {
        start: "p1".into(),
        current: c.into(),
        ..Sigma::default()
    };
    let pick =
        |facts: &[Acct], c: &str| turn(&sig(c), facts, &h_settings(&f), &fp, 720, t, &[]).pick;
    fn h_settings(f: &script::File) -> Value {
        f.head.settings.clone()
    }
    let mut facts: Vec<Acct> = (1..=7)
        .map(|i| a(&format!("p{i}"), 10.0, Some(200 + i * 10)))
        .collect();
    facts[3].resets[0] = Some(t + 30 * 60);
    assert_eq!(
        pick(&facts, "p1"),
        Pick::Use("p1".into()),
        "别的号快重置也不换掉没满的在用号"
    );
    facts[0].used[0] = 90.0;
    assert_eq!(
        pick(&facts, "p1"),
        Pick::Use("p4".into()),
        "到 90% ⇒ 换到最快重置的"
    );
    facts[6].used[0] = 95.0;
    for (i, x) in facts.iter_mut().enumerate() {
        if i != 6 {
            x.used[0] = 99.0;
        }
    }
    assert_eq!(
        pick(&facts, "p1"),
        Pick::Use("p7".into()),
        "单独 99% 的号 95% 还有额度"
    );
    facts[6].used[0] = 99.0;
    assert!(
        matches!(pick(&facts, "p7"), Pick::Hold { .. }),
        "硬上限全到 ⇒ 停发"
    );
}

/// ★ 文件名照配置名：收不下的字换 `-`；同一次升级里重名补「 2」；全是收不下的字 ⇒ 用配置的编号。
#[test]
fn u10_file_names_come_from_config_names() {
    let plain = json!({"line": null, "slots": [], "mode": "normal"});
    let c = |name: &str| {
        config(
            name,
            "1",
            json!([row(json!({"start": true}), plain.clone())]),
            json!({}),
            plain.clone(),
        )
    };
    let v = json!({"version": 2, "configs": {"c_1": c("夜里/省着用"), "c_2": c("夜里-省着用"), "c_3": c("///")}, "sessions": {}});
    let u = from_v2(
        &v,
        &Env {
            rest: &[],
            live: &|_| false,
            title: &|_| None,
            local_tz: None,
        },
    )
    .expect("读得懂");
    let ids: Vec<&str> = u.files.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(ids, ["夜里-省着用", "夜里-省着用 2", "c_3"]);
}
