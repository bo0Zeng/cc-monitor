//! ★ 额度的**显示态**（纯函数）：一个号此刻该画成什么样 —— 界面与手机端只按这里给的词拼，不自己判。
//!
//! | 态 | 判 |
//! |---|---|
//! | `unseen` | 额度账上没有它 |
//! | `refused` | 上次那一发被拒、它说的回来时刻未到（与轮换同一个判法 `decide::refused_at`；过了 ⇒ 照下面几行判，各窗口的数照旧作数） |
//! | `overageInUse` | 订阅号正在用付费超额、卡着的窗口未重置 |
//! | `resetSinceSeen` | 卡着的那个窗口（被拒 · 超额 · 按钮那个窗口）看到之后已经重置过了：上次的数不再作数 |
//! | `near` | 有语义位的窗口用到这号这一窗此刻的线（没线 ⇒ [`NEAR_DEFAULT`]）、未重置（同轮换的「过线」一个判法），或回包说越过了预警线 |
//! | `ok` | 其余 |
//!
//! 每个语义位另带一格 `full`（用满：用到 100%、未重置）：画 `✕` 只看它；被拒而没用满画「{pct}% · 被拒」；
//! 一格 `atLine`（那一窗用到了这号这一窗此刻的线、未重置；没线的窗不出）：悬停卡那一窗行尾写「到线」。
//!
//! 另叠一格 `stale`：最后一次看到距今超过 [`STALE_AFTER`]。各窗口照原名的那几格（[`windows_of`]）由调用方按额度账那一条补上。线由调用方按 [`lines_of`] 给：
//! 这台的账用默认规则的线，会话那一份用会话那一份的线。

use super::decide::{self, Kind};
use crate::agents::{QuotaReading, QuotaStatus};
use crate::common::cells::{Tone, Words};
use serde::{Deserialize, Serialize};

/// 那一窗没线（满了才换）时，「快满」的门槛（%）。
pub(crate) const NEAR_DEFAULT: u8 = 80;

/// 最后一次看到距今超过这么多秒 ⇒ 数旧。
pub(crate) const STALE_AFTER: u64 = 30 * 60;

/// 画出来的语义位，按这个次序出。
const SLOTS: [&str; 2] = ["5h", "7d"];

/// 显示态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum QuotaState {
    Ok,
    Near,
    Refused,
    OverageInUse,
    Unseen,
    ResetSinceSeen,
}

/// 这个号此刻拿不拿得到能用的登录：订阅号看凭据文件 ＋ 账号身份（`ok` / `needsLogin`）；按量号看这台 key 表里有没有它（`ok` / `needsKey`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum LoginState {
    Ok,
    /// 订阅号：要重新登录。
    NeedsLogin,
    /// 按量号：这台 key 表里没有它的 key（只对按量号出）。
    NeedsKey,
}

/// 一个语义位（`5h` / `7d`）的那一格：取整的百分比 · 几点重置（说不出 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SlotShow {
    pub slot: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub pct: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    /// `resets_at` 写给人看的样子（出口那一下按钟写，[`SlotShow::stamp`]；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub resets_at_text: Option<Words>,
    /// `resets_at` 距今（`+1h50m`；同上，只在还没到时有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub resets_at_rel_text: Option<Words>,
    /// 用满：这个窗口用到 100%、还没重置（画 `✕`；被拒而没用满画「{pct}% · 被拒」）。没用满 ⇒ 缺。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub full: bool,
    /// 到线：这一窗用到这号这一窗此刻的线、还没重置（没线 ⇒ 不出）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub at_line: bool,
    /// 这一格的字（唯一写法 [`slot_words`]：超额在兜 `超额` · 上一窗已过 / 没数 `—` · 用满 `✕` · 被拒没用满 `58% · 被拒` · 否则 `58%`）。
    #[serde(default)]
    #[cfg_attr(test, ts(type = "string"))]
    pub text: Words,
    /// 那一格的语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 卡着的窗口快满 ⇒ `warn`；否则 `plain`。
    #[serde(default = "plain")]
    #[cfg_attr(test, ts(type = "\"plain\" | \"fail\" | \"warn\""))]
    pub tone: Tone,
}

fn plain() -> Tone {
    Tone::Plain
}

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

/// 一个号的显示态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct QuotaShow {
    pub kind: Kind,
    pub state: QuotaState,
    /// 最后一次看到距今超过 30 分钟（与 `state` 叠着画）。
    pub stale: bool,
    /// 按钮上那个窗口的语义位（卡着的那个；回包没说 ⇒ 用得最多的那个）；没有分窗口的数 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub limiting: Option<String>,
    /// `5h` · `7d` 各一格（有数的才出）。
    pub slots: Vec<SlotShow>,
    pub login: LoginState,
    /// 同一订阅的稳定标识（账号身份的散列，不含原值）：两台看到的同一订阅它相同。订阅号读得出身份才有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub sub_id: Option<String>,
    /// 各窗口照原名一格（`slots` 那两格照留：同一语义位几个窗口并成一格；这里一个窗口一格）；没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<WindowShow>>"))]
    pub windows: Vec<WindowShow>,
}

/// 一个窗口（照原名）：窗口键 · 取整的百分比 · 几点重置 · 几点、从哪看到的 · 已重置未计时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct WindowShow {
    /// 那一家的窗口名（`five_hour` · `seven_day` · `seven_day_<模型>` · …）。
    pub name: String,
    /// 窗口键（`5h` · `7d` · `7d:<模型>`；轮换的上限 · 单段预算按它配）；不算用量窗口的（超额）⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub key: Option<String>,
    /// 取整的百分比（已重置未计时 ⇒ 0）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub pct: Option<u32>,
    /// 几点重置；有 ＝ 这个窗口在计时。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    /// `resets_at` 写给人看的样子（出口那一下按钟写，[`WindowShow::stamp`]；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub resets_at_text: Option<Words>,
    /// `resets_at` 距今（同上，只在还没到时有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub resets_at_rel_text: Option<Words>,
    #[cfg_attr(test, ts(type = "number"))]
    pub seen_at: u64,
    /// `seen_at` 写给人看的样子（同上）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub seen_at_text: Option<Words>,
    /// `seen_at` 距今（别台的钟走在前面时才有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub seen_at_rel_text: Option<Words>,
    pub from: super::ledger::Source,
    /// 重置时刻已过、之后没再看到：上次的数不再作数（用量当 0、窗口没开）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub reset_since_seen: bool,
}

impl SlotShow {
    /// 出口那一下：按钟写 `resets_at` 旁边那两格。
    pub(crate) fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(clock.text(t));
            self.resets_at_rel_text = clock.rel(t);
        }
    }
}

impl WindowShow {
    /// 出口那一下：按钟写 `resets_at` · `seen_at` 旁边那几格。
    pub(crate) fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(clock.text(t));
            self.resets_at_rel_text = clock.rel(t);
        }
        self.seen_at_text = Some(clock.text(self.seen_at));
        self.seen_at_rel_text = clock.rel(self.seen_at);
    }
}

impl QuotaShow {
    /// 出口那一下：每一格的时刻字按钟写好。
    pub(crate) fn stamp(&mut self, clock: &crate::common::time::TextClock) {
        self.slots.iter_mut().for_each(|s| s.stamp(clock));
        self.windows.iter_mut().for_each(|w| w.stamp(clock));
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
                    .map(|_| (decide::used_now(w, now) * 100.0).round().max(0.0) as u32),
                resets_at: w.resets_at,
                seen_at: seen.at,
                from: seen.from,
                reset_since_seen: reset,
            }
        })
        .collect()
}

/// 这个号各窗此刻的线（窗口名 → 线；没线 ⇒ `None`）：与轮换判过线同一处（[`decide::line_at`]）。
pub(crate) fn lines_of<'a>(
    cap: &'a super::rotation::Caps,
    account: &'a str,
    now: u64,
    offset: i64,
    key: &'a dyn Fn(&str) -> Option<String>,
    slot: &'a dyn Fn(&str) -> Option<&'static str>,
) -> impl Fn(&str) -> Option<u8> + 'a {
    move |name| decide::line_at(cap, account, &key(name)?, slot(name), now, offset)
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
    line: &dyn Fn(&str) -> Option<u8>,
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
                at_line: r.windows.iter().any(|x| {
                    slot(&x.name) == Some(*s)
                        && line(&x.name).is_some_and(|n| decide::at_line(x, n, now))
                }),
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

/// ★ 判一个号的显示态。`seen` ＝ 额度账上那一条（快照 ＋ 看到的时刻）；`line` ＝ 这号各窗此刻的线（[`lines_of`]）。
pub(crate) fn show(
    seen: Option<(&QuotaReading, u64)>,
    facts: Facts,
    line: &dyn Fn(&str) -> Option<u8>,
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
        };
    };
    let slots = slots_of(r, now, slot, line);
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
    let state = if decide::refused_at(r, now) {
        QuotaState::Refused
    } else if overage && passed(r.resets_at) {
        QuotaState::ResetSinceSeen
    } else if decide::overage_in_use(r, facts.kind, now) {
        QuotaState::OverageInUse
    } else if passed(shown_reset) {
        QuotaState::ResetSinceSeen
    } else if decide::window_over(
        r,
        &|w| line(w).filter(|n| *n > 0).unwrap_or(NEAR_DEFAULT),
        now,
        slot,
    )
    .is_some()
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
