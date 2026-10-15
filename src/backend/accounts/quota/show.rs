//! ★ 额度的**显示态**（纯函数）：一个号此刻该画成什么样 —— 界面与手机端只按这里给的词拼，不自己判。
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
//! 每个语义位另带一格 `full`（用满：用到 100%、未重置）：画 `✕` 只看它；被拒而没用满画「{pct}% · 被拒」。
//!
//! 另叠一格 `stale`：最后一次看到距今超过 [`STALE_AFTER`]。各窗口照原名的那几格（[`windows_of`]）由调用方按额度账那一条补上。

use super::facts::{self, Kind};
use crate::agents::{QuotaReading, QuotaStatus};
use crate::common::cells::{Tone, Words};
pub(crate) use cmd_wire::quota::{LoginState, QuotaShow, QuotaState, SlotShow, Usage, WindowShow};

/// 「快满」的门槛（%）。
pub(crate) const NEAR_DEFAULT: u8 = 80;

/// 最后一次看到距今超过这么多秒 ⇒ 数旧。
pub(crate) const STALE_AFTER: u64 = 30 * 60;

/// 画出来的语义位，按这个次序出。
const SLOTS: [&str; 2] = ["5h", "7d"];

/// ★ 一个语义位那一格写成什么字、什么语气（唯一一处：额度每号几行 · 账号面板 · 规则编辑器 · 起会话菜单都读它）。
/// `here` ＝ 它是卡人的那一格（[`QuotaShow::limiting`]）。
pub(crate) fn slot_words(
    state: QuotaState,
    here: bool,
    pct: Option<u32>,
    full: bool,
) -> (Words, Tone) {
    if here && state == QuotaState::OverageInUse {
        return (
            Words(copy_core::copy_text("acct.val.over", &[])),
            Tone::Warn,
        );
    }
    if here && matches!(state, QuotaState::ResetSinceSeen | QuotaState::Unseen) {
        return (
            Words(copy_core::copy_text("acct.val.none", &[])),
            Tone::Plain,
        );
    }
    if full {
        return (
            Words(copy_core::copy_text("acct.val.full", &[])),
            Tone::Fail,
        );
    }
    if here && state == QuotaState::Refused {
        return match pct {
            Some(p) => (
                Words(copy_core::copy_text(
                    "acct.val.refusedPct",
                    &[("pct", &p.to_string())],
                )),
                Tone::Fail,
            ),
            None => (
                Words(copy_core::copy_text("acct.val.refusedOnly", &[])),
                Tone::Fail,
            ),
        };
    }
    match pct {
        None => (
            Words(copy_core::copy_text("acct.val.none", &[])),
            Tone::Plain,
        ),
        Some(p) => (
            Words(copy_core::copy_text(
                "acct.val.pct",
                &[("pct", &p.to_string())],
            )),
            if here && state == QuotaState::Near {
                Tone::Warn
            } else {
                Tone::Plain
            },
        ),
    }
}

/// 用量那一格连成一句（窗口 · 值 · 重置）。
fn joined(u: &mut Usage) {
    let v = u.value.0.as_str();
    u.text = Words(match (&u.window, &u.reset) {
        (None, None) => v.to_string(),
        (None, Some(r)) => copy_core::copy_text("acct.usage.wv", &[("w", v), ("v", &r.0)]),
        (Some(w), None) => copy_core::copy_text("acct.usage.wv", &[("w", &w.0), ("v", v)]),
        (Some(w), Some(r)) => {
            copy_core::copy_text("acct.usage.wvr", &[("w", &w.0), ("v", v), ("r", &r.0)])
        }
    });
}

/// 出口那一下：按钟写时刻旁边那几格（线上类型住 `cmd-wire`，字在这里写）。
pub(crate) trait Stamp {
    fn stamp(&mut self, clock: &crate::common::time::TextClock);
}

impl Stamp for Usage {
    /// 写重置那一格，再连成一句。
    fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        self.reset = self.resets_at.map(|t| {
            Words(copy_core::copy_text(
                "acct.reset.at",
                &[("at", &clock.text(t).0)],
            ))
        });
        joined(self);
    }
}

/// 一个号用量那一格（还没按钟写重置那一格）。`reading_reset` ＝ 回包说的几点能再用（卡着的那一格没给时刻时用它）。
pub(crate) fn usage_of(
    kind: Kind,
    state: QuotaState,
    limiting: Option<&str>,
    slots: &[SlotShow],
    reading_reset: Option<u64>,
) -> Usage {
    let mut u = if kind == Kind::Api {
        if state == QuotaState::Refused {
            Usage {
                value: Words(copy_core::copy_text("acct.val.refusedOnly", &[])),
                tone: Tone::Fail,
                resets_at: reading_reset,
                ..Usage::default()
            }
        } else {
            Usage {
                value: Words(copy_core::copy_text("acct.kind.api", &[])),
                tone: Tone::Plain,
                ..Usage::default()
            }
        }
    } else {
        let w = limiting.unwrap_or(SLOTS[0]);
        let slot = slots.iter().find(|s| s.slot == w);
        let value = slot.map_or_else(
            || {
                Words(if state == QuotaState::Refused {
                    copy_core::copy_text("acct.val.refusedOnly", &[])
                } else {
                    copy_core::copy_text("acct.val.none", &[])
                })
            },
            |s| s.text.clone(),
        );
        let (tone, resets_at) = match state {
            QuotaState::Refused => (Tone::Fail, slot.and_then(|s| s.resets_at).or(reading_reset)),
            QuotaState::OverageInUse => (Tone::Warn, None),
            QuotaState::ResetSinceSeen | QuotaState::Unseen => (Tone::Plain, None),
            _ if slot.is_some_and(|s| s.full) => (Tone::Fail, None),
            QuotaState::Near if slot.is_some_and(|s| s.pct.is_some()) => (Tone::Warn, None),
            _ => (Tone::Plain, None),
        };
        Usage {
            slot: Some(w.to_string()),
            window: Some(Words(crate::accounts::quota::name_words::slot_text(w))),
            value,
            tone,
            resets_at,
            ..Usage::default()
        }
    };
    joined(&mut u);
    u
}

impl Stamp for SlotShow {
    /// 出口那一下：按钟写 `resets_at` 旁边那两格。
    fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(clock.text(t));
            self.resets_at_rel_text = clock.rel(t);
        }
    }
}

impl Stamp for WindowShow {
    /// 出口那一下：按钟写 `resets_at` · `seen_at` 旁边那几格。
    fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(clock.text(t));
            self.resets_at_rel_text = clock.rel(t);
        }
        self.seen_at_text = Some(clock.text(self.seen_at));
        self.seen_at_rel_text = clock.rel(self.seen_at);
    }
}

impl Stamp for QuotaShow {
    /// 出口那一下：每一格的时刻字按钟写好。
    fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        self.slots.iter_mut().for_each(|s| s.stamp(clock));
        self.windows.iter_mut().for_each(|w| w.stamp(clock));
        self.usage.stamp(clock);
    }
}

/// 额度账上一条的各窗口（照原名、照出现的次序）。
pub(crate) fn windows_of(
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
            WindowShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                seen_at_text: None,
                seen_at_rel_text: None,
                name: w.name.clone(),
                key: key(&w.name),
                pct: w
                    .used
                    .map(|_| (facts::used_now(w, now) * 100.0).round().max(0.0) as u32),
                resets_at: w.resets_at,
                seen_at: seen.at,
                from: seen.from,
                reset_since_seen: reset,
            }
        })
        .collect()
}

/// 「用到 N%」的显示判法：这个窗口用到 `n`%、还没重置。
fn over(w: &crate::agents::QuotaWindow, n: u8, now: u64) -> bool {
    w.used.is_some_and(|u| u * 100.0 >= f64::from(n)) && !super::reset_since_seen(w.resets_at, now)
}

/// 一个号除额度账之外的几格（种类 · 登录 · 订阅标识），由宿主读好交进来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) kind: Kind,
    pub(crate) login: LoginState,
    pub(crate) sub_id: Option<String>,
}

fn slots_of(
    r: &QuotaReading,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Vec<SlotShow> {
    SLOTS
        .iter()
        .filter_map(|s| {
            // 同一语义位有几个窗口（7 天那几个）⇒ 取用得最多的那个。
            let w = r
                .windows
                .iter()
                .filter(|w| slot(&w.name) == Some(*s))
                .max_by(|a, b| {
                    a.used
                        .unwrap_or(f64::MIN)
                        .total_cmp(&b.used.unwrap_or(f64::MIN))
                })?;
            Some(SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                slot: (*s).to_string(),
                pct: w.used.map(|u| (u * 100.0).round().max(0.0) as u32),
                resets_at: w.resets_at,
                full: full(w, now),
                text: Words::default(),
                tone: Tone::Plain,
            })
        })
        .collect()
}

/// 用满的唯一判法：这个窗口用到 100%、还没重置。
fn full(w: &crate::agents::QuotaWindow, now: u64) -> bool {
    w.used.is_some_and(|u| u >= 1.0) && !super::reset_since_seen(w.resets_at, now)
}

/// ★ 判一个号的显示态。`seen` ＝ 额度账上那一条（快照 ＋ 看到的时刻）。
pub(crate) fn show(
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
            sub_id: facts.sub_id,
            windows: Vec::new(),
            usage: usage_of(facts.kind, QuotaState::Unseen, None, &[], None),
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
        .any(|w| slot(&w.name).is_some() && over(w, NEAR_DEFAULT, now))
        || r.status == Some(QuotaStatus::Warning)
    {
        QuotaState::Near
    } else {
        QuotaState::Ok
    };
    let mut slots = slots;
    for x in &mut slots {
        (x.text, x.tone) = slot_words(
            state,
            limiting.as_deref() == Some(x.slot.as_str()),
            x.pct,
            x.full,
        );
    }
    QuotaShow {
        usage: usage_of(facts.kind, state, limiting.as_deref(), &slots, r.resets_at),
        kind: facts.kind,
        state,
        stale: now.saturating_sub(seen_at) > STALE_AFTER,
        limiting,
        slots,
        login: facts.login,
        sub_id: facts.sub_id,
        windows: Vec::new(),
    }
}

/// 此刻用不了的号几点回来（被拒 · 超额在兜 ⇒ 卡着的窗口的重置时刻；说不出 ⇒ `None`）。
pub(crate) fn back_at(s: &QuotaShow, r: &QuotaReading, now: u64) -> Option<u64> {
    matches!(s.state, QuotaState::Refused | QuotaState::OverageInUse)
        .then_some(r.resets_at)
        .flatten()
        .filter(|t| *t > now)
}

/// 此刻发得出去：登录拿得到，且不是被拒 / 超额在兜（快满 · 数旧 · 没采样 · 上一窗已过都算能发）。
pub(crate) fn usable(s: &QuotaShow) -> bool {
    s.login == LoginState::Ok && !matches!(s.state, QuotaState::Refused | QuotaState::OverageInUse)
}

/// 同一订阅的稳定标识：账号身份加一个固定前缀做 SHA-256，取前 12 字节的十六进制。不加每台的盐 ⇒ 两台算出来相同；
/// 账号身份是随机的 128 位 ⇒ 由它反推不出原值。
pub(crate) fn sub_id_of(identity: &str) -> String {
    let d = ring::digest::digest(
        &ring::digest::SHA256,
        format!("cc-monitor/subscription\0{identity}").as_bytes(),
    );
    d.as_ref()[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/show_tests.rs"]
mod tests;
