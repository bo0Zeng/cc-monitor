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

use crate::agents::QuotaWindow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 读数至少涨这么多点（0–100 那一种点）才给数：读数按点取整，涨得少时误差占比大。
pub(crate) const MIN_RISE: f64 = 3.0;
/// 起点之后至少数到这么多发才给数。
pub(crate) const MIN_SENDS: u64 = 5;
/// 记着「哪个会话此刻在哪个号上」的条数上限（超了先丢最早记的）。
const ROUTES_KEPT: usize = 512;

type Key = (String, String);

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

    /// 流里的一件事（会话标识 · `data:` 原文）：说出了用量 ⇒ 记在那个会话此刻的号名下。
    pub(crate) fn heard(&self, stream: &str, data: &str) {
        if stream.is_empty() {
            return;
        }
        let key = match self.lock().routes.get(stream) {
            Some((k, _)) => k.clone(),
            None => return,
        };
        let Some(s) = crate::agents::spent_of(&key.0).and_then(|f| f(data)) else {
            return;
        };
        let mut g = self.lock();
        let t = g.tally.entry(key).or_default();
        t.units += s.units;
        t.sends += u64::from(s.opens);
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
