//! 轮换的存取：池怎么算 · 整份收、不合法说哪一格 · 新勾的按量号放末尾 · 钉号与记录 · 落盘与重读。

use super::*;
use serde_json::json;

fn rot(
    order: serde_json::Value,
    enabled: serde_json::Value,
    when: serde_json::Value,
) -> serde_json::Value {
    json!({"order": order, "enabled": enabled, "when": when})
}

fn none(_: &str) -> bool {
    false
}

fn ok(a: &str) -> bool {
    !a.is_empty() && a != "_"
}

/// 池：占位换成起始账号、只取勾上的、去重（下面再勾到起始账号时跳过）。缺省只有起始账号。
#[test]
fn the_pool_starts_from_the_start_slot_and_takes_only_enabled_accounts() {
    let r = rotation_from(
        &rot(
            json!([{"start": true}, "b", "a", "c"]),
            json!(["a", "c"]),
            json!("full"),
        ),
        1..=1,
        &ok,
        &none,
        None,
    )
    .expect("ok");
    assert_eq!(r.pool("a"), ["a", "c"]);
    assert_eq!(r.pool("q"), ["q", "a", "c"]);
    assert_eq!(Rotation::default().pool("q"), ["q"]);
}

/// ★ 不合法整份拒、说哪一格；合法的读回与写进去的一样（JSON 往返）。
#[test]
fn a_bad_rotation_is_refused_whole_naming_the_cell() {
    let cases = [
        (
            rot(json!([{"start": true}, "a", "a"]), json!([]), json!("full")),
            "order[2]",
        ),
        (
            rot(json!([{"start": true}, "_"]), json!([]), json!("full")),
            "order[1]",
        ),
        (
            rot(json!(["a"]), json!([]), json!("full")),
            "`order` must hold",
        ),
        (
            rot(json!([{"start": true}, "a"]), json!(["b"]), json!("full")),
            "enabled[0]",
        ),
        (
            rot(
                json!([{"start": true}]),
                json!([]),
                json!({"threshold": {"n": 0}}),
            ),
            "when.threshold.n",
        ),
        (
            rot(
                json!([{"start": true}]),
                json!([]),
                json!({"threshold": {"n": 100}}),
            ),
            "when.threshold.n",
        ),
        (
            rot(json!([{"start": true}]), json!([]), json!("soon")),
            "`when`",
        ),
        (
            json!({"order": [{"start": true}], "enabled": [], "when": "full", "x": 1}),
            "unknown field `x`",
        ),
        (
            json!({"order": [{"start": true}], "enabled": [], "when": "full", "atLimit": "halt"}),
            "`atLimit`",
        ),
        (
            json!({"order": [{"start": true}], "enabled": [], "when": "full", "atLimit": null}),
            "`atLimit`",
        ),
    ];
    for (v, cell) in cases {
        let e = rotation_from(&v, 1..=1, &ok, &none, None).expect_err("应拒");
        assert!(e.contains(cell), "{e} 应点名 {cell}");
    }
    let good = rot(
        json!([{"start": true}, "a"]),
        json!(["a"]),
        json!({"threshold": {"n": 90}}),
    );
    let r = rotation_from(&good, 1..=1, &ok, &none, None).expect("ok");
    // 缺 `atLimit` ⇒ `continue`、缺 `wait` ⇒ 10，读回时照写出来。
    let mut back = good.clone();
    back["atLimit"] = json!("continue");
    back["wait"] = json!(10);
    assert_eq!(serde_json::to_value(&r).expect("json"), back);
    let mut stop = good.clone();
    stop["atLimit"] = json!("stop");
    stop["wait"] = json!(0);
    let r = rotation_from(&stop, 1..=1, &ok, &none, None).expect("ok");
    assert_eq!(r.at_limit, AtLimit::Stop);
    assert_eq!(r.wait, 0, "0 ＝ 不等");
    assert_eq!(serde_json::to_value(&r).expect("json"), stop);
    // 盘上旧的那一份（没有这一格）读得进来、当 `continue`。
    let old: Rotation = serde_json::from_value(good.clone()).expect("旧盘上形状");
    assert_eq!(old.at_limit, AtLimit::Continue);
    assert!(rotation_from(
        &rot(json!(["a"]), json!(["a"]), json!("full")),
        0..=1,
        &ok,
        &none,
        None
    )
    .is_ok());
}

/// ★ 新勾的按量号由后端挪到 `order` 末尾；已勾着的、订阅号原位不动。
#[test]
fn a_newly_enabled_api_account_moves_to_the_end() {
    let api = |a: &str| a.starts_with("api");
    let first = rotation_from(
        &rot(
            json!([{"start": true}, "api1", "a", "b"]),
            json!(["a"]),
            json!("full"),
        ),
        1..=1,
        &ok,
        &api,
        None,
    )
    .expect("ok");
    let r = rotation_from(
        &rot(
            json!([{"start": true}, "api1", "a", "b"]),
            json!(["a", "api1", "b"]),
            json!("full"),
        ),
        1..=1,
        &ok,
        &api,
        Some(&first),
    )
    .expect("ok");
    assert_eq!(
        serde_json::to_value(&r.order).expect("json"),
        json!([{"start": true}, "a", "b", "api1"])
    );
    let again = rotation_from(
        &rot(
            json!([{"start": true}, "api1", "a", "b"]),
            json!(["a", "api1", "b"]),
            json!("full"),
        ),
        1..=1,
        &ok,
        &api,
        Some(&r),
    )
    .expect("ok");
    assert_eq!(again.order[1], RotationSlot::Named("api1".into()));
}

fn rec(at: u64, from: &str, to: &str, why: SwitchWhy) -> SwitchRecord {
    SwitchRecord {
        at_text: None,
        from_resets_at_text: None,
        at,
        from: from.into(),
        to: to.into(),
        why,
        from_resets_at: None,
    }
}

/// 钉号：此刻的号与起算时刻跟着变；没换成的那几句自上一次真换号以来只记一次；换了起它的号 ⇒ 钉号清掉。
#[test]
fn pinning_and_relaunching_move_the_current_account() {
    let mut b = Book::default();
    assert!(b.saw("s", "claude-code", "a", 10));
    assert!(!b.saw("s", "claude-code", "a", 11));
    let skipped = [("x".to_string(), Unready::NeedsLogin)];
    b.pin(
        "s",
        rec(20, "a", "b", SwitchWhy::Full { w: None }),
        &skipped,
    );
    let s = &b.sessions["s"];
    assert_eq!((s.current.as_str(), s.since, s.history.len()), ("b", 20, 2));
    assert!(b.note_stuck("s", rec(30, "b", "b", SwitchWhy::ToOverage), &[]));
    assert!(!b.note_stuck("s", rec(31, "b", "b", SwitchWhy::ToOverage), &[]));
    assert!(!b.note_stuck("s", rec(32, "b", "b", SwitchWhy::Full { w: None }), &[]));
    assert_eq!(b.sessions["s"].history.len(), 3);
    assert!(b.saw("s", "claude-code", "c", 40));
    let s = &b.sessions["s"];
    assert_eq!(
        (s.start.as_str(), s.current.as_str(), s.since),
        ("c", "c", 40)
    );
}

/// 落盘：写进去、另一份 `Store` 读得回（后端重启后还在）；改到的会话响一下；读不懂的那份不覆盖。
#[test]
fn the_book_survives_a_restart_and_rings_for_the_sessions_it_changed() {
    let d = std::env::temp_dir().join(format!("ccm-rotation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    let path = d.join(FILE_NAME);
    let mut rx = changes().subscribe();
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| b.saw("s-ring", "claude-code", "a", 1))
        .expect("write");
    assert_eq!(drain(&mut rx, "s-ring"), vec!["s-ring".to_string()]);
    st.change(|b| b.saw("s-ring", "claude-code", "a", 2))
        .expect("write");
    assert!(drain(&mut rx, "s-ring").is_empty(), "没改就不响");
    let again = RotationStore::at(Some(path.clone()));
    assert_eq!(again.now().sessions["s-ring"].start, "a");
    std::fs::write(&path, "{not json").expect("write");
    assert!(again.change(|b| b.saw("t", "claude-code", "a", 3)).is_err());
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "{not json");
    assert_eq!(again.now(), Book::default());
    let _ = std::fs::remove_dir_all(&d);
}

// ── 积木那几格：每号上限 · 单段预算 · 切回 ───────────────────────────────────────

fn with(extra: serde_json::Value) -> serde_json::Value {
    let mut v = rot(
        json!([{"start": true}, "z", "q", "b"]),
        json!(["z", "q", "b"]),
        json!({"threshold": {"n": 90}}),
    );
    for (k, x) in extra.as_object().expect("obj") {
        v[k] = x.clone();
    }
    v
}

/// ★ 三格各只一种形状：`cap: {号: {窗口键|"*": n | [{at, n}]}}` · `stint: {号: {窗口键|"*": n}}` · `preempt: 布尔`；
/// 读回逐字同形（缺省那几格不写出）；阈值 1..=99。
#[test]
fn the_new_cells_have_one_shape_each_and_read_back_as_written() {
    let v = with(json!({
        "atLimit": "continue",
        "cap": {"q": {"*": [{"at": "01:00-20:00", "n": 99}]}, "z": {"5h": 90, "7d:opus": 95}},
        "stint": {"b": {"5h": 5}},
        "preempt": true,
        "wait": 10,
    }));
    let r = rotation_from(&v, 1..=1, &ok, &none, None).expect("ok");
    assert_eq!(serde_json::to_value(&r).expect("json"), v);
    assert!(r.preempt);
    assert_eq!(r.stint["b"]["5h"], 5);
    assert_eq!(
        r.cap["q"]["*"],
        CapValue::Slots(vec![CapSlot {
            at: "01:00-20:00".into(),
            n: 99
        }])
    );
    // 缺省（没有这三格）⇒ 读回也没有：今天的配置读进来、写回去一个字节不变（`wait` 恒写出）。
    let plain = with(json!({"atLimit": "continue", "wait": 10}));
    let r = rotation_from(&plain, 1..=1, &ok, &none, None).expect("ok");
    assert_eq!(serde_json::to_value(&r).expect("json"), plain);
    let off =
        with(json!({"atLimit": "continue", "preempt": false, "cap": {}, "stint": {}, "wait": 10}));
    assert_eq!(
        serde_json::to_value(rotation_from(&off, 1..=1, &ok, &none, None).expect("ok"))
            .expect("json"),
        plain
    );
    assert!(rotation_from(
        &with(json!({"when": {"threshold": {"n": 1}}})),
        1..=1,
        &ok,
        &none,
        None
    )
    .is_ok());
}

/// ★ 不合法 ⇒ 点名那一格、整份拒（别的形状一律不收）。
#[test]
fn a_bad_new_cell_is_refused_naming_it() {
    let cases = [
        (json!({"cap": [90]}), "`cap`"),
        (json!({"cap": {"_": {"5h": 90}}}), "`cap._`"),
        (json!({"cap": {"q": 90}}), "`cap.q`"),
        (json!({"cap": {"q": {}}}), "`cap.q`"),
        (json!({"cap": {"q": {"5 h": 90}}}), "`cap.q.5 h`"),
        (json!({"cap": {"q": {"5h": 100}}}), "`cap.q.5h`"),
        (json!({"cap": {"q": {"5h": 100}}}), "`cap.q.5h`"),
        (
            json!({"cap": {"q": {"*": [{"at": "01:00-20:00", "n": -1}]}}}),
            "`cap.q.*[0].n`",
        ),
        (json!({"cap": {"q": {"5h": []}}}), "`cap.q.5h`"),
        (
            json!({"cap": {"q": {"*": [{"at": "1:00-20:00", "n": 99}]}}}),
            "`cap.q.*[0].at`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "08:00-08:00", "n": 99}]}}}),
            "`cap.q.*[0].at`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "24:00-02:00", "n": 99}]}}}),
            "`cap.q.*[0].at`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "01:00-20:60", "n": 99}]}}}),
            "`cap.q.*[0].at`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "01:00-20:00", "n": 100}]}}}),
            "`cap.q.*[0].n`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "01:00-20:00"}]}}}),
            "`cap.q.*[0]`",
        ),
        (
            json!({"cap": {"q": {"*": [{"at": "01:00-20:00", "n": 9, "x": 1}]}}}),
            "`cap.q.*[0]`",
        ),
        (json!({"stint": {"b": 5}}), "`stint.b`"),
        (
            json!({"stint": {"b": {"5h": [{"at": "01:00-02:00", "n": 5}]}}}),
            "`stint.b.5h`",
        ),
        (json!({"stint": {"b": {"5h": 0}}}), "`stint.b.5h`"),
        (json!({"preempt": "yes"}), "`preempt`"),
        (json!({"spare": ["b"]}), "unknown field `spare`"),
    ];
    for (extra, cell) in cases {
        let e = rotation_from(&with(extra.clone()), 1..=1, &ok, &none, None)
            .expect_err(&extra.to_string());
        assert!(e.contains(cell), "{extra} ⇒ {e}，应点名 {cell}");
    }
}

/// ★ 上限可以取 0（不用这个号）：写死的 · 按时段的都收、读回同形；单段预算仍不收 0（上面那条拒）。
#[test]
fn a_cap_of_zero_is_taken_and_reads_back_the_same() {
    let ok = |a: &str| a != "_";
    let none = |_: &str| false;
    let with = |extra: serde_json::Value| {
        let mut v = json!({"order": [{"start": true}, "z", "q", "b"], "enabled": ["z", "q", "b"], "when": {"threshold": {"n": 90}}});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let cap = json!({"q": {"*": [{"at": "17:00-02:00", "n": 0}, {"at": "02:00-17:00", "n": 99}]}, "b": {"5h": 0}});
    let r = rotation_from(&with(json!({ "cap": cap })), 1..=1, &ok, &none, None).expect("收 0");
    assert_eq!(serde_json::to_value(&r).unwrap()["cap"], cap);
}

/// 时段：含起不含止；跨午夜；止可写 24:00（到当天结束）。
#[test]
fn a_slot_holds_from_its_start_until_just_before_its_end() {
    let s = |at: &str| CapSlot {
        at: at.into(),
        n: 99,
    };
    let m = |h: u16, mm: u16| h * 60 + mm;
    assert!(s("01:00-20:00").holds(m(1, 0)));
    assert!(s("01:00-20:00").holds(m(19, 59)));
    assert!(!s("01:00-20:00").holds(m(20, 0)));
    assert!(!s("01:00-20:00").holds(m(0, 59)));
    assert!(s("22:00-06:00").holds(m(23, 0)));
    assert!(s("22:00-06:00").holds(m(0, 0)));
    assert!(s("22:00-06:00").holds(m(5, 59)));
    assert!(!s("22:00-06:00").holds(m(6, 0)));
    assert!(!s("22:00-06:00").holds(m(21, 59)));
    assert!(s("00:00-24:00").holds(m(23, 59)));
    assert!(s("18:00-24:00").holds(m(23, 59)));
    assert!(!s("18:00-24:00").holds(m(0, 0)));
}

/// 这一段的基线：换号 · 换了起它的号就清掉；补的时候只补还没记的窗口（换进来时记下的那一份不被后来的数盖掉）。
#[test]
fn the_baseline_resets_on_a_switch_and_only_fills_what_is_missing() {
    let base = |used: f64| Base {
        used,
        resets_at: Some(99),
    };
    let mut b = Book::default();
    b.saw("s", "claude-code", "a", 10);
    assert!(b.rebase("s", &[("5h".to_string(), base(0.3))].into()));
    assert!(
        !b.rebase("s", &[("5h".to_string(), base(0.5))].into()),
        "已记的不盖"
    );
    assert!(b.rebase(
        "s",
        &[("5h".to_string(), base(0.5)), ("7d".to_string(), base(0.1))].into()
    ));
    assert_eq!(b.sessions["s"].baseline["5h"].used, 0.3);
    assert_eq!(b.sessions["s"].baseline.len(), 2);
    assert!(b.block_above("s", &["x".to_string()]));
    b.pin("s", rec(20, "a", "b", SwitchWhy::Preempt), &[]);
    assert!(b.sessions["s"].baseline.is_empty(), "换了号 ⇒ 清掉重记");
    assert!(
        b.sessions["s"].blocked_above.is_empty(),
        "挡在前面的也清掉重记"
    );
    b.rebase("s", &[("5h".to_string(), base(0.2))].into());
    b.saw("s", "claude-code", "c", 30);
    assert!(b.sessions["s"].baseline.is_empty(), "换了起它的号 ⇒ 清掉");
}

// ── 别的进程写了盘（命令行 · quota-warm · AI 照 skill 调）⇒ 本进程照样推 ─────────────────────

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

fn sandbox(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-rot-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    d.join(FILE_NAME)
}

/// 盘上那一份被别处改了 ⇒ 重扫时比对上一份、改到的会话各响一下；本进程自己写的那一下不再响第二次；没变不响。
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
    // 另一个进程：不经本进程的任何一份 Store，直接整份写盘。
    let mut b = match read_at(&path) {
        Read::Present(b) => b,
        _ => panic!("读不回"),
    };
    b.sessions.get_mut("a1-flip").expect("flip").source = Source::Custom;
    b.saw("a1-new", "claude-code", "a", 2);
    crate::common::own_state::write_json(&path, &b).expect("write");
    rescan(&path);
    assert_eq!(
        drain(&mut rx, "a1-"),
        vec!["a1-flip".to_string(), "a1-new".to_string()]
    );
    rescan(&path);
    assert!(drain(&mut rx, "a1-").is_empty(), "没变不响");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// 盯着那个目录：别处一写，不用谁来问，本进程就重扫（推哪几个会话由上一条钉；这里看重扫读到了别处写的那一份 ——
/// 广播通道是全进程共用的，并行的别的判据一多就会挤掉，不拿它当观测口）。
#[test]
fn the_watcher_rescans_a_foreign_write_without_being_asked() {
    let path = sandbox("a1w");
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| b.saw("a1w-s", "claude-code", "a", 1))
        .expect("write");
    let _w = watch(&path).expect("watch");
    let mut b = st.now();
    b.sessions.get_mut("a1w-s").expect("s").source = Source::Custom;
    crate::common::own_state::write_json(&path, &b).expect("write");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let seen_custom = || {
        let g = seen().lock().unwrap_or_else(|e| e.into_inner());
        g.get(&path)
            .is_some_and(|(_, b)| b.sessions["a1w-s"].source == Source::Custom)
    };
    while !seen_custom() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(seen_custom(), "盯盘线程没重扫到别处写的那一份");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// ★ 升级：旧盘上的 `default`（一份轮换）与会话的 `follow` ⇒ 一条名为「默认」的规则、设为默认；`follow: true` ⇒ 跟随、
/// `false` ⇒ 本会话；写回去只有今天的格。
#[test]
fn an_old_book_upgrades_once_into_rules() {
    let old = json!({
        "default": {"order": [{"start": true}, "b"], "enabled": ["b"], "when": "full"},
        "sessions": {
            "s-f": {"agent": "claude-code", "start": "a", "current": "a", "since": 1, "follow": true},
            "s-c": {"agent": "claude-code", "start": "a", "current": "a", "since": 1, "follow": false,
                    "custom": {"order": ["c"], "enabled": ["c"], "when": "full"}}
        }
    });
    let b: Book = serde_json::from_value(old).expect("读得进");
    assert_eq!(b.rules.len(), 1);
    let rule = &b.rules[&b.default_rule];
    assert_eq!(rule.name, copy_text("beRotation.rule.defaultName", &[]));
    assert_eq!(rule.rotation.enabled, ["b"]);
    assert_eq!(b.sessions["s-f"].source, Source::Follow);
    assert_eq!(b.sessions["s-c"].source, Source::Custom);
    assert_eq!(b.rotation_of(&b.sessions["s-c"]).enabled, ["c"]);
    let out = serde_json::to_value(&b).expect("json");
    assert!(out.get("default").is_none(), "不留旧格");
    assert!(out["sessions"]["s-f"].get("follow").is_none());
    assert_eq!(out["sessions"]["s-f"]["source"], "follow");
    let again: Book = serde_json::from_value(out).expect("再读");
    assert_eq!(again, b, "升级一次成形，再读不变");
}

/// ★ 链接也响：改了一条规则 ⇒ 用它的会话（与跟随它的）各响一下、规则那条通道也响；别的会话不响。
#[test]
fn editing_a_rule_rings_its_sessions_and_the_rules_bell() {
    let path = sandbox("ring-rule");
    let st = RotationStore::at(Some(path.clone()));
    st.change(|b| {
        b.rules.insert(
            "r_aaaaaaaa".into(),
            Rule {
                name: "x".into(),
                rotation: Rotation::default(),
                rev: 1,
                updated_at: 0,
            },
        );
        b.saw("rr-on", "claude-code", "a", 1);
        b.saw("rr-follow", "claude-code", "a", 1);
        b.saw("rr-own", "claude-code", "a", 1);
        b.sessions.get_mut("rr-on").expect("s").source = Source::Rule("r_aaaaaaaa".into());
        b.sessions.get_mut("rr-own").expect("s").source = Source::Custom;
        b.sessions.get_mut("rr-own").expect("s").custom = Some(Rotation::default());
    })
    .expect("write");
    let mut rx = changes().subscribe();
    let mut bell = rules_changes().subscribe();
    st.change(|b| b.rules.get_mut("r_aaaaaaaa").expect("r").rotation.preempt = true)
        .expect("write");
    assert_eq!(drain(&mut rx, "rr-"), vec!["rr-on".to_string()]);
    assert!(bell.try_recv().is_ok(), "规则表变了 ⇒ 规则那条响");
    let def = st.now().default_rule;
    st.change(|b| b.rules.get_mut(&def).expect("def").rotation.preempt = true)
        .expect("write");
    assert_eq!(drain(&mut rx, "rr-"), vec!["rr-follow".to_string()]);
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
