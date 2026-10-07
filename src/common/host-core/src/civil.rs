//! 天数 ⇒ 公历（Howard Hinnant 的 `civil_from_days`，前推公历、负年按 `div_euclid`）。两个前端画日期都调这里，不引日期库。

/// 自 1970-01-01 起第几天 ⇒ (年, 月, 日)。不带时区：时区差由调用方先加进天数。
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
#[path = "../../../../tests/common/host-core/civil_tests.rs"]
mod tests;
