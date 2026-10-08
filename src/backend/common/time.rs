//! 公历换算（Howard Hinnant 的 `days_from_civil` / `civil_from_days`，前推公历、负年按 `div_euclid`）与 Claude 记录里的 ISO8601 时刻。
//! 后端各处要「天数 ⇄ 年月日」、解一个 `YYYY-MM-DDTHH:MM:SS(.fff)?Z`、或把一个时刻写成给人看的样子（[`fmt_at`]）都调这里。

/// 公历 (年, 月, 日) ⇒ 自 1970-01-01 起第几天。
pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (m, d) = (i64::from(m), i64::from(d));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 自 1970-01-01 起第几天 ⇒ 公历 (年, 月, 日)。
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    // 月 ∈ 1..=12、日 ∈ 1..=31，转窄不丢。
    (yoe + era * 400 + i64::from(m <= 2), m as u32, d as u32)
}

/// `YYYY-MM-DDTHH:MM:SS(.f{1,})?Z` ⇒ 自 1970 起的毫秒（UTC）。小数秒取前 3 位、不足右补 0；太短或某格不是数字 ⇒ `None`。
pub(crate) fn parse_iso8601_ms(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let t = s.get(r)?;
        t.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| t.parse().ok())?
    };
    let (year, mon, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, min, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let millis = if s.as_bytes().get(19) == Some(&b'.') {
        let frac: String = s[20..]
            .chars()
            .take_while(char::is_ascii_digit)
            .take(3)
            .collect();
        format!("{frac:0<3}").parse::<i64>().unwrap_or(0)
    } else {
        0
    };
    let days = days_from_civil(year, u32::try_from(mon).ok()?, u32::try_from(day).ok()?);
    Some((days * 86_400 + hour * 3_600 + min * 60 + sec) * 1_000 + millis)
}

const DAY: i64 = 86_400;

/// 一个时刻（unix 秒）按此刻与时区偏移（分钟，东正）写给人看：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年 `YYYY-MM-DD HH:MM`。
/// 界面与终端要显示的时刻都由后端经它写好再交出去（界面零换算）。
pub(crate) fn fmt_at(t: i64, now: i64, tz_min: i64) -> String {
    let local = t + tz_min * 60;
    let day = local.div_euclid(DAY);
    let today = (now + tz_min * 60).div_euclid(DAY);
    let secs = local - day * DAY;
    let hm = format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60);
    if day == today {
        return hm;
    }
    let (y, m, d) = civil_from_days(day);
    if y == civil_from_days(today).0 {
        format!("{m:02}-{d:02} {hm}")
    } else {
        format!("{y}-{m:02}-{d:02} {hm}")
    }
}

/// 回包里认得的时刻格（unix 秒）：出口那一遍（[`with_texts`]）在它旁边添 `<键>Text`。闭集 —— 新的时刻格要显示就加在这里。
pub(crate) const TIME_KEYS: &[&str] = &["at", "seenAt", "resetsAt", "fromResetsAt", "since"];

/// **回包出口那一遍**：走遍整份回包，每个对象里认得的时刻格（[`TIME_KEYS`]，值是整数）旁边添一格 `<键>Text`
/// ＝ [`fmt_at`] 按 `now` 与 `tz_min` 写好的字。界面只照这一格排，不换算。
pub(crate) fn with_texts(v: &mut serde_json::Value, now: i64, tz_min: i64) {
    match v {
        serde_json::Value::Object(m) => {
            let adds: Vec<(String, String)> = TIME_KEYS
                .iter()
                .filter_map(|k| {
                    m.get(*k)
                        .and_then(serde_json::Value::as_i64)
                        .map(|t| (format!("{k}Text"), fmt_at(t, now, tz_min)))
                })
                .collect();
            for x in m.values_mut() {
                with_texts(x, now, tz_min);
            }
            for (k, t) in adds {
                m.insert(k, serde_json::Value::String(t));
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| with_texts(x, now, tz_min)),
        _ => {}
    }
}

/// [`with_texts`] 按这台此刻的本地钟（帧面答 `quota-read` · `rotation-session-read` 那一下）。
pub(crate) fn with_texts_here(v: &mut serde_json::Value, now: u64) {
    let tz_min = crate::platform::local_tz::offset_secs(now).unwrap_or(0) / 60;
    with_texts(v, i64::try_from(now).unwrap_or(i64::MAX), tz_min);
}

#[cfg(test)]
#[path = "../../../tests/backend/common/time_tests.rs"]
mod tests;
