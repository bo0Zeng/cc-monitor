use super::*;
use crate::control::cc_bus_wire::BusEnvelope;

const A: &str = "0f3c2a1e-1111-2222-3333-444455556666";
const B: &str = "9a8b7c6d-aaaa-bbbb-cccc-ddddeeeeffff";
const C: &str = "c0c0c0c0-aaaa-bbbb-cccc-ddddeeeeffff";

fn env(from: &str, to: &str, tier: u8, title: &str, id: &str) -> BusEnvelope {
    BusEnvelope {
        id: id.to_string(),
        ts: "2026-10-27T00:00:00Z".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        tier,
        act: cmd_wire::bus::BusAct::Inform,
        title: title.to_string(),
        re: None,
        refs: vec![],
        text: "占位正文".to_string(),
    }
}

fn code(r: Result<bool, crate::common::said::Said>) -> String {
    match r {
        Ok(_) => "ok".to_string(),
        Err(s) => s.code_or_fallback().to_string(),
    }
}

#[test]
fn the_same_letter_twice_within_the_window_is_a_duplicate() {
    let mut l = Ledger::default();
    let t0 = 1_000_000;
    assert_eq!(code(admit(&mut l, &env(A, B, 2, "t", "m1"), t0)), "ok");
    // 一秒之内同一封（id 与时间是后端每次另盖的，不算进「同一封」）⇒ 重复。
    let e = admit(&mut l, &env(A, B, 2, "t", "m2"), t0 + 1_000).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("duplicate"));
    assert!(
        e.said.contains("m1"),
        "那一句说的是跟哪一封重了：{}",
        e.said
    );
    // 去重窗 10 秒：差 1 毫秒仍重复；拒掉的不进账，满 10 秒照常收。
    assert_eq!(
        admit(&mut l, &env(A, B, 2, "t", "m3"), t0 + 9_999)
            .unwrap_err()
            .code
            .as_deref(),
        Some("duplicate")
    );
    assert_eq!(
        code(admit(&mut l, &env(A, B, 2, "t", "m4"), t0 + 10_000)),
        "ok"
    );
}

#[test]
fn any_differing_field_makes_it_another_letter() {
    let mut l = Ledger::default();
    let t0 = 1_000_000;
    assert_eq!(code(admit(&mut l, &env(A, B, 2, "t", "m1"), t0)), "ok");
    for e in [
        env(A, B, 2, "另一个标题", "m2"),
        env(A, C, 2, "t", "m3"),
        env(C, B, 2, "t", "m4"),
    ] {
        assert_eq!(code(admit(&mut l, &e, t0 + 1)), "ok", "{e:?}");
    }
    let mut with_re = env(A, B, 2, "t", "m5");
    with_re.re = Some("m0123456789ab".into());
    assert_eq!(code(admit(&mut l, &with_re, t0 + 2)), "ok");
    let mut other_text = env(A, B, 2, "t", "m6");
    other_text.text = "另一句".into();
    assert_eq!(code(admit(&mut l, &other_text, t0 + 3)), "ok");
}

#[test]
fn a_pair_over_its_limit_is_throttled_and_others_still_pass() {
    let mut l = Ledger::default();
    let t0 = 5_000_000;
    // 一对一窗 20 封：前 20 封收，第 21 封拒。
    for i in 0..20_u32 {
        assert_eq!(
            code(admit(
                &mut l,
                &env(A, B, 2, &format!("t{i}"), "m"),
                t0 + i64::from(i)
            )),
            "ok"
        );
    }
    let c = admit(&mut l, &env(A, B, 2, "over", "m"), t0 + 100)
        .unwrap_err()
        .code;
    assert_eq!(c.as_deref(), Some("throttled"));
    // 另一对不受这一对的限。
    assert_eq!(code(admit(&mut l, &env(A, C, 2, "x", "m"), t0 + 101)), "ok");
    // 窗 60 秒：差 1 毫秒仍拒，翻过去照常。
    assert_eq!(
        admit(&mut l, &env(A, B, 2, "still", "m"), t0 + 59_999)
            .unwrap_err()
            .code
            .as_deref(),
        Some("throttled")
    );
    assert_eq!(
        code(admit(&mut l, &env(A, B, 2, "again", "m"), t0 + 60_000)),
        "ok"
    );
}

#[test]
fn the_whole_machine_has_one_limit_too() {
    let mut l = Ledger::default();
    let t0 = 9_000_000;
    let sid = |i: u32| format!("{i:08x}-1111-2222-3333-444455556666");
    // 全台一窗 120 封：前 120 封收，第 121 封拒。
    for i in 0..120_u32 {
        assert_eq!(code(admit(&mut l, &env(&sid(i), B, 2, "t", "m"), t0)), "ok");
    }
    let c = admit(&mut l, &env(&sid(120), B, 2, "t", "m"), t0)
        .unwrap_err()
        .code;
    assert_eq!(c.as_deref(), Some("throttled"));
}

#[test]
fn tier_one_knocks_once_per_debounce_and_tier_two_never() {
    let mut l = Ledger::default();
    let t0 = 2_000_000;
    assert_eq!(admit(&mut l, &env(A, B, 2, "t2", "m"), t0), Ok(false));
    assert_eq!(admit(&mut l, &env(A, B, 1, "a", "m"), t0), Ok(true));
    assert_eq!(
        admit(&mut l, &env(C, B, 1, "b", "m"), t0 + 1_999),
        Ok(false),
        "同一条会话 2 秒去抖窗里只敲一次（不论谁发的）"
    );
    assert_eq!(admit(&mut l, &env(A, C, 1, "c", "m"), t0 + 1), Ok(true));
    assert_eq!(admit(&mut l, &env(A, B, 1, "d", "m"), t0 + 2_000), Ok(true));
}

#[test]
fn the_ledger_keeps_only_what_a_window_still_needs() {
    let mut l = Ledger::default();
    let t0 = 3_000_000;
    admit(&mut l, &env(A, B, 1, "t", "m1"), t0).unwrap();
    admit(&mut l, &env(A, C, 1, "t", "m2"), t0).unwrap();
    admit(&mut l, &env(C, B, 2, "t", "m3"), t0 + 60_000).unwrap();
    assert_eq!(l.recent.len(), 1, "去重窗外的指纹不留");
    assert_eq!(l.pairs.len(), 1, "过了窗的那几对不留");
    assert_eq!(l.knocked.len(), 0, "去抖窗外的敲门时刻不留");
}
