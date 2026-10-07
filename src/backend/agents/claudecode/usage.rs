//! 官方 claude 报一个号的用量：`claude -p /usage`（不发模型请求、不花额度）→ 通用的额度窗口。这一家的命令行与读法只住这里。
//!
//! 起法：那个号的配置目录交给 `CLAUDE_CONFIG_DIR`（账号 0 ⇒ 摘掉）· `TZ=UTC`（输出里的时刻按 UTC 写，读的时候不要时区库）·
//! 摘掉会盖掉那个号自己登录的那几格。
//!
//! 输出里读的只有这几行（别的行 —— 本机会话的用量构成等，不是按号的 —— 不读）：
//!
//! | 行 | 读成的窗口名 |
//! |---|---|
//! | `Current session: <N>% used[ · resets <时刻> (<时区>)]` | `five_hour` |
//! | `Current week (all models): …` | `seven_day` |
//! | `Current week (Sonnet only): …` | `seven_day_sonnet`（claude 自己就是这么对的） |
//! | `Current week (<名>): …` | `seven_day_<名的小写，非字母数字换成 _>` |
//!
//! `<N>` 是整数百分比（claude 向下取整）；`<时刻>` 是 `Mon D, h[:mm]am|pm`，年份不同时 `Mon D, YYYY, h[:mm]am|pm`；
//! 没写年份 ⇒ 按「离此刻最近的将来」补。有重置时刻 ＝ 这个窗口在计时；没有 ⇒ 没在计时（`resetsAt` 缺）。
//! 时区那一格不是 UTC · 一行这几种都对不上 · 一行都没有 ⇒ 整份「读不懂」（不猜、不写账）。

use crate::agents::{QuotaWindow, UsageFace};

/// 注册表 `DefaultUpstream.usage` 那一格。
pub(crate) const FACE: UsageFace = UsageFace {
    program: "claude",
    args: &["-p", "/usage"],
    dir_env: super::paths::CONFIG_DIR_ENV,
    env_remove: &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ],
    env_set: &[("TZ", "UTC")],
    read,
};

/// 输出里这几种时区写法都是 UTC（起它时给了 `TZ=UTC`）。
const UTC_NAMES: &[&str] = &["UTC", "Etc/UTC", "GMT", "Etc/GMT", "UCT", "Etc/UCT"];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// 一行标题 → 窗口名；不是这几种 ⇒ `None`。
fn window_of(title: &str) -> Option<String> {
    if title == "Current session" {
        return Some("five_hour".into());
    }
    let inner = title.strip_prefix("Current week (")?.strip_suffix(')')?;
    match inner {
        "all models" => Some("seven_day".into()),
        "Sonnet only" => Some("seven_day_sonnet".into()),
        name => {
            let slug: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect();
            let slug = slug
                .split('_')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            (!slug.is_empty()).then(|| format!("seven_day_{slug}"))
        }
    }
}

/// unix 秒 → 那一刻（UTC）是哪一年。
fn year_of(t: u64) -> i64 {
    crate::common::time::civil_from_days(i64::try_from(t / 86_400).unwrap_or(i64::MAX / 2)).0
}

/// `h[:mm]am|pm` → （时, 分）。
fn clock_of(s: &str) -> Option<(u32, u32)> {
    let (num, pm) = if let Some(n) = s.strip_suffix("pm") {
        (n, true)
    } else {
        (s.strip_suffix("am")?, false)
    };
    let (h, m) = match num.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h, m.parse::<u32>().ok()?),
        Some(_) => return None,
        None => (num, 0),
    };
    let h: u32 = h.parse().ok().filter(|h| (1..=12).contains(h))?;
    (m < 60).then_some((h % 12 + if pm { 12 } else { 0 }, m))
}

/// `Mon D[, YYYY], h[:mm]am|pm (<时区>)` → unix 秒（UTC）；没写年份 ⇒ 离 `now` 最近的将来那一年。
fn reset_of(s: &str, now: u64) -> Result<u64, String> {
    let bad = || format!("reset time `{s}`");
    let (when, tz) = s
        .strip_suffix(')')
        .and_then(|x| x.rsplit_once(" ("))
        .ok_or_else(bad)?;
    if !UTC_NAMES.contains(&tz) {
        return Err(format!("time zone `{tz}` is not UTC"));
    }
    let parts: Vec<&str> = when.split(", ").collect();
    let (md, year, clock) = match parts.as_slice() {
        [md, clock] => (*md, None, *clock),
        [md, y, clock] => (*md, Some(y.parse::<i64>().map_err(|_| bad())?), *clock),
        _ => return Err(bad()),
    };
    let (mon, day) = md.split_once(' ').ok_or_else(bad)?;
    let month = MONTHS
        .iter()
        .position(|m| *m == mon)
        .and_then(|i| u32::try_from(i + 1).ok())
        .ok_or_else(bad)?;
    let day: u32 = day
        .parse()
        .ok()
        .filter(|d| (1..=31).contains(d))
        .ok_or_else(bad)?;
    let (h, m) = clock_of(clock).ok_or_else(bad)?;
    let at = |y: i64| {
        u64::try_from(
            crate::common::time::days_from_civil(y, month, day) * 86_400
                + i64::from(h * 3600 + m * 60),
        )
        .ok()
    };
    match year {
        Some(y) => at(y).ok_or_else(bad),
        None => {
            let y = year_of(now);
            let all: Vec<u64> = [y - 1, y, y + 1].into_iter().filter_map(at).collect();
            all.iter()
                .copied()
                .filter(|t| *t >= now)
                .min()
                .or_else(|| all.iter().copied().max())
                .ok_or_else(bad)
        }
    }
}

/// 一行 → 一个窗口；这一行不是用量行 ⇒ `Ok(None)`；是而读不懂 ⇒ `Err`。
fn row_of(line: &str, now: u64) -> Result<Option<QuotaWindow>, String> {
    let line = line.trim();
    if !(line.starts_with("Current session") || line.starts_with("Current week (")) {
        return Ok(None);
    }
    let bad = || format!("usage line `{line}`");
    let (title, rest) = line.split_once(": ").ok_or_else(bad)?;
    let name = window_of(title).ok_or_else(bad)?;
    let (used, reset) = match rest.split_once(" \u{b7} resets ") {
        Some((u, r)) => (u, Some(r)),
        None => (rest, None),
    };
    let pct: u32 = used
        .strip_suffix("% used")
        .and_then(|n| n.parse().ok())
        .ok_or_else(bad)?;
    let resets_at = reset.map(|r| reset_of(r, now)).transpose()?;
    Ok(Some(QuotaWindow {
        name,
        used: Some(f64::from(pct) / 100.0),
        resets_at,
        warned_at: None,
    }))
}

/// 注册表 `UsageFace.read` 那一格：整份输出 → 窗口（按出现的次序）。
pub(crate) fn read(out: &str, now: u64) -> Result<Vec<QuotaWindow>, String> {
    let mut windows: Vec<QuotaWindow> = Vec::new();
    for line in out.lines() {
        if let Some(w) = row_of(line, now)? {
            if windows.iter().any(|x| x.name == w.name) {
                return Err(format!("window `{}` appears twice", w.name));
            }
            windows.push(w);
        }
    }
    if windows.is_empty() {
        return Err("no usage line".into());
    }
    Ok(windows)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/usage_tests.rs"]
mod tests;
