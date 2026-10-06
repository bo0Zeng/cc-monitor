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

/// 硬上限回的那份「用满」回包：429 ＋ 限流器那一族说 `rejected`、卡着的窗口与几点重置、体是 `rate_limit_error`；
/// 用这一家自己的读法读回来 ＝ 被拒、到那一刻重置（claude 照它原生的样子显示「用满 · 几点重置」）。
#[test]
fn the_limit_reply_reads_back_as_refused_until_its_reset() {
    let at = NOW + 1800;
    let r = limit_reply(at, NOW, Some("7d"));
    assert_eq!(r.status, "429 Too Many Requests");
    let got = read(429, &r.headers, NOW).expect("读得出");
    assert!(got.refused);
    assert_eq!(got.resets_at, Some(at));
    assert_eq!(got.limiting.as_deref(), Some("seven_day"));
    assert!(got.overage.is_none(), "不说有超额兜着");
    let body: serde_json::Value = serde_json::from_slice(&r.body).expect("体是 JSON");
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "rate_limit_error");
}

fn claim_and_retry_after(r: &crate::agents::LimitReply) -> (Option<&str>, Option<&str>) {
    let get = |name: &str| {
        r.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    (
        get("anthropic-ratelimit-unified-representative-claim"),
        get("retry-after"),
    )
}

/// 窗口那一格缺了，claude 就认不成「用满」（当成服务端临时限流、退避重试十次后报错，也不会到点自己续）⇒ 永远带。
/// 说不出卡在哪个窗口 ⇒ 按重置时刻的远近标：5 小时以内 `five_hour`，更远 `seven_day`；说得出 ⇒ 照它，不看远近。
/// `retry-after` ＝ 离重置还有几秒（至少 1）：按量号的 claude 据它决定等到点再发还是当场报错。
#[test]
fn the_limit_reply_always_names_a_window_and_says_when_to_retry() {
    let five_hours = 5 * 3600;
    let r = limit_reply(NOW + five_hours, NOW, None);
    assert_eq!(
        claim_and_retry_after(&r),
        (Some("five_hour"), Some("18000"))
    );
    let r = limit_reply(NOW + five_hours + 1, NOW, None);
    assert_eq!(
        claim_and_retry_after(&r),
        (Some("seven_day"), Some("18001"))
    );
    let r = limit_reply(NOW + 60, NOW, Some("7d"));
    assert_eq!(claim_and_retry_after(&r), (Some("seven_day"), Some("60")));
    let r = limit_reply(NOW + 60, NOW, Some("5h"));
    assert_eq!(claim_and_retry_after(&r), (Some("five_hour"), Some("60")));
    assert_eq!(
        claim_and_retry_after(&limit_reply(NOW, NOW, None)).1,
        Some("1")
    );
    assert_eq!(
        claim_and_retry_after(&limit_reply(NOW - 5, NOW, Some("5h"))).1,
        Some("1")
    );
    for r in [
        limit_reply(NOW + 60, NOW, None),
        limit_reply(NOW + 60, NOW, Some("7d")),
    ] {
        let got = read(429, &r.headers, NOW).expect("读得出");
        assert_eq!(
            got.resets_at,
            Some(NOW + 60),
            "重置时刻照 reset 头，不被 retry-after 盖掉"
        );
    }
}

/// ★ 窗口键：`five_hour` ⇒ `5h` · `seven_day` ⇒ `7d` · 按模型（或别的分档）的周额度 ⇒ `7d:<档>`（不再并进 `7d`）；超额那一档没有键。
#[test]
fn window_keys_keep_the_per_model_week_apart() {
    assert_eq!(key_of("five_hour").as_deref(), Some("5h"));
    assert_eq!(key_of("seven_day").as_deref(), Some("7d"));
    assert_eq!(key_of("seven_day_opus").as_deref(), Some("7d:opus"));
    assert_eq!(
        key_of("seven_day_overage_included").as_deref(),
        Some("7d:overage_included")
    );
    assert_eq!(key_of("overage"), None);
    assert_eq!(key_of("seven_day_"), None);
    // 语义位照旧：7 天那几个都是 `7d`（界面两格与换号记录照留）。
    assert_eq!(slot_of("seven_day_opus"), Some("7d"));
}

/// ★ 收掉 Opus 那条欠账：卡在 `7d:opus` 上的「用满」回包照实标 `seven_day_opus`（claude 说 Opus limit 而非 weekly limit）；
/// 有分窗口头的那一档一并给它的重置时刻；认不得的键 ⇒ 按远近标。
#[test]
fn the_limit_reply_names_a_per_model_week_by_its_own_name() {
    let r = limit_reply(NOW + 60, NOW, Some("7d:opus"));
    assert_eq!(claim_and_retry_after(&r).0, Some("seven_day_opus"));
    let r = limit_reply(NOW + 60, NOW, Some("7d:overage_included"));
    assert_eq!(
        claim_and_retry_after(&r).0,
        Some("seven_day_overage_included")
    );
    assert!(r.headers.iter().any(
        |(k, v)| k == "anthropic-ratelimit-unified-7d_oi-reset" && *v == (NOW + 60).to_string()
    ));
    let r = limit_reply(NOW + 60, NOW, Some("weird"));
    assert_eq!(claim_and_retry_after(&r).0, Some("five_hour"));
}
