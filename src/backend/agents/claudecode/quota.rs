//! Claude 回包头 → 通用的额度快照（[`crate::agents::QuotaReading`]）。这一族头名只住这里。
//!
//! 读法照 claude 程序自己的读法：
//!
//! | 头（前缀之后） | 值 | 读成 |
//! |---|---|---|
//! | `status` | `allowed` · `allowed_warning` · `rejected` | `status`；缺这个头 ⇒ 限流器没跑，整族不读 |
//! | `reset` | unix 秒（数字串，可带小数，取整） | `resets_at`（卡着的那个窗口） |
//! | `representative-claim` | `five_hour` · `seven_day` · … | `limiting` |
//! | `<窗口>-utilization` · `<窗口>-reset` · `<窗口>-surpassed-threshold` | 小数 · unix 秒 · 小数 | 一个 [`QuotaWindow`]；窗口见 [`WINDOWS`] |
//! | `overage-status` · `overage-reset` · `overage-disabled-reason` · `overage-in-use` | 三值 · unix 秒 · 原值 · `true` | `overage` |
//!
//! 被拒以状态码为准（429）：有这一族头时卡在哪、几点重置照头说；没有（API key 号的回包永远没有这一族）⇒
//! 几点能再用看 `retry-after`（秒）；两样都没说 ⇒ [`REFUSED_BRIEFLY`] 秒之后（多是短时限流，不是用满）。
//! 被拒的快照恒带回来的时刻。`status = rejected` 但回的是 200（超额兜着）不算被拒。

use crate::agents::{LimitReply, QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};

const PREFIX: &str = "anthropic-ratelimit-unified-";

/// 有分窗口读数的那几个窗口：（读成的窗口名, 头里的那一段）。
pub(crate) const WINDOWS: &[(&str, &str)] = &[
    ("five_hour", "5h"),
    ("seven_day", "7d"),
    ("seven_day_overage_included", "7d_oi"),
    ("overage", "overage"),
];

const REFUSED: u16 = 429;

/// 被拒却没说几点能再用（没有 `reset`、没有 `retry-after`）⇒ 当它这么多秒之后回来。
pub(crate) const REFUSED_BRIEFLY: u64 = 60;

/// 窗口名 → 语义位：5 小时那一个是 `5h`；7 天那几个（含分模型的）是 `7d`；超额那一档没有语义位。
pub(crate) fn slot_of(name: &str) -> Option<&'static str> {
    match name {
        "five_hour" => Some("5h"),
        n if n.starts_with("seven_day") => Some("7d"),
        _ => None,
    }
}

/// 按模型（或别的分档）的 7 天窗口名的前缀：`seven_day_<档>`。
const SEVEN_DAY_OF: &str = "seven_day_";

/// 窗口名 → 窗口键（通用层当不透明的键用：上限 · 单段预算按它配）：`five_hour` ⇒ `5h` · `seven_day` ⇒ `7d` ·
/// `seven_day_<档>` ⇒ `7d:<档>`（如 `seven_day_opus` ⇒ `7d:opus`）；超额那一档没有键（不算用量窗口）。
pub(crate) fn key_of(name: &str) -> Option<String> {
    match name {
        "five_hour" => Some("5h".into()),
        "seven_day" => Some("7d".into()),
        n => n
            .strip_prefix(SEVEN_DAY_OF)
            .filter(|m| !m.is_empty())
            .map(|m| format!("7d:{m}")),
    }
}

/// [`key_of`] 反过来：窗口键 → 窗口名（硬上限的回包照它标卡在哪）。认不得 ⇒ `None`。
fn name_of_key(key: &str) -> Option<String> {
    match key {
        "5h" => Some("five_hour".into()),
        "7d" => Some("seven_day".into()),
        k => k
            .strip_prefix("7d:")
            .filter(|m| !m.is_empty())
            .map(|m| format!("{SEVEN_DAY_OF}{m}")),
    }
}

fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn unified<'h>(headers: &'h [(String, String)], tail: &str) -> Option<&'h str> {
    header(headers, &format!("{PREFIX}{tail}"))
}

fn number(v: Option<&str>) -> Option<f64> {
    v.and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite())
}

fn instant(v: Option<&str>) -> Option<u64> {
    number(v).filter(|n| *n >= 0.0).map(|n| n.round() as u64)
}

fn status_of(v: Option<&str>) -> Option<QuotaStatus> {
    match v? {
        "allowed" => Some(QuotaStatus::Allowed),
        "allowed_warning" => Some(QuotaStatus::Warning),
        "rejected" => Some(QuotaStatus::Rejected),
        _ => None,
    }
}

/// 注册表 `DefaultUpstream.quota` 那一格。
pub(crate) fn read(status: u16, headers: &[(String, String)], now: u64) -> Option<QuotaReading> {
    let refused = status == REFUSED;
    let retry_at = || {
        refused.then(|| {
            now.saturating_add(instant(header(headers, "retry-after")).unwrap_or(REFUSED_BRIEFLY))
        })
    };
    let Some(said) = unified(headers, "status") else {
        // 限流器没说话：只有被拒时才有一件事可记（这个号此刻用不了、`retry-after` 秒之后再试，没说就 [`REFUSED_BRIEFLY`] 秒）。
        return refused.then(|| QuotaReading {
            status: None,
            refused: true,
            limiting: None,
            resets_at: retry_at(),
            windows: Vec::new(),
            overage: None,
        });
    };
    let windows = WINDOWS
        .iter()
        .filter_map(|(name, tag)| {
            let w = QuotaWindow {
                name: (*name).to_string(),
                used: number(unified(headers, &format!("{tag}-utilization"))),
                resets_at: instant(unified(headers, &format!("{tag}-reset"))),
                warned_at: number(unified(headers, &format!("{tag}-surpassed-threshold"))),
            };
            (w.used.is_some() || w.resets_at.is_some() || w.warned_at.is_some()).then_some(w)
        })
        .collect();
    let overage = {
        let o = QuotaOverage {
            status: status_of(unified(headers, "overage-status")),
            resets_at: instant(unified(headers, "overage-reset")),
            disabled: unified(headers, "overage-disabled-reason").map(str::to_string),
            in_use: unified(headers, "overage-in-use") == Some("true"),
        };
        (o.status.is_some() || o.resets_at.is_some() || o.disabled.is_some() || o.in_use)
            .then_some(o)
    };
    let resets_at = instant(unified(headers, "reset")).or_else(retry_at);
    Some(QuotaReading {
        status: status_of(Some(said)),
        refused,
        limiting: unified(headers, "representative-claim").map(str::to_string),
        resets_at,
        windows,
        overage,
    })
}

/// 说不出卡在哪个窗口时，重置时刻离此刻多远以内标 5 小时那一个（更远标 7 天那一个）。
const FIVE_HOURS: u64 = 5 * 3600;

/// 「用满」回包：照真被拒那一发的形状 —— 429、限流器那一族头说 `rejected`、卡着的窗口与几点重置、`retry-after`、体是 `rate_limit_error`。
/// - 窗口那一格（`representative-claim`）永远带：缺了 claude 认不成「用满」（当成服务端临时限流、退避重试后报错，也不会到点续）。
///   说得出卡在哪个窗口（`key` ＝ 窗口键，见 [`key_of`]）⇒ 照它的原名；说不出 ⇒ 按远近标：5 小时以内 `five_hour`、更远 `seven_day`
///   （只关显示用词，重置时刻照真的）。
/// - `retry-after` ＝ 离重置还有几秒（至少 1）：按量号的 claude 据它决定等到点再发还是当场报错。
/// - 超额那一族不给（缺 ＝ 没有超额兜着，claude 照「用满」处理）；用量比例不给（不编数）。
///
/// claude 照它原生的样子显示「用满 · 几点重置」，开着自动续时到点自己续。
pub(crate) fn limit_reply(reset_at: u64, now: u64, key: Option<&str>) -> LimitReply {
    let at = reset_at.to_string();
    let wait = reset_at.saturating_sub(now).max(1);
    let by_distance = if wait <= FIVE_HOURS { "5h" } else { "7d" };
    let mut headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        (format!("{PREFIX}status"), "rejected".to_string()),
        (format!("{PREFIX}reset"), at.clone()),
        ("retry-after".to_string(), wait.to_string()),
    ];
    // 卡着的窗口照原名标（`7d:opus` ⇒ `seven_day_opus`，claude 说「Opus limit」）；那个窗口有分窗口的头 ⇒ 一并给它的重置时刻。
    let name = key
        .and_then(name_of_key)
        .or_else(|| name_of_key(by_distance))
        .unwrap_or_default();
    headers.push((format!("{PREFIX}representative-claim"), name.clone()));
    if let Some((_, tail)) = WINDOWS.iter().find(|(n, _)| *n == name) {
        headers.push((format!("{PREFIX}{tail}-reset"), at));
    }
    let body = serde_json::json!({
        "type": "error",
        "error": {"type": "rate_limit_error", "message": "usage limit reached"},
    });
    LimitReply {
        status: "429 Too Many Requests",
        headers,
        body: body.to_string().into_bytes(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/quota_tests.rs"]
mod tests;
