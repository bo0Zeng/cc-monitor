//! 升级材料里「约下次叫醒」那一段（`setup/upgrade/turn.rhai` 末尾）逐块点名：每条一个小场景，刚好走到那一块，
//! 直接问策略一判（不经 turn 的连换），断言它约的时刻 `wake_at` 与读数 `wake_when`。哪一块改坏，点它名的那条红。
//! 一致性那几组（`s22` · `s26` · `s36`）比的是推演与逐发判对不对得上，接不住「约晚了一档」这一类改坏，这里补上。

use super::*;

/// 问策略一判：此刻 T，在用的是 `cur`，会话还没有 memo。
fn ask(p: &Material, s: &Value, cur: &str, f: &[Acct]) -> Verdict {
    let sg = sig(cur, &[]);
    p.decide(&ctx_of(&sg, f, s, clock(T), T))
        .expect("升级材料判得出")
}

/// 这个号 5 小时那一窗照这一用速（每分钟涨几个百分点）在被用。
fn burning(mut a: Acct, per_min: f64) -> Acct {
    a.rate = [Some(per_min), None];
    a
}

/// `wake_when` 里 `acct` 那一窗（0 ＝ 5 小时 · 1 ＝ 7 天）约的读数，几条都列出来。
fn readings(v: &Verdict, acct: &str, w: usize) -> Vec<f64> {
    v.wake_when
        .iter()
        .filter(|x| x.acct == acct && x.w == w)
        .map(|x| x.used_at_least)
        .collect()
}

/// 读数里有一条落在 (`lo`, `hi`]。
fn has_reading(v: &Verdict, acct: &str, w: usize, lo: f64, hi: f64) -> bool {
    readings(v, acct, w).iter().any(|u| *u > lo && *u <= hi)
}

/// 分段边界：用量上限 15:00 起换一档 ⇒ 约到 15:00（此刻 14:30）。
#[test]
fn wake_at_the_next_segment_edge() {
    let mut s = base();
    s["caps"]["5h"] = json!([{"from": 0, "to": 900, "v": 90}, {"from": 900, "to": 1440, "v": 50}]);
    let v = ask(by_list(), &s, "A", &[acct("A", 10.0), acct("B", 10.0)]);
    assert_eq!(v.wake_at, Some(T + 30 * MIN), "{v:?}");
}

/// 启用延迟到点：其余的号都没额度，耗尽后启用的号延迟 30 分钟 ⇒ 约到 30 分钟以后（停发的恢复时刻之外，策略自己也约）。
#[test]
fn wake_when_the_activation_delay_ends() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "delay", json!(30));
    let f = [acct("A", 95.0), acct("B", 95.0), acct("C", 10.0)];
    let v = ask(by_list(), &s, "A", &f);
    assert!(v.order.is_empty(), "延迟里、此刻没号可用：{v:?}");
    assert_eq!(v.wake_at, Some(T + 30 * MIN), "{v:?}");
}

/// 要用的号读数到上限：两窗各约一条读数（上限 90%）。
#[test]
fn wake_when_the_account_in_use_reaches_its_cap() {
    let v = ask(by_list(), &base(), "A", &[acct("A", 10.0), acct("B", 10.0)]);
    assert_eq!(readings(&v, "A", 0), [90.0], "{v:?}");
    assert_eq!(readings(&v, "A", 1), [90.0], "{v:?}");
    assert_eq!(v.wake_at, None, "没有别的会变的：{v:?}");
}

/// 单次用量上限：换到耗尽后启用的号、从此刻读数 10 记起，涨 20 ⇒ 约读数 30；它的 5 小时窗口 30 分钟后重置 ⇒ 重置前一分钟也来看。
#[test]
fn wake_on_the_per_use_limit_and_just_before_its_reset() {
    let mut s = base();
    fallback(&mut s, "C");
    set_row(&mut s, "C", "stint", json!(20));
    let f = [acct("A", 95.0), acct("B", 95.0), with_reset("C", 10.0, 30)];
    let v = ask(by_list(), &s, "A", &f);
    assert_eq!(v.order.first().map(String::as_str), Some("C"), "{v:?}");
    assert!(
        readings(&v, "C", 0).iter().any(|u| (u - 30.0).abs() < 1e-9),
        "涨够的那一读数：{v:?}"
    );
    assert_eq!(v.wake_at, Some(T + 30 * MIN - 60), "{v:?}");
}

/// 权重交叉那一块，要用的号窗口还没开：一用就开 ⇒ 约读数「此刻 ＋ 0.01」，开了再算。
#[test]
fn wake_as_soon_as_an_unopened_window_opens() {
    let f = [acct("A", 10.0), with_reset("B", 20.0, 60)];
    let v = ask(by_left(), &forced(25), "A", &f);
    assert_eq!(v.order.first().map(String::as_str), Some("A"), "{v:?}");
    assert!(has_reading(&v, "A", 0, 10.0, 10.02), "{v:?}");
}

/// 换过去下一判就又不排第一（权重容差 50% 比强制切换 10% 大）：下一秒就来问。
/// 此刻的号 C 用完了；A · B 在容差里同权、照号列表 A 先；可 A 一在用，B 先重置、高出它 20% ⇒ 当场又要换。
#[test]
fn wake_next_second_when_the_switch_would_be_undone_at_once() {
    let mut s = forced(10);
    s["close"] = json!(50);
    let f = [
        with_reset("A", 40.0, 100),
        with_reset("B", 30.0, 30),
        acct("C", 95.0),
    ];
    let v = ask(by_left(), &s, "C", &f);
    assert_eq!(v.order.first().map(String::as_str), Some("A"), "{v:?}");
    assert_eq!(v.wake_at, Some(T + 1), "{v:?}");
}

/// 按时刻：离重置近的权重高，强制切换 25%。A 100 分钟后重置、B 90 分钟后：
/// 两边一起往前走，过了 50 分钟 B 的权重超出 A 25% ⇒ 约在 50 分钟那一刻（二分到 0.1 分钟，约在交叉之后不到 7 秒）。
#[test]
fn wake_when_the_weights_cross_in_time() {
    let f = [with_reset("A", 10.0, 100), with_reset("B", 10.0, 90)];
    let v = ask(by_reset(), &forced(25), "A", &f);
    assert_eq!(v.order.first().map(String::as_str), Some("A"), "{v:?}");
    let at = v.wake_at.unwrap_or_else(|| panic!("没约交叉那一刻：{v:?}"));
    assert!(
        at > T + 50 * MIN && at <= T + 50 * MIN + 7,
        "{}s：{v:?}",
        at - T
    );
}

/// 按时刻至多看两小时：交叉在 130 分钟以后 ⇒ 约到两小时那一刻再来看，不越过去约。
#[test]
fn wake_after_two_hours_when_the_crossing_is_further() {
    let f = [with_reset("A", 10.0, 280), with_reset("B", 10.0, 250)];
    let v = ask(by_reset(), &forced(25), "A", &f);
    assert_eq!(v.wake_at, Some(T + 120 * MIN), "{v:?}");
}

/// 按时刻看到有额度的号里最早重置的那一刻为止：耗尽后启用的 C 20 分钟后重置（那本身是节点），
/// A · B 的交叉在 50 分钟以后 ⇒ 这一判不约它。
#[test]
fn wake_search_stops_at_the_earliest_reset() {
    let mut s = forced(25);
    fallback(&mut s, "C");
    let f = [
        with_reset("A", 10.0, 100),
        with_reset("B", 10.0, 90),
        with_reset("C", 10.0, 20),
    ];
    let v = ask(by_reset(), &s, "A", &f);
    assert_eq!(v.order.first().map(String::as_str), Some("A"), "{v:?}");
    assert_eq!(v.wake_at, None, "{v:?}");
}

/// 按时刻看到要用的号照用速用到上限的那一刻为止：A 余 46.65、每分钟 0.5 ⇒ 93.3 分钟用满，约到那一刻（到秒）。
/// B 余得多但重置得晚、换不过去，所以用满之前没有交叉。
#[test]
fn wake_search_stops_where_the_account_runs_out() {
    let f = [
        burning(with_reset("A", 43.35, 200), 0.5),
        with_reset("B", 10.0, 250),
    ];
    let v = ask(by_left(), &forced(25), "A", &f);
    let out = (90.0 - 43.35) / 0.5 * 60.0;
    assert_eq!(v.wake_at, Some(T + out as u64 + 1), "{v:?}");
}

/// 按读数：哪一窗用到底也换不了（B 重置得晚，强制切换轮不到它）⇒ 那一窗不约读数，只剩到上限那两条。
#[test]
fn wake_on_reading_skips_a_window_that_cannot_switch() {
    let f = [with_reset("A", 10.0, 30), with_reset("B", 10.0, 200)];
    let v = ask(by_left(), &forced(25), "A", &f);
    assert_eq!(readings(&v, "A", 0), [90.0], "{v:?}");
    assert_eq!(readings(&v, "A", 1), [90.0], "{v:?}");
}

/// 按读数：余量多的权重高，强制切换 25%。A 余 60、B 余 40 且先重置 ⇒ A 余量掉到 32 以下 B 就换进来：
/// 5 小时那一窗再用 28（读数 58）、7 天那一窗再用 58（7 天余量从 90 掉到 32）。只有 A 自己在变 ⇒ 直接约到那一读数。
#[test]
fn wake_on_the_reading_where_the_weights_cross() {
    let f = [with_reset("A", 30.0, 200), with_reset("B", 50.0, 150)];
    let v = ask(by_left(), &forced(25), "A", &f);
    assert!(has_reading(&v, "A", 0, 58.0, 58.05), "{v:?}");
    assert!(has_reading(&v, "A", 1, 58.0, 58.05), "{v:?}");
}

/// 同上，但 B 在被别处用着（量得到用速）：别的号也在变 ⇒ 读数约到一半（5 小时再用 14 · 7 天再用 29），到了再算；
/// 按时刻也要看：B 只会越用越少、两小时里不交叉 ⇒ 两小时那一刻再来看。
#[test]
fn wake_halfway_when_other_accounts_are_moving() {
    let f = [
        with_reset("A", 30.0, 200),
        burning(with_reset("B", 50.0, 150), 0.1),
    ];
    let v = ask(by_left(), &forced(25), "A", &f);
    assert!(has_reading(&v, "A", 0, 44.0, 44.03), "{v:?}");
    assert!(has_reading(&v, "A", 1, 29.0, 29.03), "{v:?}");
    assert_eq!(v.wake_at, Some(T + 120 * MIN), "{v:?}");
}
