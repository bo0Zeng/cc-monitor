//! 第五版的判与推演：`轮换-v5-算法.md` §6 的场景逐条一条判据（#28 节点之间不重跑归第 3 刀的中转缓存）；
//! #22 · #26 用随机生成的 500 组跑「推演说的下一次换号 ＝ 逐发判出来的那一次」。

use super::*;
use crate::accounts::quota::rules::settings::{CapKind, Caps, Row, Rules, Seg, Settings};
use crate::accounts::quota::rules::simulate::simulate;
use crate::accounts::quota::rules::{Acct, WINDOW_SECS};
use std::collections::BTreeMap;

/// 一天的整点起算（UTC），此刻 ＝ 当天 14:30。
const DAY: u64 = 1_800_000_000 / 86_400 * 86_400;
const T: u64 = DAY + 14 * 3600 + 30 * 60;
const MIN: u64 = 60;

fn acct(id: &str, used5h: f64) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used5h, 0.0],
        resets: [Some(T + 3 * 3600 + 17), None],
        refused_until: None,
        barred: None,
        rate: [None, None],
    }
}

fn whole(v: u8) -> Vec<Seg> {
    vec![Seg {
        from: 0,
        to: 1440,
        v,
    }]
}

/// A > B > C，都是软上限 90%，D = 0，R 空。
fn base() -> Settings {
    Settings {
        caps: Caps {
            w5h: Some(whole(90)),
            w7d: Some(whole(90)),
        },
        cap_kind: CapKind::Soft,
        accounts: ["A", "B", "C"]
            .iter()
            .map(|a| Row {
                id: Some(a.to_string()),
                ..Row::default()
            })
            .collect(),
    }
}

fn row<'a>(s: &'a mut Settings, a: &str) -> &'a mut Row {
    s.accounts
        .iter_mut()
        .find(|r| r.id.as_deref() == Some(a))
        .expect("row")
}

fn sig(current: &str, temp: &[&str]) -> Sigma {
    Sigma {
        start: "A".into(),
        current: current.into(),
        temp: temp.iter().map(|s| s.to_string()).collect(),
        since: BTreeMap::new(),
    }
}

fn used(d: &Decision) -> &str {
    match &d.pick {
        Pick::Use(a) => a,
        Pick::Hold { .. } => panic!("该挑一个号，实得停发：{d:?}"),
    }
}

/// 固定的分（判据用的假策略）。
struct Fixed(Vec<(&'static str, f64)>);

impl Score for Fixed {
    fn score(&self, _: &Ctx<'_>) -> Result<Scored, crate::accounts::quota::strategy::Fail> {
        Ok(Scored {
            scores: self
                .0
                .iter()
                .map(|(a, s)| (a.to_string(), (Some(*s), String::new())))
                .collect(),
            next_change_at: None,
        })
    }
}

fn run(s: &Settings, sigma: &Sigma, f: &[Acct], t: u64) -> Decision {
    decide(sigma, f, &Rules::utc(s.clone()), &NoScores, t, &[])
}

// ── #1–#5 候选次序 ───────────────────────────────────────────────────────────

#[test]
fn s01_never_switches_to_an_account_that_cannot_be_used() {
    let mut b = acct("B", 10.0);
    b.refused_until = Some(T + 30 * MIN);
    let f = [acct("A", 90.0), b, acct("C", 10.0)];
    let d = run(&base(), &sig("A", &[]), &f, T);
    assert_eq!((used(&d), &d.cause), ("C", &Cause::Unusable));
}

#[test]
fn s02_no_scores_means_the_list_order() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    assert_eq!(used(&run(&base(), &sig("A", &[]), &f, T)), "B");
}

#[test]
fn s03_scores_override_the_list_and_missing_counts_as_zero() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let d = decide(
        &sig("A", &[]),
        &f,
        &Rules::utc(base()),
        &Fixed(vec![("C", 5.0)]),
        T,
        &[],
    );
    assert_eq!(used(&d), "C");
}

#[test]
fn s04_equal_scores_fall_back_to_the_list() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let d = decide(
        &sig("A", &[]),
        &f,
        &Rules::utc(base()),
        &Fixed(vec![("B", 3.0), ("C", 3.0)]),
        T,
        &[],
    );
    assert_eq!(used(&d), "B");
}

#[test]
fn s05_a_broken_strategy_counts_as_no_scores_and_says_so() {
    let src = std::fs::read_to_string(shipped("list-order.rhai"))
        .expect("随包那份")
        .replace("#{ scores: #{} }", "loop { }");
    let file = crate::accounts::quota::rules::script::parse(&src).expect("编得过");
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let d = decide(&sig("A", &[]), &f, &Rules::utc(base()), &file, T, &[]);
    assert_eq!(used(&d), "B");
    let e = d.strategy_error.expect("带 strategyError");
    assert_eq!(
        e.kind,
        crate::accounts::quota::strategy::FailKind::Timeout,
        "{e:?}"
    );
}

// ── #6–#8 回切 ───────────────────────────────────────────────────────────────

fn back_case(a_used: f64, r: Option<u8>) -> Decision {
    let mut s = base();
    row(&mut s, "A").back = r;
    run(
        &s,
        &sig("B", &[]),
        &[acct("A", a_used), acct("B", 10.0), acct("C", 10.0)],
        T,
    )
}

#[test]
fn s06_switches_back_once_headroom_reaches_the_threshold() {
    let d = back_case(65.0, Some(20));
    assert_eq!((used(&d), &d.cause), ("A", &Cause::Back));
}

#[test]
fn s07_stays_when_headroom_is_below_the_threshold() {
    assert_eq!(used(&back_case(75.0, Some(20))), "B");
}

#[test]
fn s08_no_threshold_never_switches_back() {
    let d = back_case(10.0, None);
    assert_eq!((used(&d), &d.cause), ("B", &Cause::Stay));
}

// ── #9–#11 启用延迟 ───────────────────────────────────────────────────────────

#[test]
fn s09_activation_delay_holds_then_lets_it_in() {
    let mut s = base();
    row(&mut s, "C").delay = 30;
    let f = [acct("A", 95.0), acct("B", 95.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &f, T);
    assert_eq!(d.cause, Cause::Delay);
    assert_eq!(
        d.pick,
        Pick::Hold {
            until: Some(T + 30 * MIN),
            account: Some("C".into())
        }
    );
    assert_eq!(d.tn, Some(T + 30 * MIN), "延迟到期是一个已知节点");
    let d = run(&s, &d.sigma, &f, T + 30 * MIN);
    assert_eq!(used(&d), "C");
}

#[test]
fn s10_two_delays_ending_together_take_the_front_one() {
    let mut s = base();
    row(&mut s, "B").delay = 30;
    row(&mut s, "C").delay = 30;
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &f, T);
    let d = run(&s, &d.sigma, &f, T + 30 * MIN);
    assert_eq!(used(&d), "B");
}

#[test]
fn s11_a_break_in_the_outage_restarts_the_delay() {
    let mut s = base();
    row(&mut s, "C").delay = 30;
    let bad = [acct("A", 95.0), acct("B", 95.0), acct("C", 10.0)];
    let good = [acct("A", 10.0), acct("B", 95.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &bad, T);
    let d = run(&s, &d.sigma, &good, T + 10 * MIN);
    assert_eq!(used(&d), "A");
    assert!(
        !d.sigma.since.contains_key("C"),
        "A 又能用 ⇒ C 的计时清零：{:?}",
        d.sigma.since
    );
    let d = run(&s, &d.sigma, &bad, T + 20 * MIN);
    assert_eq!(
        d.sigma.since.get("C"),
        Some(&(T + 20 * MIN)),
        "从再次用不了时重新计"
    );
    assert_eq!(run(&s, &d.sigma, &bad, T + 30 * MIN).cause, Cause::Delay);
    assert_eq!(used(&run(&s, &d.sigma, &bad, T + 50 * MIN)), "C");
}

// ── #12–#14 临时条目 ─────────────────────────────────────────────────────────

#[test]
fn s12_a_temporary_entry_that_cannot_be_used_is_skipped() {
    let mut b = acct("B", 10.0);
    b.refused_until = Some(T + 3600);
    let f = [acct("A", 95.0), b, acct("C", 10.0)];
    let d = run(&base(), &sig("A", &["A", "B", "C"]), &f, T);
    assert_eq!(used(&d), "C");
    assert_eq!(d.sigma.temp, ["C"]);
    assert_eq!(d.skipped, ["B"]);
}

#[test]
fn s13_a_temporary_entry_stays_until_it_cannot_be_used() {
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)];
    let d = run(&base(), &sig("C", &["C"]), &f, T);
    assert_eq!(
        (used(&d), d.sigma.temp.clone()),
        ("C", vec!["C".to_string()])
    );
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 95.0)];
    let d = run(&base(), &d.sigma, &f, T + MIN);
    assert_eq!(used(&d), "A", "临时条目丢完 ⇒ 回到策略的次序");
    assert!(d.sigma.temp.is_empty());
}

#[test]
fn s14_temporary_entries_outrank_switching_back_without_turning_it_off() {
    let mut s = base();
    row(&mut s, "A").back = Some(20);
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)];
    let d = run(&s, &sig("B", &["B"]), &f, T);
    assert_eq!(used(&d), "B", "A 在候选次序里排在 B 后面");
    let d = run(&s, &sig("B", &[]), &f, T);
    assert_eq!((used(&d), &d.cause), ("A", &Cause::Back));
}

// ── #15–#19 软硬上限 · 停发 ──────────────────────────────────────────────────

#[test]
fn s15_soft_caps_hold_while_some_account_is_under_its_cap() {
    let d = run(
        &base(),
        &sig("A", &[]),
        &[acct("A", 92.0), acct("B", 95.0), acct("C", 10.0)],
        T,
    );
    assert_eq!((used(&d), d.lift), ("C", false));
}

#[test]
fn s16_soft_caps_lift_when_every_account_is_at_its_cap() {
    let mut c = acct("C", 10.0);
    c.refused_until = Some(T + 3600);
    let d = run(
        &base(),
        &sig("A", &[]),
        &[acct("A", 92.0), acct("B", 95.0), c],
        T,
    );
    assert_eq!((used(&d), d.lift), ("A", true));
}

#[test]
fn s17_hard_caps_never_lift() {
    let mut s = base();
    row(&mut s, "A").cap_kind = Some(CapKind::Hard);
    let mut c = acct("C", 10.0);
    c.refused_until = Some(T + 3600);
    assert_eq!(
        used(&run(
            &s,
            &sig("A", &[]),
            &[acct("A", 92.0), acct("B", 95.0), c],
            T
        )),
        "B"
    );
}

fn all_hard(resets: [u64; 3]) -> Decision {
    let mut s = base();
    s.cap_kind = CapKind::Hard;
    let f: Vec<Acct> = ["A", "B", "C"]
        .iter()
        .zip(resets)
        .map(|(a, r)| Acct {
            resets: [Some(r), None],
            ..acct(a, 90.0)
        })
        .collect();
    run(&s, &sig("A", &[]), &f, T)
}

#[test]
fn s18_holding_reports_the_earliest_recovery() {
    let d = all_hard([T + 90 * MIN, T + 40 * MIN, T + 20 * MIN]);
    assert_eq!(d.cause, Cause::Hold);
    assert_eq!(
        d.pick,
        Pick::Hold {
            until: Some(T + 20 * MIN),
            account: Some("C".into())
        }
    );
}

#[test]
fn s19_holding_ties_go_to_the_front_account() {
    let d = all_hard([T + 90 * MIN, T + 20 * MIN, T + 20 * MIN]);
    assert_eq!(
        d.pick,
        Pick::Hold {
            until: Some(T + 20 * MIN),
            account: Some("B".into())
        }
    );
}

// ── #20–#21 ──────────────────────────────────────────────────────────────────

#[test]
fn s20_a_zero_percent_segment_skips_the_account_until_it_ends() {
    let mut s = base();
    row(&mut s, "B").caps.w5h = Some(vec![
        Seg {
            from: 0,
            to: 20 * 60,
            v: 90,
        },
        Seg {
            from: 20 * 60,
            to: 23 * 60,
            v: 0,
        },
        Seg {
            from: 23 * 60,
            to: 1440,
            v: 90,
        },
    ]);
    let at = DAY + 20 * 3600 + 30 * 60;
    let far = |a: Acct| Acct {
        resets: [None, None],
        ..a
    };
    let f = [
        far(acct("A", 95.0)),
        far(acct("B", 10.0)),
        far(acct("C", 10.0)),
    ];
    let d = run(&s, &sig("A", &[]), &f, at);
    assert_eq!(used(&d), "C");
    assert!(
        d.tn.is_some_and(|n| n <= DAY + 23 * 3600),
        "23:00 那条边界是一个节点：{:?}",
        d.tn
    );
    assert_eq!(used(&run(&s, &sig("A", &[]), &f, DAY + 23 * 3600)), "B");
}

#[test]
fn s21_use_now_on_an_unusable_account_changes_nothing_and_says_when() {
    let f = [acct("A", 10.0), acct("B", 90.0), acct("C", 10.0)];
    let r = use_now(&sig("A", &[]), &f, &Rules::utc(base()), &NoScores, T, "B");
    assert_eq!(
        r.err(),
        Some(Some(T + 3 * 3600 + 17)),
        "用不了 ⇒ 回恢复时刻（窗口重置）"
    );
    let d = use_now(&sig("A", &[]), &f, &Rules::utc(base()), &NoScores, T, "C").expect("能用");
    assert_eq!(
        (used(&d), d.sigma.temp.clone()),
        ("C", vec!["C".to_string()])
    );
}

// ── #23 · #24 · #27 · #29 ─────────────────────────────────────────────────────

#[test]
fn s23_simulation_stops_at_the_event_cap() {
    let mut s = base();
    row(&mut s, "A").cap_kind = None;
    s.caps.w5h = Some(
        (0..48u16)
            .map(|i| Seg {
                from: i * 30,
                to: i * 30 + 30,
                v: if i % 2 == 0 { 90 } else { 80 },
            })
            .collect(),
    );
    let mut a = acct("A", 10.0);
    a.rate = [Some(1e-6), Some(1e-6)];
    let tl = simulate(
        &sig("A", &[]),
        &[a, acct("B", 10.0), acct("C", 10.0)],
        &Rules::utc(s),
        &NoScores,
        T,
        T + 30 * 86_400,
    );
    assert!(tl.truncated, "三十天每半小时一条边界 ⇒ 撞上 200 个事件");
}

#[test]
fn s24_decide_is_pure() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    assert_eq!(
        run(&base(), &sig("A", &["B"]), &f, T),
        run(&base(), &sig("A", &["B"]), &f, T)
    );
}

#[test]
fn s27_a_refused_send_picks_the_next_account_in_the_same_send() {
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)];
    let d = decide(
        &sig("B", &[]),
        &f,
        &Rules::utc(base()),
        &NoScores,
        T,
        &["B".to_string()],
    );
    assert_eq!(used(&d), "A");
    let d = decide(
        &sig("B", &[]),
        &f,
        &Rules::utc(base()),
        &NoScores,
        T,
        &["B".to_string(), "A".to_string()],
    );
    assert_eq!(used(&d), "C");
}

#[test]
fn s29_switching_back_happens_only_at_a_known_node() {
    let mut s = base();
    row(&mut s, "A").back = Some(20);
    let mut a = acct("A", 50.0);
    a.refused_until = Some(T + 30 * MIN);
    let f = [a, acct("B", 10.0), acct("C", 10.0)];
    let d = run(&s, &sig("B", &[]), &f, T);
    assert_eq!(used(&d), "B");
    assert_eq!(d.tn, Some(T + 30 * MIN));
    assert_eq!(used(&run(&s, &sig("B", &[]), &f, T + 30 * MIN - 1)), "B");
    assert_eq!(used(&run(&s, &sig("B", &[]), &f, T + 30 * MIN)), "A");
}

// ── #25 分随时间升，越过回切阈值才换 ─────────────────────────────────────────

struct Rising;

impl Score for Rising {
    fn score(&self, c: &Ctx<'_>) -> Result<Scored, crate::accounts::quota::strategy::Fail> {
        let mut out = Scored::default();
        out.scores.insert(
            "A".into(),
            (Some(((c.now - T) / MIN) as f64), String::new()),
        );
        out.scores.insert("B".into(), (Some(20.0), String::new()));
        Ok(out)
    }
    fn time_varying(&self) -> bool {
        true
    }
}

#[test]
fn s25_a_rising_score_alone_does_not_switch_back() {
    let mut s = base();
    let r = row(&mut s, "A");
    r.back = Some(30);
    r.caps.w5h = Some(whole(35));
    let mut a = acct("A", 10.0);
    a.resets = [Some(T + 40 * MIN), None];
    let f = [a, acct("B", 10.0), acct("C", 10.0)];
    let rules = Rules::utc(s);
    let d = decide(&sig("B", &[]), &f, &rules, &Rising, T + 21 * MIN, &[]);
    assert_eq!(d.order[0].account, "A", "t+20 起 A 排到 B 前面");
    assert_eq!(
        (used(&d), &d.cause),
        ("B", &Cause::Stay),
        "余量 25 < 30，不换"
    );
    let d = decide(&sig("B", &[]), &f, &rules, &Rising, T + 40 * MIN, &[]);
    assert_eq!(
        (used(&d), &d.cause),
        ("A", &Cause::Back),
        "重置后余量 35 ≥ 30 ⇒ 切回"
    );
}

// ── #22 · #26 一致性：推演说的下一次换号 ＝ 逐发判出来的那一次 ────────────────

/// 一个请求间隔（每分钟一发）＋ 推演里到上限时刻向上取整的那一秒。
const TOL: u64 = 61;

/// 小随机数（判据自带，种子固定，重跑同一组）。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn upto(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("a{i}")).collect()
}

/// 一组随机的设置 ＋ 事实。
fn group(seed: u64) -> (Settings, Vec<Acct>, Sigma) {
    let mut g = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let n = 3 + g.upto(6) as usize;
    let ids = names(n);
    let seg = |g: &mut Rng| -> Vec<Seg> {
        if g.upto(3) == 0 {
            let a = 60 * (1 + g.upto(20) as u16);
            let b = a + 60 * (1 + g.upto(3) as u16);
            vec![
                Seg {
                    from: 0,
                    to: a,
                    v: 80 + g.upto(20) as u8,
                },
                Seg {
                    from: a,
                    to: b.min(1440),
                    v: g.upto(2) as u8 * 50,
                },
                Seg {
                    from: b.min(1440),
                    to: 1440,
                    v: 80 + g.upto(20) as u8,
                },
            ]
        } else {
            whole(70 + g.upto(30) as u8)
        }
    };
    let s = Settings {
        caps: Caps {
            w5h: Some(seg(&mut g)),
            w7d: Some(whole(95)),
        },
        cap_kind: if g.upto(2) == 0 {
            CapKind::Soft
        } else {
            CapKind::Hard
        },
        accounts: ids
            .iter()
            .map(|a| Row {
                id: Some(a.clone()),
                caps: if g.upto(3) == 0 {
                    Caps {
                        w5h: Some(seg(&mut g)),
                        w7d: None,
                    }
                } else {
                    Caps::default()
                },
                delay: [0, 0, 10, 30][g.upto(4) as usize],
                back: [None, None, Some(10), Some(40)][g.upto(4) as usize],
                ..Row::default()
            })
            .collect(),
    };
    let f: Vec<Acct> = ids
        .iter()
        .map(|a| Acct {
            id: a.clone(),
            api: false,
            used: [g.upto(90) as f64, g.upto(60) as f64],
            resets: [
                Some(T + 60 + g.upto(5 * 3600)),
                Some(T + 3600 + g.upto(6 * 86_400)),
            ],
            refused_until: (g.upto(5) == 0).then(|| T + g.upto(3 * 3600)),
            barred: None,
            rate: [
                Some(0.2 + g.upto(18) as f64 / 10.0),
                Some(0.01 + g.upto(10) as f64 / 100.0),
            ],
        })
        .collect();
    let temp: Vec<String> = ids.iter().filter(|_| g.upto(4) == 0).cloned().collect();
    let sigma = Sigma {
        start: ids[0].clone(),
        current: ids[0].clone(),
        temp,
        since: BTreeMap::new(),
    };
    (s, f, sigma)
}

/// 照估计往前走一分钟（同推演的那几条：只有在用的号涨 · 到点重置 · 被拒到期 · 开始用时窗口开始计时）。
/// 在用的号的窗口在这一分钟里重置 ⇒ 重置之后接着用、窗口从重置那一刻重新计（同推演在重置那个节点上做的）。
fn step(f: &mut [Acct], using: Option<&str>, t: u64, dt: u64) {
    use crate::accounts::quota::rules::standing::used_at;
    for x in f.iter_mut() {
        let in_use = using == Some(x.id.as_str());
        for w in 0..2 {
            let rate = x.rate[w].unwrap_or(0.0);
            if in_use && x.resets[w].is_none_or(|r| r <= t) {
                x.used[w] = used_at(x, w, t);
                x.resets[w] = Some(t + WINDOW_SECS[w]);
            }
            match x.resets[w] {
                Some(r) if r <= t + dt => {
                    if in_use {
                        x.used[w] = rate * (t + dt - r) as f64 / 60.0;
                        x.resets[w] = Some(r + WINDOW_SECS[w]);
                    } else {
                        x.used[w] = 0.0;
                        x.resets[w] = None;
                    }
                }
                _ if in_use => x.used[w] += rate * dt as f64 / 60.0,
                _ => {}
            }
        }
        if x.refused_until.is_some_and(|u| u <= t + dt) {
            x.refused_until = None;
        }
    }
}

/// 推演里预计那次换号前后一个请求间隔以内还有别的节点 ⇒ 逐发那一边分不出这两件的先后（比如同一分钟里一个号到上限、另一个号重置）。
fn second_node(
    tl: &crate::accounts::quota::rules::simulate::Timeline,
    at: Option<u64>,
) -> Option<u64> {
    let at = at?;
    tl.nodes
        .iter()
        .copied()
        .find(|n| *n != at && n.abs_diff(at) <= TOL)
        .map(|_| at)
}

/// 逐发（每分钟一发）判，每次真换号之前，拿上一个真换号那一刻的状态推演一遍：推演说的下一次换号与真的那一次号相同、时刻差不超过 `tol`。
/// 回比过几次。
fn consistent(
    s: &Settings,
    f0: &[Acct],
    sigma0: &Sigma,
    p: &dyn Score,
    tol: u64,
    label: &str,
    collided: &mut usize,
) -> usize {
    let rules = Rules::utc(s.clone());
    let mut f = f0.to_vec();
    let mut sigma = sigma0.clone();
    let mut t = T;
    let mut using: Option<String> = Some(sigma.current.clone());
    let tl = simulate(&sigma, &f, &rules, p, t, t + 12 * 3600);
    let mut predicted = tl.switches();
    let mut second = second_node(&tl, predicted.first().map(|x| x.0));
    let mut made_at = T;
    let mut next_node = decide(&sigma, &f, &rules, p, T, &[]).tn;
    let mut checked = 0;
    let checked0 = *collided;
    // 每组至多比 8 次（策略来回翻的那几组每分钟一换，比满 12 小时太慢）。
    while t < T + 12 * 3600 && checked + *collided < 8 + checked0 {
        let d = decide(&sigma, &f, &rules, p, t, &[]);
        sigma = d.sigma.clone();
        let now_using = match &d.pick {
            Pick::Use(a) => Some(a.clone()),
            Pick::Hold { .. } => None,
        };
        if let Some(a) = &now_using {
            if using.as_deref().is_some_and(|u| u != a) || using.is_none() {
                if using.as_deref() != Some(a.as_str()) && using.is_some() {
                    let (pt, pa) = predicted
                        .first()
                        .cloned()
                        .unwrap_or_else(|| panic!("{label}：{t} 真换到 {a}，推演没说会换"));
                    // 推演里两个节点落在同一个请求间隔里（逐发那一边只看得到后一个）⇒ 这一次比不了，单数出来。
                    // 换到的号有启用延迟：计时起点是逐发那一边看到「前面的号都用不了」的那一发（晚到至多一个间隔），到期又要等下一发 ⇒ 至多两个间隔。
                    let tol = if rules.delay_secs(&sigma.start, a) > 0 {
                        2 * tol
                    } else {
                        tol
                    };
                    // 兜底（只靠 15 分钟一格）：格与格之间次序可能已经翻过好几次，换到谁不比，只比时刻。
                    let same = (tol > 4 * TOL || pa == *a) && t.abs_diff(pt) <= tol;
                    if same {
                        checked += 1;
                    } else if second.is_some() {
                        // 推演里预计那一刻前后一个间隔以内还有别的节点：逐发那一边分不出先后 ⇒ 这一次比不了，单数出来。
                        *collided += 1;
                    } else {
                        panic!(
                            "{label}：真 {t} 换到 {a}（{:?}）· 推演 {pt} 换到 {pa}（{made_at} 推的），差 {} 秒 > {tol}；推演 {:?}",
                            d.cause,
                            t.abs_diff(pt),
                            predicted.iter().take(3).collect::<Vec<_>>()
                        );
                    }
                }
                let tl = simulate(&sigma, &f, &rules, p, t, t + 12 * 3600);
                predicted = tl.switches();
                second = second_node(&tl, predicted.first().map(|x| x.0));
                made_at = t;
                next_node = d.tn;
            }
            using = Some(a.clone());
        }
        // 每过一个节点重推一次（同真用：节点上重跑时同时重推推演）。
        if made_at != t && next_node.is_some_and(|n| t >= n) {
            let tl = simulate(&sigma, &f, &rules, p, t, t + 12 * 3600);
            predicted = tl.switches();
            second = second_node(&tl, predicted.first().map(|x| x.0));
            made_at = t;
            next_node = d.tn;
        }
        step(&mut f, now_using.as_deref(), t, 60);
        t += 60;
    }
    checked
}

#[test]
fn s22_simulation_agrees_with_per_request_decisions_over_500_groups() {
    let (mut total, mut collided) = (0, 0);
    for seed in 1..=500u64 {
        let (s, f, sigma) = group(seed);
        total += consistent(
            &s,
            &f,
            &sigma,
            &NoScores,
            TOL,
            &format!("第 {seed} 组"),
            &mut collided,
        );
    }
    assert!(
        total > 200,
        "500 组里只比到 {total} 次换号 —— 生成的组太平，本条在空转"
    );
    assert!(
        collided * 20 <= total,
        "两个节点挤在一个请求间隔里的有 {collided} 次（比过 {total} 次），超过 5%"
    );
}

#[test]
fn s26_next_change_at_keeps_simulation_exact_and_its_absence_falls_back_to_15_minutes() {
    let src = std::fs::read_to_string(shipped("most-left.rhai")).expect("随包那份");
    let with = crate::accounts::quota::rules::script::parse(&src).expect("编得过");
    let without_src = src.replace("res.next_change_at =", "let _unused =");
    let without = crate::accounts::quota::rules::script::parse(&without_src).expect("编得过");
    let (mut exact, mut fallback, mut collided) = (0, 0, 0);
    // 策略跑在沙箱里（每判两次），组数比 #22 少：50 组。
    for seed in 1..=50u64 {
        let (mut s, f, sigma) = group(seed);
        // 只给列表头一个号回切阈值：余量大的那个排到前面就切回它，换过去以后别的号不回切 ⇒ 不会每分钟来回翻
        // （所有号都设回切阈值时，「余量大的在前」会在两个号之间每分钟翻一次 —— 那是这份策略 ＋ 这组设置本身的行为）。
        for (i, r) in s.accounts.iter_mut().enumerate() {
            r.back = (i == 0).then_some(1);
            r.delay = 0;
        }
        exact += consistent(
            &s,
            &f,
            &sigma,
            &with,
            TOL,
            &format!("第 {seed} 组（交 next_change_at）"),
            &mut collided,
        );
        // 兜底：声明了分随时间变、又不交时刻 ⇒ 推演里节点之间至多隔 15 分钟（多一秒是向上取整）。
        let tl = simulate(
            &sigma,
            &f,
            &Rules::utc(s.clone()),
            &without,
            T,
            T + 12 * 3600,
        );
        let gap = tl.nodes.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
        assert!(
            gap <= 15 * 60 + 1,
            "第 {seed} 组（兜底）：推演里有两个节点隔了 {gap} 秒"
        );
        fallback += 1;
    }
    assert!(
        exact > 50 && fallback == 50,
        "比到的换号太少：{exact} · {fallback}"
    );
    assert!(
        collided * 20 <= exact,
        "挤在一起的节点有 {collided} 次，超过 5%"
    );
}

/// 随包策略的住址。
fn shipped(name: &str) -> std::path::PathBuf {
    crate::guard_support::repo_root()
        .join("src/shared/rotation/strategies")
        .join(name)
}
