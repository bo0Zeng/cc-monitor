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

/// ★ 临时动作只剩顺序里那三样（挪一行 · 先不用 · 清掉）：时间轴那一拖（`place`）入参解析就拒；清掉 ⇒ 空。
#[test]
fn only_the_order_ops_are_left() {
    let req = |op: serde_json::Value| {
        serde_json::from_value::<RotationTempReq>(serde_json::json!({ "sid": S1, "op": op }))
    };
    assert!(
        req(serde_json::json!({"place": {"account": "b", "t": now() + 60}})).is_err(),
        "时间轴那一拖随时间轴删"
    );
    for op in [
        serde_json::json!({"move": {"account": "b", "to": 1}}),
        serde_json::json!({"off": {"account": "b", "off": true}}),
        serde_json::json!("clear"),
    ] {
        assert!(req(op.clone()).is_ok(), "{op}");
    }
    let home = Home::new("temp");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
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
    let ids = || -> Vec<String> {
        crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1)
            .lineup
            .into_iter()
            .map(|l| l.account)
            .collect()
    };
    let line = ids();
    assert!(line.len() >= 3, "{line:?}");
    let x = line[2].clone();
    let moved = mv(&x, 1).expect("ok").temp;
    assert_eq!(moved, [x.clone(), line[1].clone()], "{line:?}");
    assert!(
        crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1).manual,
        "拖过 ⇒ 回包说手动排过"
    );
    let line = ids();
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

/// ★ 「现在用」认的号表同轮换那一张：账号库里合成的账号 0 不在里头 ⇒ `not_listed`（库里有它那一行也拒）。
#[test]
fn use_now_refuses_the_bare_account() {
    let home = Home::new("usenow-bare");
    let mut ctx = home.ctx();
    let inner = std::sync::Arc::clone(&ctx.hop.library);
    ctx.hop.library = std::sync::Arc::new(move |h: &std::path::Path| {
        let mut lib = inner(h)?;
        lib.accounts.push(
            crate::accounts::upstream_select::rotate::test_support::book_row(
                acct_core::BARE_ACCOUNT,
                None,
                false,
            ),
        );
        Some(lib)
    });
    home.saw(&ctx, S1);
    let r = use_now_with(
        &ctx,
        &home.own(),
        &RotationUseNowReq {
            sid: S1.into(),
            account: acct_core::BARE_ACCOUNT.into(),
        },
        now(),
    );
    assert!(matches!(r, Err(RotationUseNowErr::NotListed(_))), "{r:?}");
    assert_eq!(ctx.hop.store.now().sessions[S1].current, "a");
}

/// ★ 不轮换的会话「现在用」照常：换过去、记一条手动换号，视图第一格跟着是它。
#[test]
fn use_now_works_without_a_strategy() {
    let home = Home::new("usenow-none");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    home.load(&ctx, S1, None);
    use_now_with(
        &ctx,
        &home.own(),
        &RotationUseNowReq {
            sid: S1.into(),
            account: "b".into(),
        },
        now(),
    )
    .expect("不轮换也换得了");
    let s = ctx.hop.store.now().sessions[S1].clone();
    assert!(s.uses.is_none(), "换号没给会话装上策略");
    assert_eq!(s.current, "b");
    assert_eq!(s.log.last().map(|l| &l.cause), Some(&book::Cause::UseNow));
    let v = crate::faces::rotation_view_face::tests::session_view(&ctx, &home, S1);
    assert_eq!(v.lineup.first().map(|l| l.account.as_str()), Some("b"));
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
        v.lineup.first().map(|l| l.account.as_str()),
        Some("a"),
        "第一格是在用的号"
    );
    let at = |x: &str| {
        v.lineup
            .iter()
            .position(|a| a.account == x)
            .expect("在顺序里")
    };
    assert!(at("api") < at("b"), "去掉的沉底：{:?}", v.lineup);
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
