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
//! 一个窗口这里只判 `full` · `near`（不上线）；它的 `tone` 由出口照这两格写（用满 ⇒ fail · 快满 ⇒ warn），线上类型里没有另一格。

use super::facts::{self, Kind};
use super::rules::standing;
use crate::agents::{QuotaReading, QuotaStatus};
use crate::common::cells::{Tone, Words};
use cmd_wire::quota::Source;
pub use cmd_wire::quota::{LoginState, QuotaState, Usage};

/// 一个语义位那一格（后端的行：线上 [`cmd_wire::quota::SlotShow`] 那几格 ＋ 只给后端看的 `window`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotShow {
    pub slot: String,
    pub pct: Option<u32>,
    pub resets_at: Option<u64>,
    pub full: bool,
    pub text: Words,
    pub tone: Tone,
    pub per_pct_text: Option<Words>,
    /// 这一格取的是哪个窗口（原名；对上 [`WindowShow`]，不上线）。
    pub window: String,
}

/// 重置时刻已过、之后还没再读到的那一窗（手上那份读数过时了）：上次的数不再作数，不写旧数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lapsed {
    /// 那一家查得了用量：到点那一下后端已在重读（`accounts/quota/reread.rs`），读回来之前写「读取中」。
    Reading,
    /// 那一家查不了用量，或这一刻重读没成（账上记着）：只说已重置，不编数。
    Reset,
}

/// 一个窗口那一格（后端的行：线上 [`cmd_wire::quota::WindowShow`] 那几格 ＋ 只给后端看的 `resets_at_text` · `resets_at_rel_text` · `seen_at` · `lapsed`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowShow {
    pub name: String,
    pub key: Option<String>,
    pub pct: Option<u32>,
    pub resets_at: Option<u64>,
    /// `resets_at` 写给人看的样子（出口那一下按钟写；写行读它，不上线）。
    pub resets_at_text: Option<Words>,
    /// `resets_at` 距今（出口那一下按钟写；写行读它，不上线）。
    pub resets_at_rel_text: Option<Words>,
    /// 几点看到的（不上线）。
    pub seen_at: u64,
    pub from: Source,
    pub tokens_per_pct: Option<u64>,
    pub per_pct_text: Option<Words>,
    /// 重置时刻已过、之后没再读到 ⇒ 读取中 / 已重置（[`Lapsed`]）；没过 ⇒ 缺。写行读它，不上线。
    pub lapsed: Option<Lapsed>,
    /// 用满（`standing::used_up` 那一处判）。写语气读它，不上线。
    pub full: bool,
    /// 快满（用到 [`NEAR_DEFAULT`]）。写语气读它，不上线。
    pub near: bool,
    /// 语气：这里不判、留常规，由出口照 `full` · `near` 写（`faces/quota_rows.rs` 的 `written`）。
    pub tone: Tone,
    /// 这一窗在这个号那几行里是第几行：这里不判、留空，由出口写行时记（`faces/quota_rows.rs` 的 `seen_block`）。
    pub row: Option<u32>,
}

/// 一个号的显示态（后端的行：线上 [`cmd_wire::quota::QuotaShow`] 那几格 ＋ 只给后端看的 `limiting`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaShow {
    pub kind: Kind,
    pub state: QuotaState,
    pub stale: bool,
    /// 按钮上那个窗口的语义位（卡着的那个；回包没说 ⇒ 用得最多的那个）；没有分窗口的数 ⇒ 缺。写行与开窗那一判读它，不上线。
    pub limiting: Option<String>,
    pub slots: Vec<SlotShow>,
    pub login: LoginState,
    pub windows: Vec<WindowShow>,
    pub usage: Usage,
}

impl SlotShow {
    /// 出口那一下：造线上那一格（只给后端看的几格留下）。
    pub fn wire(self) -> cmd_wire::quota::SlotShow {
        let SlotShow {
            slot,
            pct,
            resets_at,
            full,
            text,
            tone,
            per_pct_text,
            window: _,
        } = self;
        cmd_wire::quota::SlotShow {
            slot,
            pct,
            resets_at,
            full,
            text,
            tone,
            per_pct_text,
        }
    }
}

impl WindowShow {
    /// 出口那一下：造线上那一格（只给后端看的几格留下）。
    pub fn wire(self) -> cmd_wire::quota::WindowShow {
        let WindowShow {
            name,
            key,
            pct,
            resets_at,
            resets_at_text: _,
            resets_at_rel_text: _,
            seen_at: _,
            from,
            tokens_per_pct,
            per_pct_text,
            lapsed: _,
            full: _,
            near: _,
            tone,
            row,
        } = self;
        cmd_wire::quota::WindowShow {
            name,
            key,
            pct,
            resets_at,
            from,
            tokens_per_pct,
            per_pct_text,
            tone,
            row,
        }
    }
}

impl QuotaShow {
    /// ★ 号级「已重置」（[`QuotaState::ResetSinceSeen`]）与窗口级是一个判法（唯一一处）：卡着的那一格取的那一窗过了重置 ⇒ 照那一窗（读取中 / 已重置）；
    /// 那一窗自己没过（过了的是回包那一刻 · 超额那一刻）⇒ 那一窗也记成已重置（没有哪一窗在到点重读），行 · 语义位 · 计量条都照它。
    /// 交回号级那一句；不是这一态 ⇒ 缺。各窗口（[`windows_of`]）补上之后才作数。
    pub fn settle_lapsed(&mut self) -> Option<Lapsed> {
        if self.state != QuotaState::ResetSinceSeen {
            return None;
        }
        let window = self
            .limiting
            .as_deref()
            .and_then(|l| self.slots.iter().find(|s| s.slot == l))
            .map(|s| s.window.clone());
        let w = window.and_then(|n| self.windows.iter_mut().find(|w| w.name == n));
        Some(match w {
            Some(w) => *w.lapsed.get_or_insert(Lapsed::Reset),
            None => Lapsed::Reset,
        })
    }

    /// 出口那一下：造线上的显示态（只给后端看的那一格留下）。
    pub fn wire(self) -> cmd_wire::quota::QuotaShow {
        let QuotaShow {
            kind,
            state,
            stale,
            limiting: _,
            slots,
            login,
            windows,
            usage,
        } = self;
        cmd_wire::quota::QuotaShow {
            kind,
            state,
            stale,
            slots: slots.into_iter().map(SlotShow::wire).collect(),
            login,
            windows: windows.into_iter().map(WindowShow::wire).collect(),
            usage,
        }
    }
}

/// 「快满」的门槛（%）。
pub(crate) const NEAR_DEFAULT: u8 = 80;

/// 最后一次看到距今超过这么多秒 ⇒ 数旧。
pub(crate) const STALE_AFTER: u64 = 30 * 60;

/// 画出来的语义位，按这个次序出。
const SLOTS: [&str; 2] = [cmd_wire::quota::SLOT_5H, cmd_wire::quota::SLOT_7D];

/// 额度账上一条的各窗口（照原名、照出现的次序）：数 · 重置 · 哪来的 · 1% ≈ 多少 · 用满 / 快满那一判（语气留给出口写）。
/// 这个号会不会到点重读照那一处判（[`super::reread::rereadable`]）。
pub fn windows_of(
    o: &super::ledger::Observed,
    key: &dyn Fn(&str) -> Option<String>,
    now: u64,
) -> Vec<WindowShow> {
    windows_of_with(o, key, now, super::reread::rereadable(&o.agent, &o.account))
}

/// [`windows_of`] 的本体：`can` ＝ 这个号会到点重读（[`super::reread::rereadable`]）。
pub fn windows_of_with(
    o: &super::ledger::Observed,
    key: &dyn Fn(&str) -> Option<String>,
    now: u64,
    can: bool,
) -> Vec<WindowShow> {
    o.reading
        .windows
        .iter()
        .map(|w| {
            let seen = o.window_seen(&w.name);
            let lapsed = super::reset_since_seen(w.resets_at, now).then(|| {
                if can && !o.reread_failed_for(&w.name, w.resets_at) {
                    Lapsed::Reading
                } else {
                    Lapsed::Reset
                }
            });
            let pct = pct_now(w, now);
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
                lapsed,
                full: standing::used_up(pct),
                near: pct >= f64::from(NEAR_DEFAULT),
                tone: Tone::Plain,
                row: None,
            }
        })
        .collect()
}

/// ★ 超额那一行出不出、怎么说（唯一判法）：订阅号 · 回包说过在用，或没说关掉、状态是可用 / 预警 ⇒ 出行；
/// 出行时「在用」照事实那一处（[`facts::overage_in_use`]：卡着的窗口没重置）⇒ `Some(true)`，否则 `Some(false)`（开着）；不出行 ⇒ `None`。
pub fn overage_of(r: &QuotaReading, kind: Kind, now: u64) -> Option<bool> {
    let o = r.overage.as_ref().filter(|_| kind == Kind::Sub)?;
    let open = o.in_use
        || (o.disabled.is_none()
            && matches!(o.status, Some(QuotaStatus::Allowed | QuotaStatus::Warning)));
    open.then(|| facts::overage_in_use(r, kind, now))
}

/// 一个窗口此刻用到百分之几（已重置 ⇒ 0；同事实那一处 [`facts::used_now`]）。
fn pct_now(w: &crate::agents::QuotaWindow, now: u64) -> f64 {
    facts::used_now(w, now) * 100.0
}

/// 一个号除额度账之外的几格（种类 · 登录 · 续令牌一时没成时几点再试），由宿主读好交进来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub kind: Kind,
    pub login: LoginState,
    /// 订阅号续令牌一时没成：到这一刻（unix 秒）再试，之前这个号用不了；显示态不用它，事实那一格照它出「几点回来」。
    pub retry_at: Option<u64>,
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
