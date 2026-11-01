//! 推演的内核：卡住 · 换号重写 · 周上限卡着的额度 · 推完还剩的，逐份照手算对得上；挂进积木库后 `lib::project` 是它们加起来。

use super::*;

fn piece(who: &str, amt: f64, exp: i64, weekly: bool) -> Piece {
    Piece {
        amt,
        start: 0,
        exp,
        who: who.into(),
        weekly,
        late: false,
    }
}

fn walk(mine: &str, at: Option<usize>, rho: f64, end: i64) -> Walk {
    Walk {
        m: 1.0,
        at,
        rho,
        share: None,
        mine: mine.into(),
        lump: 0.0,
        kfull: 500.0,
        grow: 0.0,
        cache: BTreeMap::new(),
        ttl: 3600,
        now: 0,
        end,
    }
}

/// 本会话守着 A 那份 600 tok、每分钟 60：10 分钟用完并进池子，池子里没有别的份 ⇒ 余下 50 分钟每分钟 60 全卡住。
#[test]
fn what_the_pool_cannot_serve_is_stuck() {
    let r = rollout(
        &[piece("A", 600.0, 1800, false)],
        &[],
        &walk("A", Some(0), 60.0, 3600),
    );
    assert_eq!(r.stuck, 3000.0);
    assert_eq!(r.rewrite, 0.0);
    assert!(r.weekly.is_empty() && r.left.is_empty(), "{r:?}");
}

/// 本会话在池子里用 A（10 分钟后作废），A 作废时换到 B：整段重写 500 记进重写、也从 B 里扣；B 是周上限卡着的 ⇒ 重写与之后用的都记在 B 的周额度上；
/// A 作废时剩 400、B 推完剩 8900，都列在 `left`。
#[test]
fn a_switch_rewrites_and_is_paid_from_the_new_piece() {
    let r = rollout(
        &[
            piece("A", 1000.0, 600, false),
            piece("B", 10_000.0, 3600, true),
        ],
        &[],
        &walk("A", None, 60.0, 1200),
    );
    assert_eq!(r.stuck, 0.0);
    assert_eq!(r.rewrite, 500.0);
    assert_eq!(r.weekly, BTreeMap::from([("B".to_string(), 1100.0)]));
    assert_eq!(
        r.left,
        vec![
            ("A".to_string(), 400.0, 600),
            ("B".to_string(), 8900.0, 3600)
        ]
    );
}

/// 缓存期内回到用过的号只补写新长的那段：本会话从 B 起，B 上有缓存到 10_000；换回 B 时按 `grow` 算（每分钟 6）。
#[test]
fn back_to_a_cached_account_only_rewrites_what_grew() {
    let mut w = walk("A", None, 60.0, 1200);
    w.grow = 6.0;
    w.cache = BTreeMap::from([("B".to_string(), 10_000)]);
    let r = rollout(
        &[
            piece("A", 1000.0, 600, false),
            piece("B", 10_000.0, 3600, false),
        ],
        &[],
        &w,
    );
    assert_eq!(r.rewrite, 60.0, "换回 B 那一刻是第 10 分钟：6 × 10");
}

/// 别的会话守在 C（周上限卡着）那份上用到用完：他用掉的记在 C 的周额度上，之后并进池子跟本会话抢 A。
#[test]
fn others_guard_their_piece_then_join_the_pool() {
    let pieces = [
        piece("A", 1200.0, 3600, false),
        piece("C", 600.0, 3600, true),
    ];
    let others = [Guard {
        acct: "C".into(),
        at: Some(1),
        rate: 30.0,
    }];
    let mut w = walk("A", None, 60.0, 1800);
    w.m = 2.0;
    let r = rollout(&pieces, &others, &w);
    assert_eq!(r.weekly, BTreeMap::from([("C".to_string(), 600.0)]));
    // 0–600 s：池子只有本会话（60/分）⇒ A 用掉 600；600 s 起他并进来（60/分）⇒ 120/分，余下 600 再用 5 分钟；之后 15 分钟全卡住。
    assert_eq!(r.stuck, 120.0 * 15.0);
    assert!(r.left.is_empty(), "{r:?}");
}

/// 积木库里：`lib::project_raw` 交这几份原始量，`lib::project` 是卡住 ＋ 重写 ＋ 各号周额度加起来。
#[test]
fn the_lib_adds_the_raw_parts_up() {
    let e = super::super::code::engine().expect("沙箱挂得上积木库");
    let src = r#"
        let amt = [1000.0, 10000.0]; let st = [0, 0]; let ex = [600, 3600]; let who = ["A", "B"];
        let wk = [false, true]; let late = [false, false]; let oev = #{ acc: [], q: [], r: [] };
        let o = #{ m: 1.0, at: (), rho: 60.0, share: (), mine: "A", lump: 0.0, kfull: 500.0, grow: 0.0,
                   cache: #{}, ttl: 3600, now: 0, end: 1200 };
        let r = lib::project_raw(amt, st, ex, who, wk, late, oev, o);
        [r.stuck, r.rewrite, r.weekly.B, r.left.len(), lib::project(amt, st, ex, who, wk, late, oev, o)]
    "#;
    let v: rhai::Array = e.eval(src).expect("跑得通");
    let f = |i: usize| v[i].as_float().unwrap_or(f64::NAN);
    assert_eq!((f(0), f(1), f(2)), (0.0, 500.0, 1100.0));
    assert_eq!(v[3].as_int().ok(), Some(2));
    assert_eq!(f(4), 1600.0);
}

/// 参数形状不对 ⇒ 报错（策略这一判走兜底），不当 0。
#[test]
fn a_bad_argument_is_an_error() {
    let e = super::super::code::engine().expect("沙箱挂得上积木库");
    let src = r#"
        lib::project_raw([1.0], [0, 1], [600], ["A"], [false], [false], #{ acc: [], q: [], r: [] },
          #{ m: 1.0, at: (), rho: 60.0, mine: "A", lump: 0.0, kfull: 0.0, grow: 0.0, ttl: 3600, now: 0, end: 60 })
    "#;
    let err = e.eval::<rhai::Dynamic>(src).expect_err("列长不一样该报错");
    assert!(err.to_string().contains("project_raw"), "{err}");
}
