//! Claude 回包头 → 额度快照：读法表的金样（头值全是现编的，形状照 claude 程序的读法）。

use super::*;
use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};

const NOW: u64 = 1_800_000_000;

fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn win(
    name: &str,
    used: Option<f64>,
    resets_at: Option<u64>,
    warned_at: Option<f64>,
) -> QuotaWindow {
    QuotaWindow {
        name: name.to_string(),
        used,
        resets_at,
        warned_at,
    }
}

#[test]
fn an_ordinary_answer_reads_into_both_windows_and_the_limiting_one() {
    let got = read(
        200,
        &h(&[
            ("anthropic-ratelimit-unified-status", "allowed"),
            ("anthropic-ratelimit-unified-5h-utilization", "0.42"),
            ("anthropic-ratelimit-unified-5h-reset", "1800003600"),
            ("anthropic-ratelimit-unified-7d-utilization", "0.1"),
            ("anthropic-ratelimit-unified-7d-reset", "1800500000.6"),
            (
                "anthropic-ratelimit-unified-representative-claim",
                "five_hour",
            ),
            ("anthropic-ratelimit-unified-reset", "1800003600"),
            ("content-type", "text/event-stream"),
        ]),
        NOW,
    );
    assert_eq!(
        got,
        Some(QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(1_800_003_600),
            windows: vec![
                win("five_hour", Some(0.42), Some(1_800_003_600), None),
                win("seven_day", Some(0.1), Some(1_800_500_001), None),
            ],
            overage: None,
        })
    );
}

#[test]
fn a_warning_carries_the_threshold_it_crossed() {
    let got = read(
        200,
        &h(&[
            ("Anthropic-Ratelimit-Unified-Status", "allowed_warning"),
            ("anthropic-ratelimit-unified-7d-utilization", "0.91"),
            ("anthropic-ratelimit-unified-7d-surpassed-threshold", "0.9"),
        ]),
        NOW,
    )
    .expect("有 status 头就有读数");
    assert_eq!(got.status, Some(QuotaStatus::Warning));
    assert_eq!(
        got.windows,
        vec![win("seven_day", Some(0.91), None, Some(0.9))]
    );
    assert_eq!(got.peak_used(), Some(0.91));
}

#[test]
fn a_429_with_the_limiter_saying_rejected_is_refused_until_its_reset() {
    let got = read(
        429,
        &h(&[
            ("anthropic-ratelimit-unified-status", "rejected"),
            (
                "anthropic-ratelimit-unified-representative-claim",
                "seven_day",
            ),
            ("anthropic-ratelimit-unified-reset", "1800090000"),
            ("anthropic-ratelimit-unified-7d-utilization", "1.0"),
            ("retry-after", "5"),
        ]),
        NOW,
    )
    .expect("被拒也要读");
    assert!(got.refused);
    assert_eq!(got.status, Some(QuotaStatus::Rejected));
    assert_eq!(got.limiting.as_deref(), Some("seven_day"));
    // 限流器给了重置时刻 ⇒ 用它，不用 retry-after。
    assert_eq!(got.resets_at, Some(1_800_090_000));
}

#[test]
fn rejected_but_served_by_paid_overage_is_not_a_refusal() {
    let got = read(
        200,
        &h(&[
            ("anthropic-ratelimit-unified-status", "rejected"),
            ("anthropic-ratelimit-unified-overage-status", "allowed"),
            ("anthropic-ratelimit-unified-overage-in-use", "true"),
            ("anthropic-ratelimit-unified-overage-reset", "1800100000"),
        ]),
        NOW,
    )
    .expect("读数");
    assert!(!got.refused);
    assert_eq!(
        got.overage,
        Some(QuotaOverage {
            status: Some(QuotaStatus::Allowed),
            resets_at: Some(1_800_100_000),
            disabled: None,
            in_use: true,
        })
    );
}

#[test]
fn a_429_without_the_family_is_refused_for_retry_after_seconds() {
    // API key 号的回包永远没有这一族头。
    let got = read(429, &h(&[("retry-after", "30")]), NOW);
    assert_eq!(
        got,
        Some(QuotaReading {
            status: None,
            refused: true,
            limiting: None,
            resets_at: Some(NOW + 30),
            windows: vec![],
            overage: None,
        })
    );
    assert_eq!(read(429, &h(&[]), NOW).and_then(|r| r.resets_at), None);
}

#[test]
fn an_answer_without_the_family_says_nothing() {
    assert_eq!(read(200, &h(&[("retry-after", "30")]), NOW), None);
    assert_eq!(read(529, &h(&[]), NOW), None);
}

#[test]
fn a_value_outside_the_table_is_left_unsaid_not_guessed() {
    let got = read(
        200,
        &h(&[
            ("anthropic-ratelimit-unified-status", "maybe"),
            ("anthropic-ratelimit-unified-5h-utilization", "lots"),
            ("anthropic-ratelimit-unified-5h-reset", "-3"),
        ]),
        NOW,
    )
    .expect("status 头在 ⇒ 有读数");
    assert_eq!(got.status, None);
    assert!(
        got.windows.is_empty(),
        "读不懂的值不成窗口：{:?}",
        got.windows
    );
}

#[test]
fn the_adapter_registry_hands_this_reading_to_the_relay_route_of_claude() {
    let f = crate::agents::quota_read_of(super::super::UPSTREAM.route_id).expect("登记了");
    let sample = h(&[("anthropic-ratelimit-unified-status", "allowed_warning")]);
    assert_eq!(f(200, &sample, NOW), read(200, &sample, NOW));
    assert!(f(200, &sample, NOW).is_some());
    assert!(crate::agents::quota_read_of("no-such-agent").is_none());
}
