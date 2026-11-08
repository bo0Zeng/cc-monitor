//! ★ 额度的**显示态**（纯函数）：一个号此刻该画成什么样的那一判 —— 态 · 卡人的那一格 · 每个语义位的数与用满。
//! 写成什么字、什么语气不在这里（`faces/quota_rows.rs` 照这里判好的写）。
//!
//! | 态 | 判 |
//! |---|---|
//! | `unseen` | 额度账上没有它 |
//! | `refused` | 上次那一发被拒、它说的回来时刻未到（与轮换同一个判法 `facts::refused_at`；过了 ⇒ 照下面几行判，各窗口的数照旧作数） |
//! | `overageInUse` | 订阅号正在用付费超额、卡着的窗口未重置 |
//! | `resetSinceSeen` | 卡着的那个窗口（被拒 · 超额 · 按钮那个窗口）看到之后已经重置过了：上次的数不再作数 |
//! | `near` | 有语义位的窗口用到 [`NEAR_DEFAULT`]、未重置，或回包说越过了预警线（线是策略的设置，后端不解它的意思，说不出「到线」） |
//! | `ok` | 其余 |
//!
//! 每个语义位另带一格 `full`（用满：照 `rules::standing::used_up` 那一处判）。
//! 语义位取哪个窗口照事实那一处的挑法（`facts::pick`）。「此刻发不发得出去」不在这里判（`rules::standing`）。
//!
//! 另叠一格 `stale`：最后一次看到距今超过 [`STALE_AFTER`]。各窗口照原名的那几格（[`windows_of`]）由调用方按额度账那一条补上；
//! 一个窗口的 `tone` 就是它那一判（用满 ⇒ fail · 快满 ⇒ warn），线上类型里没有另一格。

use super::facts::{self, Kind};
use super::rules::standing;
use crate::agents::{QuotaReading, QuotaStatus};
use crate::common::cells::{Tone, Words};
pub use cmd_wire::quota::{LoginState, QuotaShow, QuotaState, SlotShow, Usage, WindowShow};

/// 「快满」的门槛（%）。
pub(crate) const NEAR_DEFAULT: u8 = 80;

/// 最后一次看到距今超过这么多秒 ⇒ 数旧。
pub(crate) const STALE_AFTER: u64 = 30 * 60;

/// 画出来的语义位，按这个次序出。
const SLOTS: [&str; 2] = [cmd_wire::quota::SLOT_5H, cmd_wire::quota::SLOT_7D];

/// 额度账上一条的各窗口（照原名、照出现的次序）：数 · 重置 · 哪来的 · 1% ≈ 多少 · 用满 / 快满那一判。
pub fn windows_of(
    o: &super::ledger::Observed,
    key: &dyn Fn(&str) -> Option<String>,
    now: u64,
) -> Vec<WindowShow> {
    o.reading
        .windows
        .iter()
        .map(|w| {
            let seen = o.window_seen(&w.name);
            let reset = super::reset_since_seen(w.resets_at, now);
            let pct = pct_now(w, now);
            let tone = if standing::used_up(pct) {
                Tone::Fail
            } else if pct >= f64::from(NEAR_DEFAULT) {
                Tone::Warn
            } else {
                Tone::Plain
            };
            WindowShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                name: w.name.clone(),
                key: key(&w.name),
                pct: w.used.map(|_| pct.round().max(0.0) as u32),
                resets_at: w.resets_at,
                seen_at: seen.at,
                from: seen.from,
                tokens_per_pct: o.tokens_per_pct(&w.name, now),
                per_pct_text: None,
                reset_since_seen: reset,
                tone,
            }
        })
        .collect()
}

/// 一个窗口此刻用到百分之几（已重置 ⇒ 0；同事实那一处 [`facts::used_now`]）。
fn pct_now(w: &crate::agents::QuotaWindow, now: u64) -> f64 {
    facts::used_now(w, now) * 100.0
}

/// 一个号除额度账之外的几格（种类 · 登录），由宿主读好交进来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub kind: Kind,
    pub login: LoginState,
}

fn slots_of(
    r: &QuotaReading,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Vec<SlotShow> {
    SLOTS
        .iter()
        .filter_map(|s| {
            // 同一语义位有几个窗口（7 天那几个）⇒ 同事实那一处的挑法。
            let w = facts::pick(r, s, now, slot)?;
            let pct = pct_now(w, now);
            Some(SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                reset_text: None,
                slot: (*s).to_string(),
                pct: w.used.map(|_| pct.round().max(0.0) as u32),
                resets_at: w.resets_at,
                full: standing::used_up(pct),
                text: Words::default(),
                tone: Tone::Plain,
                per_pct_text: None,
                window: w.name.clone(),
            })
        })
        .collect()
}

/// ★ 判一个号的显示态。`seen` ＝ 额度账上那一条（快照 ＋ 看到的时刻）。字与语气（每格的 `text` · `tone` · `usage`）留空，由出口写。
pub fn show(
    seen: Option<(&QuotaReading, u64)>,
    facts: Facts,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> QuotaShow {
    let Some((r, seen_at)) = seen else {
        return QuotaShow {
            kind: facts.kind,
            state: QuotaState::Unseen,
            stale: false,
            limiting: None,
            slots: Vec::new(),
            login: facts.login,
            windows: Vec::new(),
            usage: Usage::default(),
        };
    };
    let slots = slots_of(r, now, slot);
    let limiting = r
        .limiting
        .as_deref()
        .and_then(slot)
        .filter(|s| slots.iter().any(|x| x.slot == *s))
        .map(str::to_string)
        .or_else(|| {
            slots
                .iter()
                .max_by_key(|x| x.pct.unwrap_or(0))
                .map(|x| x.slot.clone())
        });
    let passed = |t: Option<u64>| super::reset_since_seen(t, now);
    let shown_reset = limiting
        .as_deref()
        .and_then(|l| slots.iter().find(|x| x.slot == l))
        .and_then(|x| x.resets_at)
        .or(r.resets_at);
    let overage = facts.kind == Kind::Sub && r.overage.as_ref().is_some_and(|o| o.in_use);
    let state = if facts::refused_at(r, now) {
        QuotaState::Refused
    } else if overage && passed(r.resets_at) {
        QuotaState::ResetSinceSeen
    } else if facts::overage_in_use(r, facts.kind, now) {
        QuotaState::OverageInUse
    } else if passed(shown_reset) {
        QuotaState::ResetSinceSeen
    } else if r
        .windows
        .iter()
        .any(|w| slot(&w.name).is_some() && pct_now(w, now) >= f64::from(NEAR_DEFAULT))
        || r.status == Some(QuotaStatus::Warning)
    {
        QuotaState::Near
    } else {
        QuotaState::Ok
    };
    QuotaShow {
        usage: Usage::default(),
        kind: facts.kind,
        state,
        stale: now.saturating_sub(seen_at) > STALE_AFTER,
        limiting,
        slots,
        login: facts.login,
        windows: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/show_tests.rs"]
mod tests;
