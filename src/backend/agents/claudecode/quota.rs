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
//! 几点能再用看 `retry-after`（秒）。`status = rejected` 但回的是 200（超额兜着）不算被拒。

use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};

const PREFIX: &str = "anthropic-ratelimit-unified-";

/// 有分窗口读数的那几个窗口：（读成的窗口名, 头里的那一段）。
pub(crate) const WINDOWS: &[(&str, &str)] = &[
    ("five_hour", "5h"),
    ("seven_day", "7d"),
    ("seven_day_overage_included", "7d_oi"),
    ("overage", "overage"),
];

const REFUSED: u16 = 429;

/// 窗口名 → 语义位：5 小时那一个是 `5h`；7 天那几个（含分模型的）是 `7d`；超额那一档没有语义位。
pub(crate) fn slot_of(name: &str) -> Option<&'static str> {
    match name {
        "five_hour" => Some("5h"),
        n if n.starts_with("seven_day") => Some("7d"),
        _ => None,
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
        refused
            .then(|| instant(header(headers, "retry-after")).map(|s| now.saturating_add(s)))
            .flatten()
    };
    let Some(said) = unified(headers, "status") else {
        // 限流器没说话：只有被拒时才有一件事可记（这个号此刻用不了、`retry-after` 秒之后再试）。
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

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/quota_tests.rs"]
mod tests;
