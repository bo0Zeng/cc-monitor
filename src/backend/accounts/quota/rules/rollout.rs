//! ★ 推演的内核（原生，只此一处）：一份份额度 · 守着的人 · 池子最先作废先用 · 换号重写从额度里扣，推到视野末尾，
//! 交几份原始量（卡住的 · 一路换号重写的 · 每个号用掉的周上限卡着的额度 · 推完各份还剩多少），不收系数、不合成一个代价：
//! 怎么加权由策略定（积木库里的 `lib::price` 是一种定法）。
//! 积木库里挂两个入口：`lib::project_raw`（份数由策略按列拼好，`lib::project` 照它加起来）与 `lib::rollout(ctx, plan)`
//! （份数照 ctx 的事实搭：空着的 5 小时窗用了才开、取到 10 分钟格，之后一份接一份；周窗在固定时刻重置、两窗同扣；
//! 本会话用速走两态路径，别的会话的用速上下各一档，各走法共用同一组路径）。
//! 另有时间轴推演（`simulate`）与它共用的「号的动态」：号每分钟涨几点（[`rate_of`]）。

use super::Acct;
use rhai::{Array, Dynamic, EvalAltResult, Map};
use std::collections::BTreeMap;

/// 推演里号 `x` 第 `w` 窗每分钟涨几点。`pace` ＝ 本会话的用速（折合输入 token / 分），按这一窗「1% ≈ 多少」折成点：
/// - 量得到别的会话压在它上面的（`others`，只有 5 小时窗）⇒ 它，本会话在它上面再加本会话那一份；
/// - 量不到 ⇒ 号自己在额度账里量出来的用速（本会话在它上面时量的已含本会话，不再加）；在用的号这也量不到 ⇒ 只算本会话那一份；
/// - 都量不到 ⇒ 空（不编数）。
pub(crate) fn rate_of(x: &Acct, w: usize, in_use: bool, pace: Option<f64>) -> Option<f64> {
    let mine = pace
        .zip(x.per_pct[w].filter(|t| *t > 0))
        .map(|(p, t)| p / t as f64);
    let others = if w == 0 { x.others } else { None };
    match (others, in_use, mine) {
        (Some(o), true, Some(m)) => Some(o + m),
        (Some(o), false, _) => Some(o),
        (_, true, Some(m)) if x.rate[w].is_none() => Some(m),
        _ => x.rate[w],
    }
}

/// 5 小时窗 · 一周（秒）。
const WIN5: i64 = 18_000;
const WEEK: i64 = 7 * 86_400;
/// 本会话用速路径的一步（秒）：同会话用量分段的一格（`session.bins` 每格 10 分钟）。
const STEP: i64 = 600;
/// 开窗时刻取到的格（秒）。
const GRID: i64 = crate::accounts::quota::ledger::GRID_STEP as i64;
/// 还没开窗的那份的作废时刻：用了才开，开了才知道。
pub(crate) const SHUT: i64 = i64::MAX;
/// 一遍推演里至多几份（接上来的份到这里就不再接）：排队键按 [`SLOT`] 编第几份。
const MAX_PIECES: usize = 1000;

/// 一份额度：数量（折合输入 token）· 几点起能用 · 几点作废（[`SHUT`] ＝ 还没开窗）· 哪个号 · 周上限卡着 · 延迟切入的号 ·
/// 接在哪个号的 [`Chain`] 上（空 ⇒ 这一份用完或作废就没了，`lib::project_raw` 拼的都是这样）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub(crate) amt: f64,
    pub(crate) start: i64,
    pub(crate) exp: i64,
    pub(crate) who: String,
    pub(crate) weekly: bool,
    pub(crate) late: bool,
    pub(crate) chain: Option<usize>,
}

impl Piece {
    /// 一份还没开窗的：`start` 起能用，头一次用的那一刻开窗，数量照那一刻的分段上限定。
    pub(crate) fn shut(who: &str, start: i64, chain: usize, late: bool) -> Self {
        Self {
            amt: f64::INFINITY,
            start,
            exp: SHUT,
            who: who.into(),
            weekly: false,
            late,
            chain: Some(chain),
        }
    }

    fn is_shut(&self) -> bool {
        self.exp == SHUT
    }
}

/// 分段上限：一天里的分钟 `[from, to)` ⇒ 百分点，照先后取头一段；落空 ⇒ `dflt`。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Caps {
    pub(crate) segs: Vec<(i64, i64, f64)>,
    pub(crate) dflt: f64,
}

impl Caps {
    pub(crate) fn at(&self, clock: i64) -> f64 {
        self.segs
            .iter()
            .find(|(f, t, _)| *f <= clock && clock < *t)
            .map_or(self.dflt, |s| s.2)
    }

    /// 一天里平均的上限。
    fn mean(&self) -> f64 {
        (0..1440).map(|m| self.at(m)).sum::<f64>() / 1440.0
    }
}

/// 一个号的周窗：剩多少（折合输入 token）· 下一次重置（固定时刻）· 重置后满额多少。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Week {
    pub(crate) left: f64,
    pub(crate) until: i64,
    pub(crate) full: f64,
}

/// 一个号的份怎么接上：每份 5 小时窗作废以后下一份就能用，用了才开窗；1% ≈ 多少 · 5 小时窗的分段上限 ·
/// 推演起点那一刻策略时区的钟点（一天里第几分钟）· 开窗取整往上（否则往下）· 周窗（没有周上限 ⇒ 空）。
/// 每一份用的同时也从周窗里扣（两窗同扣）；周窗扣光了，这个号到周重置之前哪一份都用不了。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chain {
    pub(crate) k5: f64,
    pub(crate) cap5: Caps,
    pub(crate) clock: i64,
    pub(crate) up: bool,
    pub(crate) week: Option<Week>,
}

/// 一个守着号的别的会话：守在哪个号 · 那份在第几格（说不出 ⇒ 空：一开始就并进池子）· 用速（token / 分，未乘档位倍数）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Guard {
    pub(crate) acct: String,
    pub(crate) at: Option<usize>,
    pub(crate) rate: f64,
}

/// 一种走法：别的会话用速的倍数 · 本会话要守的那份在第几格 · 本会话用速 · 别的会话也守在这个号上的用速 · 本会话守的号 ·
/// 现在换过去当场重写的量 · 整段重写的量 · 每分钟多出的重写量 · 本会话在各号上的缓存到几点 · 缓存留几秒 · 此刻 · 视野末尾 ·
/// 本会话每一步（[`STEP`] 秒）的用速（空 ⇒ 一直是 `rho`；排完了照最后一步）· 此刻要开窗的那几份 · 停发（不守哪一份，
/// 等到不是延迟切入的哪一份能用才并进池子，之前要的都算卡住）· 一起守的别的会话（`share`）在那份用完或作废时照它们的用速
/// 并进池子（`rejoin`；不并 ⇒ 它们的用量从那一刻起不算，`lib::project` 那一版的口径）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Walk {
    pub(crate) m: f64,
    pub(crate) at: Option<usize>,
    pub(crate) rho: f64,
    pub(crate) share: Option<f64>,
    pub(crate) mine: String,
    pub(crate) lump: f64,
    pub(crate) kfull: f64,
    pub(crate) grow: f64,
    pub(crate) cache: BTreeMap<String, i64>,
    pub(crate) ttl: i64,
    pub(crate) now: i64,
    pub(crate) end: i64,
    pub(crate) steps: Vec<f64>,
    pub(crate) warm: Vec<usize>,
    pub(crate) hold: bool,
    pub(crate) rejoin: bool,
}

impl Walk {
    /// 本会话在 `t` 那一刻的用速。
    fn rate_at(&self, t: i64) -> f64 {
        match self.steps.len() {
            0 => self.rho,
            n => self.steps[(((t - self.now).max(0) / STEP) as usize).min(n - 1)],
        }
    }

    /// 本会话用速的最高一步。
    fn peak(&self) -> f64 {
        self.steps
            .iter()
            .copied()
            .fold(if self.steps.is_empty() { self.rho } else { 0.0 }, f64::max)
    }

    /// 本会话 `[a, b)` 里要的量。
    fn demand(&self, a: i64, b: i64) -> f64 {
        if self.steps.is_empty() {
            return self.rho * (b - a).max(0) as f64 / 60.0;
        }
        let mut t = a;
        let mut sum = 0.0;
        while t < b {
            let j = (t - self.now).max(0) / STEP;
            let seg_end = if j as usize + 1 >= self.steps.len() {
                b
            } else {
                (self.now + (j + 1) * STEP).min(b)
            };
            sum += self.rate_at(t) * (seg_end - t) as f64 / 60.0;
            t = seg_end;
        }
        sum
    }

    /// 本会话（连同 `share`）头一次要用的那一刻。
    fn first_use(&self, share: f64) -> i64 {
        self.steps
            .iter()
            .position(|r| r + share > 0.0)
            .map_or(self.now, |j| self.now + j as i64 * STEP)
    }

    /// 本会话（连同 `share`）守着一份 `need` 用到用完或 `exp` 作废：交用了几秒 · 本会话自己用掉多少。
    fn guard(&self, need: f64, share: f64, exp: i64) -> (f64, f64) {
        if self.steps.is_empty() {
            let dt = hold_out(need, self.rho + share, exp, self.now);
            return (dt, self.rho * dt / 60.0);
        }
        let lim_all = (exp - self.now) as f64;
        let n = self.steps.len();
        let (mut left, mut t, mut mine) = (need, 0.0_f64, 0.0_f64);
        for j in 0..n {
            let seg = if j + 1 == n {
                f64::INFINITY
            } else {
                ((j as i64 + 1) * STEP) as f64
            };
            let lim = seg.min(lim_all);
            if lim <= t {
                break;
            }
            let r = self.steps[j] + share;
            if r > 0.0 {
                let dur = left / r * 60.0;
                if t + dur <= lim {
                    return (t + dur, mine + self.steps[j] * dur / 60.0);
                }
                left -= r * (lim - t) / 60.0;
            }
            mine += self.steps[j] * (lim - t) / 60.0;
            t = lim;
        }
        (t, mine)
    }
}

/// 推完的几份原始量（折合输入 token）。`left` ＝ 推完还剩的每一份（开了窗的）：号 · 剩多少 · 几点作废，照作废先后排。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Raw {
    pub(crate) stuck: f64,
    pub(crate) rewrite: f64,
    pub(crate) weekly: BTreeMap<String, f64>,
    pub(crate) left: Vec<(String, f64, i64)>,
}

impl Raw {
    fn spend_weekly(&mut self, who: &str, x: f64) {
        *self.weekly.entry(who.to_string()).or_insert(0.0) += x;
    }
}

/// 事件与排队的编码：时刻 × 1024 ＋ 第几条（同一时刻照先后）。
const SLOT: i64 = 1024;
/// 延迟切入的号排在最后：排队键上加这么多。
const LATE: i64 = 4_000_000_000_000_000;
/// 没开窗的那几份排在开着的后面（照几点起能用）。
const DORM: i64 = 2_000_000_000_000_000;

/// 进池子的那几条：时刻编码 · 用速（换步那几条是本会话用速的增量）· 带进来要先付的重写 · 是不是本会话 · 是不是换步。
#[derive(Default)]
struct Joins {
    at: Vec<i64>,
    rho: Vec<f64>,
    lump: Vec<f64>,
    me: Vec<bool>,
    step: Vec<bool>,
}

impl Joins {
    fn push(&mut self, t: i64, rho: f64, lump: f64, me: bool) {
        self.add(t, rho, lump, me, false);
    }

    fn add(&mut self, t: i64, rho: f64, lump: f64, me: bool, step: bool) {
        if self.rho.len() as i64 >= SLOT {
            return;
        }
        self.at.push(t * SLOT + self.rho.len() as i64);
        self.rho.push(rho);
        self.lump.push(lump);
        self.me.push(me);
        self.step.push(step);
    }
}

/// 守着自己那份的人从此刻用到用完或作废：交用了几秒。
fn hold_out(amt: f64, rate: f64, exp: i64, now: i64) -> f64 {
    let dt = amt / rate * 60.0;
    if ((exp - now) as f64) < dt {
        (exp - now) as f64
    } else {
        dt
    }
}

/// 排队键：延迟切入的最后 · 没开窗的在开着的后面（照几点起能用）· 开着的照作废先后。
fn key(p: &Piece, i: usize) -> i64 {
    (if p.late { LATE } else { 0 })
        + if p.is_shut() {
            DORM + p.start * SLOT
        } else {
            p.exp * SLOT
        }
        + i as i64
}

/// 一遍推演里在变的：每份还剩多少 · 每个号的周窗剩多少、几点重置 · 推出来的原始量。
struct World<'a> {
    pieces: Vec<Piece>,
    amt: Vec<f64>,
    chains: &'a [Chain],
    week: Vec<Option<(f64, i64)>>,
    first: Vec<i64>,
    now: i64,
    end: i64,
    out: Raw,
}

impl World<'_> {
    /// 过了周重置 ⇒ 周窗满额重来。
    fn roll(&mut self, c: usize, tau: i64) {
        if let (Some((left, until)), Some(w)) = (self.week[c].as_mut(), &self.chains[c].week) {
            while tau >= *until {
                *left = w.full;
                *until += WEEK;
            }
        }
    }

    /// 这一份此刻用得上的：周窗卡着 ⇒ 两样取小。
    fn eff(&mut self, k: usize, tau: i64) -> f64 {
        let a = self.amt[k];
        match self.pieces[k].chain {
            Some(c) => {
                self.roll(c, tau);
                self.week[c].map_or(a, |(l, _)| a.min(l.max(0.0)))
            }
            None => a,
        }
    }

    /// 此刻能用：用得上的还有，没开窗的还要此刻的上限不是 0。
    fn usable(&mut self, k: usize, tau: i64) -> bool {
        if self.eff(k, tau) <= 0.0 {
            return false;
        }
        match (self.pieces[k].is_shut(), self.pieces[k].chain) {
            (true, Some(c)) => {
                let ch = &self.chains[c];
                ch.cap5.at(clock_at(ch.clock, self.now, tau)) > 0.0
            }
            _ => true,
        }
    }

    /// 从这一份用掉 `x`（数量由调用方扣）：周窗跟着扣；周上限卡着的份 · 周重置之前用的周窗 ⇒ 记进 `weekly`。
    fn spend(&mut self, k: usize, x: f64, tau: i64) {
        let mut count = self.pieces[k].weekly;
        if let Some(c) = self.pieces[k].chain {
            self.roll(c, tau);
            if let Some((left, _)) = self.week[c].as_mut() {
                *left -= x;
                count |= tau < self.first[c];
            }
        }
        if count {
            let who = self.pieces[k].who.clone();
            self.out.spend_weekly(&who, x);
        }
    }

    /// 只从周窗里扣（别的会话跟本会话守同一份时他那部分）。
    fn drain(&mut self, k: usize, x: f64, tau: i64) {
        if let Some(c) = self.pieces[k].chain {
            self.roll(c, tau);
            if let Some((left, _)) = self.week[c].as_mut() {
                *left -= x;
            }
        }
    }

    /// 在 `t` 给没开窗的那份开窗：作废 ＝ `t` 取到格 ＋ 5 小时，数量照那一刻的上限；下一份接上。此刻上限是 0 ⇒ 不开。
    fn open(&mut self, k: usize, t: i64, pend: &mut Vec<i64>) -> bool {
        let Some(c) = self.pieces[k].chain.filter(|_| self.pieces[k].is_shut()) else {
            return false;
        };
        let ch = &self.chains[c];
        let cap = ch.cap5.at(clock_at(ch.clock, self.now, t));
        if cap <= 0.0 {
            return false;
        }
        let g = if ch.up {
            (t + GRID - 1).div_euclid(GRID) * GRID
        } else {
            t.div_euclid(GRID) * GRID
        };
        self.pieces[k].exp = g + WIN5;
        self.amt[k] = cap * ch.k5;
        self.renew(k, pend);
        true
    }

    /// 开着的一份作废以后，同一个号的下一份就能用（用了才开窗）。
    fn renew(&mut self, k: usize, pend: &mut Vec<i64>) {
        let p = &self.pieces[k];
        let Some(c) = p.chain else { return };
        if p.is_shut() || p.exp >= self.end || self.pieces.len() >= MAX_PIECES {
            return;
        }
        let next = Piece::shut(&p.who, p.exp, c, p.late);
        let i = self.pieces.len();
        pend.push(next.start * SLOT + i as i64);
        self.pieces.push(next);
        self.amt.push(f64::INFINITY);
    }
}

/// 起点钟点 `clock`（一天里第几分钟，对应 `now`）推到 `t`。
fn clock_at(clock: i64, now: i64, t: i64) -> i64 {
    (clock + (t - now).div_euclid(60)).rem_euclid(1440)
}

/// 推一遍，份数由调用方按列拼好（`lib::project_raw`）。
pub(crate) fn rollout(pieces: &[Piece], others: &[Guard], w: &Walk) -> Raw {
    run(pieces, &[], others, w)
}

/// ★ 推一遍：别的会话各守着自己那份，用完或作废的那一刻并进池子；本会话守着 `w.at` 那份，用完再并进；池子照最先作废的那份用
/// （没开窗的排在开着的后面、头一次用时开窗；延迟切入的最后、还没开始的到点才进）。本会话并进池子以后跟着池子走：池子每换一个号
/// 本会话就换一次，缓存期内用过的号只补写新长的那段，否则整段重写；重写的量也从那个号的额度里出。没有一份能用的那几段，池子要的量
/// 整份算卡住。接在 [`Chain`] 上的份作废了就接下一份；周窗扣光了，那个号到周重置之前用不了。
pub(crate) fn run(pieces: &[Piece], chains: &[Chain], others: &[Guard], w: &Walk) -> Raw {
    let now = w.now;
    let t_end = w.end;
    let mut s = World {
        pieces: pieces.to_vec(),
        amt: pieces.iter().map(|p| p.amt).collect(),
        chains,
        week: chains
            .iter()
            .map(|c| c.week.as_ref().map(|x| (x.left, x.until)))
            .collect(),
        first: chains
            .iter()
            .map(|c| c.week.as_ref().map_or(i64::MIN, |x| x.until))
            .collect(),
        now,
        end: t_end,
        out: Raw {
            rewrite: w.lump,
            ..Raw::default()
        },
    };
    let mut pend: Vec<i64> = Vec::new();
    for k in 0..pieces.len() {
        s.renew(k, &mut pend);
    }
    for &k in &w.warm {
        if k < s.pieces.len() {
            s.open(k, now, &mut pend);
        }
    }
    let mut cache = w.cache.clone();
    let mut ev = Joins::default();
    // 守到视野末尾还没用完的那几份：到末尾还剩多少（交在 `left` 里）。
    let span = (t_end - now) as f64;
    let mut held: Vec<(usize, f64)> = Vec::new();
    for j in 1..w.steps.len() {
        let t = now + j as i64 * STEP;
        if t >= t_end {
            break;
        }
        ev.add(t, w.steps[j] - w.steps[j - 1], 0.0, false, true);
    }
    for g in others {
        let r = g.rate * w.m;
        if g.acct == w.mine || r <= 0.0 {
            continue;
        }
        let mut tt = now;
        if let Some(z) = g.at {
            if s.pieces[z].is_shut() {
                s.open(z, now, &mut pend);
            }
            if !s.pieces[z].is_shut() {
                let a = s.eff(z, now);
                let dt = hold_out(a, r, s.pieces[z].exp, now);
                s.spend(z, r * dt / 60.0, now);
                s.amt[z] = 0.0;
                if dt >= span && s.pieces[z].exp > t_end {
                    held.push((z, a - r * span / 60.0));
                }
                tt = now + dt as i64;
            }
        }
        if tt < t_end {
            ev.push(tt, r, 0.0, false);
        }
    }
    let share = w.share.unwrap_or(0.0);
    let rho = w.peak() + share;
    // 一起守的别的会话几点离开那份、并进池子（守不住 ⇒ 此刻）。
    let mut part = now;
    match w.at {
        _ if w.hold => {
            let back = (0..s.pieces.len())
                .filter(|&i| !s.pieces[i].late && s.amt[i] > 0.0 && s.pieces[i].exp > now)
                .map(|i| s.pieces[i].start.max(now))
                .min()
                .unwrap_or(t_end)
                .min(t_end);
            s.out.stuck += w.demand(now, back);
            if back < t_end {
                ev.push(back, w.rate_at(back), w.lump, true);
            }
        }
        Some(k) if rho > 0.0 => {
            if s.pieces[k].is_shut() {
                s.open(k, w.first_use(share).max(now), &mut pend);
            }
            if s.pieces[k].is_shut() {
                ev.push(now, w.rate_at(now), w.lump, true);
            } else {
                let need = s.eff(k, now) - w.lump;
                s.spend(k, w.lump, now);
                s.amt[k] = 0.0;
                if need <= 0.0 {
                    ev.push(now, w.rate_at(now), -need, true);
                } else {
                    let (dt, mine) = w.guard(need, share, s.pieces[k].exp);
                    s.spend(k, mine, now);
                    s.drain(k, share * dt / 60.0, now);
                    if dt >= span && s.pieces[k].exp > t_end {
                        held.push((k, need - w.demand(now, t_end) - share * span / 60.0));
                    }
                    let t = now + dt as i64;
                    part = t;
                    if t < t_end {
                        ev.push(t, w.rate_at(t), 0.0, true);
                    }
                }
            }
        }
        _ => ev.push(now, w.rate_at(now), w.lump, true),
    }
    if w.rejoin && !w.hold && share > 0.0 && part < t_end {
        ev.push(part, share, 0.0, false);
    }
    let mut evt = ev.at.clone();
    evt.sort_unstable();
    let mut avail: Vec<i64> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        if s.amt[i] > 0.0 && s.pieces[i].exp > now {
            if p.start > now {
                pend.push(p.start * SLOT + i as i64);
            } else {
                avail.push(key(&s.pieces[i], i));
            }
        }
    }
    pend.sort_unstable();
    avail.sort_unstable();
    let mut wakes: Vec<i64> = chains
        .iter()
        .filter_map(|c| c.week.as_ref())
        .flat_map(|x| {
            (0..)
                .map(move |n| x.until + n * WEEK)
                .take_while(move |t| *t < t_end)
        })
        .collect();
    wakes.sort_unstable();
    let at_of = |code: i64| (code % SLOT) as usize;
    let (mut d, mut due, mut tau) = (0.0_f64, 0.0_f64, now);
    let (mut ei, mut pi, mut wi) = (0usize, 0usize, 0usize);
    let mut me_in = false;
    let mut my = w.mine.clone();
    // 每一步至少挪掉一样（并进一条 · 进一份 · 开一份 · 用完或作废一份 · 到下一个时刻），封顶照份数与条数。
    let spawn = chains.len() * ((t_end - now).max(0) / WIN5 + 2) as usize;
    let cap = 4 * (pieces.len() + spawn) + 4 * evt.len() + 16 + 4 * (wakes.len() + spawn);
    let mut steps = 0usize;
    while tau < t_end && steps < cap {
        steps += 1;
        while ei < evt.len() && evt[ei] / SLOT <= tau {
            let q = at_of(evt[ei]);
            if ev.step[q] {
                if me_in {
                    d += ev.rho[q];
                }
            } else {
                d += ev.rho[q];
                due += ev.lump[q];
                me_in |= ev.me[q];
            }
            ei += 1;
        }
        let mut moved = false;
        while pi < pend.len() && pend[pi] / SLOT <= tau {
            let i = at_of(pend[pi]);
            avail.push(key(&s.pieces[i], i));
            pi += 1;
            moved = true;
        }
        if moved {
            avail.sort_unstable();
        }
        while wi < wakes.len() && wakes[wi] <= tau {
            wi += 1;
        }
        let mut head = None;
        let mut p = 0;
        while p < avail.len() {
            let j = at_of(avail[p]);
            if s.amt[j] <= 0.0 || s.pieces[j].exp <= tau {
                avail.remove(p);
                continue;
            }
            if s.usable(j, tau) {
                head = Some(p);
                break;
            }
            p += 1;
        }
        let mut nxt = if ei < evt.len() {
            evt[ei] / SLOT
        } else {
            t_end
        };
        if pi < pend.len() && pend[pi] / SLOT < nxt {
            nxt = pend[pi] / SLOT;
        }
        if wi < wakes.len() && wakes[wi] < nxt {
            nxt = wakes[wi];
        }
        if d <= 0.0 && due <= 0.0 {
            tau = nxt;
            continue;
        }
        let Some(h) = head else {
            s.out.stuck += d * (nxt - tau) as f64 / 60.0 + due;
            due = 0.0;
            tau = nxt;
            continue;
        };
        let k = at_of(avail[h]);
        if s.pieces[k].is_shut() {
            avail.remove(h);
            if s.open(k, tau, &mut pend) {
                avail.push(key(&s.pieces[k], k));
                avail.sort_unstable();
                pend[pi..].sort_unstable();
            }
            continue;
        }
        let who = s.pieces[k].who.clone();
        if me_in && who != my {
            let r = match cache.get(&who) {
                Some(&c) if c > tau => w.grow * (tau - now) as f64 / 60.0,
                _ => w.kfull,
            };
            s.out.rewrite += r;
            due += r;
            cache.insert(my.clone(), tau + w.ttl);
            my.clone_from(&who);
        }
        let a = s.eff(k, tau);
        if due > 0.0 {
            let take = due.min(a);
            s.amt[k] -= take;
            due -= take;
            s.spend(k, take, tau);
            continue;
        }
        let mut end = s.pieces[k].exp.min(nxt);
        let e2 = tau + (a / d * 60.0).ceil() as i64;
        if e2 < end {
            end = e2;
        }
        if end <= tau {
            end = tau + 1;
        }
        let used = (d * (end - tau) as f64 / 60.0).min(a);
        s.amt[k] -= used;
        s.spend(k, used, tau);
        tau = end;
    }
    let mut left: Vec<(String, f64, i64)> = held
        .into_iter()
        .filter(|(_, a)| *a > 0.0)
        .map(|(k, a)| (s.pieces[k].who.clone(), a, s.pieces[k].exp))
        .collect();
    for k in 0..s.pieces.len() {
        if s.pieces[k].is_shut() {
            continue;
        }
        let a = s.eff(k, t_end);
        if a > 0.0 {
            left.push((s.pieces[k].who.clone(), a, s.pieces[k].exp));
        }
    }
    left.sort_by_key(|x| x.2);
    let mut out = s.out;
    out.left = left;
    out
}

// ───────────── 挂进积木库（`lib::project_raw` · `lib::rollout`）─────────────

type Bad = Box<EvalAltResult>;

fn bad(what: &str) -> Bad {
    format!("lib::project_raw: {what}").into()
}

fn num(d: &Dynamic, what: &str) -> Result<f64, Bad> {
    d.as_float()
        .ok()
        .or_else(|| d.as_int().ok().map(|i| i as f64))
        .ok_or_else(|| bad(&format!("{what} must be a number")))
}

fn int(d: &Dynamic, what: &str) -> Result<i64, Bad> {
    d.as_int()
        .map_err(|_| bad(&format!("{what} must be an integer")))
}

fn flag(d: &Dynamic, what: &str) -> Result<bool, Bad> {
    d.as_bool()
        .map_err(|_| bad(&format!("{what} must be true or false")))
}

fn text(d: &Dynamic, what: &str) -> Result<String, Bad> {
    d.clone()
        .into_string()
        .map_err(|_| bad(&format!("{what} must be a string")))
}

/// 第几格：`()` ⇒ 空；越界 ⇒ 错。
fn index(d: Option<&Dynamic>, len: usize, what: &str) -> Result<Option<usize>, Bad> {
    match d {
        None => Ok(None),
        Some(x) if x.is_unit() => Ok(None),
        Some(x) => usize::try_from(int(x, what)?)
            .ok()
            .filter(|i| *i < len)
            .map(Some)
            .ok_or_else(|| bad(&format!("{what} is out of range"))),
    }
}

fn field<'a>(m: &'a Map, k: &str) -> Result<&'a Dynamic, Bad> {
    m.get(k).ok_or_else(|| bad(&format!("o.{k} is missing")))
}

fn column(m: &Map, k: &str) -> Result<Array, Bad> {
    m.get(k)
        .and_then(|x| x.clone().try_cast::<Array>())
        .ok_or_else(|| bad(&format!("oev.{k} must be an array")))
}

/// 照 `lib::project` 的八个参数（按列放的每份额度 · 别的会话 · 这一种走法）拼出内核的输入。
#[allow(clippy::too_many_arguments)]
pub(crate) fn inputs(
    amt: &Array,
    st: &Array,
    ex: &Array,
    who: &Array,
    wk: &Array,
    late: &Array,
    oev: &Map,
    o: &Map,
) -> Result<(Vec<Piece>, Vec<Guard>, Walk), Bad> {
    let n = amt.len();
    if [st.len(), ex.len(), who.len(), wk.len(), late.len()]
        .iter()
        .any(|l| *l != n)
    {
        return Err(bad("the six piece columns must be the same length"));
    }
    let pieces = (0..n)
        .map(|i| {
            Ok(Piece {
                amt: num(&amt[i], "amt")?,
                start: int(&st[i], "st")?,
                exp: int(&ex[i], "ex")?,
                who: text(&who[i], "who")?,
                weekly: flag(&wk[i], "wk")?,
                late: flag(&late[i], "late")?,
                chain: None,
            })
        })
        .collect::<Result<Vec<_>, Bad>>()?;
    let (acc, q, r) = (column(oev, "acc")?, column(oev, "q")?, column(oev, "r")?);
    if q.len() != acc.len() || r.len() != acc.len() {
        return Err(bad("oev.acc, oev.q and oev.r must be the same length"));
    }
    let others = (0..acc.len())
        .map(|i| {
            Ok(Guard {
                acct: text(&acc[i], "oev.acc")?,
                at: index(Some(&q[i]), n, "oev.q")?,
                rate: num(&r[i], "oev.r")?,
            })
        })
        .collect::<Result<Vec<_>, Bad>>()?;
    let cache = match o.get("cache").filter(|c| !c.is_unit()) {
        None => BTreeMap::new(),
        Some(c) => c
            .clone()
            .try_cast::<Map>()
            .ok_or_else(|| bad("o.cache must be a map"))?
            .into_iter()
            .filter(|(_, v)| !v.is_unit())
            .map(|(k, v)| Ok((k.to_string(), int(&v, "o.cache")?)))
            .collect::<Result<_, Bad>>()?,
    };
    let share = match o.get("share").filter(|x| !x.is_unit()) {
        None => None,
        Some(x) => Some(num(x, "o.share")?),
    };
    let walk = Walk {
        m: num(field(o, "m")?, "o.m")?,
        at: index(o.get("at"), n, "o.at")?,
        rho: num(field(o, "rho")?, "o.rho")?,
        share,
        mine: text(field(o, "mine")?, "o.mine")?,
        lump: num(field(o, "lump")?, "o.lump")?,
        kfull: num(field(o, "kfull")?, "o.kfull")?,
        grow: num(field(o, "grow")?, "o.grow")?,
        cache,
        ttl: int(field(o, "ttl")?, "o.ttl")?,
        now: int(field(o, "now")?, "o.now")?,
        end: int(field(o, "end")?, "o.end")?,
        steps: Vec::new(),
        warm: Vec::new(),
        hold: false,
        rejoin: false,
    };
    Ok((pieces, others, walk))
}

/// 推完的原始量交回策略的样子：`#{stuck, rewrite, weekly: #{号: tok}, left: [[号, tok, 作废时刻]]}`。
pub(crate) fn raw_dynamic(r: &Raw) -> Map {
    let mut weekly = Map::new();
    for (k, v) in &r.weekly {
        weekly.insert(k.as_str().into(), Dynamic::from(*v));
    }
    let left: Array = r
        .left
        .iter()
        .map(|(who, a, exp)| {
            Dynamic::from(vec![
                Dynamic::from(who.clone()),
                Dynamic::from(*a),
                Dynamic::from(*exp),
            ])
        })
        .collect();
    let mut m = Map::new();
    m.insert("stuck".into(), Dynamic::from(r.stuck));
    m.insert("rewrite".into(), Dynamic::from(r.rewrite));
    m.insert("weekly".into(), Dynamic::from(weekly));
    m.insert("left".into(), Dynamic::from(left));
    m
}

// ───────────── `lib::rollout(ctx, plan)`：份数照 ctx 的事实搭 ─────────────

fn rbad(what: &str) -> Bad {
    format!("lib::rollout: {what}").into()
}

/// 表里一格：没有 · `()` ⇒ 空。
fn opt<'a>(m: &'a Map, k: &str) -> Option<&'a Dynamic> {
    m.get(k).filter(|v| !v.is_unit())
}

fn fnum(d: Option<&Dynamic>) -> Option<f64> {
    d.and_then(|d| {
        d.as_float()
            .ok()
            .or_else(|| d.as_int().ok().map(|i| i as f64))
    })
}

fn fint(d: Option<&Dynamic>) -> Option<i64> {
    d.and_then(|d| d.as_int().ok())
}

fn fmap(d: Option<&Dynamic>) -> Option<Map> {
    d.and_then(|d| d.clone().try_cast::<Map>())
}

/// 一个号一窗：`m.<w>`（`w5h` · `w7d`）。
fn win(m: &Map, k: &str, w: &str) -> Option<Dynamic> {
    fmap(opt(m, k)).and_then(|x| opt(&x, w).cloned())
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// 一格上限：数 ⇒ 整天一样；`[#{from, to, v}]` ⇒ 分段，落空取 `dflt`。
fn caps_of(d: &Dynamic, dflt: f64) -> Result<Caps, Bad> {
    if let Some(v) = fnum(Some(d)) {
        return Ok(Caps {
            segs: Vec::new(),
            dflt: v,
        });
    }
    let segs = d
        .clone()
        .try_cast::<Array>()
        .ok_or_else(|| rbad("plan.caps must give a number or segments per window"))?
        .into_iter()
        .map(|s| {
            let m = s
                .try_cast::<Map>()
                .ok_or_else(|| rbad("a cap segment must be a map"))?;
            match (fint(m.get("from")), fint(m.get("to")), fnum(m.get("v"))) {
                (Some(f), Some(t), Some(v)) => Ok((f, t, v)),
                _ => Err(rbad("a cap segment needs from, to and v")),
            }
        })
        .collect::<Result<Vec<_>, Bad>>()?;
    Ok(Caps { segs, dflt })
}

/// 本会话的用速：常态 ＝ `pace`；猛涨 ＝ `bins` 里高过中位数 1.5 倍的那几格的均值（有那几格才有这一态），进出的概率从 `bins` 前后两格数；
/// 此刻在不在猛涨看 `paceShort` · `inflight`。
struct Load {
    pace: f64,
    burst: Option<(f64, f64, f64)>,
    hot: bool,
}

/// `session.bins` 每格几分钟。
const BIN_MIN: f64 = STEP as f64 / 60.0;

impl Load {
    fn of(s: &Map) -> Option<Self> {
        let bins: Vec<Option<f64>> = opt(s, "bins")
            .and_then(|b| b.clone().try_cast::<Array>())
            .map(|a| a.iter().map(|x| fnum(Some(x))).collect())
            .unwrap_or_default();
        let seen: Vec<f64> = bins.iter().flatten().copied().collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let pace =
            fnum(opt(s, "pace")).or_else(|| (!seen.is_empty()).then(|| mean(&seen) / BIN_MIN))?;
        let mut load = Load {
            pace,
            burst: None,
            hot: false,
        };
        if seen.len() < 6 {
            return Some(load);
        }
        let base = median(seen.clone())
            .filter(|m| *m > 0.0)
            .unwrap_or(mean(&seen));
        if base <= 0.0 {
            return Some(load);
        }
        let theta = 1.5 * base;
        let highs: Vec<f64> = seen.iter().copied().filter(|b| *b > theta).collect();
        if highs.is_empty() || mean(&highs) / BIN_MIN <= pace {
            return Some(load);
        }
        let (mut nn, mut nb, mut bn, mut bb) = (0.0, 0.0, 0.0, 0.0);
        for pair in bins.windows(2) {
            if let [Some(a), Some(b)] = pair {
                match (*a > theta, *b > theta) {
                    (false, false) => nn += 1.0,
                    (false, true) => nb += 1.0,
                    (true, false) => bn += 1.0,
                    (true, true) => bb += 1.0,
                }
            }
        }
        let up = (nb + 0.5) / (nn + nb + 1.0);
        let down = (bn + 0.5) / (bn + bb + 1.0);
        load.burst = Some((mean(&highs) / BIN_MIN, up, down));
        load.hot = fnum(opt(s, "paceShort")).is_some_and(|p| p > theta / BIN_MIN)
            || fnum(opt(s, "inflight")).is_some_and(|f| f >= 2.0);
        Some(load)
    }

    /// 第 `p` 条路径的每一步用速（`n` 步）。没有猛涨那一态 ⇒ 每一步都是常态。
    fn path(&self, n: usize, p: usize) -> Vec<f64> {
        let Some((hi, up, down)) = self.burst else {
            return vec![self.pace; n];
        };
        let mut x = 0x9E37_79B9_7F4A_7C15_u64 ^ (p as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
        let mut uniform = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut hot = self.hot;
        (0..n)
            .map(|_| {
                let r = if hot { hi } else { self.pace };
                let u = uniform();
                hot = if hot { u >= down } else { u < up };
                r
            })
            .collect()
    }
}

/// 一种走法：换到（或留在）这个号 · 停发 · 留着、此刻给这个号开窗。
enum Way {
    To(String),
    Hold,
    Warm(String),
}

/// 别的会话用速的几档（照路径轮着取）：量到的值、往下一档、往上一档。
const OTHERS: [f64; 3] = [1.0, 0.5, 1.5];
/// 交回的原始量至多这么大（数组元素 ＋ 表项，照路径减）：沙箱的数组 / 表上限按一个值里所有层加起来算。
const BUDGET: usize = 6000;
/// 视野至多多长（秒）。
const MAX_HORIZON: i64 = 86_400;

/// 一个号交给定价的几样：1% ≈ 是估的 · 每分钟能供多少（5 小时窗的上限按一天平均，周剩的摊到周重置）· 此刻开着那份剩多少 ·
/// 周剩多少 · 周重置。
struct Summary {
    est: bool,
    rate: f64,
    open: f64,
    week: Option<f64>,
    reset7: Option<i64>,
}

/// ★ `lib::rollout(ctx, plan)`：照 ctx 搭份数，每种走法、每条路径各推一遍，交原始量（不定价）。
/// 量不到本会话的用速（`pace` 与 `bins` 都空）· 一个号的 1% ≈ 都量不到 ⇒ `()`。
pub(crate) fn rollout_entry(ctx: &Map, plan: &Map) -> Result<Dynamic, Bad> {
    let now = fint(opt(ctx, "now")).ok_or_else(|| rbad("ctx.now must be an integer"))?;
    let clock = fint(opt(ctx, "clock")).unwrap_or(now.div_euclid(60).rem_euclid(1440));
    let current = opt(ctx, "current")
        .and_then(|c| c.clone().into_string().ok())
        .unwrap_or_default();
    let session = fmap(opt(ctx, "session")).unwrap_or_default();
    let caps = fmap(opt(plan, "caps")).ok_or_else(|| rbad("plan.caps must be a map"))?;
    let dflt = fnum(opt(plan, "dflt")).unwrap_or(100.0);
    let late: Vec<String> = opt(plan, "late")
        .and_then(|l| l.clone().try_cast::<Array>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.into_string().ok())
        .collect();
    let ways = opt(plan, "walks")
        .and_then(|l| l.clone().try_cast::<Array>())
        .ok_or_else(|| rbad("plan.walks must be an array"))?
        .into_iter()
        .map(|x| {
            if let Some(m) = x.clone().try_cast::<Map>() {
                return opt(&m, "warm")
                    .and_then(|a| a.clone().into_string().ok())
                    .map(Way::Warm)
                    .ok_or_else(|| rbad("a walk map must be #{warm: account}"));
            }
            match x.into_string() {
                Ok(h) if h == "hold" => Ok(Way::Hold),
                Ok(id) => Ok(Way::To(id)),
                Err(_) => Err(rbad(
                    "each walk is an account, \"hold\" or #{warm: account}",
                )),
            }
        })
        .collect::<Result<Vec<_>, Bad>>()?;
    let want = fint(opt(plan, "paths")).unwrap_or(8).clamp(1, 32) as usize;
    let Some(load) = Load::of(&session) else {
        return Ok(Dynamic::UNIT);
    };
    let accounts: Vec<Map> = opt(ctx, "accounts")
        .and_then(|a| a.clone().try_cast::<Array>())
        .ok_or_else(|| rbad("ctx.accounts must be an array"))?
        .into_iter()
        .filter_map(|a| a.try_cast::<Map>())
        .filter(|a| {
            opt(a, "id")
                .and_then(|i| i.clone().into_string().ok())
                .is_some_and(|id| caps.contains_key(id.as_str()))
                && opt(a, "kind")
                    .and_then(|k| k.clone().into_string().ok())
                    .as_deref()
                    != Some("api")
        })
        .collect();
    let tpp = |a: &Map, w: &str| fnum(win(a, "tokensPerPct", w).as_ref()).filter(|t| *t > 0.0);
    let Some(mid5) = median(accounts.iter().filter_map(|a| tpp(a, "w5h")).collect()) else {
        return Ok(Dynamic::UNIT);
    };
    let mid7 = median(accounts.iter().filter_map(|a| tpp(a, "w7d")).collect());
    let used_of = |a: &Map, w: &str| {
        let r = fint(win(a, "resets", w).as_ref());
        if r.is_some_and(|r| r <= now) {
            0.0
        } else {
            fnum(win(a, "used", w).as_ref()).unwrap_or(0.0)
        }
    };
    let mut pieces = Vec::new();
    let mut chains = Vec::new();
    let mut first: BTreeMap<String, usize> = BTreeMap::new();
    let mut others: BTreeMap<String, f64> = BTreeMap::new();
    let mut sums: BTreeMap<String, Summary> = BTreeMap::new();
    let mut est = Vec::new();
    for a in &accounts {
        let id = opt(a, "id")
            .and_then(|i| i.clone().into_string().ok())
            .unwrap_or_default();
        if first.contains_key(&id) {
            continue;
        }
        let refused = fint(opt(a, "refusedUntil")).filter(|r| *r > now);
        if opt(a, "sendable").and_then(|s| s.as_bool().ok()) != Some(true) && refused.is_none() {
            continue;
        }
        let start = refused.unwrap_or(now);
        let row =
            fmap(caps.get(id.as_str())).ok_or_else(|| rbad("plan.caps.<account> must be a map"))?;
        let c5 = caps_of(
            opt(&row, "5h").ok_or_else(|| rbad("plan.caps.<account> needs \"5h\""))?,
            dflt,
        )?;
        let c7 = match opt(&row, "7d") {
            Some(d) => caps_of(d, dflt)?,
            None => c5.clone(),
        };
        let k5 = tpp(a, "w5h").unwrap_or_else(|| {
            est.push(id.clone());
            mid5
        });
        let k7 = tpp(a, "w7d").or(mid7).unwrap_or(k5);
        let week = (opt(a, "weekly").and_then(|w| w.as_bool().ok()) == Some(true)).then(|| {
            let mut until = fint(win(a, "resets", "w7d").as_ref()).unwrap_or(now + WEEK);
            while until <= now {
                until += WEEK;
            }
            let c = c7.at(clock);
            Week {
                left: (c - used_of(a, "w7d")).max(0.0) * k7,
                until,
                full: c * k7,
            }
        });
        let ci = chains.len();
        let lt = late.contains(&id);
        let r5 = fint(win(a, "resets", "w5h").as_ref()).filter(|r| *r > now);
        let piece = match r5 {
            Some(r) => Piece {
                amt: (c5.at(clock) - used_of(a, "w5h")).max(0.0) * k5,
                start,
                exp: r,
                who: id.clone(),
                weekly: false,
                late: lt,
                chain: Some(ci),
            },
            None => Piece::shut(&id, start, ci, lt),
        };
        let open = if piece.is_shut() {
            0.0
        } else {
            week.as_ref().map_or(piece.amt, |w| piece.amt.min(w.left))
        };
        let rate5 = c5.mean() * k5 / 300.0;
        let rate = match &week {
            Some(w) => rate5.min(w.left / (((w.until - now) as f64 / 60.0).max(1.0))),
            None => rate5,
        };
        sums.insert(
            id.clone(),
            Summary {
                est: est.last() == Some(&id),
                rate,
                open,
                week: week.as_ref().map(|w| w.left),
                reset7: week.as_ref().map(|w| w.until),
            },
        );
        chains.push(Chain {
            k5,
            cap5: c5,
            clock,
            up: win(a, "windowGrid", "up").and_then(|u| u.as_bool().ok()) == Some(true),
            week,
        });
        if let Some(o) = fnum(win(a, "others", "w5h").as_ref()).filter(|o| *o > 0.0) {
            others.insert(id.clone(), o * k5);
        }
        first.insert(id, pieces.len());
        pieces.push(piece);
    }
    let horizon = fint(opt(plan, "horizon")).map_or_else(
        || {
            pieces
                .iter()
                .filter(|p| !p.is_shut())
                .map(|p| p.exp + WIN5)
                .max()
                .unwrap_or(0)
                .max(now + WIN5)
                - now
        },
        |h| h.max(STEP),
    );
    let end = now + horizon.min(MAX_HORIZON);
    let n_steps = ((end - now + STEP - 1) / STEP).max(1) as usize;
    let guards: Vec<Guard> = others
        .iter()
        .map(|(id, r)| Guard {
            acct: id.clone(),
            at: first.get(id).copied(),
            rate: *r,
        })
        .collect();
    let we = fnum(opt(&session, "writeExtra")).unwrap_or(1.9);
    let context = fnum(opt(&session, "context")).unwrap_or(0.0);
    let ttl = fint(opt(&session, "ttl")).unwrap_or(3600);
    let kfull = we * context;
    let grow = we * fnum(opt(&session, "grow")).unwrap_or(0.0);
    let cached = fmap(opt(&session, "cached")).unwrap_or_default();
    let mut cache: BTreeMap<String, i64> = cached
        .iter()
        .filter_map(|(k, v)| {
            fmap(Some(v))
                .and_then(|c| fint(opt(&c, "until")))
                .map(|u| (k.to_string(), u))
        })
        .collect();
    cache.insert(current.clone(), now + ttl);
    let lump = |x: &str| {
        if x == current {
            return 0.0;
        }
        match fmap(cached.get(x)) {
            Some(c)
                if fint(opt(&c, "until")).is_some_and(|u| u > now)
                    && fnum(opt(&c, "tokens")).is_some() =>
            {
                we * (context - fnum(opt(&c, "tokens")).unwrap_or(0.0)).max(0.0)
            }
            _ => kfull,
        }
    };
    // 份数 × 路径 × 走法封顶：先照份数估一条路径交回多大，推之前就减路径；推完照实际大小再减一次。
    let per_path = ways.len() * (6 + sums.len() + 4 * (pieces.len() + chains.len()));
    let room = BUDGET.saturating_sub(8 * sums.len() + ways.len() + 8);
    let n_paths = if load.burst.is_none() && guards.is_empty() {
        1
    } else {
        want.min((room / per_path.max(1)).max(1))
    };
    let paths: Vec<Vec<f64>> = (0..n_paths).map(|p| load.path(n_steps, p)).collect();
    let at_of = |id: &str| {
        first
            .get(id)
            .copied()
            .ok_or_else(|| rbad(&format!("walk account {id} is not usable in this rollout")))
    };
    let mut runs: Vec<Vec<Raw>> = Vec::new();
    for way in &ways {
        let mut per = Vec::new();
        for (p, steps) in paths.iter().enumerate() {
            let m = if guards.is_empty() {
                1.0
            } else {
                OTHERS[p % OTHERS.len()]
            };
            let (target, hold, warm) = match way {
                Way::To(x) => (Some(x.as_str()), false, Vec::new()),
                Way::Hold => (None, true, Vec::new()),
                Way::Warm(x) => (Some(current.as_str()), false, vec![at_of(x)?]),
            };
            let walk = Walk {
                m,
                at: target.map(at_of).transpose()?,
                rho: steps[0],
                share: target.and_then(|x| others.get(x)).map(|o| o * m),
                mine: target.unwrap_or("").to_string(),
                lump: target.map_or(0.0, lump),
                kfull,
                grow,
                cache: cache.clone(),
                ttl,
                now,
                end,
                steps: steps.clone(),
                warm,
                hold,
                rejoin: true,
            };
            per.push(run(&pieces, &chains, &guards, &walk));
        }
        runs.push(per);
    }
    // 交回的原始量撞不了沙箱的上限：照路径算大小，超了就减路径（至少一条）。
    let size = |p: usize| -> usize {
        runs.iter()
            .map(|r| 6 + r[p].weekly.len() + 4 * r[p].left.len())
            .sum()
    };
    let mut keep = 0;
    let mut total = 8 * sums.len() + ways.len() + 8;
    while keep < n_paths && (keep == 0 || total + size(keep) <= BUDGET) {
        total += size(keep);
        keep += 1;
    }
    let base = ways
        .iter()
        .position(|w| matches!(w, Way::To(x) if *x == current))
        .map_or(Dynamic::UNIT, |i| Dynamic::from(i as i64));
    let mut accts = Map::new();
    for (id, s) in &sums {
        let mut m = Map::new();
        m.insert("est".into(), Dynamic::from(s.est));
        m.insert("rate".into(), Dynamic::from(s.rate));
        m.insert("open".into(), Dynamic::from(s.open));
        m.insert("week".into(), s.week.map_or(Dynamic::UNIT, Dynamic::from));
        m.insert(
            "reset7".into(),
            s.reset7.map_or(Dynamic::UNIT, Dynamic::from),
        );
        accts.insert(id.as_str().into(), Dynamic::from(m));
    }
    let mut out = Map::new();
    out.insert("now".into(), Dynamic::from(now));
    out.insert("end".into(), Dynamic::from(end));
    out.insert("paths".into(), Dynamic::from(keep as i64));
    out.insert("base".into(), base);
    out.insert("others".into(), Dynamic::from(others.values().sum::<f64>()));
    out.insert("accts".into(), Dynamic::from(accts));
    out.insert(
        "runs".into(),
        Dynamic::from(
            runs.iter()
                .map(|r| {
                    Dynamic::from(
                        r[..keep]
                            .iter()
                            .map(|x| Dynamic::from(raw_dynamic(x)))
                            .collect::<Array>(),
                    )
                })
                .collect::<Array>(),
        ),
    );
    Ok(Dynamic::from(out))
}

/// 挂进积木库那一份模块：`lib::project_raw(amt, st, ex, who, wk, late, oev, o)`（参数同 `lib::project`）· `lib::rollout(ctx, plan)`。
pub(crate) fn install(m: &mut rhai::Module) {
    m.set_native_fn(
        "project_raw",
        |amt: Array,
         st: Array,
         ex: Array,
         who: Array,
         wk: Array,
         late: Array,
         oev: Map,
         o: Map|
         -> Result<Map, Bad> {
            let (p, g, w) = inputs(&amt, &st, &ex, &who, &wk, &late, &oev, &o)?;
            Ok(raw_dynamic(&rollout(&p, &g, &w)))
        },
    );
    m.set_native_fn("rollout", |ctx: Map, plan: Map| -> Result<Dynamic, Bad> {
        rollout_entry(&ctx, &plan)
    });
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/rollout_tests.rs"]
mod tests;
