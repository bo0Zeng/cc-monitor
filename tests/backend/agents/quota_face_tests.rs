//! 被拒判法一处：两个替身家各声明一种读法，被拒与否、被拒到几点只由通用层判。
//! 头名与写法两家都不同、与真的几家也不同（头值全是现编的）。

use super::*;

const NOW: u64 = 1_800_000_000;
const RESET: u64 = NOW + 7200;

fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn win(name: &str, used: Option<f64>, resets_at: Option<u64>) -> QuotaWindow {
    QuotaWindow {
        name: name.to_string(),
        used,
        resets_at,
        warned_at: None,
    }
}

fn reading(windows: Vec<QuotaWindow>) -> QuotaReading {
    QuotaReading {
        status: None,
        refused: false,
        limiting: None,
        resets_at: None,
        windows,
        overage: None,
    }
}

/// 替身家甲：一窗一组头 `x-alpha-<窗>-used`（百分数）· `x-alpha-<窗>-ends`（unix 秒）。
fn read_alpha(_status: u16, headers: &[(String, String)], _now: u64) -> Option<QuotaReading> {
    let used = header(headers, "x-alpha-burst-used")?.parse::<f64>().ok()? / 100.0;
    let ends = header(headers, "x-alpha-burst-ends").and_then(|v| v.parse().ok());
    Some(reading(vec![win("burst", Some(used), ends)]))
}

/// 替身家乙：一个头写全 `q-beta: <窗>;<比例>;<距重置几秒>`（重置照此刻折算）。
fn read_beta(_status: u16, headers: &[(String, String)], now: u64) -> Option<QuotaReading> {
    let v = header(headers, "Q-Beta")?;
    let mut it = v.split(';');
    let name = it.next()?;
    let used = it.next()?.parse::<f64>().ok();
    let ends = it
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|s| now + s);
    Some(reading(vec![win(name, used, ends)]))
}

/// 替身家丙：用量没到 100、读数却说被拒（限流器点名的那一窗），重置照它说的。
fn read_gamma(_status: u16, headers: &[(String, String)], _now: u64) -> Option<QuotaReading> {
    let ends = header(headers, "g-ends").and_then(|v| v.parse().ok());
    Some(QuotaReading {
        status: Some(QuotaStatus::Rejected),
        resets_at: ends,
        ..reading(vec![win("week", Some(0.4), ends)])
    })
}

#[test]
fn two_families_answering_the_same_full_429_are_refused_until_the_same_reset() {
    let a = judged(
        read_alpha,
        429,
        &h(&[
            ("x-alpha-burst-used", "100"),
            ("x-alpha-burst-ends", &RESET.to_string()),
        ]),
        NOW,
    )
    .expect("甲读得出");
    let b = judged(read_beta, 429, &h(&[("q-beta", "burst;1.0;7200")]), NOW).expect("乙读得出");
    assert!(a.refused && b.refused);
    assert_eq!(a.resets_at, Some(RESET));
    assert_eq!(a.resets_at, b.resets_at);
}

#[test]
fn a_429_with_no_full_window_is_only_a_brief_throttle() {
    let a = judged(
        read_alpha,
        429,
        &h(&[("x-alpha-burst-used", "80"), ("retry-after", "30")]),
        NOW,
    )
    .expect("照常读数");
    assert!(!a.refused);
    assert_eq!(a.windows[0].used, Some(0.8));
}

#[test]
fn a_full_window_on_a_served_answer_is_not_a_refusal() {
    let b = judged(read_beta, 200, &h(&[("q-beta", "burst;1.0;60")]), NOW).expect("读得出");
    assert!(!b.refused);
}

#[test]
fn a_refusal_without_a_reset_waits_for_retry_after_then_the_one_default() {
    let full = ("x-alpha-burst-used", "100");
    let after = judged(read_alpha, 429, &h(&[full, ("Retry-After", "45")]), NOW).expect("读得出");
    assert_eq!(after.resets_at, Some(NOW + 45));
    let bare = judged(read_alpha, 429, &h(&[full]), NOW).expect("读得出");
    let bare_b = judged(read_beta, 429, &h(&[("q-beta", "burst;1.0")]), NOW).expect("读得出");
    assert_eq!(bare.resets_at, Some(NOW + REFUSED_BRIEFLY));
    assert_eq!(bare_b.resets_at, bare.resets_at);
}

#[test]
fn a_window_the_family_says_is_refused_counts_below_full() {
    let g = judged(read_gamma, 429, &h(&[("g-ends", &RESET.to_string())]), NOW).expect("读得出");
    assert!(g.refused);
    assert_eq!(g.resets_at, Some(RESET));
}

#[test]
fn header_lookup_ignores_case_and_blank_values() {
    let hs = h(&[("X-Thing", " 7 "), ("empty", "  ")]);
    assert_eq!(header(&hs, "x-thing"), Some("7"));
    assert_eq!(header(&hs, "EMPTY"), None);
    assert_eq!(header(&hs, "absent"), None);
}
