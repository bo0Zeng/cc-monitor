//! 会话侧的临时动作（基础功能层）：拖出来的先后 · 先不用的号 · 现在用。临时家目录（同 `rotation_view_face` 的判据家）。

use super::*;
use crate::faces::rotation_view_face::tests::{now, seen, Home, S1};

/// ★ 拖出来的顺序：放到第几位（越界取末尾、已在就挪）· 清掉；不在这台账号里的号 ⇒ `not_listed`。
#[test]
fn the_dragged_order_is_placed_and_cleared() {
    let home = Home::new("temp");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let place = |account: &str, at: usize| {
        temp_with(
            &ctx,
            &home.own(),
            &RotationTempReq {
                sid: S1.into(),
                op: RotTempOp::Place {
                    account: account.into(),
                    at,
                },
            },
        )
    };
    assert_eq!(place("b", 0).expect("ok").temp, ["b"]);
    assert_eq!(place("c", 9).expect("ok").temp, ["b", "c"]);
    assert_eq!(place("c", 0).expect("ok").temp, ["c", "b"]);
    assert!(matches!(place("zz", 0), Err(RotationTempErr::NotListed(_))));
    let cleared = temp_with(
        &ctx,
        &home.own(),
        &RotationTempReq {
            sid: S1.into(),
            op: RotTempOp::Clear,
        },
    )
    .expect("ok");
    assert!(cleared.temp.is_empty());
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

/// ★ 会话侧的临时动作：拖完的那一串整串换上（重的只留一个）；去掉一个号的勾 ⇒ 一判当它用不了（a 用满也不换到它）、
/// 「现在用」它被拦、顺序里沉底、推演那一行整行「不用」；再勾上就恢复。
#[test]
fn an_account_left_out_for_this_session_is_not_used() {
    let home = Home::new("off");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let op = |op: RotTempOp| {
        temp_with(&ctx, &home.own(), &RotationTempReq { sid: S1.into(), op }).expect("ok")
    };
    assert_eq!(
        op(RotTempOp::Set(vec!["b".into(), "api".into(), "b".into()])).temp,
        ["b", "api"]
    );
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
