//! ★ 换号的**唯一判定**（纯函数）：此刻这个会话的号该不该换、换到谁、为什么。
//!
//! | 情况 | 判 |
//! |---|---|
//! | 刚回来的回包被拒（429） | 触发 `full{w}` |
//! | 发之前：额度账上此刻的号被拒、还没到重置时刻 | 触发 `full{w}` |
//! | 发之前：此刻的号正在用付费超额 | 触发；只找**订阅号**接，都满 ⇒ 留在超额（`toOverage`） |
//! | 发之前：「到 N% 换」且 5h / 7d 有一个窗口用到 N%、还没重置 | 触发 `threshold{n}` |
//! | 其余 | 不换 |
//!
//! 触发之后按轮换的顺序**从头**取首个能接的号（列表顺序 ＝ 偏好；起始账号恢复了就回到它），跳过：此刻的号 · 这一发已经试过的 ·
//! 额度账上满着的（被拒未到重置 · 订阅号超额在用 · 阈值模式过了阈值）· 接不上的（`ready` 说为什么，记成「跳过」）。
//! 一个都没有 ⇒ 不换（被拒就原样交回上游的拒绝）。候选只从池里、且不含试过的 ⇒ 一发请求至多换「池子大小」次，不打转。

use super::rotation::{RotationWhen, SwitchWhy, Unready};
use crate::agents::QuotaReading;

/// 号的种类（接超额只找订阅号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Sub,
    Api,
}

/// 判一次要的全部事实。
pub(crate) struct Facts<'a> {
    /// 这个会话的轮换池（按序）。
    pub(crate) pool: &'a [String],
    pub(crate) when: RotationWhen,
    /// 此刻走的号（刚回来的那个回包就是它答的）。
    pub(crate) current: &'a str,
    pub(crate) now: u64,
    /// 刚回来的回包读成的额度快照；问「发之前」时为 `None`。
    pub(crate) heard: Option<&'a QuotaReading>,
    /// 额度账上各号最近的快照。
    pub(crate) seen: &'a dyn Fn(&str) -> Option<QuotaReading>,
    pub(crate) kind: &'a dyn Fn(&str) -> Kind,
    /// 窗口名 → 语义位（`5h` / `7d`）。
    pub(crate) slot: &'a dyn Fn(&str) -> Option<&'static str>,
    /// 这一发已经试过的号（含此刻的）。
    pub(crate) tried: &'a [String],
}

/// 判的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// 不用换。
    Stay,
    /// 换到 `to`。
    Switch {
        to: String,
        why: SwitchWhy,
        from_resets_at: Option<u64>,
        skipped: Vec<(String, Unready)>,
    },
    /// 该换、没得换：照原样走（被拒 ⇒ 原样交回；超额在兜 ⇒ `why = toOverage`，留在超额）。
    Stuck {
        why: SwitchWhy,
        from_resets_at: Option<u64>,
        skipped: Vec<(String, Unready)>,
    },
}

/// 触发的那一种。
struct Trigger {
    why: SwitchWhy,
    resets_at: Option<u64>,
    /// 超额在兜：只找订阅号接。
    overage: bool,
}

fn live(t: Option<u64>, now: u64) -> bool {
    t.is_none_or(|t| t > now)
}

fn full_why(r: &QuotaReading, f: &Facts<'_>) -> SwitchWhy {
    SwitchWhy::Full {
        w: r.limiting
            .as_deref()
            .and_then(|l| (f.slot)(l))
            .map(str::to_string),
    }
}

/// 阈值模式下过了阈值、还没重置的那个窗口的重置时刻（`Some(None)` ＝ 过了、说不出几点重置）。
fn over_threshold(r: &QuotaReading, f: &Facts<'_>) -> Option<(u8, Option<u64>)> {
    let RotationWhen::Threshold { n } = f.when else {
        return None;
    };
    r.windows
        .iter()
        .filter(|w| (f.slot)(&w.name).is_some())
        .find(|w| w.used.is_some_and(|u| u * 100.0 >= f64::from(n)) && live(w.resets_at, f.now))
        .map(|w| (n, w.resets_at))
}

fn overage_in_use(r: &QuotaReading, kind: Kind, now: u64) -> bool {
    kind == Kind::Sub && r.overage.as_ref().is_some_and(|o| o.in_use) && live(r.resets_at, now)
}

fn trigger(f: &Facts<'_>) -> Option<Trigger> {
    if let Some(r) = f.heard {
        return r.refused.then(|| Trigger {
            why: full_why(r, f),
            resets_at: r.resets_at,
            overage: false,
        });
    }
    let r = (f.seen)(f.current)?;
    if r.refused && r.resets_at.is_some_and(|t| t > f.now) {
        return Some(Trigger {
            why: full_why(&r, f),
            resets_at: r.resets_at,
            overage: false,
        });
    }
    if overage_in_use(&r, (f.kind)(f.current), f.now) {
        return Some(Trigger {
            why: full_why(&r, f),
            resets_at: r.resets_at,
            overage: true,
        });
    }
    over_threshold(&r, f).map(|(n, resets_at)| Trigger {
        why: SwitchWhy::Threshold { n },
        resets_at,
        overage: false,
    })
}

/// 额度账上这个号此刻满着（换过去也接不住）。
pub(crate) fn spent(account: &str, f: &Facts<'_>) -> bool {
    (f.seen)(account).is_some_and(|r| {
        (r.refused && r.resets_at.is_some_and(|t| t > f.now))
            || overage_in_use(&r, (f.kind)(account), f.now)
            || over_threshold(&r, f).is_some()
    })
}

/// 按序取首个能接的号（不含此刻的 · 试过的 · 满着的；`subs_only` ⇒ 只要订阅号）；跳过的交回。
fn pick(
    f: &Facts<'_>,
    subs_only: bool,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> (Option<String>, Vec<(String, Unready)>) {
    let mut skipped: Vec<(String, Unready)> = Vec::new();
    let candidates = f.pool.iter().filter(|a| {
        a.as_str() != f.current
            && !f.tried.iter().any(|t| t == *a)
            && (!subs_only || (f.kind)(a) == Kind::Sub)
            && !spent(a, f)
    });
    for a in candidates {
        match ready(a) {
            Ok(()) => return (Some(a.clone()), skipped),
            Err(u) => skipped.push((a.clone(), u)),
        }
    }
    (None, skipped)
}

/// ★ 判一次。`ready(号)` 问这个号此刻接不接得上（要登录 / key / 账号身份）—— 只对真轮到的号问。
pub(crate) fn decide(f: &Facts<'_>, ready: &mut dyn FnMut(&str) -> Result<(), Unready>) -> Verdict {
    let Some(t) = trigger(f) else {
        return Verdict::Stay;
    };
    match pick(f, t.overage, ready) {
        (Some(to), skipped) => Verdict::Switch {
            to,
            why: t.why,
            from_resets_at: t.resets_at,
            skipped,
        },
        (None, skipped) => Verdict::Stuck {
            why: if t.overage {
                SwitchWhy::ToOverage
            } else {
                t.why
            },
            from_resets_at: t.resets_at,
            skipped,
        },
    }
}

/// 此刻的号要是触发了会换到谁（不管此刻触没触发；同一套候选与次序）。
pub(crate) fn next_of(
    f: &Facts<'_>,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> Option<String> {
    pick(f, false, ready).0
}

/// 此刻的号发不出去（被拒未到重置；超额在兜不算 —— 那一发照过）。
pub(crate) fn refused_now(f: &Facts<'_>) -> bool {
    trigger(f).is_some_and(|t| !t.overage && matches!(t.why, SwitchWhy::Full { .. }))
}

/// 这个号满着的话几点回来：被拒 / 超额 ⇒ 卡着的那个窗口的重置时刻；阈值 ⇒ 过了阈值那个窗口的重置时刻。说不出 ⇒ `None`。
pub(crate) fn back_at(account: &str, f: &Facts<'_>) -> Option<u64> {
    let r = (f.seen)(account)?;
    if r.refused || overage_in_use(&r, (f.kind)(account), f.now) {
        return r.resets_at.filter(|t| *t > f.now);
    }
    over_threshold(&r, f).and_then(|(_, t)| t)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/decide_tests.rs"]
mod tests;
