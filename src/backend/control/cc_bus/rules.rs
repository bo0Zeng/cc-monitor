//! cc-bus 的发信规矩：去重 · 限流 · 敲不敲。纯函数：账（[`Ledger`]）由收件箱那一处（[`super::store`]）在同一把锁里读出来、
//! 判完与那一封一起写回；投不进去就不写，账也就不动（投到了才记数）。规矩只住这一处，入口脚本不判。
//!
//! - 去重：同一发件人发给同一收件人，act · title · re · refs · 正文都一样（id 与时间是每次另盖的，不算），
//!   [`DEDUP_WINDOW_MS`] 内第二封 ⇒ `duplicate`。原样弹回来的信由它挡。
//! - 限流：每一对 [`RATE_PAIR`] 封、全台 [`RATE_GLOBAL`] 封 / [`RATE_WINDOW_MS`]（固定窗）；超了 ⇒ `throttled`。来回不停的两条会话由它挡。
//! - 敲不敲：tier 1 且收件人那条会话 [`KNOCK_DEBOUNCE_MS`] 内没敲过 ⇒ 敲（记下这一刻）；tier 2 不敲。

use crate::control::cc_bus_envelope::Refusal;
use crate::control::cc_bus_wire::BusEnvelope;
use std::collections::BTreeMap;

/// 去重窗。
pub(crate) const DEDUP_WINDOW_MS: i64 = 10_000;
/// 限流窗（固定窗）。
pub(crate) const RATE_WINDOW_MS: i64 = 60_000;
/// 一对（发件人 → 收件人）一窗最多几封。
pub(crate) const RATE_PAIR: u32 = 20;
/// 这台一窗最多几封。
pub(crate) const RATE_GLOBAL: u32 = 120;
/// 同一条会话两次敲门至少隔多久。
pub(crate) const KNOCK_DEBOUNCE_MS: i64 = 2_000;

/// 规矩要记的那几样（落盘在收件箱那一处的状态文件里）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Ledger {
    /// 去重窗里投到的那几封。
    #[serde(default)]
    pub(crate) recent: Vec<Recent>,
    /// 每一对的限流窗，键 `发件人>收件人`。
    #[serde(default)]
    pub(crate) pairs: BTreeMap<String, Window>,
    /// 全台的限流窗。
    #[serde(default)]
    pub(crate) global: Window,
    /// 各会话上次敲门的时刻（UNIX 毫秒）。
    #[serde(default)]
    pub(crate) knocked: BTreeMap<String, i64>,
}

/// 去重窗里的一封：指纹 · 投到的时刻 · 它的消息 id。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Recent {
    pub(crate) key: String,
    pub(crate) at: i64,
    pub(crate) id: String,
}

/// 一个固定窗：从哪一刻起 · 窗里已经几封。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Window {
    pub(crate) start: i64,
    pub(crate) count: u32,
}

/// 「同一封」的指纹：除了 id 与时间（每次另盖）以外的每一格。
fn fingerprint(e: &BusEnvelope) -> String {
    let same = serde_json::json!([e.from, e.to, e.tier, e.act, e.title, e.re, e.refs, e.text]);
    format!("{:016x}", cmd_wire::fnv::fnv1a64(same.to_string().as_bytes()))
}

/// 这一窗还开着没有（没开过 ⇒ 当已经关了）。
fn open(w: &Window, now: i64) -> bool {
    w.count > 0 && now - w.start < RATE_WINDOW_MS
}

/// 窗里还放得下一封没有。
fn room(w: &Window, limit: u32, now: i64) -> bool {
    !open(w, now) || w.count < limit
}

/// 记上一封：窗关了就从这一刻另开一窗。
fn bump(w: &mut Window, now: i64) {
    if !open(w, now) {
        *w = Window {
            start: now,
            count: 0,
        };
    }
    w.count += 1;
}

/// 判一封：收 ⇒ `Ok(敲不敲)`，账照它记上；拒 ⇒ 码 ＋ 原话，账不动。顺手扔掉窗外再也用不着的那几笔。
pub(crate) fn admit(l: &mut Ledger, e: &BusEnvelope, now: i64) -> Result<bool, Refusal> {
    l.recent.retain(|r| now - r.at < DEDUP_WINDOW_MS);
    l.pairs.retain(|_, w| open(w, now));
    l.knocked.retain(|_, at| now - *at < KNOCK_DEBOUNCE_MS);
    let key = fingerprint(e);
    if let Some(r) = l.recent.iter().find(|r| r.key == key) {
        return Err((
            "duplicate",
            format!(
                "same letter as {} sent {} ms ago to {:?}",
                r.id,
                now - r.at,
                e.to
            ),
        ));
    }
    let pair = format!("{}>{}", e.from, e.to);
    let pw = l.pairs.get(&pair).copied().unwrap_or_default();
    if !room(&pw, RATE_PAIR, now) {
        return Err((
            "throttled",
            format!(
                "{RATE_PAIR} letters from {:?} to {:?} within {RATE_WINDOW_MS} ms",
                e.from, e.to
            ),
        ));
    }
    if !room(&l.global, RATE_GLOBAL, now) {
        return Err((
            "throttled",
            format!("{RATE_GLOBAL} letters on this machine within {RATE_WINDOW_MS} ms"),
        ));
    }
    l.recent.push(Recent {
        key,
        at: now,
        id: e.id.clone(),
    });
    bump(l.pairs.entry(pair).or_default(), now);
    bump(&mut l.global, now);
    let knock = e.tier == 1 && !l.knocked.contains_key(&e.to);
    if knock {
        l.knocked.insert(e.to.clone(), now);
    }
    Ok(knock)
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/cc_bus_rules_tests.rs"]
mod tests;
