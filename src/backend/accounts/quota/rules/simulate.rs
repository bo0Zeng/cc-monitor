//! ★ 推演（算法 §5）：离散事件模拟，状态是 (Σ, F) 的一份拷贝；每个事件点调的是和真用时同一个 [`turn`]。
//! 每个号照它自己在额度账里量出来的用速涨（所有会话的用量都在那个号的读数里）；量不到的不涨、不换算时刻。
//! 事件只有事实（重置 · 被拒到期 · 用到 100）和策略约的叫醒（时刻 · 读数 —— 量得到用速的号按用速换算成时刻，量不到不编数）。

use super::decide::{turn, Pick, Sigma, Strategy};
use super::standing;
use super::{Acct, WINDOW_SECS};
use serde_json::Value;

/// 一次推演最多几个事件。
pub(crate) const MAX_EVENTS: usize = 200;

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
        // 下一个事件：已知节点 · 用到 100 · 读数叫醒（按用速换算成时刻）· 视窗末尾。
        let mut ev: Vec<u64> = vec![t1];
        ev.extend(d.tn);
        // 用速是每个号自己在额度账里量出来的（别的会话在用的号也量得到）：量得到的号照它涨、照它换算用到 100 · 读数叫醒的时刻；
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
                match x.rate[w] {
                    Some(r) if open && r > 0.0 => {
                        for n in ns {
                            let room = n - standing::used_at(x, w, t);
                            if room > 0.0 {
                                ev.push(t + ((room / r) * 60.0).ceil() as u64);
                            }
                        }
                    }
                    Some(_) => {}
                    None => unknown |= in_use,
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
            for w in 0..2 {
                if let (Some(r), true) = (x.rate[w], x.resets[w].is_some_and(|r| r > t)) {
                    x.used[w] = standing::used_at(x, w, t) + r * (te - t) as f64 / 60.0;
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
