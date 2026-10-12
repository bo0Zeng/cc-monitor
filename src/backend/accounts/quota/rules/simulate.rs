//! ★ 推演（算法 §3）：离散事件模拟，状态是 (Σ, F) 的一份拷贝；每个事件点调的是和真用时同一个 [`decide`]。
//! 只模拟这个会话自己在用的号的消耗；别的会话用掉的额度在下一次读数时进事实，下一次重算跟上。

use super::decide::{decide, Pick, Score, Sigma};
use super::settings::Rules;
use super::standing;
use super::{Acct, RATE_DEFAULT, WINDOW_SECS};

/// 一次推演最多几个事件。
pub(crate) const MAX_EVENTS: usize = 200;

/// 「用哪个号」那一条的一段。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UseSeg {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) pick: Pick,
    /// 这一段的号来自临时条目。
    pub(crate) from_temp: bool,
    /// 用速是默认值估的（在用的号没有用速）。
    pub(crate) guessed: bool,
    /// 这一判丢掉的临时条目（画成「跳过」）。
    pub(crate) skipped: Vec<String>,
}

/// 每个号那一行一段的状态（只由同一串事件点上的用量与上限得出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneKind {
    /// 能用。
    Usable,
    /// 到了上限（还没用到 100）。
    AtCap,
    /// 这段上限是 0%。
    Off,
    /// 用到 100 / 被拒 / 登录 · key · 超额挡着。
    Blocked,
}

/// 每个号那一行的一段。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaneSeg {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) kind: LaneKind,
    /// 这一段开头的余量（订阅号）。
    pub(crate) left: Option<f64>,
    /// 这一段开头刚重置过。
    pub(crate) reset: bool,
}

/// 推演结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Timeline {
    /// 推演开始那一刻在用的号（第一段要是换了号，它也是一次换号）。
    pub(crate) current: String,
    pub(crate) uses: Vec<UseSeg>,
    pub(crate) lanes: Vec<(String, Vec<LaneSeg>)>,
    /// 到了事件上限、剩下的没画。
    pub(crate) truncated: bool,
    /// 判过的每一个事件点（合并段之前）。
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

fn lane_kind(r: &Rules, start: &str, a: &Acct, t: u64) -> LaneKind {
    if standing::barred_at(a, t) || (!a.api && (0..2).any(|w| standing::used_at(a, w, t) >= 100.0))
    {
        LaneKind::Blocked
    } else if !a.api && (0..2).any(|w| r.cap(start, &a.id, w, t) == 0.0) {
        LaneKind::Off
    } else if !a.api && standing::headroom(r, start, a, t, false) <= 0.0 {
        LaneKind::AtCap
    } else {
        LaneKind::Usable
    }
}

/// ★ 从 `t0` 推到 `t1`。
pub(crate) fn simulate(
    sigma: &Sigma,
    facts: &[Acct],
    rules: &Rules,
    p: &dyn Score,
    t0: u64,
    t1: u64,
) -> Timeline {
    let start = sigma.start.clone();
    let list = rules.list(&start);
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
        let d = decide(&s, &f, rules, p, t, &[]);
        let using = match &d.pick {
            Pick::Use(a) => Some(a.clone()),
            Pick::Hold { .. } => None,
        };
        // 在用的号窗口没开 ⇒ 从此刻起计时。
        if let Some(a) = &using {
            if let Some(x) = f.iter_mut().find(|x| &x.id == a) {
                for w in 0..2 {
                    if !x.api && x.resets[w].is_none_or(|r| r <= t) {
                        x.used[w] = standing::used_at(x, w, t);
                        x.resets[w] = Some(t + WINDOW_SECS[w]);
                    }
                }
            }
        }
        points.push((t, f.clone()));
        // 下一个事件。
        let mut ev: Vec<u64> = vec![t1];
        ev.extend(d.tn);
        let mut guessed = false;
        if let Some(a) = &using {
            if let Some(x) = f.iter().find(|x| &x.id == a) {
                if !x.api {
                    for w in 0..2 {
                        let rate = x.rate[w].unwrap_or_else(|| {
                            guessed = true;
                            RATE_DEFAULT[w]
                        });
                        let room = standing::cap_eff(rules, &start, x, w, t, d.lift)
                            - standing::used_at(x, w, t);
                        if rate > 0.0 && room > 0.0 {
                            ev.push(t + ((room / rate) * 60.0).ceil() as u64);
                        }
                        // 用到 100 也是一个点（放开软上限时）。
                        let full = 100.0 - standing::used_at(x, w, t);
                        if rate > 0.0 && full > 0.0 {
                            ev.push(t + ((full / rate) * 60.0).ceil() as u64);
                        }
                    }
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
            from_temp: using.as_ref().is_some_and(|a| d.sigma.temp.contains(a)),
            guessed: guessed && using.is_some(),
            skipped: d.skipped.clone(),
        });
        // 推进到 te：只有在用的号涨。
        if let Some(a) = &using {
            if let Some(x) = f.iter_mut().find(|x| &x.id == a) {
                for w in 0..2 {
                    if !x.api {
                        let rate = x.rate[w].unwrap_or(RATE_DEFAULT[w]);
                        x.used[w] = standing::used_at(x, w, t) + rate * (te - t) as f64 / 60.0;
                    }
                }
            }
        }
        for x in &mut f {
            for w in 0..2 {
                if x.resets[w].is_some_and(|r| r <= te) {
                    x.used[w] = 0.0;
                    x.resets[w] = None;
                }
            }
            if x.refused_until.is_some_and(|u| u <= te) {
                x.refused_until = None;
            }
        }
        s = d.sigma;
        t = te;
    }
    // 合并相邻同样的「用哪个号」段。
    let mut merged: Vec<UseSeg> = Vec::new();
    for u in uses {
        match merged.last_mut() {
            Some(m) if m.pick == u.pick && m.from_temp == u.from_temp && u.skipped.is_empty() => {
                m.to = u.to
            }
            _ => merged.push(u),
        }
    }
    let end = t;
    let lanes = list
        .iter()
        .map(|a| {
            let mut segs: Vec<LaneSeg> = Vec::new();
            let mut prev_open: Option<bool> = None;
            for (i, (at, fs)) in points.iter().enumerate() {
                let to = points.get(i + 1).map_or(end, |p| p.0);
                let Some(x) = fs.iter().find(|x| &x.id == a) else {
                    continue;
                };
                let kind = lane_kind(rules, &start, x, *at);
                let open = x.resets.iter().any(|r| r.is_some_and(|r| r > *at));
                let reset = prev_open == Some(true) && !open
                    || (i > 0
                        && x.used
                            .iter()
                            .zip(
                                points[i - 1]
                                    .1
                                    .iter()
                                    .find(|y| &y.id == a)
                                    .map_or([0.0; 2], |y| y.used),
                            )
                            .any(|(u, p)| *u < p));
                prev_open = Some(open);
                let left = (!x.api).then(|| standing::headroom(rules, &start, x, *at, false));
                match segs.last_mut() {
                    Some(l) if l.kind == kind && !reset => l.to = to,
                    _ => segs.push(LaneSeg {
                        from: *at,
                        to,
                        kind,
                        left,
                        reset,
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
