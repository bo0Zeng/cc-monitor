//! 额度账：这台每个号最近一次看到的额度快照 ＋ 看到的时刻，落 `~/.cc-monitor/quota.json`。
//!
//! - 同一种事实两个来源：中转经手的回包头（上游选择的 `observe` 交到 [`record_seen`]，来源 `headers`）· 官方客户端报的用量
//!   （帧命令 `quota-probe` 交到 [`record_probe`]，来源 `usage`）。两者记进同一条账：**按窗口合并**（同名窗口新的盖旧的，
//!   别的窗口留着），每个窗口记自己几点、从哪看到的；状态 · 被拒 · 卡在哪 · 超额那几格只有回包头说得出，只随它变。
//! - 两个写者（常驻里的中转 · 一次性的 CLI）可能在两个进程里：「读盘 → 合并 → 原子替换」只在 [`write_merged`] 一处、在跨进程锁里做，
//!   谁也冲不掉谁的窗口。常驻那份内存账在用的那一刻比一次盘上的戳（不定时）：别人写过就读进来合并。
//! - 「变了」只看显示得出来的几格（各窗口取整的百分比 · 重置时刻 · 状态 · 被拒 · 卡在哪 · 超额）：变了才推一帧、才立刻落盘；
//!   没变的观测只刷新内存里的时刻，盘上那份的时刻至多落后 [`PERSIST_EVERY`] 秒。
//! - 很久没流量的号照实只有「最后一次看到是几点」，不编。
//! - 每个窗口另记「1% ≈ 多少用量」（[`Observed::per_pct`]）：只有中转那一路量得出（[`super::spend`]），量出来那一刻记下；
//!   换了窗口照留（同一个号的 1% 一样大），量出来超过 [`PER_PCT_KEPT`] 秒就不作数（套餐可能换了）。

use crate::agents::QuotaReading;
use crate::common::said::Said;
use crate::platform::fs::{Own, Ro, Stamp};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::QUOTA_LEDGER_REL);

/// 显示没变时，盘上那份「看到的时刻」至多落后这么多秒。
pub(crate) const PERSIST_EVERY: u64 = 60;

/// 一个号的一条账。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observed {
    /// 路由第 1 段（哪一家）。
    pub agent: String,
    /// 路由第 2 段（哪个号；`_` ＝ 起会话时没说是哪个号）。
    pub account: String,
    /// 最后一次看到这个号（任何一个来源）的时刻（unix 秒）。
    pub seen_at: u64,
    pub reading: QuotaReading,
    /// 各窗口最后一次几点、从哪看到的（窗口名 → …）；没列的窗口 ＝ 与 `seen_at` 同一刻、来自回包头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub windows_seen: BTreeMap<String, WindowSeen>,
    /// 各窗口用量的走势（窗口名 → …）：估「几点到」只按它（[`Observed::eta`]）。只在账上，不上 `quota-read`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trend: BTreeMap<String, Trend>,
    /// 各窗口量出来的「1% ≈ 多少用量」（窗口名 → …）：读只经 [`Observed::tokens_per_pct`]。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_pct: BTreeMap<String, PerPct>,
    /// 各窗口换窗的历史（窗口名 → …）：周剩几点 · 开窗取整往哪边只按它（[`Observed::weekly_left_at_reset`] ·
    /// [`Observed::window_grid`]）。只在账上，不上 `quota-read`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub history: BTreeMap<String, History>,
}

/// 一个窗口换窗的历史：旧窗最后读到的用量（[`ENDS_KEPT`] 次）· 新窗头一次看到的时刻（[`OPENS_KEPT`] 次）；都照重置时刻排、旧的在前。
/// 只在单发读数推一步、看到重置时刻往后换了的那一下记（[`turnover`]）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) ends: Vec<End>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) opens: Vec<Open>,
}

/// 一窗走完：它的重置时刻 · 最后读到的用量（比例）· 那次读数几点看到的。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct End {
    pub(crate) resets: u64,
    pub(crate) used: f64,
    pub(crate) at: u64,
}

/// 一窗开了：它的重置时刻 · 头一次看到它的时刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Open {
    pub(crate) resets: u64,
    pub(crate) seen: u64,
}

/// 周剩几点留最近这么多次（估周重置时剩多少用）。
const ENDS_KEPT: usize = 4;
/// 开窗时刻留最近这么多次（判取整往哪边）。
const OPENS_KEPT: usize = 8;

/// 开窗时刻取整的格（秒）：重置时刻落在这个格上，开窗时刻照它往下或往上取。
pub(crate) const GRID_STEP: u64 = 600;
/// 头一次看到比格早这么多秒以上 ⇒ 开窗是往上取的（看到时刻前后差几秒不算）。
const GRID_EARLY: i64 = 60;
/// 往下取至少要这么多次作数的开窗。
const GRID_DOWN_MIN: usize = 3;

/// ★ 历史合并的唯一判法：按重置时刻并集（[`same_reset`] 算同一窗）；同一窗走完的取看到得晚的那次、开窗的取看到得早的那次；
/// 照重置时刻排、各留最近那几次。
fn absorb(into: &mut History, add: &History) {
    for e in &add.ends {
        match into
            .ends
            .iter_mut()
            .find(|x| same_reset(Some(x.resets), Some(e.resets)))
        {
            Some(x) if x.at < e.at => *x = *e,
            Some(_) => {}
            None => into.ends.push(*e),
        }
    }
    for o in &add.opens {
        match into
            .opens
            .iter_mut()
            .find(|x| same_reset(Some(x.resets), Some(o.resets)))
        {
            Some(x) if o.seen < x.seen => *x = *o,
            Some(_) => {}
            None => into.opens.push(*o),
        }
    }
    into.ends.sort_by_key(|e| e.resets);
    into.opens.sort_by_key(|o| o.resets);
    let n = into.ends.len().saturating_sub(ENDS_KEPT);
    into.ends.drain(..n);
    let n = into.opens.len().saturating_sub(OPENS_KEPT);
    into.opens.drain(..n);
}

/// 单发读数 `new`（`at` 看到的）接在旧读数 `old`（`old_at` 最后看到的）后面：重置时刻往后换了 ⇒ 旧窗走完、新窗开了各一笔；
/// 没换 · 说不出重置时刻 ⇒ 没有。
fn turnover(
    old: &crate::agents::QuotaWindow,
    old_at: u64,
    new: &crate::agents::QuotaWindow,
    at: u64,
) -> Option<History> {
    let (r0, r1) = (old.resets_at?, new.resets_at?);
    if same_window(old, new) || r1 <= r0 {
        return None;
    }
    Some(History {
        ends: old
            .used
            .map(|used| End {
                resets: r0,
                used,
                at: old_at,
            })
            .into_iter()
            .collect(),
        opens: vec![Open {
            resets: r1,
            seen: at,
        }],
    })
}

/// 量出来的「1% ≈ 多少用量」：折合输入 token（[`crate::agents::Spent`]）与量出来的时刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerPct {
    pub tokens: u64,
    pub at: u64,
}

/// 量出来的「1% ≈ 多少」过了这么多秒就不作数（套餐可能换了；再量到就换新的）。
pub(crate) const PER_PCT_KEPT: u64 = 7 * 86_400;

/// 一个窗口用量的走势：此刻这个值头一次看到的时刻 ＋ 同一窗口里此前**变过**的那几个值（旧的在前；只留离此刻这个值
/// [`EST_FRESH`] 以内的，至少留上一个；又看到一次同一个值不算新采样：说不出涨多快）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trend {
    pub(crate) since: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) steps: Vec<Sample>,
}

/// 一个窗口至多留这么多个变过的值（读数一跳一点，30 分钟里涨得再快也够用）。
const STEPS_KEPT: usize = 64;

/// 一次采样：用量（比例）与它头一次看到的时刻。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sample {
    pub(crate) used: f64,
    pub(crate) since: u64,
}

/// 估「几点到」只看最近这么多秒里头一次看到的值（再早的涨法不作数）。
pub(crate) const EST_FRESH: u64 = 30 * 60;

/// 两个读数的重置时刻差在这么多秒以内 ⇒ 同一个窗口（来源不同时重置时刻可能差几秒）。
const SAME_WINDOW_SLACK: u64 = 300;

fn same_window(a: &crate::agents::QuotaWindow, b: &crate::agents::QuotaWindow) -> bool {
    same_reset(a.resets_at, b.resets_at)
}

/// 两个重置时刻是不是同一个窗口的（唯一判法；说不出 ⇒ 不是）。
pub(crate) fn same_reset(a: Option<u64>, b: Option<u64>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x.abs_diff(y) <= SAME_WINDOW_SLACK)
}

/// 新读数 `new`（`at` 看到的）进来之后这个窗口的走势：值没变 ⇒ 照旧；同一窗口里值变了 ⇒ 旧值记进变过的那几个，
/// 离 `at` 超过 [`EST_FRESH`] 的丢掉（至少留上一个）；窗口换了 · 头一次看到 · 说不出用量 ⇒ 从这一刻重新记。
fn step(
    old: Option<(&crate::agents::QuotaWindow, Option<Trend>, u64)>,
    new: &crate::agents::QuotaWindow,
    at: u64,
) -> Trend {
    let fresh = Trend {
        since: at,
        steps: Vec::new(),
    };
    let Some((o, t, old_at)) = old else {
        return fresh;
    };
    let since = t.as_ref().map_or(old_at, |t| t.since);
    match (o.used, new.used) {
        (Some(u0), Some(u1)) if same_window(o, new) => {
            if (u0 - u1).abs() < 1e-9 {
                return t.unwrap_or(Trend {
                    since,
                    steps: Vec::new(),
                });
            }
            let mut steps = t.map(|t| t.steps).unwrap_or_default();
            steps.push(Sample { used: u0, since });
            let stale = steps
                .iter()
                .take_while(|s| at.saturating_sub(s.since) > EST_FRESH)
                .count()
                .min(steps.len() - 1)
                .max(steps.len().saturating_sub(STEPS_KEPT));
            steps.drain(..stale);
            Trend { since: at, steps }
        }
        _ => fresh,
    }
}

/// 一个窗口的数从哪来 · 那份文件读到的三态：线上形住 `cmd-wire`（`quota-read` 带它们）。
pub use cmd_wire::quota::{ReadState, Source};

/// 一个窗口几点、从哪看到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSeen {
    pub at: u64,
    pub from: Source,
}

impl Observed {
    /// ★ 用速（E2）：这个窗口里留着的最早那个变过的值（离此刻这个值至多 [`EST_FRESH`]）到此刻这个值的斜率，点 / 分
    /// （用量 0–100 那一种点）。读数按整点走、一跳常是几点，只看最后两次会把几秒里的一跳量成每分几十点。
    /// 只有一次采样 · 不涨 · 最近 30 分没变过 ⇒ `None`（没根据就不给，推演不外推）。
    pub(crate) fn rate(&self, name: &str, now: u64) -> Option<f64> {
        let w = self.reading.windows.iter().find(|w| w.name == name)?;
        let u1 = w.used?;
        let t = self.trend.get(name)?;
        let p = t.steps.first()?;
        if now.saturating_sub(t.since) > EST_FRESH || u1 <= p.used || t.since <= p.since {
            return None;
        }
        Some((u1 - p.used) * 100.0 / ((t.since - p.since) as f64 / 60.0))
    }

    /// ★ 号的总用速（点 / 分，别的会话那一份从它扣）：量得到 ⇒ [`Self::rate`]；最近 [`EST_FRESH`] 里还看到过这个窗口、
    /// 读数却已经超过 [`EST_FRESH`] 没动 ⇒ 0（量到没人在用）；其余（只有一次采样 · 好久没看到）⇒ `None`。
    pub(crate) fn load(&self, name: &str, now: u64) -> Option<f64> {
        if let Some(r) = self.rate(name, now) {
            return Some(r);
        }
        let t = self.trend.get(name)?;
        let seen = self.window_seen(name).at;
        (now.saturating_sub(seen) <= EST_FRESH && now.saturating_sub(t.since) > EST_FRESH)
            .then_some(0.0)
    }

    /// ★ 这个窗口「1% ≈ 多少用量」（折合输入 token）：量到过、还没过 [`PER_PCT_KEPT`] ⇒ 那个数；否则 `None`（不猜）。
    pub(crate) fn tokens_per_pct(&self, name: &str, now: u64) -> Option<u64> {
        self.per_pct
            .get(name)
            .filter(|p| now.saturating_sub(p.at) <= PER_PCT_KEPT)
            .map(|p| p.tokens)
    }

    /// ★ 这个窗口前几次走完时剩几点（0–100，旧的在前）：100 − 最后读到的用量；没记到 ⇒ 空。
    pub(crate) fn weekly_left_at_reset(&self, name: &str) -> Vec<f64> {
        self.history.get(name).map_or_else(Vec::new, |h| {
            h.ends
                .iter()
                .map(|e| ((1.0 - e.used) * 10_000.0).round().max(0.0) / 100.0)
                .collect()
        })
    }

    /// ★ 这一窗（窗长 `len` 秒）开窗时刻取到 [`GRID_STEP`] 格是往上（`Some(true)`）还是往下（`Some(false)`）：
    /// 格 ＝ 重置时刻取到最近的格 − 窗长；差 ＝ 头一次看到 − 格，差一个格以上的不作数（隔了好久才看到）；
    /// 有一次早于格 [`GRID_EARLY`] 秒以上 ⇒ 往上；作数的至少 [`GRID_DOWN_MIN`] 次、都不早 ⇒ 往下；其余说不出。
    pub(crate) fn window_grid(&self, name: &str, len: u64) -> Option<bool> {
        let opens = &self.history.get(name)?.opens;
        let step = GRID_STEP as i64;
        let near: Vec<i64> = opens
            .iter()
            .map(|o| {
                let grid = (o.resets as i64 + step / 2).div_euclid(step) * step - len as i64;
                o.seen as i64 - grid
            })
            .filter(|d| d.abs() < step)
            .collect();
        if near.iter().any(|d| *d < -GRID_EARLY) {
            Some(true)
        } else {
            (near.len() >= GRID_DOWN_MIN).then_some(false)
        }
    }

    /// 一个窗口几点、从哪看到的（没列 ⇒ 这一条的时刻、回包头）。
    pub fn window_seen(&self, name: &str) -> WindowSeen {
        self.windows_seen.get(name).copied().unwrap_or(WindowSeen {
            at: self.seen_at,
            from: Source::Headers,
        })
    }
}

/// ★ 合并的唯一判法：`add` 的窗口逐个并进 `into`（同名窗口看到得晚的那份留下，一样晚 ⇒ `add` 的；「1% ≈ 多少」同样量得晚的留下）；
/// `headers` ⇒ 状态 · 被拒 · 卡在哪 · 超额那几格也换成 `add` 的（只有回包头说得出它们）。
pub(crate) fn merge(into: &mut Observed, add: &Observed, headers: bool) {
    let mut meta: BTreeMap<String, WindowSeen> = into
        .reading
        .windows
        .iter()
        .map(|w| (w.name.clone(), into.window_seen(&w.name)))
        .collect();
    for w in &add.reading.windows {
        let m = add.window_seen(&w.name);
        // 带着走势来的（盘上那份 · 内存里那份）照它的；只有一发读数的照旧值推一步。
        let trend = |old: Option<(&crate::agents::QuotaWindow, Option<Trend>, u64)>| {
            add.trend
                .get(&w.name)
                .cloned()
                .unwrap_or_else(|| step(old, w, m.at))
        };
        match into.reading.windows.iter_mut().find(|x| x.name == w.name) {
            Some(_) if meta.get(&w.name).is_some_and(|old| old.at > m.at) => {}
            Some(x) => {
                let old_at = meta.get(&w.name).map_or(into.seen_at, |o| o.at);
                let t = trend(Some((x, into.trend.get(&w.name).cloned(), old_at)));
                into.trend.insert(w.name.clone(), t);
                // 换窗只从单发读数看（带着历史来的已在上面并过）。
                if let Some(h) = (!add.trend.contains_key(&w.name))
                    .then(|| turnover(x, old_at, w, m.at))
                    .flatten()
                {
                    absorb(into.history.entry(w.name.clone()).or_default(), &h);
                }
                *x = w.clone();
                meta.insert(w.name.clone(), m);
            }
            None => {
                into.trend.insert(w.name.clone(), trend(None));
                into.reading.windows.push(w.clone());
                meta.insert(w.name.clone(), m);
            }
        }
    }
    for (k, h) in &add.history {
        absorb(into.history.entry(k.clone()).or_default(), h);
    }
    for (k, p) in &add.per_pct {
        if into.per_pct.get(k).is_none_or(|q| q.at <= p.at) {
            into.per_pct.insert(k.clone(), *p);
        }
    }
    if headers {
        let r = &add.reading;
        into.reading.status = r.status;
        into.reading.refused = r.refused;
        into.reading.limiting.clone_from(&r.limiting);
        into.reading.resets_at = r.resets_at;
        into.reading.overage.clone_from(&r.overage);
    }
    into.seen_at = into.seen_at.max(add.seen_at);
    let at = into.seen_at;
    into.windows_seen = meta
        .into_iter()
        .filter(|(_, m)| !(m.at == at && m.from == Source::Headers))
        .collect();
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Book {
    pub(crate) accounts: Vec<Observed>,
}

type Key = (String, String);

/// 显示得出来的那几格（判「变了没」用）。
fn shown(r: &QuotaReading) -> String {
    let wins: Vec<String> = r
        .windows
        .iter()
        .map(|w| {
            format!(
                "{}:{:?}:{:?}",
                w.name,
                w.used.map(|u| (u * 100.0).round() as i64),
                w.resets_at
            )
        })
        .collect();
    format!(
        "{:?}|{}|{:?}|{:?}|{}|{:?}",
        r.status,
        r.refused,
        r.limiting,
        r.resets_at,
        wins.join(","),
        r.overage
            .as_ref()
            .map(|o| (o.status, o.in_use, &o.disabled))
    )
}

struct State {
    mem: BTreeMap<Key, Observed>,
    /// 盘上那一份每个号的 `seenAt`（判要不要落盘）。
    on_disk: BTreeMap<Key, u64>,
    /// 上一次读 / 写之后盘上那份文件的戳：变了 ＝ 别人（另一个进程）写过。
    stamp: Option<Stamp>,
}

/// 额度账本体。`path = None` ⇒ 家推不出来，只记在内存。
pub struct Ledger {
    path: Option<PathBuf>,
    state: Mutex<State>,
    /// 显示变了往哪发节点（生产是进程里那一条；判据各给自己的一条）。
    nodes: super::nodes::Nodes,
    /// 每发用量的累计（生产是进程里那一本，tap 口往里记；判据各开各的）。
    spend: std::sync::Arc<super::spend::Spend>,
}

impl Ledger {
    /// 在这个落点开账：盘上已有的读进内存（重启后被拒的号仍记得几点重置）。节点发到进程里那一条。
    pub fn at(path: Option<PathBuf>) -> Self {
        Self::open(
            path,
            super::nodes::bus().clone(),
            super::spend::book().clone(),
        )
    }

    /// 在这个落点开账，节点发到给的那一条、用量记在给的那一本里（判据各给自己的，不跟并行的判据串）。
    pub(crate) fn open(
        path: Option<PathBuf>,
        nodes: super::nodes::Nodes,
        spend: std::sync::Arc<super::spend::Spend>,
    ) -> Self {
        let mut mem = BTreeMap::new();
        if let Some(Read::Present(book)) = path.as_deref().map(read_at) {
            for e in book.accounts {
                mem.insert((e.agent.clone(), e.account.clone()), e);
            }
        }
        let on_disk = mem.iter().map(|(k, e)| (k.clone(), e.seen_at)).collect();
        let stamp = path.as_deref().and_then(Ro::stamp);
        Self {
            path,
            state: Mutex::new(State {
                mem,
                on_disk,
                stamp,
            }),
            nodes,
            spend,
        }
    }

    /// 用的那一刻比一次盘上的戳：别人写过 ⇒ 读进来、按窗口并进内存（内存里中转那几格照旧 —— 回包头只有中转这一个写者）。
    fn refresh(&self, g: &mut State) {
        let Some(p) = self.path.as_deref() else {
            return;
        };
        let s = Ro::stamp(p);
        if s.is_none() || s == g.stamp {
            return;
        }
        g.stamp = s;
        let Read::Present(book) = read_at(p) else {
            return;
        };
        for e in book.accounts {
            let key = (e.agent.clone(), e.account.clone());
            g.on_disk.insert(key.clone(), e.seen_at);
            match g.mem.get_mut(&key) {
                Some(m) => merge(m, &e, false),
                None => {
                    g.mem.insert(key, e);
                }
            }
        }
    }

    /// 发节点的那一条（判的缓存订它）。
    pub(crate) fn nodes(&self) -> &super::nodes::Nodes {
        &self.nodes
    }

    /// 每发用量那一本（判要本会话量出来的 · 别的会话压在各号上的）。
    pub(crate) fn spend(&self) -> &std::sync::Arc<super::spend::Spend> {
        &self.spend
    }

    /// 发一件事实的节点（被拒 · 续期失败由换号那一路调；显示变了由记账这里自己调）。
    pub(crate) fn notify(&self, source: super::nodes::Source, sid: Option<&str>) {
        super::nodes::emit(&self.nodes, source, sid);
    }

    /// 那个号的最近一条（盘上别人写过 ⇒ 先并进来）。
    pub fn entry(&self, agent: &str, account: &str) -> Option<Observed> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh(&mut g);
        g.mem
            .get(&(agent.to_string(), account.to_string()))
            .cloned()
    }

    /// 显示变了：哪个会话的回包带来的（官方客户端报的 ⇒ 空，算外来的）。
    fn ring(&self, sid: Option<&str>) {
        self.notify(super::nodes::Source::Reading, sid);
    }

    /// 落了盘之后：戳跟上；内存那一条换成盘上合并好的那份（内存里那条没更新过才换）。
    fn landed(&self, written: Observed, at: Option<Stamp>) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if at.is_some() {
            g.stamp = at;
        }
        let key = (written.agent.clone(), written.account.clone());
        g.on_disk.insert(key.clone(), written.seen_at);
        if g.mem.get(&key).is_none_or(|m| m.seen_at <= written.seen_at) {
            g.mem.insert(key, written);
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// 落点：数据目录 `data` 底下的 `quota.json`（数据目录与 API key 凭据同一个出处；文件名只在这里拼）。
pub fn path_under(data: &Path) -> PathBuf {
    data.join(FILE_NAME)
}

/// 记一条观测（中转那一路的写口，来源 `headers`）。显示变了 ⇒ 发一件读数节点（带这一发的会话）、立刻落盘；没变 ⇒ 盘上的时刻落后够久才落。
/// 落盘失败只出声，不影响这一发（额度账不是请求的前提）。
pub fn record_seen(
    ledger: &Ledger,
    own: &Own,
    sid: Option<&str>,
    agent: &str,
    account: &str,
    reading: QuotaReading,
    now: u64,
) {
    let key = (agent.to_string(), account.to_string());
    ledger.spend.answered(sid, agent, account);
    let per_pct = reading
        .windows
        .iter()
        .filter_map(|w| {
            let tokens = ledger.spend.pace(agent, account, w)?;
            Some((w.name.clone(), PerPct { tokens, at: now }))
        })
        .collect();
    let heard = Observed {
        agent: key.0.clone(),
        account: key.1.clone(),
        seen_at: now,
        reading,
        windows_seen: BTreeMap::new(),
        trend: BTreeMap::new(),
        per_pct,
        history: BTreeMap::new(),
    };
    let (changed, persist, entry) = {
        let mut g = ledger.state.lock().unwrap_or_else(|e| e.into_inner());
        ledger.refresh(&mut g);
        let old = g.mem.get(&key).cloned();
        let entry = match &old {
            Some(o) => {
                let mut m = o.clone();
                merge(&mut m, &heard, true);
                m
            }
            None => {
                let mut h = heard.clone();
                merge_trend_fresh(&mut h);
                h
            }
        };
        let changed = old.is_none_or(|o| shown(&o.reading) != shown(&entry.reading));
        g.mem.insert(key.clone(), entry.clone());
        let stale = g
            .on_disk
            .get(&key)
            .is_none_or(|at| now.saturating_sub(*at) >= PERSIST_EVERY);
        let persist = ledger.path.is_some() && (changed || stale);
        if persist {
            g.on_disk.insert(key, now);
        }
        (changed, persist, entry)
    };
    if persist {
        match write_merged(own, &entry, true) {
            Ok((written, at)) => ledger.landed(written, at),
            Err(e) => tracing::warn!("[quota] {e}: {}", e.raw.as_deref().unwrap_or("")),
        }
    }
    if changed {
        ledger.ring(sid);
    }
}

/// 记一次官方客户端报的用量（`quota-probe` 那一路的写口，来源 `usage`）：只并窗口，不碰回包头那几格；一定落盘（落不了 ⇒ `Err`）。
/// 交回合并好的那一条。
pub fn record_probe(
    ledger: &Ledger,
    own: &Own,
    agent: &str,
    account: &str,
    windows: Vec<crate::agents::QuotaWindow>,
    now: u64,
) -> Result<Observed, Said> {
    if ledger.path.is_none() {
        return Err(Said::from(copy_text("beQuotaLedger.read.noHome", &[])));
    }
    let windows_seen = windows
        .iter()
        .map(|w| {
            (
                w.name.clone(),
                WindowSeen {
                    at: now,
                    from: Source::Usage,
                },
            )
        })
        .collect();
    let probed = Observed {
        agent: agent.to_string(),
        account: account.to_string(),
        seen_at: now,
        reading: QuotaReading {
            status: None,
            refused: false,
            limiting: None,
            resets_at: None,
            windows,
            overage: None,
        },
        windows_seen,
        trend: BTreeMap::new(),
        per_pct: BTreeMap::new(),
        history: BTreeMap::new(),
    };
    let before = ledger.entry(agent, account);
    let (written, at) = write_merged(own, &probed, false)?;
    let changed = before.is_none_or(|b| shown(&b.reading) != shown(&written.reading));
    ledger.landed(written.clone(), at);
    if changed {
        ledger.ring(None);
    }
    Ok(written)
}

/// 读一次盘。三态：没有（还没看到过）/ 读得懂 / 读不懂（不覆盖）。
pub(crate) type Read = crate::common::own_state::Read<Book>;

/// 那一份：状态（读不懂不覆盖；版本照声明表，读写口管）。
const DOC: crate::common::own_state::Doc<Book> =
    crate::common::own_state::Doc::of(&relay_route_core::QUOTA_FILE);

pub(crate) fn read_at(path: &Path) -> Read {
    DOC.read(path)
}

/// ★ 盘上那份的唯一写法：在跨进程锁里读盘 → 那个号那一条按窗口并进 `add`（[`merge`]；`headers` 同它）→ `O_EXCL` 临时文件 →
/// 写满 → 原子挪过去 → 记下新的戳。读不懂的那份不覆盖。交回落盘的那一条与戳。
fn write_merged(
    own: &Own,
    add: &Observed,
    headers: bool,
) -> Result<(Observed, Option<Stamp>), Said> {
    let file = own.path(FILE_NAME)?;
    let dir = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beQuotaLedger.read.noHome", &[])))?;
    let path = file.as_path();
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &path.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let held = own.lock(&dir)?;
    let mut book = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        // 读不懂的那份不覆盖：那一句说清是哪份、没覆盖，读不懂的原因（已带原因词）与原话照它的。
        Read::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beLineage.write.unreadable", &[("said", said)])))
        }
    };
    let entry = match book
        .accounts
        .iter()
        .position(|e| e.agent == add.agent && e.account == add.account)
    {
        Some(i) => {
            let mut e = book.accounts.remove(i);
            merge(&mut e, add, headers);
            e
        }
        None => {
            let mut a = add.clone();
            merge_trend_fresh(&mut a);
            a
        }
    };
    book.accounts.push(entry.clone());
    book.accounts
        .sort_by(|a, b| (&a.agent, &a.account).cmp(&(&b.agent, &b.account)));
    DOC.write(own, &dir, &held, &book)?;
    Ok((entry, Ro::stamp(path)))
}

/// 头一次看到的那一条：每个窗口从这一刻起记走势（已带着的照旧）。
fn merge_trend_fresh(o: &mut Observed) {
    for w in &o.reading.windows {
        let at = o.window_seen(&w.name).at;
        o.trend.entry(w.name.clone()).or_insert(Trend {
            since: at,
            steps: Vec::new(),
        });
    }
}

/// 帧命令 `quota-read` 的底子：现读这台的额度账（不读内存 —— 一次性 CLI 那一形里内存是空的）；显示态由帧面宿主补上。
pub fn answer_of(path: Option<&Path>, now: u64) -> LedgerRead {
    let (state, why, accounts) = match path.map(read_at) {
        None => (
            ReadState::Unreadable,
            Some(Said::from(copy_text("beQuotaLedger.read.noHome", &[]))),
            vec![],
        ),
        Some(Read::Absent) => (ReadState::Absent, None, vec![]),
        Some(Read::Present(b)) => (ReadState::Present, None, b.accounts),
        Some(Read::Unreadable(e)) => (ReadState::Unreadable, Some(e), vec![]),
    };
    let (reason, detail) = crate::wire::detail::unreadable("quota-read", why.as_ref());
    LedgerRead {
        state,
        reason,
        detail,
        path: path.map(|p| p.display().to_string()),
        now,
        // 走势 · 历史只在账上（估「几点到」· 周剩几点 · 开窗取整用），不上线。
        accounts: accounts
            .into_iter()
            .map(|mut o| {
                o.trend.clear();
                o.history.clear();
                o
            })
            .collect(),
    }
}

/// 额度账那份文件此刻读到的样子（`quota-read` 的底：文件三态 ＋ 每个号一条）。
#[derive(Debug, Serialize)]
pub struct LedgerRead {
    pub state: ReadState,
    pub reason: Option<String>,
    pub detail: Option<String>,
    pub path: Option<String>,
    pub now: u64,
    pub accounts: Vec<Observed>,
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/ledger_tests.rs"]
mod tests;
