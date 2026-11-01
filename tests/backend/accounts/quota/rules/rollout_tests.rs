//! 推演的内核：卡住 · 换号重写 · 周上限卡着的额度 · 推完还剩的，逐份照手算对得上；挂进积木库后 `lib::project` 是它们加起来。
//! `lib::rollout` 照 ctx 搭份数：空着的 5 小时窗用了才开（取到 10 分钟格）· 周窗在固定时刻重置 · 两窗同扣。只测动态，定价在 `editor_tests`。

use super::*;

fn piece(who: &str, amt: f64, exp: i64, weekly: bool) -> Piece {
    Piece {
        amt,
        start: 0,
        exp,
        who: who.into(),
        weekly,
        late: false,
        chain: None,
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
        steps: Vec::new(),
        warm: Vec::new(),
        hold: false,
        rejoin: false,
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

// ───────────── 照 ctx 搭份数：`lib::rollout(ctx, plan)` ─────────────

const DAY: i64 = 1_800_000_000 / 86_400 * 86_400;

fn chain(k5: f64) -> Chain {
    Chain {
        k5,
        cap5: Caps {
            segs: Vec::new(),
            dflt: 100.0,
        },
        clock: 0,
        up: false,
        week: None,
    }
}

/// ★ 反例 C：两号都空着、3 小时后起饱和需求 10 小时。此刻给 an 开窗（之后 10 小时里它供第 0–5 · 5–10 · 10–15 小时三份），
/// 不开就等用到才开（第 3 小时 50 分起，只供两份）⇒ 开窗那一走法卡住的少一整份（100 点 × 1% ≈ 1000）。
#[test]
fn warming_an_idle_window_now_buys_a_whole_piece_under_later_saturation() {
    let k = 1000.0;
    let chains = [chain(k), chain(k)];
    let pieces = [
        Piece::shut("an", 0, 0, false),
        Piece::shut("b", 0, 1, false),
    ];
    let mut steps = vec![0.0; 18];
    steps.extend(vec![2000.0; 60]);
    let mut w = walk("b", Some(1), 0.0, 13 * 3600);
    w.kfull = 0.0;
    w.steps = steps;
    let idle = run(&pieces, &chains, &[], &w);
    w.warm = vec![0];
    let warm = run(&pieces, &chains, &[], &w);
    assert!(
        idle.stuck - warm.stuck >= 100.0 * k - 1e-6,
        "开窗少卡 {} · 不开 {idle:?} · 开 {warm:?}",
        idle.stuck - warm.stuck
    );
}

/// ctx 里一个号（Rhai 原文）：`resets` 两格 · 两窗用量 · 1% ≈ · 有没有周上限 · 开窗取整往哪边。
fn one(
    id: &str,
    used: (f64, f64),
    resets: (&str, &str),
    tpp: (u64, &str),
    weekly: bool,
    up: &str,
) -> String {
    format!(
        r#"#{{ id: "{id}", kind: "sub", sendable: true, used: #{{ w5h: {}, w7d: {} }}, resets: #{{ w5h: {}, w7d: {} }},
              refusedUntil: (), tokensPerPct: #{{ w5h: {}, w7d: {} }}, weekly: {weekly}, others: (), windowGrid: #{{ up: {up} }} }}"#,
        used.0, used.1, resets.0, resets.1, tpp.0, tpp.1
    )
}

/// 拼一份 ctx ＋ plan，调 `lib::rollout`，回第一种走法第一条路径那一份。
fn raw_of(now: i64, accts: &[String], current: &str, pace: f64, plan: &str) -> rhai::Map {
    let src = format!(
        r#"let ctx = #{{ now: {now}, clock: {}, current: "{current}", start: "{current}", accounts: [{}],
              session: #{{ pace: {pace}, paceShort: (), inflight: (), bins: [], spread: (), context: (),
                          writeExtra: (), ttl: (), grow: (), cached: #{{}} }} }};
           let parts = lib::rollout(ctx, {plan});
           parts.runs[0][0]"#,
        (now % 86_400) / 60,
        accts.join(", ")
    );
    super::super::code::engine()
        .expect("沙箱挂得上积木库")
        .eval(&src)
        .unwrap_or_else(|e| panic!("跑得通：{e}"))
}

fn f(m: &rhai::Map, k: &str) -> f64 {
    m.get(k).and_then(|v| v.as_float().ok()).unwrap_or(f64::NAN)
}

/// ★ 反例 D：周窗在固定时刻重置（facts 推到的那个 `resets.w7d`，此刻 10:00 ⇒ 22:00），不是此刻 ＋ 7 天：
/// b 周剩 10 点（1% ≈ 1000），本会话每分钟 100：100 分钟用完周额度、卡到 22:00，之后新的一周接着供。
#[test]
fn the_weekly_window_resets_at_its_fixed_time() {
    let now = DAY + 10 * 3600;
    let b = one(
        "b",
        (0.0, 90.0),
        ("()", &(now + 12 * 3600).to_string()),
        (1000, "1000"),
        true,
        "()",
    );
    let plan = format!(
        r#"#{{ caps: #{{ b: #{{ "5h": 100, "7d": 100 }} }}, walks: ["b"], paths: 1, horizon: {} }}"#,
        13 * 3600
    );
    let r = raw_of(now, &[b], "b", 100.0, &plan);
    assert_eq!(f(&r, "stuck"), 100.0 * (720.0 - 100.0), "{r:?}");
    let wk = r["weekly"].clone().cast::<rhai::Map>();
    assert_eq!(
        wk.get("b").and_then(|v| v.as_float().ok()),
        Some(10_000.0),
        "{r:?}"
    );
}

/// ★ 空着的 5 小时窗 12:07 头一次用 ⇒ 开窗取到 12:00、17:00 重置（取整往哪边说不出 ⇒ 往下，作废早算）；`windowGrid.up` ⇒ 12:10 · 17:10。
#[test]
fn a_window_first_used_at_12_07_resets_at_17_00() {
    let now = DAY + 12 * 3600 + 7 * 60;
    let plan =
        r#"#{ caps: #{ a: #{ "5h": 100, "7d": 100 } }, walks: ["a"], paths: 1, horizon: 3600 }"#;
    for (up, reset) in [("()", DAY + 17 * 3600), ("true", DAY + 17 * 3600 + 600)] {
        let a = one("a", (0.0, 0.0), ("()", "()"), (1000, "()"), false, up);
        let r = raw_of(now, &[a], "a", 100.0, plan);
        let left = r["left"].clone().cast::<rhai::Array>();
        let at: Vec<i64> = left
            .iter()
            .map(|x| x.clone().cast::<rhai::Array>()[2].as_int().unwrap_or(0))
            .collect();
        assert_eq!(at, vec![reset], "取整 {up}：{r:?}");
    }
}

/// 号多 · 走法多时积木自己减路径（交回的原始量不撞沙箱的数组 / 表上限）：150 个号、6 种走法、要 32 条路径 ⇒ 少于 32 条、至少 1 条。
#[test]
fn many_accounts_cut_paths_not_the_sandbox_limits() {
    let now = DAY + 10 * 3600;
    let mut accts = Vec::new();
    let mut caps = Vec::new();
    for i in 0..150 {
        let id = format!("n{i:03}");
        accts.push(format!(
            r#"#{{ id: "{id}", kind: "sub", sendable: true, used: #{{ w5h: 50.0, w7d: 0.0 }}, resets: #{{ w5h: {}, w7d: () }},
                  refusedUntil: (), tokensPerPct: #{{ w5h: 1000, w7d: () }}, weekly: false, others: #{{ w5h: 0.1 }}, windowGrid: #{{ up: () }} }}"#,
            now + 600 * (i + 1)
        ));
        caps.push(format!(r#"{id}: #{{ "5h": 100, "7d": 100 }}"#));
    }
    let src = format!(
        r#"let ctx = #{{ now: {now}, clock: 600, current: "n000", start: "n000", accounts: [{}],
              session: #{{ pace: 300.0, paceShort: (), inflight: (), bins: [], spread: (), context: 1000,
                          writeExtra: 1.9, ttl: 3600, grow: 1.0, cached: #{{}} }} }};
           let parts = lib::rollout(ctx, #{{ caps: #{{ {} }}, walks: ["n000", "n001", "n002", "n003", "hold", #{{ warm: "n004" }}], paths: 32 }});
           [parts.paths, parts.runs.len(), parts.runs[5].len()]"#,
        accts.join(", "),
        caps.join(", ")
    );
    let v: rhai::Array = super::super::code::engine()
        .expect("沙箱挂得上积木库")
        .eval(&src)
        .unwrap_or_else(|e| panic!("跑得通：{e}"));
    let n = v[0].as_int().unwrap_or(0);
    assert!((1..32).contains(&n), "路径 {n}");
    assert_eq!(v[1].as_int().ok(), Some(6));
    assert_eq!(v[2].as_int().ok(), Some(n));
}

/// ★ 换去别的会话也守着的号：一起守的那份用完，别的会话照它们的用速并进池子，不是从此不用了。同上一条的号与用速，本会话换去 C：
/// 0–300 s 两条一起守 C（120/分）把 600 用完；之后两条都在池子里（120/分）用 A 的 1200，10 分钟；余下 15 分钟全卡住 ⇒ 跟留着一样卡 1800。
/// 不并进池子（`lib::project` 的口径）⇒ 别的会话 300 s 后的用量不算，看着只卡 300，像是换过去少卡 1500。
#[test]
fn others_sharing_the_target_rejoin_the_pool_when_it_runs_out() {
    let pieces = [
        piece("A", 1200.0, 3600, false),
        piece("C", 600.0, 3600, true),
    ];
    let others = [Guard {
        acct: "C".into(),
        at: Some(1),
        rate: 30.0,
    }];
    let mut w = walk("C", Some(1), 60.0, 1800);
    w.m = 2.0;
    w.share = Some(60.0);
    w.kfull = 0.0;
    w.rejoin = true;
    assert_eq!(rollout(&pieces, &others, &w).stuck, 120.0 * 15.0);
    w.rejoin = false;
    assert_eq!(rollout(&pieces, &others, &w).stuck, 60.0 * 5.0);
}
