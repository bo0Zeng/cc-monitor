//! ★ 轮换的**事实 F**：每个号的用量 · 重置 · 用速 · 1% ≈ 多少用量 · 被拒 · 超额 · 登录。
//! 只读额度账 ＋ 账号库 ＋ 登录，换一份策略不变；上限、余量、「能不能用」里跟设置有关的那一半不在这里（在判的那一层）。
//! 「事实上发不发得出去」不在这里判：照 [`super::rules::standing`] 那一处出 `blocked`。
//!
//! 拼装只有一处（[`crate::accounts::upstream_select::rotate::Hop::facts`]，它手上有额度账与账号库）；这里是形状与纯函数。

use super::rules::{standing, Acct, Barred};
use super::show::LoginState;
use crate::agents::{QuotaReading, QuotaWindow};

/// 号的种类（线上 `sub` · `api`）：线上形住 `cmd-wire`。
pub use cmd_wire::quota::Kind;
/// 每个号的事实（`rotation-view` · `rotation-facts` 带它）：线上形住 `cmd-wire`。
pub use cmd_wire::rotation::{Fact, FactBlocked, FactWhy, WinFact};

fn live(t: Option<u64>, now: u64) -> bool {
    !super::reset_since_seen(t, now)
}

/// 「额度账上这个号此刻被拒着」的唯一判法：上次那一发被拒、它说的回来时刻还没到（说不出时刻的旧账 ⇒ 不算）。显示态同问这一处。
pub(crate) fn refused_at(r: &QuotaReading, now: u64) -> bool {
    r.refused && r.resets_at.is_some_and(|t| t > now)
}

/// 订阅号正在用付费超额、卡着的窗口还没重置。
pub(crate) fn overage_in_use(r: &QuotaReading, kind: Kind, now: u64) -> bool {
    kind == Kind::Sub && r.overage.as_ref().is_some_and(|o| o.in_use) && live(r.resets_at, now)
}

/// 一个窗口此刻的用量（已重置、未计时 ⇒ 0）。
pub(crate) fn used_now(w: &QuotaWindow, now: u64) -> f64 {
    if super::reset_since_seen(w.resets_at, now) {
        0.0
    } else {
        w.used.unwrap_or(0.0)
    }
}

/// 一个窗口此刻在计时：有用量、重置时刻还没到。
pub(crate) fn open(w: &QuotaWindow, now: u64) -> bool {
    w.used.is_some() && w.resets_at.is_some_and(|t| t > now)
}

/// ★ 一个语义位（`5h` / `7d`）此刻取哪个窗口（唯一的挑法：事实 · 显示态 · 别的会话的用速都照它）：
/// 那一位有几个窗口（`7d` 分档）⇒ 在计时的先，再取此刻用得最多的那个（[`used_now`]，已重置的算 0）。
pub(crate) fn pick<'a>(
    r: &'a QuotaReading,
    slot: &str,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
) -> Option<&'a QuotaWindow> {
    r.windows
        .iter()
        .filter(|x| slot_of(&x.name) == Some(slot))
        .max_by(|a, b| {
            open(a, now)
                .cmp(&open(b, now))
                .then(used_now(a, now).total_cmp(&used_now(b, now)))
        })
}

/// 一个窗口在额度账上量出来的那两样（按窗口原名问；调用方交）：用速（点 / 分）· 1% ≈ 多少用量。
pub(crate) struct Measured<'a> {
    pub(crate) rate: &'a dyn Fn(&str) -> Option<f64>,
    pub(crate) per_pct: &'a dyn Fn(&str) -> Option<u64>,
}

/// 一个语义位此刻的窗口事实（[`pick`] 挑的那个在计时才算开着；量出来的那两样由调用方按窗口原名问额度账）。
/// 周窗不在计时 ⇒ 重置时刻照 [`next_week_reset`] 推。
fn win_of(
    r: Option<&QuotaReading>,
    slot: &str,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
    m: &Measured<'_>,
    when: &dyn Fn(u64) -> String,
) -> WinFact {
    match r
        .and_then(|r| pick(r, slot, now, slot_of))
        .filter(|x| open(x, now))
    {
        None => {
            let resets = (slot == WEEK_SLOT)
                .then(|| r.and_then(|r| pick(r, slot, now, slot_of))?.resets_at)
                .flatten()
                .map(|t| next_week_reset(t, now));
            WinFact {
                open: false,
                used: 0.0,
                resets,
                resets_text: resets.map(when),
                rate: None,
                tokens_per_pct: None,
            }
        }
        Some(x) => WinFact {
            open: true,
            used: used_now(x, now) * 100.0,
            resets: x.resets_at,
            resets_text: x.resets_at.map(when),
            rate: (m.rate)(&x.name),
            tokens_per_pct: (m.per_pct)(&x.name),
        },
    }
}

/// 周窗那一位：每个号有固定的每周重置时刻、跟用不用无关（5 小时窗由用触发，没有这回事）。
const WEEK_SLOT: &str = "7d";
const WEEK: u64 = 7 * 86_400;

/// 周窗不在计时时交的重置：读数留着的那一刻还没到 ⇒ 照它；过了 ⇒ 往后推整几周，取第一个在此刻之后的。
/// 事实里 `open = false` 而 `resets` 有值 ⇒ 这个时刻是推出来的，不是读到的。
fn next_week_reset(t: u64, now: u64) -> u64 {
    if t > now {
        t
    } else {
        t + WEEK * ((now - t) / WEEK + 1)
    }
}

/// 跟窗口无关的「用不了」：超额在兜（到卡着的窗口重置）· 缺 key · 要重新登录 · 续令牌一时没成（`retry` ＝ 几点再试）。
fn barred_of(
    r: Option<&QuotaReading>,
    kind: Kind,
    login: LoginState,
    retry: Option<u64>,
    now: u64,
) -> Option<Barred> {
    if let Some(r) = r.filter(|r| overage_in_use(r, kind, now)) {
        return Some(Barred {
            why: FactWhy::Overage,
            until: r.resets_at,
        });
    }
    match login {
        LoginState::NeedsKey => Some(Barred {
            why: FactWhy::Key,
            until: None,
        }),
        LoginState::NeedsLogin => Some(Barred {
            why: FactWhy::Login,
            until: None,
        }),
        LoginState::Ok => retry.filter(|t| *t > now).map(|t| Barred {
            why: FactWhy::Login,
            until: Some(t),
        }),
    }
}

/// ★ 拼一个号的事实（唯一一处的纯函数部分）：额度账那一条 ＋ 种类 ＋ 登录（`retry` ＝ 续令牌一时没成、几点再试）。
/// 用不了的那一格照 [`standing::sendable`] / [`standing::sendable_again`] 出。`when` 把时刻写成给人看的样子。
#[expect(
    clippy::too_many_arguments,
    reason = "纯函数：一个号的几样事实（额度账那一条 · 种类 · 登录 · 再试时刻 · 此刻 · 槽位 · 时刻写法）逐样交进来，判据照样逐样喂"
)]
pub(crate) fn fact_of(
    account: &str,
    r: Option<&QuotaReading>,
    seen_at: Option<u64>,
    kind: Kind,
    login: LoginState,
    retry: Option<u64>,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
    m: &Measured<'_>,
    when: &dyn Fn(u64) -> String,
) -> Fact {
    let w5h = win_of(r, "5h", now, slot_of, m, when);
    let w7d = win_of(r, "7d", now, slot_of, m, when);
    let a = Acct {
        id: account.to_string(),
        api: kind == Kind::Api,
        used: [w5h.used, w7d.used],
        resets: [w5h.resets, w7d.resets],
        refused_until: r.filter(|r| refused_at(r, now)).and_then(|r| r.resets_at),
        barred: barred_of(r, kind, login, retry, now),
        rate: [w5h.rate, w7d.rate],
        per_pct: [w5h.tokens_per_pct, w7d.tokens_per_pct],
        weekly: r.is_some_and(|r| r.windows.iter().any(|x| slot_of(&x.name) == Some("7d"))),
        others: None,
        past: Default::default(),
    };
    let blocked = (!standing::sendable(&a, now)).then(|| {
        let why = if a.refused_until.is_some() {
            FactWhy::Refused
        } else {
            a.barred
                .filter(|b| b.until.is_none_or(|u| u > now))
                .map_or(FactWhy::Used, |b| b.why)
        };
        let until = standing::sendable_again(&a, now);
        FactBlocked {
            why,
            until,
            text: blocked_text(why, until.map(when).as_deref()),
        }
    });
    Fact {
        account: account.to_string(),
        kind,
        w5h,
        w7d,
        weekly: a.weekly,
        login,
        seen_at,
        blocked,
    }
}

/// 用不了的那一句。
fn blocked_text(why: FactWhy, at: Option<&str>) -> String {
    use copy_core::copy_text;
    match (why, at) {
        (FactWhy::Refused, Some(at)) => copy_text("beRotation.fact.refused", &[("at", at)]),
        (FactWhy::Refused, None) => copy_text("reason.code.refused", &[]),
        (FactWhy::Overage, _) => copy_text("beRotation.fact.overage", &[]),
        (FactWhy::Used, Some(at)) => copy_text("beRotation.fact.used", &[("at", at)]),
        (FactWhy::Used, None) => copy_text("beRotation.fact.usedOpen", &[]),
        (FactWhy::Login, Some(at)) => copy_text("beRotation.fact.loginRetry", &[("at", at)]),
        (FactWhy::Login, None) => copy_text("beRotation.fact.login", &[]),
        (FactWhy::Key, _) => copy_text("beRotation.fact.key", &[]),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/facts_tests.rs"]
mod tests;
