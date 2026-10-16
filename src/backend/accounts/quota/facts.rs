//! ★ 轮换的**事实 F**：每个号的用量 · 重置 · 用速 · 1% ≈ 多少用量 · 被拒 · 超额 · 登录。
//! 只读额度账 ＋ 账号库 ＋ 登录，换一份策略不变；上限、余量、「能不能用」里跟设置有关的那一半不在这里（在判的那一层）。
//!
//! 拼装只有一处（[`crate::accounts::upstream_select::rotate::Hop::facts`]，它手上有额度账与账号库）；这里是形状与纯函数。

use super::show::LoginState;
use crate::agents::QuotaReading;

/// 号的种类（线上 `sub` · `api`）：线上形住 `cmd-wire`。
pub(crate) use cmd_wire::quota::Kind;
/// 每个号的事实（`rotation-view` · `rotation-facts` 带它）：线上形住 `cmd-wire`。
pub(crate) use cmd_wire::rotation::{Fact, FactBlocked, FactWhy, WinFact};

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
pub(crate) fn used_now(w: &crate::agents::QuotaWindow, now: u64) -> f64 {
    if super::reset_since_seen(w.resets_at, now) {
        0.0
    } else {
        w.used.unwrap_or(0.0)
    }
}

/// 一个语义位此刻的窗口：在计时吗 · 用了几点 · 几点重置。
struct Win {
    open: bool,
    used: f64,
    resets: Option<u64>,
}

/// 一个语义位（`5h` / `7d`）此刻的窗口。那一位有几个窗口（`7d` 分档）⇒ 取用得最多的那个。
fn window_fact(
    r: Option<&QuotaReading>,
    w: &str,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Win {
    let closed = Win {
        open: false,
        used: 0.0,
        resets: None,
    };
    let Some(r) = r else { return closed };
    r.windows
        .iter()
        .filter(|x| slot(&x.name) == Some(w))
        .filter(|x| x.used.is_some() && x.resets_at.is_some_and(|t| t > now))
        .map(|x| Win {
            open: true,
            used: used_now(x, now) * 100.0,
            resets: x.resets_at,
        })
        .max_by(|a, b| a.used.total_cmp(&b.used))
        .unwrap_or(closed)
}

/// 事实上用不了的几种（判的那一下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhyNot {
    Refused,
    Full,
    Overage,
    Login,
}

/// ★ 一个号此刻**事实上**能不能用（唯一一处）：被拒 · 超额在兜 · 用满（某个窗口用到 100%、还没重置）· 要重新登录 ·
/// 续令牌一时没成（`retry` ＝ 几点再试）。回 `(为什么不能用, 几点回来)`；能用 ⇒ `None`。
fn standing(
    r: Option<&QuotaReading>,
    kind: Kind,
    login_ok: bool,
    retry: Option<u64>,
    now: u64,
) -> Option<(WhyNot, Option<u64>)> {
    if let Some(r) = r {
        if refused_at(r, now) {
            return Some((WhyNot::Refused, r.resets_at));
        }
        if overage_in_use(r, kind, now) {
            return Some((WhyNot::Overage, r.resets_at));
        }
        let full: Vec<Option<u64>> = r
            .windows
            .iter()
            .filter(|w| w.used.is_some_and(|u| u >= 1.0) && live(w.resets_at, now))
            .map(|w| w.resets_at)
            .collect();
        if !full.is_empty() {
            // 几个窗口都满着 ⇒ 都重置的那一刻；有一个说不出 ⇒ 说不出。
            let back = full
                .iter()
                .copied()
                .collect::<Option<Vec<u64>>>()
                .and_then(|v| v.into_iter().max());
            return Some((WhyNot::Full, back));
        }
    }
    if !login_ok {
        return Some((WhyNot::Login, None));
    }
    retry.filter(|t| *t > now).map(|t| (WhyNot::Login, Some(t)))
}

/// 一个窗口在额度账上量出来的那两样（按窗口原名问；调用方交）：用速（点 / 分）· 1% ≈ 多少用量。
pub(crate) struct Measured<'a> {
    pub(crate) rate: &'a dyn Fn(&str) -> Option<f64>,
    pub(crate) per_pct: &'a dyn Fn(&str) -> Option<u64>,
}

/// 一个语义位此刻的窗口事实（量出来的那两样由调用方按窗口原名问额度账）。
fn win_of(
    r: Option<&QuotaReading>,
    slot: &str,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
    m: &Measured<'_>,
    when: &dyn Fn(u64) -> String,
) -> WinFact {
    let w = window_fact(r, slot, now, slot_of);
    // 量出来的那两样取那一位上用得最多的那个窗口（同 window_fact 的取法）。
    let name = r.and_then(|r| {
        r.windows
            .iter()
            .filter(|x| slot_of(&x.name) == Some(slot))
            .filter(|x| x.used.is_some() && x.resets_at.is_some_and(|t| t > now))
            .max_by(|a, b| used_now(a, now).total_cmp(&used_now(b, now)))
            .map(|x| x.name.clone())
    });
    WinFact {
        open: w.open,
        used: w.used,
        resets: w.resets,
        resets_text: w.resets.map(when),
        rate: name.as_deref().and_then(m.rate),
        tokens_per_pct: name.as_deref().and_then(m.per_pct),
    }
}

/// ★ 拼一个号的事实（唯一一处的纯函数部分）：额度账那一条 ＋ 种类 ＋ 登录（`retry` ＝ 续令牌一时没成、几点再试）。
/// `when` 把时刻写成给人看的样子。
#[allow(clippy::too_many_arguments)]
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
    let st = standing(r, kind, login == LoginState::Ok, retry, now);
    let blocked = st.map(|(w, until)| {
        let why = match w {
            WhyNot::Refused => FactWhy::Refused,
            WhyNot::Overage => FactWhy::Overage,
            WhyNot::Full => FactWhy::Used,
            WhyNot::Login if login == LoginState::NeedsKey => FactWhy::Key,
            WhyNot::Login => FactWhy::Login,
        };
        FactBlocked {
            why,
            until,
            text: blocked_text(why, until.map(when).as_deref()),
        }
    });
    Fact {
        account: account.to_string(),
        kind,
        w5h: win_of(r, "5h", now, slot_of, m, when),
        w7d: win_of(r, "7d", now, slot_of, m, when),
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
        (FactWhy::Refused, None) => copy_text("beRotation.fact.refusedOpen", &[]),
        (FactWhy::Overage, _) => copy_text("beRotation.fact.overage", &[]),
        (FactWhy::Used, Some(at)) => copy_text("beRotation.fact.used", &[("at", at)]),
        (FactWhy::Used, None) => copy_text("beRotation.fact.usedOpen", &[]),
        (FactWhy::Login, Some(at)) => copy_text("beRotation.fact.loginRetry", &[("at", at)]),
        (FactWhy::Login, None) => copy_text("beRotation.fact.login", &[]),
        (FactWhy::Key, _) => copy_text("beRotation.fact.key", &[]),
    }
}
