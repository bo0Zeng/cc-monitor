//! ★ 推演（算法 §5）：离散事件模拟，状态是 (Σ, F) 的一份拷贝；每个事件点调的是和真用时同一个 [`turn`]。
//! 每个号照 [`rate_of`] 涨：别的会话压在它上面的 ＋ 本会话在它上面时本会话那一份（本会话换到哪，它那一份跟到哪）；量不到的不涨、不换算时刻。
//! 事件只有事实（重置 · 被拒到期 · 用到 100）和策略约的叫醒（时刻 · 读数 —— 量得到用速的号按用速换算成时刻，量不到不编数）。
//! ★ 回放（[`replay`]）：同一个 [`turn`] 每分钟判一次，本会话的需求照一段真轨迹走（不照用速），量卡住 · 换号 · 作废。

use super::decide::{turn, Me, Pick, PromptCache, Sigma, Strategy};
use super::standing;
use super::{Acct, WINDOW_SECS};
use serde_json::Value;

/// 一次推演最多几个事件。
pub(crate) const MAX_EVENTS: usize = 200;

/// 推演里号 `x` 第 `w` 窗每分钟涨几点。`pace` ＝ 本会话的用速（折合输入 token / 分），按这一窗「1% ≈ 多少」折成点：
/// - 量得到别的会话压在它上面的（`others`，只有 5 小时窗）⇒ 它，本会话在它上面再加本会话那一份；
/// - 量不到 ⇒ 号自己在额度账里量出来的用速（本会话在它上面时量的已含本会话，不再加）；在用的号这也量不到 ⇒ 只算本会话那一份；
/// - 都量不到 ⇒ 空（不编数）。
fn rate_of(x: &Acct, w: usize, in_use: bool, pace: Option<f64>) -> Option<f64> {
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

/// 号用起来时窗口没开 ⇒ 从此刻起计时（推演与回放同此一处）。
fn open_windows(x: &mut Acct, t: u64) {
    for w in 0..2 {
        if !x.api && x.resets[w].is_none_or(|r| r <= t) {
            x.used[w] = standing::used_at(x, w, t);
            x.resets[w] = Some(t + WINDOW_SECS[w]);
        }
    }
}

/// 到 `te` 为止重置过的窗口归零、被拒到期的放开（推演与回放同此一处）；交回作废的那几窗（窗 · 作废时刻 · 那时的用量）。
fn expire(x: &mut Acct, te: u64) -> Vec<(usize, u64, f64)> {
    let mut gone = Vec::new();
    for w in 0..2 {
        if let Some(r) = x.resets[w].filter(|r| *r <= te) {
            gone.push((w, r, x.used[w]));
            x.used[w] = 0.0;
            x.resets[w] = None;
        }
    }
    if x.refused_until.is_some_and(|u| u <= te) {
        x.refused_until = None;
    }
    gone
}

/// 「用哪个号」那一条的一段。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UseSeg {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) pick: Pick,
    /// 这一段的号来自用户拖出来的顺序。
    pub(crate) from_temp: bool,
    /// 在用的号用到上限的时刻还不知道（额度账里量不到它的用速）：这一段只排确定的事件。
    pub(crate) cap_unknown: bool,
    /// 这一判丢掉的拖出来的条目（画成「跳过」）。
    pub(crate) skipped: Vec<String>,
}

/// 每个号那一行一段：事实上发得出去吗，以及这一段开头两窗的用量。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaneSeg {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) sendable: bool,
    pub(crate) used: [f64; 2],
    /// 这一段开头刚重置过。
    pub(crate) reset: bool,
    /// 这个会话先不用这个号（整行「不用」）。
    pub(crate) off: bool,
}

/// 推演结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Timeline {
    /// 推演开始那一刻在用的号（第一段要是换了号，它也是一次换号）。
    pub(crate) current: String,
    pub(crate) uses: Vec<UseSeg>,
    pub(crate) lanes: Vec<(String, Vec<LaneSeg>)>,
    /// 到了事件上限 / 策略耗时用完，剩下的没画。
    pub(crate) truncated: bool,
    /// 判过的每一个事件点。
    pub(crate) nodes: Vec<u64>,
}

impl Timeline {
    /// 将来每次换到别的号（时刻 · 号）：时间轴上方那几个「16:07 work」。
    pub(crate) fn switches(&self) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        let mut last: Option<&str> = Some(self.current.as_str());
        for s in &self.uses {
            if let Pick::Use(a) = &s.pick {
                if last.is_some_and(|l| l != a) {
                    out.push((s.from, a.clone()));
                }
                last = Some(a);
            }
        }
        out
    }
}

/// ★ 从 `t0` 推到 `t1`。`clock_of(t)` ＝ 策略时区里 t 是一天的第几分钟。
pub(crate) fn simulate(
    sigma: &Sigma,
    facts: &[Acct],
    settings: &Value,
    p: &dyn Strategy,
    clock_of: &dyn Fn(u64) -> u16,
    t0: u64,
    t1: u64,
) -> Timeline {
    let mut s = sigma.clone();
    let mut f: Vec<Acct> = facts.to_vec();
    let mut t = t0;
    let mut uses: Vec<UseSeg> = Vec::new();
    let mut points: Vec<(u64, Vec<Acct>)> = Vec::new();
    let mut truncated = false;
    let mut n = 0;
    while t < t1 {
        if n >= MAX_EVENTS || p.exhausted() {
            truncated = true;
            break;
        }
        n += 1;
        let d = turn(&s, &f, settings, p, clock_of(t), t, &[]);
        let using = match &d.pick {
            Pick::Use(a) => Some(a.clone()),
            Pick::Hold { .. } => None,
        };
        if let Some(a) = &using {
            if let Some(x) = f.iter_mut().find(|x| &x.id == a) {
                open_windows(x, t);
            }
        }
        points.push((t, f.clone()));
        // 下一个事件：已知节点 · 用到 100 · 读数叫醒（按用速换算成时刻）· 视窗末尾。
        let mut ev: Vec<u64> = vec![t1];
        ev.extend(d.tn);
        // 用速照 [`rate_of`]：量得到的号照它涨、照它换算用到 100 · 读数叫醒的时刻；
        // 量不到 ⇒ 不编数，那一窗的这几个时刻不换算，只排确定的事件。在用的号量不到 ⇒ 这一段标「用到上限的时刻还不知道」。
        let mut unknown = false;
        for x in f.iter().filter(|x| !x.api) {
            let in_use = using.as_deref() == Some(x.id.as_str());
            for w in 0..2 {
                let open = in_use || x.resets[w].is_some_and(|r| r > t);
                let mut ns: Vec<f64> = Vec::new();
                if open {
                    ns.push(100.0);
                }
                ns.extend(
                    d.sigma
                        .wake_when
                        .iter()
                        .filter(|k| k.acct == x.id && k.w == w)
                        .map(|k| k.used_at_least),
                );
                if ns.is_empty() {
                    continue;
                }
                match rate_of(x, w, in_use, sigma.me.pace) {
                    Some(r) if open && r > 0.0 => {
                        for n in ns {
                            let room = n - standing::used_at(x, w, t);
                            if room > 0.0 {
                                ev.push(t + ((room / r) * 60.0).ceil() as u64);
                            }
                        }
                    }
                    Some(_) => {}
                    // 没有周上限的号周窗用不满，量不到它的用速不算「还不知道」。
                    None => unknown |= in_use && (w == 0 || x.weekly),
                }
            }
        }
        let te = ev
            .into_iter()
            .filter(|x| *x > t)
            .min()
            .unwrap_or(t1)
            .min(t1)
            .max(t + 1);
        uses.push(UseSeg {
            from: t,
            to: te,
            pick: d.pick.clone(),
            from_temp: d.from_temp,
            cap_unknown: unknown,
            skipped: d.skipped.clone(),
        });
        // 推进到 te：量得到用速的号照它涨（窗口开着的；在用的号上面已经开窗）。
        for x in f.iter_mut().filter(|x| !x.api) {
            let in_use = using.as_deref() == Some(x.id.as_str());
            for w in 0..2 {
                let r = rate_of(x, w, in_use, sigma.me.pace);
                if let (Some(r), true) = (r, x.resets[w].is_some_and(|r| r > t)) {
                    x.used[w] = standing::used_at(x, w, t) + r * (te - t) as f64 / 60.0;
                }
            }
        }
        for x in &mut f {
            expire(x, te);
        }
        s = d.sigma;
        t = te;
    }
    // 合并相邻同样的「用哪个号」段。
    let mut merged: Vec<UseSeg> = Vec::new();
    for u in uses {
        match merged.last_mut() {
            Some(m) if m.pick == u.pick && m.from_temp == u.from_temp && u.skipped.is_empty() => {
                m.to = u.to;
                m.cap_unknown |= u.cap_unknown;
            }
            _ => merged.push(u),
        }
    }
    let end = t;
    let lanes = facts
        .iter()
        .map(|a0| {
            let a = &a0.id;
            let off = sigma.off.contains(a);
            let mut segs: Vec<LaneSeg> = Vec::new();
            let mut prev: Option<[f64; 2]> = None;
            for (i, (at, fs)) in points.iter().enumerate() {
                let to = points.get(i + 1).map_or(end, |p| p.0);
                let Some(x) = fs.iter().find(|x| &x.id == a) else {
                    continue;
                };
                let used = [standing::used_at(x, 0, *at), standing::used_at(x, 1, *at)];
                let reset = prev.is_some_and(|p| used.iter().zip(p).any(|(u, p)| *u < p));
                prev = Some(used);
                let ok = !off && standing::sendable(x, *at);
                match segs.last_mut() {
                    Some(l) if l.sendable == ok && !reset => l.to = to,
                    _ => segs.push(LaneSeg {
                        from: *at,
                        to,
                        sendable: ok,
                        used,
                        reset,
                        off,
                    }),
                }
            }
            (a.clone(), segs)
        })
        .collect();
    let nodes = points.iter().map(|p| p.0).collect();
    Timeline {
        current: sigma.current.clone(),
        uses: merged,
        lanes,
        truncated,
        nodes,
    }
}

/// 回放的需求：本会话从 `t0` 起每分钟用了多少（折合输入 token）· 那一分钟的上下文（token）；
/// 写缓存那一档：比读缓存多几倍 · 留几秒。
#[derive(Debug, Clone)]
pub(crate) struct Trace {
    pub(crate) t0: u64,
    pub(crate) mins: Vec<(f64, u64)>,
    pub(crate) extra: f64,
    pub(crate) ttl: u64,
}

/// 本会话的用速看最近这么多秒 · 刚开头这么久不给（同额度账那一本的口径）。
const PACE_SPAN: u64 = super::super::ledger::EST_FRESH;
const PACE_FIRST: u64 = 10 * 60;
/// 起伏按 10 分钟一段、看最近 6 段，不到 3 段不给。
const SPREAD_BIN: u64 = 10 * 60;
const SPREAD_BINS: u64 = 6;
const SPREAD_MIN: usize = 3;

impl Trace {
    fn minute(&self, t: u64) -> Option<(f64, u64)> {
        let k = usize::try_from(t.checked_sub(self.t0)? / 60).ok()?;
        self.mins.get(k).copied()
    }

    fn units(&self, from: u64, to: u64) -> f64 {
        (from.max(self.t0)..to)
            .step_by(60)
            .filter_map(|t| self.minute(t))
            .map(|m| m.0)
            .sum()
    }

    /// 此刻交给判的 `session`：只看 `t` 以前的那几分钟（判的时候还不知道之后用多少）。
    fn me(&self, t: u64, cached: Vec<PromptCache>) -> Me {
        let pace = (t >= self.t0 + PACE_FIRST).then(|| {
            let from = t.saturating_sub(PACE_SPAN).max(self.t0);
            self.units(from, t) / ((t - from) as f64 / 60.0)
        });
        let bins: Vec<f64> = (0..SPREAD_BINS)
            .map(|k| t.saturating_sub(SPREAD_BIN * (k + 1)))
            .filter(|from| *from >= self.t0)
            .map(|from| self.units(from, from + SPREAD_BIN))
            .collect();
        let spread = (bins.len() >= SPREAD_MIN).then(|| {
            let n = bins.len() as f64;
            let mu = bins.iter().sum::<f64>() / n;
            let var = bins.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / n;
            if mu > 0.0 {
                var.sqrt() / mu
            } else {
                0.0
            }
        });
        let context = t.checked_sub(60).and_then(|p| self.minute(p)).map(|m| m.1);
        let back = t.saturating_sub(PACE_SPAN).max(self.t0);
        let grow = self
            .minute(back)
            .zip(context)
            .filter(|_| t >= back + 120)
            .map(|(a, b)| b.saturating_sub(a.1) as f64 / ((t - 60 - back) as f64 / 60.0));
        Me {
            pace: pace.filter(|p| *p > 0.0),
            spread,
            context,
            write_extra: Some(self.extra),
            ttl: Some(self.ttl),
            grow,
            cached,
        }
    }
}

/// 别的会话压在一个号 5 小时窗上的用速（点 / 分），在 `[from, to)` 里。
#[derive(Debug, Clone)]
pub(crate) struct Load {
    pub(crate) acct: String,
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) rate: f64,
}

/// 回放里策略管不着的事：别的会话的用量 · 号被拒的时段（号 · 起 · 止）· 用户「现在用」（时刻 · 号）。
#[derive(Debug, Clone)]
pub(crate) struct World {
    pub(crate) others: Vec<Load>,
    pub(crate) refused: Vec<(String, u64, u64)>,
    pub(crate) use_now: Vec<(u64, String)>,
}

/// 一次换号为什么换：在用的号用不了了（到上限 · 被拒）· 在用的号还能用、策略要提前换 · 用户「现在用」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HopCause {
    Forced,
    Early,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hop {
    pub(crate) at: u64,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) cause: HopCause,
}

/// 一份额度作废：哪个号的哪一窗 · 几点 · 作废时还剩几点（100 − 用量）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Expired {
    pub(crate) acct: String,
    pub(crate) w: usize,
    pub(crate) at: u64,
    pub(crate) left: f64,
}

/// 回放结果：换号 · 停发的时段 · 卡住的需求（折合输入 token）· 作废的每一份 · 跑完时各号的事实。
#[derive(Debug, Clone)]
pub(crate) struct Replay {
    pub(crate) hops: Vec<Hop>,
    pub(crate) holds: Vec<(u64, u64)>,
    pub(crate) stuck: f64,
    pub(crate) expired: Vec<Expired>,
    pub(crate) end: Vec<Acct>,
}

/// ★ 回放：从 `from` 到 `to` 每分钟判一次（和真用同一个 [`turn`]），需求照 `trace` 而不是用速。
/// 在用的号这一分钟涨 ＝ 本会话这一分钟的用量 ÷ 它「1% ≈ 多少」（换过去那一下另加整段重写：写缓存倍数 × 上下文，
/// 那个号上的缓存还没过期就不加），用到 100 那一刻起多出来的算卡住；停发的那几分钟整份卡住。
/// 别的号照 [`rate_of`] 涨（本会话不在上面 ⇒ 只有别的会话那一份）；窗口关着、别的会话开始用 ⇒ 从那一刻开窗。
#[allow(clippy::too_many_arguments)]
pub(crate) fn replay(
    sigma: &Sigma,
    facts: &[Acct],
    settings: &Value,
    p: &dyn Strategy,
    clock_of: &dyn Fn(u64) -> u16,
    trace: &Trace,
    world: &World,
    from: u64,
    to: u64,
) -> Replay {
    let mut s = sigma.clone();
    let mut f: Vec<Acct> = facts.to_vec();
    let mut out = Replay {
        hops: Vec::new(),
        holds: Vec::new(),
        stuck: 0.0,
        expired: Vec::new(),
        end: Vec::new(),
    };
    // 号 ⇒ 本会话在它上面的缓存留到几点 · 当时的上下文。
    let mut cache: Vec<(String, u64, u64)> = Vec::new();
    let mut held: Option<u64> = None;
    let mut t = from;
    while t < to {
        let te = (t + 60).min(to);
        for x in f.iter_mut() {
            if let Some(u) = world
                .refused
                .iter()
                .filter(|(a, a0, a1)| *a == x.id && *a0 < te && *a1 > t)
                .map(|r| r.2)
                .max()
            {
                x.refused_until = Some(u);
            }
            x.others = Some(
                world
                    .others
                    .iter()
                    .filter(|o| o.acct == x.id && o.from <= t && t < o.to)
                    .map(|o| o.rate)
                    .sum(),
            );
        }
        for (_, a) in world.use_now.iter().filter(|(at, _)| t <= *at && *at < te) {
            if let Ok(next) = super::decide::use_now(&s, &f, t, a) {
                if next.current != s.current {
                    out.hops.push(Hop {
                        at: t,
                        from: s.current.clone(),
                        to: next.current.clone(),
                        cause: HopCause::Manual,
                    });
                }
                s = next;
            }
        }
        cache.retain(|c| c.1 > t);
        s.me = trace.me(
            t,
            cache
                .iter()
                .map(|(a, until, tokens)| PromptCache {
                    account: a.clone(),
                    until: *until,
                    tokens: *tokens,
                })
                .collect(),
        );
        let prev = s.current.clone();
        let d = turn(&s, &f, settings, p, clock_of(t), t, &[]);
        let (tok, ctx) = trace.minute(t).unwrap_or((0.0, 0));
        match &d.pick {
            Pick::Use(a) => {
                if let Some(h) = held.take() {
                    out.holds.push((h, t));
                }
                let mut lump = 0.0;
                if *a != prev {
                    let still = d.order.contains(&prev)
                        && f.iter().any(|x| x.id == prev && standing::sendable(x, t));
                    out.hops.push(Hop {
                        at: t,
                        from: prev.clone(),
                        to: a.clone(),
                        cause: if still {
                            HopCause::Early
                        } else {
                            HopCause::Forced
                        },
                    });
                    if !cache.iter().any(|c| c.0 == *a) {
                        lump = trace.extra * ctx as f64;
                    }
                }
                if let Some(x) = f.iter_mut().find(|x| x.id == *a) {
                    open_windows(x, t);
                    // 这一分钟发得出去的：每一窗（量得到 1% 的）到 100 为止还装得下多少。
                    let room = (0..2)
                        .filter(|w| *w == 0 || x.weekly)
                        .filter_map(|w| x.per_pct[w].map(|k| (100.0 - x.used[w]).max(0.0) * k as f64))
                        .fold(f64::INFINITY, f64::min);
                    let served = (tok + lump).min(room);
                    out.stuck += tok - (served - lump).max(0.0).min(tok);
                    for w in 0..2 {
                        if let Some(k) = x.per_pct[w].filter(|k| *k > 0) {
                            x.used[w] = (x.used[w] + served / k as f64).min(100.0);
                        }
                    }
                }
                cache.retain(|c| c.0 != *a);
                cache.push((a.clone(), te + trace.ttl, ctx));
            }
            Pick::Hold { .. } => {
                held.get_or_insert(t);
                out.stuck += tok;
            }
        }
        // 别的会话那一份：窗口关着、它开始用 ⇒ 从此刻开窗。
        for x in f.iter_mut().filter(|x| !x.api) {
            for w in 0..2 {
                let Some(r) = rate_of(x, w, false, None).filter(|r| *r > 0.0) else {
                    continue;
                };
                if x.resets[w].is_none_or(|r| r <= t) {
                    open_windows(x, t);
                }
                x.used[w] = (x.used[w] + r * (te - t) as f64 / 60.0).min(100.0);
            }
        }
        for x in f.iter_mut() {
            for (w, at, used) in expire(x, te) {
                if w == 0 || x.weekly {
                    out.expired.push(Expired {
                        acct: x.id.clone(),
                        w,
                        at,
                        left: (100.0 - used).max(0.0),
                    });
                }
            }
        }
        s = d.sigma;
        t = te;
    }
    if let Some(h) = held {
        out.holds.push((h, to));
    }
    out.end = f;
    out
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/replay_tests.rs"]
mod replay_tests;
