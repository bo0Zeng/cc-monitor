//! **公历换算与 ISO8601 时刻** —— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | T1 | 定点：1970-01-01 = 0 · 1969-12-31 = −1 · 2000-02-29 · 0000-03-01 · −0001-12-31 | 真值 |
//! | T2 | 两个方向互逆（−800 年到 +800 年逐天） | 全量 |
//! | T3 | ISO8601：秒 · 毫秒 · 1/2 位小数补零 · 太短 / 不是数字 ⇒ `None` | 真值 |
//! | T5 | 给人看的时刻：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年 · 时区偏移跨日 | 真值 |
//! | T4 | 换算常量 `719_468` 在后端生产段只住本模块 | 文本，零命中 ＋ 正控 |

use super::*;

#[test]
fn t1_fixed_points() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    assert_eq!(civil_from_days(20_361), (2025, 9, 30));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29));
    assert_eq!(
        days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28),
        2
    );
    assert_eq!(
        days_from_civil(0, 3, 1) - days_from_civil(0, 2, 28),
        2,
        "0 年是闰年"
    );
    assert_eq!(days_from_civil(0, 1, 1) - days_from_civil(-1, 12, 31), 1);
    assert_eq!(civil_from_days(days_from_civil(-1, 12, 31)), (-1, 12, 31));
}

#[test]
fn t2_round_trip() {
    let lo = days_from_civil(-800, 1, 1);
    let hi = days_from_civil(800, 1, 1);
    for z in lo..hi {
        let (y, m, d) = civil_from_days(z);
        assert_eq!(days_from_civil(y, m, d), z, "{y}-{m}-{d}");
    }
}

#[test]
fn t3_iso8601() {
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.250Z"), Some(1_250));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.2Z"), Some(1_200));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.25Z"), Some(1_250));
    assert_eq!(
        parse_iso8601_ms("2026-10-07T12:34:56.789Z"),
        Some((days_from_civil(2026, 10, 7) * 86_400 + 12 * 3600 + 34 * 60 + 56) * 1000 + 789)
    );
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.123456Z"), Some(123));
    assert_eq!(
        parse_iso8601_ms("2021-01-01T00:00:00Z"),
        Some(1_609_459_200_000)
    );
    assert_eq!(parse_iso8601_ms("garbage"), None);
    assert_eq!(parse_iso8601_ms("2026-10-07T12:34"), None);
    assert_eq!(parse_iso8601_ms("2026-1x-07T12:34:56Z"), None);
}

#[test]
fn t4_the_constant_lives_only_here() {
    let root = crate::guard_support::src_root();
    let needle = "719_468";
    let files = guard_core::scan_tree!(&root, &["rs"]);
    let scanned = files.len();
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, src)| crate::guard_support::production_code(src).contains(needle))
        .map(|(p, _)| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert!(scanned >= 60, "只扫到 {scanned} 份 —— 遍历坏了");
    assert_eq!(
        hits,
        vec!["common/time.rs".to_string()],
        "公历换算又长出了一份"
    );
    let sample = "fn civil(z: i64) { let z = z + 719_468; }";
    assert!(
        crate::guard_support::production_code(sample).contains(needle),
        "针认不出样本"
    );
}

#[test]
fn t5_display_forms_follow_the_day_and_the_year() {
    let now = days_from_civil(2026, 10, 7) * 86_400 + 12 * 3_600;
    assert_eq!(fmt_at(now + 2 * 3_600 + 5 * 60, now, 0), "14:05");
    assert_eq!(fmt_at(now + 86_400, now, 0), "10-08 12:00");
    assert_eq!(fmt_at(now - 300 * 86_400, now, 0), "2025-12-11 12:00");
    // 东八区：UTC 20:00 是本地次日 04:00。
    assert_eq!(fmt_at(now + 8 * 3_600, now, 480), "10-08 04:00");
}
