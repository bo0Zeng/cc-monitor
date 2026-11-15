//! `rotation-try`：拿一份策略原文照给的事实判一次、往后推演，不写盘不换号；读不懂 ⇒ `bad_strategy`。

use super::*;

const NOW: u64 = 4_000_000_000;

fn a(id: &str, used: f64, rate: Option<f64>) -> Acct {
    Acct {
        agent: String::new(),
        id: id.into(),
        api: false,
        used: [used, 0.0],
        resets: [Some(NOW + 4 * 3600), None],
        refused_until: None,
        barred: None,
        rate: [rate, None],
        per_pct: [None, None],
        weekly: false,
        others: None,
        past: Default::default(),
    }
}

fn source() -> String {
    crate::accounts::quota::setup::samples::of(0).to_string()
}

fn settings() -> serde_json::Value {
    serde_json::json!({
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]},
        "accounts": [{"id": "work"}, {"id": "team"}, {"id": "lab"}]
    })
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
    let got = try_with(&req, "", &facts, NOW, &crate::common::time::Tz::default()).expect("ok");
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
        crate::accounts::quota::setup::script::parse(&src)
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
    let got = try_with(
        &req("team"),
        "",
        &facts,
        NOW,
        &crate::common::time::Tz::default(),
    )
    .expect("ok");
    assert_eq!(
        (got.decision.pick.as_deref(), got.decision.temp.clone()),
        (Some("team"), vec!["team".to_string()])
    );
    let got = try_with(
        &req("lab"),
        "",
        &facts,
        NOW,
        &crate::common::time::Tz::default(),
    )
    .expect("ok");
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
    let tz = crate::common::time::Tz::iana("Asia/Shanghai").expect("tz");
    let face = |t: u64| crate::common::time::hm(tz.local(i64::try_from(t).expect("i64")));
    let utc = |t: u64| {
        crate::common::time::hm(
            crate::common::time::Tz::default().local(i64::try_from(t).expect("i64")),
        )
    };
    let got = try_with(&req, "", &facts, NOW, &tz).expect("ok");
    let sw = got.timeline.switches.first().expect("换号");
    assert_eq!(sw.at_text, face(sw.at));
    assert_ne!(sw.at_text, utc(sw.at), "八个钟头差得出来");
    for t in &got.timeline.ticks {
        assert_eq!(t.text, face(t.at), "{:?}", got.timeline.ticks);
    }
    for l in &got.timeline.lanes {
        // 此刻那段剩多少 ＝ 100 − 两窗里用得多的（work 89 · team 10）。
        let left = if l.account == "work" { 11 } else { 90 };
        for (k, g) in l.segs.iter().enumerate() {
            let want = match (g.sendable, g.reset, k) {
                (false, ..) => String::new(),
                (true, true, _) => format!("↻{}", face(g.from)),
                (true, false, 0) => format!("剩 {left}%"),
                _ => String::new(),
            };
            assert_eq!(g.text, want, "{g:?}");
        }
    }
}

/// ★ 最上那条每段写的字由后端写好：用哪个号 ⇒ 它的号名；停发 ⇒ 「停下」那一句（界面不再自己挑）。
#[test]
fn each_use_carries_the_words_it_shows() {
    let req = RotationTryReq {
        source: source(),
        settings: Some(settings()),
        ..Default::default()
    };
    let facts = [a("work", 89.0, Some(0.5)), a("team", 10.0, None)];
    let got = try_with(&req, "", &facts, NOW, &crate::common::time::Tz::default()).expect("ok");
    assert!(!got.timeline.uses.is_empty());
    for u in &got.timeline.uses {
        let want = match &u.account {
            Some(a) => crate::accounts::quota::name_words::account_text_of("", a),
            None => copy_core::copy_text("rot.sim.hold", &[]),
        };
        assert_eq!(u.text, want, "{u:?}");
    }
    let held = try_with(
        &req,
        "",
        &[a("work", 100.0, None)],
        NOW,
        &crate::common::time::Tz::default(),
    )
    .expect("ok");
    let first = held.timeline.uses.first().expect("一段");
    assert_eq!(
        (first.account.as_deref(), first.text.as_str()),
        (None, copy_core::copy_text("rot.sim.hold", &[]).as_str()),
        "只有一个号且用满 ⇒ 停发"
    );
}

/// ★ 读不懂的原文 ⇒ `bad_strategy`，带第几行。
#[test]
fn an_unreadable_source_is_refused_with_its_line() {
    let req = RotationTryReq {
        source: "/*--- rotation\n{".into(),
        ..Default::default()
    };
    match try_with(&req, "", &[], NOW, &crate::common::time::Tz::default()) {
        Err(RotationTryErr::BadStrategy(s)) => assert!(s.contains("line"), "{s}"),
        other => panic!("{other:?}"),
    }
}

/// ★ 入参多一格 ⇒ 解不出。登记表里的码表（`bad_args` · `bad_strategy`）读命令表 ⇒ `on_upper_tests`。
#[test]
fn it_refuses_fields_it_does_not_use() {
    assert!(serde_json::from_value::<RotationTryReq>(
        serde_json::json!({"source": "", "sid": "x"})
    )
    .is_err());
}

/// ★ 试跑同真判：事实里本家的号在前、别家的号接在后；策略交了别家的号 ⇒ 当策略出错，照兜底用本家的号。
#[test]
fn a_trial_run_calls_out_picking_another_agents_account() {
    let req = RotationTryReq {
        source: "/*--- rotation\n{\"format\": 5, \"settings\": {\"accounts\": [{\"start\": true}]}}\n---*/\nfn decide(ctx) { #{ order: [\"x\"] } }".into(),
        ..Default::default()
    };
    let other = Acct {
        agent: "other-agent".into(),
        ..a("x", 10.0, None)
    };
    let mine = Acct {
        agent: "stand-in".into(),
        ..a("work", 10.0, None)
    };
    let got = try_with(
        &req,
        "stand-in",
        &[mine, other],
        NOW,
        &crate::common::time::Tz::default(),
    )
    .expect("ok");
    assert!(got.decision.strategy_error.is_some(), "{:?}", got.decision);
    assert_eq!(got.decision.pick.as_deref(), Some("work"));
}

/// 时间线上一段的号名照推演那条会话那一家问：账号 0 叫那一家默认家目录的短形：默认那一家有本机布局、写得出短形，替身家不在注册表里、说不出 ⇒ 空串；两家叫法不同，传错家就对不上。
#[test]
fn a_use_segment_names_its_account_after_the_sessions_family() {
    use crate::accounts::quota::name_words::account_text_of;
    let (default, fake) = (
        crate::agents::default_kind(),
        crate::agents::fake::AGENT_KIND,
    );
    assert_ne!(
        account_text_of(default, "0"),
        account_text_of(fake, "0"),
        "两家账号 0 的叫法一样 ⇒ 下面两条分不出是不是照家问的"
    );
    assert_eq!(use_text(default, Some("0")), account_text_of(default, "0"));
    assert_eq!(use_text(fake, Some("0")), account_text_of(fake, "0"));
}

/// 试跑那一下交进来的家一路带到时间线：推演那条会话是替身家的、在用账号 0 ⇒ 那一段叫替身家账号 0 的名字，不是默认那一家的。
#[test]
fn a_trial_names_its_segments_after_the_family_it_was_asked_for() {
    use crate::accounts::quota::name_words::account_text_of;
    let fake = crate::agents::fake::AGENT_KIND;
    assert_ne!(
        account_text_of(fake, "0"),
        account_text_of(crate::agents::default_kind(), "0")
    );
    let req = RotationTryReq {
        source: source(),
        settings: Some(serde_json::json!({
            "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]},
            "accounts": [{"id": "0"}]
        })),
        ..Default::default()
    };
    let facts = [Acct {
        agent: fake.into(),
        ..a("0", 10.0, None)
    }];
    let got = try_with(&req, fake, &facts, NOW, &crate::common::time::Tz::default()).expect("ok");
    let zero: Vec<_> = got
        .timeline
        .uses
        .iter()
        .filter(|u| u.account.as_deref() == Some("0"))
        .collect();
    assert!(
        !zero.is_empty(),
        "推演里没用到账号 0：{:?}",
        got.timeline.uses
    );
    for u in zero {
        assert_eq!(u.text, account_text_of(fake, "0"), "{u:?}");
    }
}
