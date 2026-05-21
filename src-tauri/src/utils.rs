//! 跨模块工具：日期 / 时间换算。
//!
//! `days_from_civil` 在 `subagent`（按 timestamp 关联 subagent meta）
//! 与 `history`（jsonl ISO8601 时间戳排序）两处都用，提取至此避免重复。

/// Howard Hinnant 的 days_from_civil：把公历 (y, m, d) 转换为相对 1970-01-01 的天数。
/// 跨月 / 跨年 / 闰年都单调。
///
/// 参考：http://howardhinnant.github.io/date_algorithms.html
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_zero() {
        // 1970-01-01 = 0 days since itself
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn monotonic_across_month() {
        let jan_31 = days_from_civil(2026, 1, 31);
        let feb_1 = days_from_civil(2026, 2, 1);
        assert_eq!(feb_1 - jan_31, 1);
    }

    #[test]
    fn monotonic_across_year() {
        let dec_31 = days_from_civil(2025, 12, 31);
        let jan_1 = days_from_civil(2026, 1, 1);
        assert_eq!(jan_1 - dec_31, 1);
    }

    #[test]
    fn leap_year_feb_29() {
        // 2024 是闰年，Feb 有 29 天
        let feb_28 = days_from_civil(2024, 2, 28);
        let feb_29 = days_from_civil(2024, 2, 29);
        let mar_1 = days_from_civil(2024, 3, 1);
        assert_eq!(feb_29 - feb_28, 1);
        assert_eq!(mar_1 - feb_29, 1);
    }
}
