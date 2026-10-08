//! 天数 ⇒ 公历：定点 ＋ 与一份另写的正向（就在本文件里）逐日往返。

use super::*;

/// 正向参照（年月日 ⇒ 天数），与被测的逆向不同源。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[test]
fn fixed_points() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(civil_from_days(20_361), (2025, 9, 30));
}

#[test]
fn round_trips_day_by_day() {
    let mut checked = 0u32;
    for y in 1899..=2101 {
        for m in 1..=12 {
            let dim = match m {
                2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            for d in 1..=dim {
                let z = days_from_civil(y, m, d);
                assert_eq!(civil_from_days(z), (y, m, d), "第 {z} 天往返不回原样");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 74_144, "1899-01-01 ～ 2101-12-31 应当是 74144 天");
}
