//! `rotation-try`：拿一份策略原文照给的事实判一次、往后推演，不写盘不换号；读不懂 ⇒ `bad_strategy`。

use super::*;

const NOW: u64 = 4_000_000_000;

fn a(id: &str, used: f64, rate: Option<f64>) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used, 0.0],
        resets: [Some(NOW + 4 * 3600), None],
        refused_until: None,
        barred: None,
        rate: [rate, None],
    }
}

fn source() -> String {
    crate::accounts::quota::rules::upgrade::material(0).to_string()
}

fn settings() -> serde_json::Value {
    (serde_json::json!({
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]},
        "accounts": [{"id": "work"}, {"id": "team"}, {"id": "lab"}]
    }))
}

/// ★ 在用的 work 2 分钟后到上限 ⇒ 这一判留着，推演里约 2 分钟后换到 team；每个号一行；没写盘。
#[test]
fn it_decides_and_simulates_from_the_given_facts() {
    let req = RotationTryReq {
        source: source(),
        settings: Some(settings()),
        ..Default::default()
    };
    let facts = [
        a("work", 89.0, Some(0.5)),
        a("team", 10.0, None),
        a("lab", 95.0, None),
    ];
    let got = try_with(&req, &facts, NOW, &crate::Tz::default()).expect("ok");
    assert_eq!(got.decision.pick.as_deref(), Some("work"));
    assert_eq!(
        got.decision.order,
        ["work", "team"],
        "lab 到了上限，策略不放它"
    );
    let first_switch = got
        .timeline
        .uses
        .iter()
        .find(|u| u.account.as_deref() == Some("team"))
        .expect("换到 team");
    assert_eq!(first_switch.from, NOW + 120, "{:?}", got.timeline.uses);
    assert_eq!(
        got.timeline
            .switches
            .first()
            .map(|s| (s.at, s.account.as_str())),
        Some((NOW + 120, "team"))
    );
    assert_eq!(
        got.timeline
            .lanes
            .iter()
            .map(|l| l.account.as_str())
            .collect::<Vec<_>>(),
        ["work", "team", "lab"]
    );
    assert!(
        got.timeline.lanes[2].segs[0].sendable,
        "lab 只是到了策略的上限，事实上发得出去"
    );
    assert_eq!(got.use_now_refused, None);
    assert!(
        got.description.starts_with("换号时按账号列表顺序"),
        "说明照原文开头的注释：{}",
        got.description
    );
    let src = got.source.expect("给了设置 ⇒ 回换了头的原文");
    assert_eq!(
        crate::accounts::quota::rules::script::parse(&src)
            .expect("读得回")
            .head
            .settings,
        settings()
    );
}

/// ★ 试跑「现在用」：事实上发得出去 ⇒ 换过去、进拖出来的顺序最前（策略的上限不拦）；发不出去 ⇒ 不换、回几点恢复。
#[test]
fn use_now_switches_or_says_when() {
    let facts = [
        a("work", 50.0, Some(0.5)),
        a("team", 10.0, None),
        a("lab", 100.0, None),
    ];
    let req = |who: &str| RotationTryReq {
        source: source(),
        settings: Some(settings()),
        use_now: Some(who.into()),
        ..Default::default()
    };
    let got = try_with(&req("team"), &facts, NOW, &crate::Tz::default()).expect("ok");
    assert_eq!(
        (got.decision.pick.as_deref(), got.decision.temp.clone()),
        (Some("team"), vec!["team".to_string()])
    );
    let got = try_with(&req("lab"), &facts, NOW, &crate::Tz::default()).expect("ok");
    assert_eq!(
        got.decision.pick.as_deref(),
        Some("work"),
        "lab 用到 100 ⇒ 发不出去，不换"
    );
    assert_eq!(got.use_now_refused, Some(Some(NOW + 4 * 3600)));
}

/// ★ 时刻的字（刻度 · 换号时刻 · 每段起点）照看的那一台的时区写，不按 UTC。
#[test]
fn the_clock_words_follow_the_viewing_machine() {
    let req = RotationTryReq {
        source: source(),
        settings: Some(settings()),
        ..Default::default()
    };
    let facts = [a("work", 89.0, Some(0.5)), a("team", 10.0, None)];
    let tz = crate::Tz::iana("Asia/Shanghai").expect("tz");
    let face = |t: u64| crate::common::time::hm(tz.local(i64::try_from(t).expect("i64")));
    let utc = |t: u64| {
        crate::common::time::hm(crate::Tz::default().local(i64::try_from(t).expect("i64")))
    };
    let got = try_with(&req, &facts, NOW, &tz).expect("ok");
    let sw = got.timeline.switches.first().expect("换号");
    assert_eq!(sw.at_text, face(sw.at));
    assert_ne!(sw.at_text, utc(sw.at), "八个钟头差得出来");
    for t in &got.timeline.ticks {
        assert_eq!(t.text, face(t.at), "{:?}", got.timeline.ticks);
    }
    for l in &got.timeline.lanes {
        for g in &l.segs {
            assert_eq!(g.from_text, face(g.from));
        }
    }
}

/// ★ 读不懂的原文 ⇒ `bad_strategy`，带第几行。
#[test]
fn an_unreadable_source_is_refused_with_its_line() {
    let req = RotationTryReq {
        source: "/*--- rotation\n{".into(),
        ..Default::default()
    };
    match try_with(&req, &[], NOW, &crate::Tz::default()) {
        Err(RotationTryErr::BadStrategy(s)) => assert!(s.contains("line"), "{s}"),
        other => panic!("{other:?}"),
    }
}

/// ★ 登记成 typed：码表 `bad_args` · `bad_strategy`；入参多一格 ⇒ 解不出。
#[test]
fn it_is_registered_typed() {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "rotation-try")
        .expect("登记了");
    assert!(matches!(
        spec.run,
        crate::stream::inbound::spec::Run::Typed(_)
    ));
    assert_eq!(spec.codes, ["bad_args", "bad_strategy"]);
    assert!(serde_json::from_value::<RotationTryReq>(
        serde_json::json!({"source": "", "sid": "x"})
    )
    .is_err());
}
