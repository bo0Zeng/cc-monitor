//! 每号「1% 约多少用量」的底：这台中转经手的每发用量按号累计，配上那个号读数涨了多少，量出这个号这个窗口 1% ≈ 多少（折合输入 token）。
//!
//! - **出处**：用量读自中转抄出来的应答流（tap 口，读法住那一家的流面 [`crate::agents::StreamFace::spent`]）；读数读自回包头（额度账记的那一份）。
//!   流事件只带会话标识、不带号 ⇒ 回包头到的那一刻记下「这个会话此刻答它的是哪个号」，流里的用量照它记在那个号名下。
//! - **口径**：从一个窗口里读数头一次变的那一刻起数（落在读数的台阶上，取整误差两头都小），到之后读数又变的那一刻：
//!   这段时间里经手的用量 ÷ 读数涨的点数。涨不到 [`MIN_RISE`] 点 · 数到的发数不到 [`MIN_SENDS`] ⇒ 不给。
//! - **从头数**：换了窗口（重置时刻不同）· 读数往回走 · 中间有一发这台数不到（回包没带会话标识）⇒ 起点作废，重新等台阶。
//!   进程起来时也是从头数（累计只在这个进程里）。
//! - **偏差照实说**：同一个号别的机器、没走中转的会话也在用时，读数涨得比这台数到的多 ⇒ 量出来偏小；这台看不出来。
//!   换号那一刻正在说的那一发，收尾那一件可能记到新号名下。
//! - **按会话记**（[`Spend::me`]）：同一份流按会话再记一本 —— 每件用量（记在哪个号名下）· 每发的上下文 · 写缓存那一档 · 每号最后一发；
//!   交给判的「本会话的用速与起伏 · 上下文 · 缓存」都从这一本算，「别的会话压在这个号上的」也从它扣（[`Spend::here`] · [`Spend::others`]）。

use super::rules::decide::{Cached, Me};
use crate::agents::{QuotaWindow, Tier};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// 读数至少涨这么多点（0–100 那一种点）才给数：读数按点取整，涨得少时误差占比大。
pub(crate) const MIN_RISE: f64 = 3.0;
/// 起点之后至少数到这么多发才给数。
pub(crate) const MIN_SENDS: u64 = 5;
/// 记着「哪个会话此刻在哪个号上」的条数上限（超了先丢最早记的）。
const ROUTES_KEPT: usize = 512;

/// 本会话的用速看最近这么多秒（与额度账量号的用速同一段）。
const PACE_SPAN: u64 = super::ledger::EST_FRESH;
/// 会话在这个进程里至少记了这么多秒才给用速（刚开的会话量不准）。
const PACE_FIRST: u64 = 10 * 60;
/// 起伏按这么长一段一段比（σ/μ），看最近 [`SPREAD_BINS`] 段，记满的段不到 [`SPREAD_MIN`] 段不给。
const SPREAD_BIN: u64 = 10 * 60;
const SPREAD_BINS: u64 = 6;
const SPREAD_MIN: usize = 3;
/// 记着的会话条数上限（超了先丢最久没动的）。
const RUNS_KEPT: usize = 256;

type Key = (String, String);

/// 一个会话这一本：哪一家 · 头一次记到 · 每件用量（时刻 · 记在哪个号名下 · 折合输入 token；只留最近一小时）·
/// 最近的上下文（时刻 · token；变小 ＝ 压缩过，从头记）· 写缓存那一档（最近一次说得出的）· 每号最后一发（时刻 · 上下文 · 档）。
#[derive(Debug, Clone, Default)]
struct Run {
    agent: String,
    first: u64,
    last: u64,
    spent: VecDeque<(u64, String, f64)>,
    prompts: VecDeque<(u64, u64)>,
    tier: Option<Tier>,
    tail: HashMap<String, (u64, u64, Option<Tier>)>,
}

impl Run {
    fn add(&mut self, acct: &str, units: f64, now: u64) {
        match self.spent.back_mut() {
            Some((t, a, u)) if *t == now && a == acct => *u += units,
            _ => self.spent.push_back((now, acct.to_string(), units)),
        }
        let keep = (SPREAD_BIN * SPREAD_BINS).max(PACE_SPAN);
        while self.spent.front().is_some_and(|(t, _, _)| t + keep < now) {
            self.spent.pop_front();
        }
    }

    /// `[from, to)` 里用了多少（只算记在 `acct` 名下的 ⇒ 给 `acct`）。
    fn units(&self, from: u64, to: u64, acct: Option<&str>) -> f64 {
        self.spent
            .iter()
            .filter(|(t, a, _)| *t >= from && *t < to && acct.is_none_or(|x| x == a))
            .map(|(_, _, u)| u)
            .sum()
    }

    /// 本会话的用速（折合输入 token / 分）：记了不到 [`PACE_FIRST`] · 最近 [`PACE_SPAN`] 里一发都没有 · 说不出上下文 ⇒ 空。
    fn pace(&self, now: u64) -> Option<f64> {
        if now < self.first + PACE_FIRST || self.prompts.is_empty() {
            return None;
        }
        let from = now.saturating_sub(PACE_SPAN);
        let u = self.units(from, now + 1, None);
        let span = PACE_SPAN.min(now - self.first) as f64 / 60.0;
        (u > 0.0).then(|| u / span)
    }

    /// 起伏（σ/μ）：最近几段里、会话记到之后整段都在的那几段。
    fn spread(&self, now: u64) -> Option<f64> {
        let bins: Vec<f64> = (0..SPREAD_BINS)
            .map(|k| now.saturating_sub(SPREAD_BIN * (k + 1)))
            .filter(|from| *from >= self.first && *from > 0)
            .map(|from| self.units(from, from + SPREAD_BIN, None))
            .collect();
        if bins.len() < SPREAD_MIN {
            return None;
        }
        let n = bins.len() as f64;
        let mu = bins.iter().sum::<f64>() / n;
        let var = bins.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / n;
        (mu > 0.0).then(|| var.sqrt() / mu)
    }

    /// 上下文每分钟长多少：最近 [`PACE_SPAN`] 里头一份与末一份之差 ÷ 时间（不到一分钟 ⇒ 空）。
    fn grow(&self, now: u64) -> Option<f64> {
        let from = now.saturating_sub(PACE_SPAN);
        let mut it = self.prompts.iter().filter(|(t, _)| *t >= from);
        let (t0, k0) = *it.next()?;
        let (t1, k1) = *it.last()?;
        (t1 >= t0 + 60).then(|| (k1.saturating_sub(k0)) as f64 / ((t1 - t0) as f64 / 60.0))
    }
}

/// 一个号到此刻为止在这个进程里数到的：折合输入 token · 几发 · 几发数不到。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Tally {
    units: f64,
    sends: u64,
    blind: u64,
}

/// 一个窗口的起点。`edge` ＝ 已落在读数的台阶上（读数变过一次之后）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Base {
    used: f64,
    resets_at: Option<u64>,
    at: Tally,
    edge: bool,
}

#[derive(Default)]
struct Inner {
    routes: HashMap<String, (Key, u64)>,
    order: u64,
    tally: HashMap<Key, Tally>,
    base: HashMap<(Key, String), Base>,
    runs: HashMap<String, Run>,
}

/// 一本累计（生产是进程里那一本 [`book`]；判据各开各的）。
#[derive(Default)]
pub(crate) struct Spend {
    g: Mutex<Inner>,
}

impl Spend {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.g.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 回包头到了：这一发由 `(agent, account)` 答。带会话标识 ⇒ 记下这个会话此刻在哪个号上；不带 ⇒ 这一发的用量数不到，记一笔。
    pub(crate) fn answered(&self, sid: Option<&str>, agent: &str, account: &str) {
        let key = (agent.to_string(), account.to_string());
        let mut g = self.lock();
        let Some(sid) = sid.filter(|s| !s.is_empty()) else {
            g.tally.entry(key).or_default().blind += 1;
            return;
        };
        g.order += 1;
        let n = g.order;
        g.routes.insert(sid.to_string(), (key, n));
        if g.routes.len() > ROUTES_KEPT {
            if let Some(old) = g
                .routes
                .iter()
                .min_by_key(|(_, (_, n))| *n)
                .map(|(k, _)| k.clone())
            {
                g.routes.remove(&old);
            }
        }
    }

    /// 流里的一件事（会话标识 · `data:` 原文 · 此刻）：说出了用量 ⇒ 记在那个会话此刻的号名下，并记进这个会话那一本。
    pub(crate) fn heard(&self, stream: &str, data: &str, now: u64) {
        if stream.is_empty() {
            return;
        }
        let key = match self.lock().routes.get(stream) {
            Some((k, _)) => k.clone(),
            None => return,
        };
        let Some(s) =
            crate::agents::spent_of(&crate::agents::kind_of_route(&key.0)).and_then(|f| f(data))
        else {
            return;
        };
        let mut g = self.lock();
        let t = g.tally.entry(key.clone()).or_default();
        t.units += s.units;
        t.sends += u64::from(s.prompt.is_some());
        if !g.runs.contains_key(stream) && g.runs.len() >= RUNS_KEPT {
            if let Some(old) = g
                .runs
                .iter()
                .min_by_key(|(_, r)| r.last)
                .map(|(k, _)| k.clone())
            {
                g.runs.remove(&old);
            }
        }
        let r = g.runs.entry(stream.to_string()).or_insert_with(|| Run {
            agent: key.0.clone(),
            first: now,
            ..Run::default()
        });
        r.last = now;
        r.add(&key.1, s.units, now);
        if let Some(p) = s.prompt {
            if r.prompts.back().is_some_and(|(_, k)| p.tokens < *k) {
                r.prompts.clear();
            }
            r.prompts.push_back((now, p.tokens));
            while r.prompts.front().is_some_and(|(t, _)| t + PACE_SPAN < now) {
                r.prompts.pop_front();
            }
            r.tier = p.tier.or(r.tier);
            r.tail.insert(key.1.clone(), (now, p.tokens, r.tier));
        }
    }

    /// ★ 本会话此刻量出来的（交给判的 `session`）：量不到的空着，不编数。
    pub(crate) fn me(&self, sid: &str, now: u64) -> Me {
        let g = self.lock();
        let Some(r) = g.runs.get(sid) else {
            return Me::default();
        };
        let mut cached: Vec<Cached> = r
            .tail
            .iter()
            .filter_map(|(a, (at, tokens, tier))| {
                let until = at + tier.as_ref()?.ttl;
                (until > now).then(|| Cached {
                    account: a.clone(),
                    until,
                    tokens: *tokens,
                })
            })
            .collect();
        cached.sort_by(|a, b| a.account.cmp(&b.account));
        Me {
            pace: r.pace(now),
            spread: r.spread(now),
            context: r.prompts.back().map(|(_, k)| *k),
            write_extra: r.tier.map(|t| t.extra),
            ttl: r.tier.map(|t| t.ttl),
            grow: r.grow(now),
            cached,
        }
    }

    /// 这台这一家所有会话最近 [`PACE_SPAN`] 里记在 `account` 名下的用量（折合输入 token / 分）。
    pub(crate) fn here(&self, agent: &str, account: &str, now: u64) -> f64 {
        let from = now.saturating_sub(PACE_SPAN);
        self.lock()
            .runs
            .values()
            .filter(|r| r.agent == agent)
            .map(|r| r.units(from, now + 1, Some(account)))
            .sum::<f64>()
            / (PACE_SPAN as f64 / 60.0)
    }

    /// 这台这一家除 `sid` 以外量得出用速的会话：会话标识 · 它此刻答它的号 · 用速（折合输入 token / 分）。
    pub(crate) fn others(&self, agent: &str, sid: &str, now: u64) -> Vec<(String, String, f64)> {
        let g = self.lock();
        let mut out: Vec<(String, String, f64)> = g
            .runs
            .iter()
            .filter(|(k, r)| *k != sid && r.agent == agent)
            .filter_map(|(k, r)| {
                let at = &g.routes.get(k)?.0 .1;
                Some((k.clone(), at.clone(), r.pace(now)?))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// 这个号这个窗口刚看到的读数（回包头那一份）⇒ 量得出就交「1% ≈ 多少」（折合输入 token，取整）；量不到 ⇒ `None`。
    pub(crate) fn pace(&self, agent: &str, account: &str, w: &QuotaWindow) -> Option<u64> {
        let used = w.used? * 100.0;
        let key = (agent.to_string(), account.to_string());
        let mut g = self.lock();
        let now = g.tally.get(&key).copied().unwrap_or_default();
        let fresh = |edge| Base {
            used,
            resets_at: w.resets_at,
            at: now,
            edge,
        };
        let slot = (key, w.name.clone());
        let Some(b) = g.base.get(&slot).copied() else {
            g.base.insert(slot, fresh(false));
            return None;
        };
        if !super::ledger::same_reset(b.resets_at, w.resets_at)
            || used < b.used - 1e-9
            || now.blind != b.at.blind
        {
            g.base.insert(slot, fresh(false));
            return None;
        }
        if (used - b.used).abs() < 1e-9 {
            return None;
        }
        if !b.edge {
            g.base.insert(slot, fresh(true));
            return None;
        }
        let rise = used - b.used;
        let sends = now.sends - b.at.sends;
        (rise >= MIN_RISE && sends >= MIN_SENDS)
            .then(|| ((now.units - b.at.units) / rise).round() as u64)
    }
}

/// 进程里那一本（中转的 tap 口与额度账同用它）。
pub(crate) fn book() -> &'static Arc<Spend> {
    static BOOK: std::sync::OnceLock<Arc<Spend>> = std::sync::OnceLock::new();
    BOOK.get_or_init(Default::default)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/spend_tests.rs"]
mod tests;
