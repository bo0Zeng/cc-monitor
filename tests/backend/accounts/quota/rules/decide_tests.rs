//! 后端的一判与推演（`轮换-v5-算法.md` §7）。「策」＝ 拿三份升级材料（`rules::upgrade::material`：照号列表 · 离重置近的权重高 · 余量多的权重高，同一段换号的写法，`轮换-v5-分层.md` §2.5）＋ 积木库跑后端的 `turn`；
//! 「后」＝ 只测后端积木，策略是判据里的假策略。

use super::*;
use crate::accounts::quota::rules::script::{self, File};
use crate::accounts::quota::rules::simulate::{simulate, Timeline};
use crate::accounts::quota::rules::WINDOW_SECS;
use crate::accounts::quota::strategy::{Fail, FailKind};
use serde_json::{json, Value};
use std::sync::OnceLock;

/// 一天的整点起算（UTC），此刻 ＝ 当天 14:30。
const DAY: u64 = 1_800_000_000 / 86_400 * 86_400;
const T: u64 = DAY + 14 * 3600 + 30 * 60;
const MIN: u64 = 60;

fn clock(t: u64) -> u16 {
    ((t % 86_400) / 60) as u16
}

fn material(n: usize) -> Material {
    Material(script::parse(crate::accounts::quota::rules::upgrade::material(n)).expect("读得懂"))
}

/// 一份升级材料，判在判据这一层的沙箱里（[`test_engine`]）。
struct Material(File);

impl Strategy for Material {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        self.0.decide_in(test_engine(), ctx)
    }
    fn time_varying(&self) -> bool {
        self.0.time_varying()
    }
}

/// 判据这一层的沙箱：跟进程里那一份同一个造法（[`fresh_engine`](crate::accounts::quota::strategy::fresh_engine) ＋ 积木库），
/// 操作数 · 调用深度 · 大小的上限都照旧（同样的输入停在同一处，超了照样报超时）；只是不按墙钟掐。
/// 墙钟兜底在生产是秒级；测试档的 Rhai 慢，几路一起跑、机器一忙，一判就可能过了它 ⇒ 那一判走兜底，比的就不是同一份策略。
/// 这里测的是运算量，墙钟只是安全网：挂住的由测试程序统一的上限兜。
fn test_engine() -> &'static rhai::Engine {
    static E: OnceLock<rhai::Engine> = OnceLock::new();
    E.get_or_init(|| {
        let mut e = crate::accounts::quota::strategy::fresh_engine();
        e.on_progress(|_| None);
        script::with_lib(e).expect("积木库编得过")
    })
}

/// 权重恒 0：照号列表先后（升级材料 1）。
fn by_list() -> &'static Material {
    static F: OnceLock<Material> = OnceLock::new();
    F.get_or_init(|| material(0))
}

/// 离 5 小时重置越近权重越高（升级材料 2）。
fn by_reset() -> &'static Material {
    static F: OnceLock<Material> = OnceLock::new();
    F.get_or_init(|| material(1))
}

/// 余量多的权重高（升级材料 3）。
fn by_left() -> &'static Material {
    static F: OnceLock<Material> = OnceLock::new();
    F.get_or_init(|| material(2))
}

fn acct(id: &str, used5h: f64) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used5h, 0.0],
        resets: [None, None],
        refused_until: None,
        barred: None,
        rate: [None, None],
    }
}

fn refused(mut a: Acct, until: u64) -> Acct {
    a.refused_until = Some(until);
    a
}

/// A > B > C 同一排，都是软上限 90%，D = 0。
fn base() -> Value {
    json!({
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}], "7d": [{"from": 0, "to": 1440, "v": 90}]},
        "capKind": "soft",
        "accounts": [{"id": "A"}, {"id": "B"}, {"id": "C"}]
    })
}

fn set_row(s: &mut Value, id: &str, k: &str, v: Value) {
    for r in s["accounts"].as_array_mut().expect("accounts") {
        if r["id"] == id {
            r[k] = v.clone();
        }
    }
}

fn sig(current: &str, temp: &[&str]) -> Sigma {
    Sigma {
        start: "A".into(),
        current: current.into(),
        temp: temp.iter().map(|s| s.to_string()).collect(),
        ..Sigma::default()
    }
}

fn used(d: &Decision) -> &str {
    match &d.pick {
        Pick::Use(a) => a,
        Pick::Hold { .. } => panic!("该挑一个号，实得停发：{d:?}"),
    }
}

fn run(s: &Value, sigma: &Sigma, f: &[Acct], t: u64) -> Decision {
    turn(sigma, f, s, by_list(), clock(t), t, &[])
}

/// 判据用的假策略：固定的先后 · 约的叫醒。
#[derive(Default)]
struct Fixed {
    order: Vec<&'static str>,
    wake_at: Option<u64>,
    wake_when: Vec<WakeWhen>,
    broken: bool,
}

impl Strategy for Fixed {
    fn decide(&self, _: &Ctx<'_>) -> Result<Verdict, Fail> {
        if self.broken {
            return Err(Fail {
                kind: FailKind::Broke,
                line: Some(3),
                col: None,
                msg: "boom".into(),
                why: crate::accounts::quota::strategy::FailWhy::Other,
            });
        }
        Ok(Verdict {
            order: self.order.iter().map(|s| s.to_string()).collect(),
            wake_at: self.wake_at,
            wake_when: self.wake_when.clone(),
            ..Verdict::default()
        })
    }
}

// ── 策：升级材料 ＋ 积木库 ─────────────────────────────────────────────────────

#[test]
fn s01_never_switches_to_an_account_that_cannot_be_used() {
    let f = [
        acct("A", 90.0),
        refused(acct("B", 10.0), T + 30 * MIN),
        acct("C", 10.0),
    ];
    assert_eq!(used(&run(&base(), &sig("A", &[]), &f, T)), "C");
}

#[test]
fn s02_the_list_order_when_nothing_else_matters() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    assert_eq!(used(&run(&base(), &sig("A", &[]), &f, T)), "B");
}

fn with_reset(id: &str, used5h: f64, in_min: u64) -> Acct {
    Acct {
        resets: [Some(T + in_min * MIN), None],
        ..acct(id, used5h)
    }
}

/// ★ 用满了换谁：权重高的先（离重置近的）。
#[test]
fn s03_the_weight_picks_who_is_next() {
    let f = [
        acct("A", 95.0),
        with_reset("B", 50.0, 200),
        with_reset("C", 10.0, 30),
    ];
    let d = turn(&sig("A", &[]), &f, &base(), by_reset(), clock(T), T, &[]);
    assert_eq!(used(&d), "C", "离重置近的权重高");
    let d = turn(&sig("A", &[]), &f, &base(), by_left(), clock(T), T, &[]);
    assert_eq!(used(&d), "C", "余量多的权重高");
}

/// ★ 同权照号列表先后：权重一样；或较高的超出较低的不到权重容差。
#[test]
fn s04_equal_weights_follow_the_account_list() {
    let f = [
        acct("A", 95.0),
        with_reset("B", 30.0, 60),
        with_reset("C", 30.0, 60),
    ];
    let d = turn(&sig("A", &[]), &f, &base(), by_reset(), clock(T), T, &[]);
    assert_eq!(used(&d), "B");
    let f = [
        acct("A", 95.0),
        with_reset("B", 30.0, 60),
        with_reset("C", 30.0, 50),
    ];
    let d = turn(&sig("A", &[]), &f, &base(), by_reset(), clock(T), T, &[]);
    assert_eq!(used(&d), "C", "容差 0：C 高 20% 就排前");
    let mut s = base();
    s["close"] = json!(25);
    let d = turn(&sig("A", &[]), &f, &s, by_reset(), clock(T), T, &[]);
    assert_eq!(used(&d), "B", "容差 25%：C 只高 20%，算同权，照号列表");
}

/// ★ 在用的号没到用量上限就不换：权重再高的号也不换过去（没设强制切换阈值）。
#[test]
fn s06_an_account_under_its_limit_is_kept() {
    let f = [acct("A", 10.0), acct("B", 50.0), acct("C", 10.0)];
    assert_eq!(used(&run(&base(), &sig("B", &[]), &f, T)), "B");
    let f = [
        with_reset("A", 10.0, 5),
        with_reset("B", 50.0, 200),
        acct("C", 10.0),
    ];
    let d = turn(&sig("B", &[]), &f, &base(), by_reset(), clock(T), T, &[]);
    assert_eq!(used(&d), "B", "快重置的权重高，也不换");
}

fn forced(pct: u64) -> Value {
    let mut s = base();
    s["force"] = json!(pct);
    s
}

/// ★ 强制切换阈值：在用的号没满时，只有重置比它早、权重超出它这一比例的号直接换过去。
#[test]
fn s07_a_forced_switch_needs_an_earlier_reset_and_the_threshold() {
    let f = [
        with_reset("A", 10.0, 5),
        with_reset("B", 50.0, 200),
        acct("C", 10.0),
    ];
    let d = turn(
        &sig("B", &[]),
        &f,
        &forced(25),
        by_reset(),
        clock(T),
        T,
        &[],
    );
    assert_eq!(used(&d), "A", "A 先重置、权重高出远超 25%");
    assert_eq!(d.order.first().map(String::as_str), Some("A"));
    // 余量：B 剩得多但后重置 ⇒ 不换；C 先重置、剩得多 ⇒ 换。
    let f = [
        with_reset("A", 80.0, 30),
        with_reset("B", 0.0, 200),
        with_reset("C", 10.0, 10),
    ];
    let d = turn(&sig("A", &[]), &f, &forced(25), by_left(), clock(T), T, &[]);
    assert_eq!(used(&d), "C");
    let f = [
        with_reset("A", 80.0, 30),
        with_reset("B", 0.0, 200),
        with_reset("C", 10.0, 40),
    ];
    let d = turn(&sig("A", &[]), &f, &forced(25), by_left(), clock(T), T, &[]);
    assert_eq!(used(&d), "A", "后重置的不换过去");
    // 超出不到阈值 ⇒ 不换：A 剩 10、C 剩 12（高 20%）。
    let f = [
        with_reset("A", 80.0, 30),
        with_reset("B", 90.0, 200),
        with_reset("C", 78.0, 10),
    ];
    let d = turn(&sig("A", &[]), &f, &forced(25), by_left(), clock(T), T, &[]);
    assert_eq!(used(&d), "A", "只高 20%，不到 25%");
}

fn fallback(s: &mut Value, id: &str) {
    set_row(s, id, "fallback", json!(true));
}

/// ★ 耗尽后启用：其余的号都没额度才用它；其余的号一有额度就换走（它没满也换）。
#[test]
fn s08_a_fallback_is_used_only_while_the_others_are_out() {
    let mut s = base();
    fallback(&mut s, "A");
    let f = [acct("A", 10.0), acct("B", 50.0), acct("C", 10.0)];
    assert_eq!(
        used(&run(&s, &sig("A", &[]), &f, T)),
        "B",
        "列表最前也排最后"
    );
    let out = [acct("A", 10.0), acct("B", 95.0), acct("C", 95.0)];
    assert_eq!(used(&run(&s, &sig("B", &[]), &out, T)), "A");
    let back = [acct("A", 20.0), acct("B", 95.0), acct("C", 10.0)];
    assert_eq!(
        used(&run(&s, &sig("A", &[]), &back, T)),
        "C",
        "C 一有额度就换走"
    );
}

/// ★ 强制切换之后，被换下来的号重置得晚，不会再换回去（不来回换）。
#[test]
fn s25_after_a_forced_switch_the_later_one_does_not_come_back() {
    let s = forced(25);
    let f = [with_reset("A", 80.0, 60), with_reset("B", 10.0, 20)];
    let d = turn(&sig("A", &[]), &f, &s, by_left(), clock(T), T, &[]);
    assert_eq!(used(&d), "B");
    let f = [with_reset("A", 80.0, 60), with_reset("B", 85.0, 20)];
    let d = turn(
        &d.sigma,
        &f,
        &s,
        by_left(),
        clock(T + 10 * MIN),
        T + 10 * MIN,
        &[],
    );
    assert_eq!(used(&d), "B", "A 剩得多了，但重置得晚，B 没满不换");
    let f = [with_reset("A", 80.0, 60), with_reset("B", 90.0, 20)];
    let d = turn(
        &d.sigma,
        &f,
        &s,
        by_left(),
        clock(T + 15 * MIN),
        T + 15 * MIN,
        &[],
    );
    assert_eq!(used(&d), "A", "B 满了才换：{d:?}");
}

#[test]
fn s09_activation_delay_holds_then_lets_it_in() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "delay", json!(30));
    let f = [acct("A", 95.0), acct("B", 95.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &f, T);
    assert!(
        matches!(d.pick, Pick::Hold { until: Some(u), .. } if u == T + 30 * MIN),
        "{d:?}"
    );
    assert_eq!(d.tn, Some(T + 30 * MIN), "延迟到点是策略约的时刻");
    assert_eq!(
        used(&run(&s, &d.sigma, &f, T + 30 * MIN)),
        "C",
        "起点记在 memo 里，原样交回"
    );
}

/// ★ 两个耗尽后启用的号启用延迟一起到点：到点前一直停发（它们此刻都有额度，只是在延迟里），到点取号列表前面那个。
#[test]
fn s10_two_delays_ending_together_take_the_front_one() {
    let mut s = base();
    for id in ["B", "C"] {
        fallback(&mut s, id);
        set_row(&mut s, id, "delay", json!(30));
    }
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &f, T);
    assert!(
        matches!(d.pick, Pick::Hold { until: Some(u), .. } if u == T + 30 * MIN),
        "B · C 都在启用延迟里：停到一起到点的那一刻，不当场用：{d:?}"
    );
    let d = run(&s, &d.sigma, &f, T + 29 * MIN);
    assert!(matches!(d.pick, Pick::Hold { .. }), "还差一分钟：{d:?}");
    assert_eq!(used(&run(&s, &d.sigma, &f, T + 30 * MIN)), "B");
}

#[test]
fn s11_a_break_in_the_outage_restarts_the_delay() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "delay", json!(30));
    let bad = [acct("A", 95.0), acct("B", 95.0), acct("C", 10.0)];
    let good = [acct("A", 10.0), acct("B", 95.0), acct("C", 10.0)];
    let d = run(&s, &sig("A", &[]), &bad, T);
    let d = run(&s, &d.sigma, &good, T + 10 * MIN);
    assert_eq!(used(&d), "A");
    let d = run(&s, &d.sigma, &bad, T + 20 * MIN);
    assert!(matches!(d.pick, Pick::Hold { .. }));
    assert!(
        matches!(
            run(&s, &d.sigma, &bad, T + 30 * MIN).pick,
            Pick::Hold { .. }
        ),
        "从再次用不了时重新计"
    );
    assert_eq!(used(&run(&s, &d.sigma, &bad, T + 50 * MIN)), "C");
}

/// ★ 单次用量上限：换到耗尽后启用的号以后，5 小时用量涨够这一比例就不再用它，直到其余的号恢复。
#[test]
fn s34_a_per_use_limit_stops_a_fallback_until_the_others_recover() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "stint", json!(20));
    let at = |c: f64| [acct("A", 95.0), acct("B", 95.0), acct("C", c)];
    let d = run(&s, &sig("A", &[]), &at(10.0), T);
    assert_eq!(used(&d), "C");
    assert!(
        d.sigma
            .wake_when
            .iter()
            .any(|w| w.acct == "C" && (w.used_at_least - 30.0).abs() < 1e-9),
        "涨够的那一读数约来问：{:?}",
        d.sigma.wake_when
    );
    let d = run(&s, &d.sigma, &at(25.0), T + 10 * MIN);
    assert_eq!(used(&d), "C", "涨了 15，还没够");
    let d = run(&s, &d.sigma, &at(30.0), T + 20 * MIN);
    assert!(matches!(d.pick, Pick::Hold { .. }), "涨够 20：{d:?}");
    let back = [acct("A", 10.0), acct("B", 95.0), acct("C", 30.0)];
    let d = run(&s, &d.sigma, &back, T + 30 * MIN);
    assert_eq!(used(&d), "A");
    let d = run(&s, &d.sigma, &at(30.0), T + 40 * MIN);
    assert_eq!(used(&d), "C", "其余的号恢复过一次，账清了，从头记");
}

/// ★ 单次用量上限：中途窗口重置了 ⇒ 新窗口从 0 接着记（重置前涨的照算）。
#[test]
fn s35_a_per_use_limit_keeps_counting_across_a_reset() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "stint", json!(20));
    let c = |u: f64, r: u64| [acct("A", 95.0), acct("B", 95.0), with_reset("C", u, r)];
    let d = run(&s, &sig("A", &[]), &c(70.0, 30), T);
    assert_eq!(used(&d), "C");
    let d = run(&s, &d.sigma, &c(80.0, 30), T + 10 * MIN);
    assert_eq!(used(&d), "C", "涨了 10");
    let d = run(&s, &d.sigma, &c(5.0, 330), T + 40 * MIN);
    assert_eq!(used(&d), "C", "重置后 5：一共 15");
    let d = run(&s, &d.sigma, &c(10.0, 330), T + 50 * MIN);
    assert!(matches!(d.pick, Pick::Hold { .. }), "一共 20：{d:?}");
}

#[test]
fn s14_a_dragged_order_does_not_stop_a_forced_switch() {
    let f = [
        with_reset("A", 10.0, 5),
        with_reset("B", 10.0, 200),
        acct("C", 10.0),
    ];
    let d = turn(
        &sig("B", &["B"]),
        &f,
        &forced(25),
        by_reset(),
        clock(T),
        T,
        &[],
    );
    assert_eq!(used(&d), "A", "拖出来的顺序只在在用的号该换时生效");
}

#[test]
fn s15_soft_caps_hold_while_some_account_is_under_its_cap() {
    assert_eq!(
        used(&run(
            &base(),
            &sig("A", &[]),
            &[acct("A", 92.0), acct("B", 95.0), acct("C", 10.0)],
            T
        )),
        "C"
    );
}

#[test]
fn s16_soft_caps_lift_when_every_account_is_at_its_cap() {
    let f = [
        acct("A", 92.0),
        acct("B", 95.0),
        refused(acct("C", 10.0), T + 3600),
    ];
    assert_eq!(used(&run(&base(), &sig("A", &[]), &f, T)), "A");
}

#[test]
fn s17_hard_caps_never_lift() {
    let mut s = base();
    set_row(&mut s, "A", "capKind", json!("hard"));
    let f = [
        acct("A", 92.0),
        acct("B", 95.0),
        refused(acct("C", 10.0), T + 3600),
    ];
    assert_eq!(used(&run(&s, &sig("A", &[]), &f, T)), "B");
}

fn all_hard(resets: [u64; 3]) -> Decision {
    let mut s = base();
    s["capKind"] = json!("hard");
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
    assert!(
        matches!(d.pick, Pick::Hold { until: Some(u), .. } if u == T + 20 * MIN),
        "{d:?}"
    );
}

#[test]
fn s19_holding_uses_the_strategys_hold_until() {
    let d = all_hard([T + 90 * MIN, T + 20 * MIN, T + 20 * MIN]);
    assert!(
        matches!(d.pick, Pick::Hold { until: Some(u), .. } if u == T + 20 * MIN),
        "{d:?}"
    );
}

#[test]
fn s20_a_zero_percent_segment_skips_the_account_until_it_ends() {
    let mut s = base();
    set_row(
        &mut s,
        "B",
        "caps",
        json!({"5h": [{"from": 0, "to": 1200, "v": 90}, {"from": 1200, "to": 1380, "v": 0}, {"from": 1380, "to": 1440, "v": 90}]}),
    );
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    let at = DAY + 20 * 3600 + 30 * 60;
    let d = run(&s, &sig("A", &[]), &f, at);
    assert_eq!(used(&d), "C");
    assert!(
        d.tn.is_some_and(|n| n <= DAY + 23 * 3600),
        "23:00 那条边界是策略约的时刻：{:?}",
        d.tn
    );
    assert_eq!(used(&run(&s, &sig("A", &[]), &f, DAY + 23 * 3600)), "B");
}

/// ★ 耗尽后启用的号只在节点上换走：其余的号被拒到期那一刻（已知节点）才换回去，之前一发都不换。
#[test]
fn s29_leaving_a_fallback_happens_only_at_a_known_node() {
    let mut s = base();
    fallback(&mut s, "B");
    let f = [
        refused(acct("A", 50.0), T + 30 * MIN),
        acct("B", 10.0),
        acct("C", 95.0),
    ];
    let d = run(&s, &sig("B", &[]), &f, T);
    assert_eq!((used(&d), d.tn), ("B", Some(T + 30 * MIN)));
    assert_eq!(used(&run(&s, &sig("B", &[]), &f, T + 30 * MIN - 1)), "B");
    assert_eq!(used(&run(&s, &sig("B", &[]), &f, T + 30 * MIN)), "A");
}

/// ★ #32 不来回换：余量多的权重高，强制切换阈值只有 5%（阈值本身几乎没有回差）。
/// 别的会话一直在用 B：它的窗口常开着、用了一截，跟 A 重置的时刻挨着。这个会话用哪个号，哪个号就多涨这个会话的那一份。
/// 换到 B（先重置、余量多）以后，A 的余量很快就又超过 B —— 只靠「重置得晚的号权重超不过在用的号」才不换回去。
/// 逐发每分钟判一次，走 12 小时：换号次数有上限、两次换号不挨着。
#[test]
fn s32_never_flip_flops() {
    let s = json!({
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]}, "capKind": "hard", "force": 5,
        "accounts": [{"id": "A"}, {"id": "B"}]
    });
    let ours = 0.5;
    let other = |id: &str| if id == "B" { 0.3 } else { 0.0 };
    let mut f = vec![with_reset("A", 40.0, 120), with_reset("B", 30.0, 100)];
    let (mut sigma, mut using, mut sw) = (sig("A", &[]), Some("A".to_string()), Vec::new());
    let mut t = T;
    while t < T + 12 * 3600 {
        let d = turn(&sigma, &f, &s, by_left(), clock(t), t, &[]);
        sigma = d.sigma.clone();
        let now = match &d.pick {
            Pick::Use(a) => Some(a.clone()),
            Pick::Hold { .. } => None,
        };
        if now.is_some() && now != using {
            sw.push((t, now.clone().unwrap_or_default()));
        }
        if now.is_some() {
            using = now.clone();
        }
        // 走一分钟：在用的号多涨这个会话的那一份；有人在用的号窗口开着，到点重置、接着计。
        for x in f.iter_mut() {
            let mine = now.as_deref() == Some(x.id.as_str());
            let r = other(&x.id) + if mine { ours } else { 0.0 };
            if x.resets[0].is_some_and(|z| z <= t + MIN) {
                x.used[0] = 0.0;
                x.resets[0] = None;
            }
            if r > 0.0 && x.resets[0].is_none() {
                x.resets[0] = Some(t + MIN + WINDOW_SECS[0]);
            }
            if x.resets[0].is_some() {
                x.used[0] = (x.used[0] + r).min(100.0);
            }
        }
        t += MIN;
    }
    assert!(sw.len() >= 3, "本条在空转：{sw:?}");
    assert!(sw.len() <= 8, "12 小时换了 {} 次：{sw:?}", sw.len());
    assert!(
        sw.windows(2).all(|w| w[1].0 - w[0].0 >= 30 * MIN),
        "两次换号挨得太近：{sw:?}"
    );
}

// ── 后：只测后端积木 ────────────────────────────────────────────────────────────

#[test]
fn s05_a_broken_strategy_falls_back_to_the_current_or_the_first_sendable() {
    let p = Fixed {
        broken: true,
        ..Fixed::default()
    };
    let f = [acct("A", 10.0), acct("B", 10.0)];
    let d = turn(&sig("B", &[]), &f, &base(), &p, 0, T, &[]);
    assert_eq!(used(&d), "B", "此刻的号发得出去就照用");
    assert!(d.strategy_error.is_some());
    let f = [acct("A", 10.0), refused(acct("B", 10.0), T + 60)];
    assert_eq!(used(&turn(&sig("B", &[]), &f, &base(), &p, 0, T, &[])), "A");
}

#[test]
fn s12_a_dragged_entry_that_cannot_be_used_is_skipped() {
    let p = Fixed {
        order: vec!["B", "C"],
        ..Fixed::default()
    };
    let f = [
        acct("A", 10.0),
        refused(acct("B", 10.0), T + 3600),
        acct("C", 10.0),
    ];
    let d = turn(&sig("A", &["A", "B", "C"]), &f, &base(), &p, 0, T, &[]);
    assert_eq!(
        (used(&d), d.sigma.temp.clone(), d.skipped.clone()),
        ("C", vec!["C".to_string()], vec!["B".to_string()])
    );
    assert!(d.from_temp);
}

#[test]
fn s13_a_dragged_entry_stays_until_the_strategy_drops_it() {
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)];
    let keep = Fixed {
        order: vec!["C", "A", "B"],
        ..Fixed::default()
    };
    let d = turn(&sig("C", &["C"]), &f, &base(), &keep, 0, T, &[]);
    assert_eq!(
        (used(&d), d.sigma.temp.clone()),
        ("C", vec!["C".to_string()])
    );
    let drop = Fixed {
        order: vec!["A", "B"],
        ..Fixed::default()
    };
    let d = turn(&d.sigma, &f, &base(), &drop, 0, T + MIN, &[]);
    assert_eq!(used(&d), "A");
    assert!(d.sigma.temp.is_empty());
}

/// 判据用的假策略：先后照此刻在用的是谁（`by_current[在用的号]`，没写 ⇒ `rest`）。
struct ByCurrent {
    by_current: Vec<(&'static str, Vec<&'static str>)>,
    rest: Vec<&'static str>,
    calls: std::cell::Cell<usize>,
}

impl Strategy for ByCurrent {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        self.calls.set(self.calls.get() + 1);
        let o = self
            .by_current
            .iter()
            .find(|(c, _)| *c == ctx.current)
            .map_or(&self.rest, |(_, o)| o);
        Ok(Verdict {
            order: o.iter().map(|s| s.to_string()).collect(),
            ..Verdict::default()
        })
    }
}

/// ★ #37 换了号、照新的在用号再问策略，它此刻就又要换（例：拖出来的顺序选中的号，策略照强制切换要换走）⇒ 同一刻接着换，
/// 不留到下一发（不然推演从这一刻起说的是「此刻换走」，逐发那边却等到下一发、事实变了换到别的号）。
#[test]
fn s37_a_switch_the_strategy_would_undo_at_once_is_followed_at_once() {
    let p = ByCurrent {
        by_current: vec![("B", vec!["C", "B"])],
        rest: vec!["C", "B"],
        calls: Default::default(),
    };
    let f = [
        refused(acct("A", 10.0), T + 3600),
        acct("B", 10.0),
        acct("C", 10.0),
    ];
    let d = turn(&sig("A", &["B"]), &f, &base(), &p, 0, T, &[]);
    assert_eq!(
        used(&d),
        "C",
        "拖出来的顺序先选中 B，策略对着 B 当场要 C：{d:?}"
    );
    assert_eq!(d.sigma.current, "C");
    assert!(!d.from_temp, "最后用的号不是拖出来的那个");
    assert_eq!(
        d.sigma.temp,
        vec!["B".to_string()],
        "B 还能用，留在拖出来的顺序里"
    );
    let again = turn(&d.sigma, &f, &base(), &p, 0, T, &[]);
    assert_eq!(used(&again), "C", "同一刻再判一次不再换");
    // 策略对着谁都要换到另一个（来回）⇒ 同一刻里一个号只换到一次，不打转。
    let flip = ByCurrent {
        by_current: vec![("B", vec!["C", "B"]), ("C", vec!["B", "C"])],
        rest: vec!["B", "C"],
        calls: Default::default(),
    };
    let d = turn(&sig("A", &[]), &f, &base(), &flip, 0, T, &[]);
    assert!(matches!(d.pick, Pick::Use(_)), "{d:?}");
    assert!(
        flip.calls.get() <= f.len() + 1,
        "问了策略 {} 次",
        flip.calls.get()
    );
}

#[test]
fn s21_use_now_only_refuses_what_cannot_be_sent() {
    let f = [
        acct("A", 10.0),
        Acct {
            resets: [Some(T + 3600), None],
            ..acct("B", 100.0)
        },
        acct("C", 95.0),
    ];
    assert_eq!(
        use_now(&sig("A", &[]), &f, T, "B").err(),
        Some(Some(T + 3600))
    );
    let s = use_now(&sig("A", &[]), &f, T, "C").expect("C 只是到了策略的上限，照换");
    assert_eq!(
        (s.current.as_str(), s.temp.clone()),
        ("C", vec!["C".to_string()])
    );
}

#[test]
fn s23_simulation_stops_at_the_event_cap() {
    let p = Fixed {
        order: vec!["A"],
        wake_at: None,
        ..Fixed::default()
    };
    struct Every(Fixed);
    impl Strategy for Every {
        fn decide(&self, c: &Ctx<'_>) -> Result<Verdict, Fail> {
            let mut v = self.0.decide(c)?;
            v.wake_at = Some(c.now + 60);
            Ok(v)
        }
    }
    let tl = simulate(
        &sig("A", &[]),
        &[acct("A", 1.0)],
        &base(),
        &Every(p),
        &clock,
        T,
        T + 30 * 86_400,
    );
    assert!(tl.truncated);
    assert_eq!(
        tl.nodes.len(),
        crate::accounts::quota::rules::simulate::MAX_EVENTS
    );
}

#[test]
fn s24_turn_is_pure() {
    let f = [acct("A", 95.0), acct("B", 10.0), acct("C", 10.0)];
    assert_eq!(
        run(&base(), &sig("A", &["B"]), &f, T),
        run(&base(), &sig("A", &["B"]), &f, T)
    );
}

#[test]
fn s27_a_refused_send_skips_what_was_tried() {
    let f = [acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)];
    let p = Fixed {
        order: vec!["B", "A", "C"],
        ..Fixed::default()
    };
    let d = turn(&sig("B", &[]), &f, &base(), &p, 0, T, &["B".to_string()]);
    assert_eq!(used(&d), "A");
}

/// ★ #30 按时刻叫醒：策略约的时刻就是下一个节点；推演在那一刻多一个事件。
#[test]
fn s30_a_wake_at_becomes_the_next_node() {
    let p = Fixed {
        order: vec!["A"],
        wake_at: Some(T + 600),
        ..Fixed::default()
    };
    let f = [acct("A", 10.0)];
    assert_eq!(
        turn(&sig("A", &[]), &f, &base(), &p, 0, T, &[]).tn,
        Some(T + 600)
    );
    let tl = simulate(&sig("A", &[]), &f, &base(), &p, &clock, T, T + 900);
    assert!(tl.nodes.contains(&(T + 600)), "{:?}", tl.nodes);
}

/// ★ #31 按读数叫醒：在用的号读数按用速换算成时刻（80 → 90、每分 1 ⇒ 10 分钟后）。
#[test]
fn s31_a_wake_on_reading_becomes_a_time_in_the_simulation() {
    let p = Fixed {
        order: vec!["B"],
        wake_when: vec![WakeWhen {
            acct: "B".into(),
            w: 0,
            used_at_least: 90.0,
        }],
        ..Fixed::default()
    };
    let b = Acct {
        rate: [Some(1.0), Some(0.0)],
        resets: [Some(T + 5 * 3600), None],
        ..acct("B", 80.0)
    };
    let tl = simulate(&sig("B", &[]), &[b], &base(), &p, &clock, T, T + 3600);
    assert!(tl.nodes.contains(&(T + 600)), "{:?}", tl.nodes);
}

// ── #22 一致性：推演说的下一次换号 ＝ 逐发判出来的那一次（照列表先后 ＋ 积木库） ─────────

/// 一个请求间隔（每分钟一发）＋ 推演里到点时刻向上取整的那一秒。
const TOL: u64 = 61;

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

fn group(seed: u64) -> (Value, Vec<Acct>, Sigma) {
    let mut g = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let n = 3 + g.upto(6) as usize;
    let ids: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
    let seg = |g: &mut Rng| -> Value {
        if g.upto(3) == 0 {
            let a = 60 * (1 + g.upto(20));
            let b = (a + 60 * (1 + g.upto(3))).min(1440);
            json!([{"from": 0, "to": a, "v": 80 + g.upto(20)}, {"from": a, "to": b, "v": g.upto(2) * 50}, {"from": b, "to": 1440, "v": 80 + g.upto(20)}])
        } else {
            json!([{"from": 0, "to": 1440, "v": 70 + g.upto(30)}])
        }
    };
    let rows: Vec<Value> = ids
        .iter()
        .map(|a| {
            let mut r = json!({"id": a});
            if g.upto(3) == 0 {
                r["caps"] = json!({"5h": seg(&mut g)});
            }
            if g.upto(3) == 0 {
                r["fallback"] = json!(true);
                r["delay"] = json!([0, 0, 10, 30][g.upto(4) as usize]);
                if g.upto(2) == 0 {
                    r["stint"] = json!(10 + g.upto(30));
                }
            }
            r
        })
        .collect();
    let s = json!({
        "caps": {"5h": seg(&mut g), "7d": [{"from": 0, "to": 1440, "v": 95}]},
        "capKind": if g.upto(2) == 0 { "soft" } else { "hard" },
        "accounts": rows
    });
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
        ..Sigma::default()
    };
    (s, f, sigma)
}

/// 照估计往前走一分钟（同推演：量得到用速的号照它涨（窗口开着的）· 到点重置 · 被拒到期 · 开始用时窗口开始计时；重置落在这一分钟里照拆开算）。
fn step(f: &mut [Acct], using: Option<&str>, t: u64, dt: u64) {
    for x in f.iter_mut() {
        let in_use = using == Some(x.id.as_str());
        for w in 0..2 {
            let rate = x.rate[w].unwrap_or(0.0);
            if in_use && x.resets[w].is_none_or(|r| r <= t) {
                x.used[w] = standing::used_at(x, w, t);
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
                Some(_) => x.used[w] += rate * dt as f64 / 60.0,
                None => {}
            }
        }
        if x.refused_until.is_some_and(|u| u <= t + dt) {
            x.refused_until = None;
        }
    }
}

/// 推演里预计那一刻前后一个间隔以内还有别的节点 ⇒ 逐发那一边分不出先后。
fn crowded(tl: &Timeline, at: u64) -> bool {
    tl.nodes.iter().any(|n| *n != at && n.abs_diff(at) <= TOL)
}

/// 逐发（每分钟一发）判；每过一个节点、每次真换号都重推一次（同真用）：推演说的下一次换号与真的那一次号相同、时刻差 ≤ 一个间隔
/// （换到的号有启用延迟 ⇒ 两个）。挤在一起分不出先后的单数出来。回（比过几次，挤的几次，比过的里强制切换的几次）。
fn consistent(
    s: &Value,
    f0: &[Acct],
    sigma0: &Sigma,
    p: &dyn Strategy,
    label: &str,
) -> (usize, usize, usize) {
    let delay = |a: &str| {
        s["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|r| r["id"] == a && r["delay"].as_u64().unwrap_or(0) > 0)
    };
    let (mut f, mut sigma, mut t) = (f0.to_vec(), sigma0.clone(), T);
    let mut using = Some(sigma.current.clone());
    let fresh =
        |sigma: &Sigma, f: &[Acct], t: u64| simulate(sigma, f, s, p, &clock, t, t + 12 * 3600);
    // 推演说的换号时刻 pt 落在上一发与这一发之间（逐发每分钟才判一次，一个间隔里先后发生的两件事分不出）：
    // 把上一发的事实照估计推到 pt、在 pt 真判一次 —— 判出来就是推演说的那个号 ⇒ 两边一致，只是逐发看不见那一刻。
    let between = |last: &Option<(Vec<Acct>, u64, Option<String>)>,
                   sigma_in: &Sigma,
                   pt: u64,
                   t: u64,
                   pa: &str| {
        last.as_ref().is_some_and(|(fp, tp, up)| {
            *tp < pt && pt <= t && {
                let mut g = fp.clone();
                step(&mut g, up.as_deref(), *tp, pt - tp);
                matches!(&turn(sigma_in, &g, s, p, clock(pt), pt, &[]).pick, Pick::Use(x) if x == pa)
            }
        })
    };
    let mut tl = fresh(&sigma, &f, t);
    let mut next_node = turn(&sigma, &f, s, p, clock(t), t, &[]).tn;
    let (mut checked, mut crowd, mut forced) = (0, 0, 0);
    // 上一发那一刻的状态（事实 · 时刻 · 在用的号）与这一判之前的会话状态：推演说的换号落在两发之间时，照那一刻真判一次。
    let mut last: Option<(Vec<Acct>, u64, Option<String>)> = None;
    while t < T + 4 * 3600 && checked + crowd < 4 {
        let sigma_in = sigma.clone();
        let d = turn(&sigma, &f, s, p, clock(t), t, &[]);
        assert!(
            d.strategy_error.is_none(),
            "{label}：{t} 策略出错 {:?}，走了兜底 —— 比的不是同一份策略",
            d.strategy_error
        );
        sigma = d.sigma.clone();
        let now_using = match &d.pick {
            Pick::Use(a) => Some(a.clone()),
            Pick::Hold { .. } => None,
        };
        let mut refresh = next_node.is_some_and(|n| t >= n);
        if let Some(a) = &now_using {
            if using.as_deref().is_some_and(|u| u != a) {
                let (pt, pa) = tl
                    .switches()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| panic!("{label}：{t} 真换到 {a}，推演没说会换"));
                let tol = if delay(a) { 2 * TOL } else { TOL };
                if pa == *a && t.abs_diff(pt) <= tol {
                    checked += 1;
                    forced += usize::from(d.why.iter().any(|(id, w)| id == a && w == "强制切换"));
                } else if crowded(&tl, pt) || between(&last, &sigma_in, pt, t, &pa) {
                    crowd += 1;
                } else {
                    let near: Vec<String> = tl
                        .uses
                        .iter()
                        .filter(|u| u.to + 600 >= pt && u.from <= t + 600)
                        .map(|u| format!("{}–{} {:?}", u.from, u.to, u.pick))
                        .collect();
                    panic!(
                        "{label}：真 {t} 换到 {a} · 推演 {pt} 换到 {pa}，差 {} 秒；推演那几段：{near:?}；起点 {:?}",
                        t.abs_diff(pt),
                        tl.uses.first().map(|u| u.from)
                    );
                }
                refresh = true;
            }
            using = Some(a.clone());
        }
        if refresh {
            tl = fresh(&sigma, &f, t);
            next_node = d.tn;
        }
        last = Some((f.clone(), t, now_using.clone()));
        step(&mut f, now_using.as_deref(), t, 60);
        t += 60;
    }
    (checked, crowd, forced)
}

/// #22 跑 `n` 组（照列表先后）：回（比过几次，挤的几次）。
fn s22_groups(n: u64) -> (usize, usize) {
    let (mut total, mut crowd) = (0, 0);
    for seed in 1..=n {
        let (s, f, sigma) = group(seed);
        let (c, k, _) = consistent(&s, &f, &sigma, by_list(), &format!("第 {seed} 组"));
        total += c;
        crowd += k;
    }
    assert!(
        total > n as usize,
        "{n} 组里只比到 {total} 次换号 —— 本条在空转"
    );
    assert!(
        crowd * 20 <= total,
        "挤在一起分不出先后的有 {crowd} 次（比过 {total} 次），超过 5%"
    );
    (total, crowd)
}

/// #26 跑 `n` 组：强制切换（余量多的权重高 ＋ 强制切换阈值 25% ＋ 权重容差 10%）。耗尽后启用的号不起强制切换，这里都去掉，
/// 换号就只有两种：用满了照先后换 · 权重交叉、强制切换（靠策略约的按时刻 · 按读数叫醒，推演才知道那一刻）。
/// 回（比过几次，挤的几次，其中强制切换几次）。
fn s26_groups(n: u64) -> (usize, usize, usize) {
    let (mut total, mut crowd, mut forced) = (0, 0, 0);
    for seed in 1..=n {
        let (c, k, x) = s26_one(seed);
        total += c;
        crowd += k;
        forced += x;
    }
    assert!(total > n as usize - 1, "比到的换号太少：{total}");
    assert!(crowd * 20 <= total.max(20), "挤在一起的有 {crowd} 次");
    assert!(
        forced * 5 >= total,
        "比过的 {total} 次换号里强制切换只有 {forced} 次 —— 本条比的不是强制切换"
    );
    (total, crowd, forced)
}

fn s26_one(seed: u64) -> (usize, usize, usize) {
    let (mut s, f, sigma) = group(seed);
    for r in s["accounts"].as_array_mut().expect("accounts") {
        let r = r.as_object_mut().expect("一行");
        for k in ["fallback", "delay", "stint"] {
            r.remove(k);
        }
    }
    s["force"] = json!(25);
    s["close"] = json!(10);
    consistent(
        &s,
        &f,
        &sigma,
        by_left(),
        &format!("第 {seed} 组（余量 · 强制切换）"),
    )
}

/// #36 跑 `n` 组：耗尽后启用的号照余量排（权重容差 10%），在用的那个用着用着被别的耗尽后启用的号超过 ⇒ 换。
/// 这一种换号也靠策略约的叫醒（500 组里第 19 组：没约时推演晚了 969 秒）。
fn s36_groups(n: u64) -> (usize, usize) {
    let (mut total, mut crowd) = (0, 0);
    for seed in 1..=n {
        let (c, k) = s36_one(seed);
        total += c;
        crowd += k;
    }
    assert!(crowd * 20 <= total.max(20), "挤在一起的有 {crowd} 次");
    (total, crowd)
}

fn s36_one(seed: u64) -> (usize, usize) {
    let (mut s, f, sigma) = group(seed);
    for r in s["accounts"].as_array_mut().expect("accounts") {
        if r.get("delay").is_some() {
            r["delay"] = json!(0);
        }
    }
    s["force"] = json!(25);
    s["close"] = json!(10);
    let (c, k, _) = consistent(
        &s,
        &f,
        &sigma,
        by_left(),
        &format!("第 {seed} 组（余量 · 耗尽后启用）"),
    );
    (c, k)
}

#[test]
fn s22_simulation_agrees_with_per_request_decisions() {
    // 策略跑在沙箱里（测试档的 Rhai 每判几毫秒），常规门禁里跑 20 组、每组至多比 4 次；500 组那一版见下面那条（发布档、发版前的全量里跑）。
    s22_groups(20);
}

#[test]
fn s26_forced_switches_agree_with_per_request_decisions() {
    s26_groups(5);
}

/// ★ #26 第 357 组原样：七个号都在被用、各有用速，约交叉那一段往前看的那几步要在一判的运算量以内（超了 ⇒ 策略出错走兜底）。
#[test]
fn s26_many_moving_accounts_stay_within_the_operation_budget() {
    let (c, _, _) = s26_one(357);
    assert!(c >= 1, "本条在空转：只比到 {c} 次换号");
}

/// ★ #36 耗尽后启用的号照余量排（权重容差 10%）：在用的那个用着用着被超过就换，推演说的那一刻 ＝ 逐发那一刻。500 组里挑出来的两组原样：
/// · 第 19 组：五个号全是耗尽后启用的，在用的 a0 余量往下掉，a2 超出它 10% 的那一刻换；
/// · 第 302 组：在用的 a3 余量最高，a5 第二、a0 第三；a5 被别的会话用着、余量往下掉，降到跟 a0 同权以后，
///   a3 一掉进跟 a0 同权的那一档就换到号列表里排在前面的 a0 —— 要看的不只是在用的号自己的读数。
#[test]
fn s36_fallbacks_ranked_by_weight_switch_where_the_simulation_says() {
    for seed in [19, 302] {
        let (c, _) = s36_one(seed);
        assert!(c >= 2, "第 {seed} 组在空转：只比到 {c} 次换号");
    }
}

/// #22 · #26 · #36 的大样本（各 500 组）。测试档的 Rhai 太慢（100 组二十多分钟），所以不进常规那一趟：
/// 门禁的 `rotation-500` 格用 `perf` 档（发布档的优化、不做 LTO）单编单跑，只在全量里跑（`gate-affected` 改到轮换那几处时也跑）。
#[test]
#[ignore = "大样本：由门禁 rotation-500 格用 perf 档起"]
fn consistency_500_groups() {
    let (t22, k22) = s22_groups(500);
    let (t26, k26, x26) = s26_groups(500);
    let (t36, k36) = s36_groups(500);
    eprintln!("#22：比过 {t22} 次、挤的 {k22} 次 · #26：比过 {t26} 次、挤的 {k26} 次、强制切换 {x26} 次 · #36：比过 {t36} 次、挤的 {k36} 次");
}
