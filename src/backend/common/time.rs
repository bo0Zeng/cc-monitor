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

/// 自 1970-01-01 起第几天 ⇒ 公历 (年, 月, 日)。换算只住 `copy_core::civil_from_days` 一份。
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let (y, m, d) = copy_core::civil_from_days(z);
    // 月 ∈ 1..=12、日 ∈ 1..=31，转窄不丢。
    (y, m as u32, d as u32)
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

/// 距今（只写未来）：`+12m` · `+1h50m` · `+2h` · `+3d`（满 24h 只写天）；已过 ⇒ `None`。分钟向上取整。
pub(crate) fn fmt_rel(t: i64, now: i64) -> Option<String> {
    let d = t - now;
    if d <= 0 {
        return None;
    }
    if d >= DAY {
        return Some(format!("+{}d", d / DAY));
    }
    let mins = (d + 59) / 60;
    if mins < 60 {
        return Some(format!("+{mins}m"));
    }
    let (h, m) = (mins / 60, mins % 60);
    Some(if m == 0 {
        format!("+{h}h")
    } else {
        format!("+{h}h{m}m")
    })
}

/// **回包出口那一遍**：走遍整份回包，每个对象里认得的时刻格（[`TIME_KEYS`]，值是整数）旁边添一格 `<键>Text`
/// ＝ [`fmt_at`] 按 `now` 与 `tz_min` 写好的字；还没到的时刻再添一格 `<键>RelText` ＝ 距今（[`fmt_rel`]，发出那一刻的字，
/// 要它跟着走就按节拍重问）。界面只照这几格排，不换算。
pub(crate) fn with_texts(v: &mut serde_json::Value, now: i64, tz_min: i64) {
    match v {
        serde_json::Value::Object(m) => {
            let adds: Vec<(String, String)> = TIME_KEYS
                .iter()
                .filter_map(|k| {
                    m.get(*k)
                        .and_then(serde_json::Value::as_i64)
                        .map(|t| (*k, t))
                })
                .flat_map(|(k, t)| {
                    std::iter::once((format!("{k}Text"), fmt_at(t, now, tz_min)))
                        .chain(fmt_rel(t, now).map(|r| (format!("{k}RelText"), r)))
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

/// [`with_texts`] 按这台此刻的本地钟（帧面答 `rotation-session-read` 那一下）。
pub(crate) fn with_texts_here(v: &mut serde_json::Value, now: u64) {
    TextClock::here(now).on(v);
}

/// 成品写时刻字的那一把钟：此刻 ＋ 时区偏移（分钟，东正）。有类型的成品逐格按它写 `…Text` / `…RelText`（同 [`with_texts`] 一个写法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextClock {
    pub(crate) now: i64,
    pub(crate) tz_min: i64,
}

impl TextClock {
    /// 这台此刻的本地钟。
    pub(crate) fn here(now: u64) -> Self {
        TextClock {
            now: i64::try_from(now).unwrap_or(i64::MAX),
            tz_min: crate::platform::local_tz::offset_secs(now).unwrap_or(0) / 60,
        }
    }

    /// 那一刻写给人看的样子（[`fmt_at`]）。
    pub(crate) fn text(&self, t: u64) -> crate::common::cells::Words {
        crate::common::cells::Words(fmt_at(
            i64::try_from(t).unwrap_or(i64::MAX),
            self.now,
            self.tz_min,
        ))
    }

    /// 那一刻距今（[`fmt_rel`]；已过 ⇒ `None`）。
    pub(crate) fn rel(&self, t: u64) -> Option<crate::common::cells::Words> {
        fmt_rel(i64::try_from(t).unwrap_or(i64::MAX), self.now).map(crate::common::cells::Words)
    }

    /// 透传的一团（原数）里认得的时刻格旁边添字（[`with_texts`]）。
    pub(crate) fn on(&self, v: &mut serde_json::Value) {
        with_texts(v, self.now, self.tz_min);
    }
}

/// 一个时刻（unix 秒）在这台本地钟上的秒数：偏移按**那一刻**算（夏令时跟着那一刻）；问不到时区 ⇒ 按 UTC。
pub(crate) fn local_secs(t: i64) -> i64 {
    let off = u64::try_from(t)
        .ok()
        .and_then(crate::platform::local_tz::offset_secs)
        .unwrap_or(0);
    t + off
}

/// 本地钟秒数 ⇒ 钟面 `HH:MM`（不写日子）。
pub(crate) fn hm(local: i64) -> String {
    let secs = local.rem_euclid(DAY);
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// 本地钟秒数 ⇒ 钟面 `HH:MM:SS`（终端快照那一格）。
pub(crate) fn hms(local: i64) -> String {
    let secs = local.rem_euclid(DAY);
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// 秒时刻 ⇒ 这台本地钟的 `HH:MM:SS`。
pub(crate) fn secs_hms_here(t: i64) -> String {
    hms(local_secs(t))
}

/// 记录里写着的 ISO 时刻 ⇒ 这台本地钟的 `HH:MM`（记录卡 · 轮次起止 · 分叉那一轮）。解不出 ⇒ `None`。
pub(crate) fn iso_hm_here(iso: &str) -> Option<String> {
    parse_iso8601_ms(iso).map(ms_hm_here)
}

/// 毫秒时刻 ⇒ 这台本地钟的 `HH:MM`（子运行开始 · 终端快照那几处）。
pub(crate) fn ms_hm_here(ms: i64) -> String {
    hm(local_secs(ms.div_euclid(1_000)))
}

/// 文件的修改时间（本地钟秒数；`today` ＝ 本地今天是第几天）⇒ `(列里那一格, 完整那一格)`：
/// 今天 `15:01` · 今年 `10-02` · 往年 `2025-12-31`；完整 `2026-10-02 15:01:23`。
pub(crate) fn mtime_texts(local: i64, today: i64) -> (String, String) {
    let day = local.div_euclid(DAY);
    let (y, m, d) = civil_from_days(day);
    let short = if day == today {
        hm(local)
    } else if y == civil_from_days(today).0 {
        format!("{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    };
    (short, format!("{y:04}-{m:02}-{d:02} {}", hms(local)))
}

/// [`mtime_texts`] 按这台此刻的本地钟（`files-ls` · `files-stat` 回包那一下）。
pub(crate) fn mtime_texts_here(secs: u64) -> (String, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let today = local_secs(now).div_euclid(DAY);
    mtime_texts(local_secs(i64::try_from(secs).unwrap_or(i64::MAX)), today)
}

// ───────── 历史页那几格（入参都是本地钟秒数：调用方先按各自那一刻的偏移排过）─────────

/// 本地钟秒数那一天的 `MM-DD`。
fn md(local: i64) -> String {
    let (_, m, d) = civil_from_days(local.div_euclid(DAY));
    format!("{m:02}-{d:02}")
}

/// 分段头：今天 · 昨天 · 本周（周一起，周日算上一周的末尾）· 再往前按月（今年 `9 月`、往年 `2025 年 9 月`）。
pub(crate) fn section_text(at: i64, now: i64) -> String {
    let (day, today) = (at.div_euclid(DAY), now.div_euclid(DAY));
    if day >= today {
        return copy_core::copy_text("history.section.today", &[]);
    }
    if day == today - 1 {
        return copy_core::copy_text("history.section.yesterday", &[]);
    }
    // 1970-01-01 是周四 ⇒ 以周一为 0 时它是 3。
    let dow = (today + 3).rem_euclid(7);
    if day >= today - dow {
        return copy_core::copy_text("history.section.week", &[]);
    }
    let (y, m, _) = civil_from_days(day);
    let month = m.to_string();
    if y == civil_from_days(today).0 {
        copy_core::copy_text("history.section.month", &[("month", &month)])
    } else {
        copy_core::copy_text(
            "history.section.yearMonth",
            &[("year", &y.to_string()), ("month", &month)],
        )
    }
}

/// 行尾那一格：今天 `14:02` · 昨天 `10-01 22:10` · 更早 `09-30`（不是今年的 `2025-09-30`）。
pub(crate) fn row_time(at: i64, now: i64) -> String {
    let (day, today) = (at.div_euclid(DAY), now.div_euclid(DAY));
    if day >= today {
        return hm(at);
    }
    if day == today - 1 {
        return format!("{} {}", md(at), hm(at));
    }
    let y = civil_from_days(day).0;
    if y == civil_from_days(today).0 {
        md(at)
    } else {
        format!("{y}-{}", md(at))
    }
}

/// 内容头那一段：`09-30 03:00 – 10-01 04:57`；同一天 `02:01–14:02`（今天）或 `09-30 02:01–14:02`。
pub(crate) fn span_text(from: i64, to: i64, now: i64) -> String {
    let (a, b) = (from.div_euclid(DAY), to.div_euclid(DAY));
    if a == b {
        return if b == now.div_euclid(DAY) {
            format!("{}–{}", hm(from), hm(to))
        } else {
            format!("{} {}–{}", md(from), hm(from), hm(to))
        };
    }
    format!("{} {} – {} {}", md(from), hm(from), md(to), hm(to))
}

/// 会话内查找那一行的时刻：今天 `01:52` · 昨天 `昨天 18:20` · 更早 `09-30 18:20`（不是今年的 `2025-09-30 18:20`）。
pub(crate) fn hit_time(at: i64, now: i64) -> String {
    let (day, today) = (at.div_euclid(DAY), now.div_euclid(DAY));
    if day >= today {
        return hm(at);
    }
    if day == today - 1 {
        return format!(
            "{} {}",
            copy_core::copy_text("history.section.yesterday", &[]),
            hm(at)
        );
    }
    let y = civil_from_days(day).0;
    if y == civil_from_days(today).0 {
        format!("{} {}", md(at), hm(at))
    } else {
        format!("{y}-{} {}", md(at), hm(at))
    }
}

/// 会话内查找命中的时刻按这台此刻的本地钟写（[`hit_time`]）；读不出（`0`）⇒ 空串。
pub(crate) fn hit_text_here(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return String::new();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    hit_time(local_secs(ts_ms.div_euclid(1_000)), local_secs(now))
}

/// **历史页一行的三格**：`at` ⇒ 行尾那一格 ＋ 分段头；`from` → `to` ⇒ 内容头那一段。
/// 时刻是毫秒；`local` 把 unix 秒按那一刻的偏移排成本地钟秒数（生产里是 [`local_secs`]）。
pub(crate) fn history_times(
    at: i64,
    from: i64,
    to: i64,
    now_ms: i64,
    local: &dyn Fn(i64) -> i64,
) -> (
    crate::common::cells::Words,
    crate::common::cells::Words,
    crate::common::cells::Words,
) {
    let l = |ms: i64| local(ms.div_euclid(1_000));
    let now = l(now_ms);
    (
        crate::common::cells::Words(row_time(l(at), now)),
        crate::common::cells::Words(section_text(l(at), now)),
        crate::common::cells::Words(span_text(l(from), l(to), now)),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/common/time_tests.rs"]
mod tests;
