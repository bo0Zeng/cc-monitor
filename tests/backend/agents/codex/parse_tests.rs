//! 钉两样：rollout 文件名取 sid 的行为（注册表记录面那一格在用；turn-end 识别那一半零读者，连同它的判据一起删了）·
//! Codex 自报两窗的读法（rollout `token_count.rate_limits` · 回包头 `x-codex-*`；回包头那一读中转的默认上游那一格在用）。

use super::*;

/// Phase D 审计修：sid 提取校验对齐 monitor（rollout- 前缀 + 末36 UUID 形），畸形名 → None（跳过、
/// 不吐幽灵行），合法名 → 末36 UUID。补后端侧此前缺的畸形名覆盖。
#[test]
fn codex_sid_from_path_validates_like_monitor() {
    let p = |n: &str| PathBuf::from(n);
    assert_eq!(
        codex_sid_from_path(&p(
            "rollout-2026-07-18T08-00-00-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl"
        ))
        .as_deref(),
        Some("019f75dd-875c-7c81-9eda-32f866b2c60f")
    );
    for bad in [
        "rollout-garbage.jsonl",                                 // 剥前缀后 <36
        "notrollout-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl", // 无 rollout- 前缀
        "rollout-2026-07-18T08-00-00-zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz.jsonl", // 末36 结构对但非 hex
    ] {
        assert!(codex_sid_from_path(&p(bad)).is_none(), "畸形名应跳: {bad}");
    }
}

#[test]
fn is_uuid_matches_8_4_4_4_12_hex() {
    assert!(is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60f"));
    assert!(!is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60")); // 末段 11 位
    assert!(!is_uuid("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz")); // 非 hex
    assert!(!is_uuid("019f75dd875c7c819eda32f866b2c60f")); // 无分隔
}

// ── 额度：Codex 自报的两窗 ─────────────────────────────────────────────────────

fn line(ts: &str, payload: serde_json::Value) -> Value {
    serde_json::json!({"timestamp": ts, "type": "event_msg", "payload": payload})
}

fn names(w: &[QuotaWindow]) -> Vec<&str> {
    w.iter().map(|w| w.name.as_str()).collect()
}

/// 新形（`resets_at` unix 秒）：两窗按时长起名、用量记成比例、重置照它说的。
#[test]
fn a_token_count_with_rate_limits_reads_as_two_windows() {
    let v = line(
        "2026-11-08T12:00:00.000Z",
        serde_json::json!({"type": "token_count", "info": null, "rate_limits": {
            "limit_id": "codex",
            "primary": {"used_percent": 42.0, "window_minutes": 300, "resets_at": 1_794_150_000u64},
            "secondary": {"used_percent": 13.5, "window_minutes": 10080, "resets_at": 1_794_600_000u64},
            "credits": {"has_credits": false, "unlimited": false, "balance": null},
            "plan_type": "plus"
        }}),
    );
    let w = super::super::record::quota_said(&v).expect("带两窗");
    assert_eq!(names(&w), ["300m", "10080m"]);
    assert_eq!(w[0].used, Some(0.42));
    assert_eq!(w[1].used, Some(0.135));
    assert_eq!(
        (w[0].resets_at, w[1].resets_at),
        (Some(1_794_150_000), Some(1_794_600_000))
    );
    assert_eq!(slot_of(&w[0].name), Some(cmd_wire::quota::SLOT_5H));
    assert_eq!(slot_of(&w[1].name), Some(cmd_wire::quota::SLOT_7D));
    assert_eq!(
        key_of(&w[1].name).as_deref(),
        Some(cmd_wire::quota::SLOT_7D)
    );
}

/// 0.4x 旧形：没有 `resets_at`、是 `resets_in_seconds` ⇒ 从那一行的时刻起算；说不出时长 ⇒ 那一格的名字、没有语义位。
#[test]
fn the_old_relative_reset_counts_from_the_line_and_an_unknown_length_has_no_slot() {
    let v = line(
        "2026-11-08T10:00:00.000Z",
        serde_json::json!({"type": "token_count", "rate_limits": {
            "primary": {"used_percent": 5.0, "window_minutes": 300, "resets_in_seconds": 18_000},
            "secondary": {"used_percent": 10.0, "resets_in_seconds": 600_000}
        }}),
    );
    let w = super::super::record::quota_said(&v).expect("带两窗");
    assert_eq!(names(&w), ["300m", "secondary"]);
    assert_eq!(w[0].resets_at, Some(1_794_132_000 + 18_000));
    assert_eq!(w[1].resets_at, Some(1_794_132_000 + 600_000));
    assert_eq!(slot_of("secondary"), None);
    assert_eq!(
        key_of("primary"),
        None,
        "说不出时长 ⇒ 没有键（不拿上游字段名当窗口名）"
    );
    assert_eq!(slot_of("60m"), None, "别的时长没有语义位");
}

/// ★ 每一窗都有键（收到几窗出几行，不丢窗）：认得出语义位 ⇒ 照位；别的时长 ⇒ 照时长通用地写（整天写天、整点写小时、否则写分钟）；
/// 说不出时长 ⇒ 没有键。
#[test]
fn every_window_has_a_key_named_by_its_length() {
    for (name, key) in [
        ("300m", "5h"),
        ("299m", "5h"),
        ("10080m", "7d"),
        ("60m", "1h"),
        ("120m", "2h"),
        ("43200m", "30d"),
        ("1440m", "1d"),
        ("90m", "90m"),
    ] {
        assert_eq!(key_of(name).as_deref(), Some(key), "{name}");
    }
}

/// 时长照 Codex 自己的认法（`codex_tui::chatwidget::rate_limits::get_limits_duration`：与 5 小时 · 一周差在 ±5% 以内就算那一窗）：上游报的分钟数不必恰好整。
#[test]
fn a_window_length_near_five_hours_or_a_week_counts_as_that_slot() {
    for (name, slot) in [
        ("299m", Some(cmd_wire::quota::SLOT_5H)),
        ("314m", Some(cmd_wire::quota::SLOT_5H)),
        ("10079m", Some(cmd_wire::quota::SLOT_7D)),
        ("9600m", Some(cmd_wire::quota::SLOT_7D)),
        ("285m", Some(cmd_wire::quota::SLOT_5H)),
        ("315m", Some(cmd_wire::quota::SLOT_5H)),
        ("284m", None),
        ("316m", None),
        ("9576m", Some(cmd_wire::quota::SLOT_7D)),
        ("10584m", Some(cmd_wire::quota::SLOT_7D)),
        ("9575m", None),
        ("10585m", None),
        ("1440m", None),
        ("43200m", None),
        ("m", None),
    ] {
        assert_eq!(slot_of(name), slot, "{name}");
    }
}

/// 不是 `token_count` · 没带两窗 · 按模型另算的那一族 ⇒ 不出。
#[test]
fn lines_without_the_default_windows_say_nothing() {
    let ts = "2026-11-08T11:00:00.000Z";
    let window = serde_json::json!({"used_percent": 1.0, "window_minutes": 300, "resets_at": 1u64});
    for payload in [
        serde_json::json!({"type": "token_count", "info": {"last_token_usage": {"total_tokens": 3}}}),
        serde_json::json!({"type": "token_count", "rate_limits": null}),
        serde_json::json!({"type": "token_count", "rate_limits": {"primary": null, "secondary": null}}),
        serde_json::json!({"type": "token_count", "rate_limits": {"limit_id": "codex_other", "primary": window}}),
        serde_json::json!({"type": "agent_message", "message": "x", "rate_limits": {"primary": window}}),
    ] {
        assert_eq!(
            super::super::record::quota_said(&line(ts, payload.clone())),
            None,
            "{payload}"
        );
    }
    let not_event = serde_json::json!({"timestamp": ts, "type": "response_item",
        "payload": {"type": "token_count", "rate_limits": {"primary": window}}});
    assert_eq!(super::super::record::quota_said(&not_event), None);
}

fn heads(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

const FULL_5H: &[(&str, &str)] = &[
    ("X-Codex-Primary-Used-Percent", "100.0"),
    ("x-codex-primary-window-minutes", "300"),
    ("x-codex-primary-reset-at", "1794150000"),
    ("x-codex-secondary-used-percent", "40"),
    ("x-codex-secondary-window-minutes", "10080"),
    ("x-codex-secondary-reset-at", "1794600000"),
];

/// 回包头那一族：两窗同 rollout 一形（头名不分大小写）；没有那一族 ⇒ `None`；Codex 不说状态。
#[test]
fn reply_headers_read_as_the_same_two_windows() {
    let r = quota_of_headers(200, &heads(FULL_5H), 1_794_139_300).expect("有那一族");
    assert_eq!(names(&r.windows), ["300m", "10080m"]);
    assert_eq!(r.windows[0].used, Some(1.0));
    assert_eq!(r.windows[1].resets_at, Some(1_794_600_000));
    assert_eq!(r.status, None);
    assert!(!r.refused, "200 不算被拒（用满也照常读数）");
    assert_eq!(
        quota_of_headers(200, &heads(&[("content-type", "text/event-stream")]), 1),
        None
    );
}

/// 这一家只读数：429 满窗也不自己说被拒；被不被拒由通用层一处判（`agents::judged`），Codex 的读法接得上它。
#[test]
fn the_reading_never_judges_and_the_common_judge_refuses_a_full_window() {
    let now = 1_794_139_300;
    let r = quota_of_headers(429, &heads(FULL_5H), now).expect("有那一族");
    assert!(
        !r.refused && r.limiting.is_none() && r.resets_at.is_none(),
        "{r:?}"
    );
    let j = crate::agents::judged(quota_of_headers, 429, &heads(FULL_5H), now).expect("有那一族");
    assert_eq!((j.refused, j.resets_at), (true, Some(1_794_150_000)));
    let mut half = heads(FULL_5H);
    half[0].1 = "60".into();
    let j = crate::agents::judged(quota_of_headers, 429, &half, now).expect("有那一族");
    assert!(!j.refused, "{j:?}");
}
