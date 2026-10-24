//! 会话侧的临时动作（基础功能层）：拖出来的先后 · 先不用的号 · 现在用。临时家目录（同 `rotation_view_face` 的判据家）。

use super::*;
use crate::faces::rotation_view_face::tests::{now, seen, Home, S1};

fn temp(ctx: &Ctx, home: &Home, op: RotTempOp) -> Result<RotationTempReply, RotationTempErr> {
    temp_with(
        ctx,
        &home.own(),
        &RotationTempReq { sid: S1.into(), op },
        now(),
    )
}

/// ★ 放到第几位（越界取末尾、已在就挪）只是那一串上的事；不在这台账号里的号 ⇒ `not_listed`。
#[test]
fn a_name_is_put_at_its_place_in_the_dragged_order() {
    let mut t: Vec<String> = Vec::new();
    for (a, at, want) in [
        ("b", 0, vec!["b"]),
        ("c", 9, vec!["b", "c"]),
        ("c", 0, vec!["c", "b"]),
    ] {
        put_at(&mut t, a, at);
        assert_eq!(t, want);
    }
    let home = Home::new("temp-bad");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let place = RotTempOp::Place {
        account: "zz".into(),
        t: now() + 60,
    };
    assert!(matches!(
        temp(&ctx, &home, place),
        Err(RotationTempErr::NotListed(_))
    ));
}

/// ★ 拖号名到最上那条、松在时刻 `t` ⇒ 排到那一刻之后那一段（此刻那段是第 0 位，之后每段换号往后数一位）：
/// 段数照这个会话此刻的推演，由后端数（界面只交松手那一刻）；清掉 ⇒ 空。
#[test]
fn a_name_dropped_on_the_top_lane_goes_after_the_uses_before_that_moment() {
    let home = Home::new("temp");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let place = |account: &str, t: u64| {
        temp(
            &ctx,
            &home,
            RotTempOp::Place {
                account: account.into(),
                t,
            },
        )
        .expect("ok")
        .temp
    };
    let v = crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1);
    let uses = &v.timeline.as_ref().expect("推演").uses;
    let later = uses.iter().filter(|u| u.from > now()).count();
    assert_eq!(later, 0, "这一家推演里此刻之后不换号：{uses:?}");
    assert_eq!(place("b", now() + 60), ["b"]);
    assert_eq!(place("c", now() + 3600), ["b", "c"]);
    assert_eq!(place("b", now() + 60), ["c", "b"], "已在就挪到第 1 位");
    let cleared = temp(&ctx, &home, RotTempOp::Clear).expect("ok");
    assert!(cleared.temp.is_empty());
}

/// ★ 顺序里拖一行（第几格由后端照这个会话的顺序认）：松在第一格 ⇒ 现在用它；别处 ⇒ 「接下来先用谁」换成拖完那一串
/// （第二格起到挪动那一截的尽头）；没挪 ⇒ 不动；不在这台账号里 ⇒ `not_listed`。
#[test]
fn a_row_dragged_in_the_order_is_moved_there() {
    let home = Home::new("temp-move");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let mv = |account: &str, to: usize| {
        temp(
            &ctx,
            &home,
            RotTempOp::Move {
                account: account.into(),
                to,
            },
        )
    };
    let line = crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1).lineup;
    assert!(line.len() >= 3, "{line:?}");
    let x = line[2].clone();
    let moved = mv(&x, 1).expect("ok").temp;
    assert_eq!(moved, [x.clone(), line[1].clone()], "{line:?}");
    let line = crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1).lineup;
    let i = line.iter().position(|a| *a == x).expect("在");
    assert_eq!(mv(&x, i).expect("ok").temp, moved, "没挪 ⇒ 不动");
    assert!(matches!(mv("zz", 1), Err(RotationTempErr::NotListed(_))));
    mv("api", 0).expect("第一格 ⇒ 现在用");
    assert_eq!(ctx.hop.store.now().sessions[S1].current, "api");
}

/// ★ 「现在用」：事实上发得出去 ⇒ 当场换过去、放进拖出来的顺序最前、记一条；发不出去（被拒）⇒ 不换、说几点恢复。
/// 策略的上限不拦它（a 用到 85 也照换回 a）。
#[test]
fn use_now_is_stopped_only_by_facts() {
    let home = Home::new("usenow");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let back = now() + 600;
    seen(&ctx, "b", 0.0, Some(back));
    let go = |account: &str| {
        use_now_with(
            &ctx,
            &home.own(),
            &RotationUseNowReq {
                sid: S1.into(),
                account: account.into(),
            },
            now(),
        )
    };
    match go("b") {
        Err(RotationUseNowErr::Unusable(_, u)) => assert_eq!(u.until, Some(back)),
        other => panic!("该拒：{other:?}"),
    }
    assert_eq!(ctx.hop.store.now().sessions[S1].current, "a");
    go("api").expect("换");
    let s = ctx.hop.store.now().sessions[S1].clone();
    assert_eq!(
        (s.current.as_str(), s.temp.first().map(String::as_str)),
        ("api", Some("api"))
    );
    assert_eq!(s.log.last().map(|l| &l.cause), Some(&book::Cause::UseNow));
    seen(&ctx, "a", 0.85, None);
    go("a").expect("策略的上限不拦");
}

/// ★ 去掉一个号的勾 ⇒ 一判当它用不了（a 用满也不换到它）、
/// 「现在用」它被拦、顺序里沉底、推演那一行整行「不用」；再勾上就恢复。
#[test]
fn an_account_left_out_for_this_session_is_not_used() {
    let home = Home::new("off");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let op = |op: RotTempOp| temp(&ctx, &home, op).expect("ok");
    let got = op(RotTempOp::Off {
        account: "b".into(),
        off: true,
    });
    assert_eq!(got.off, ["b"]);
    home.copy_of(&ctx, S1, |c| {
        c.settings = serde_json::json!({"caps": {}, "capKind": "soft",
            "accounts": [{"start": true}, {"id": "b"}, {"id": "api"}]});
    });
    op(RotTempOp::Clear);
    seen(&ctx, "a", 1.0, None);
    let v = crate::faces::rotation_view_face::view_with(
        &ctx,
        &crate::faces::rotation_view_face::RotationViewReq {
            target: crate::faces::rotation_view_face::RotTarget::Session { sid: S1.into() },
            hours: None,
        },
        now(),
        &crate::Tz::default(),
        &home.alive(),
    )
    .expect("ok");
    let pick = v.decision.as_ref().and_then(|d| d.pick.clone());
    assert_eq!(pick.as_deref(), Some("api"), "去掉的号不该被换上");
    assert_eq!(v.off, ["b"]);
    assert_eq!(
        v.lineup.first().map(String::as_str),
        Some("a"),
        "第一格是在用的号"
    );
    let at = |x: &str| v.lineup.iter().position(|a| a == x).expect("在顺序里");
    assert!(at("api") < at("b"), "去掉的沉底：{:?}", v.lineup);
    let lane = v
        .timeline
        .as_ref()
        .and_then(|t| t.lanes.iter().find(|l| l.account == "b"))
        .expect("b 那一行");
    assert!(
        lane.segs
            .iter()
            .all(|s| !s.sendable && s.text == copy_text("rot.sim.off", &[])),
        "{:?}",
        lane.segs
    );
    match use_now_with(
        &ctx,
        &home.own(),
        &RotationUseNowReq {
            sid: S1.into(),
            account: "b".into(),
        },
        now(),
    ) {
        Err(RotationUseNowErr::Unusable(said, _)) => {
            assert_eq!(said, copy_text("beRotation.edit.offNow", &[("acct", "b")]))
        }
        other => panic!("该拦：{other:?}"),
    }
    assert!(op(RotTempOp::Off {
        account: "b".into(),
        off: false,
    })
    .off
    .is_empty());
}
