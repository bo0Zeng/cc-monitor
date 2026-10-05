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
                json!({"threshold": {"n": 49}}),
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
    assert_eq!(serde_json::to_value(&r).expect("json"), good);
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
    assert_eq!(rx.try_recv().ok().as_deref(), Some("s-ring"));
    st.change(|b| b.saw("s-ring", "claude-code", "a", 2))
        .expect("write");
    assert!(rx.try_recv().is_err(), "没改就不响");
    let again = RotationStore::at(Some(path.clone()));
    assert_eq!(again.now().sessions["s-ring"].start, "a");
    std::fs::write(&path, "{not json").expect("write");
    assert!(again.change(|b| b.saw("t", "claude-code", "a", 3)).is_err());
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "{not json");
    assert_eq!(again.now(), Book::default());
    let _ = std::fs::remove_dir_all(&d);
}
