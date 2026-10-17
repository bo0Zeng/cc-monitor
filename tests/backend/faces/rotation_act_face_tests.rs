//! 轮换的切换（基础功能层）：拖出来的先后 · 现在用。临时家目录（同 `rotation_view_face` 的判据家）。

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
