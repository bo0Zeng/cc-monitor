//! ★ 推演的内核（原生，只此一处）：一份份额度 · 守着的人 · 池子最先作废先用 · 换号重写从额度里扣，推到视野末尾，
//! 交几份原始量（卡住的 · 一路换号重写的 · 每个号用掉的周上限卡着的额度 · 推完各份还剩多少），不收系数、不合成一个代价：
//! 怎么加权由策略定。积木库里挂成 `lib::project_raw`，`lib::project` 照它加起来。
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

/// 一份额度：数量（折合输入 token）· 几点起能用 · 几点作废 · 哪个号 · 周上限卡着 · 延迟切入的号。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub(crate) amt: f64,
    pub(crate) start: i64,
    pub(crate) exp: i64,
    pub(crate) who: String,
    pub(crate) weekly: bool,
    pub(crate) late: bool,
}

/// 一个守着号的别的会话：守在哪个号 · 那份在第几格（说不出 ⇒ 空：一开始就并进池子）· 用速（token / 分，未乘档位倍数）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Guard {
    pub(crate) acct: String,
    pub(crate) at: Option<usize>,
    pub(crate) rate: f64,
}

/// 一种走法：用速档位倍数 · 本会话要守的那份在第几格 · 本会话用速 · 别的会话也守在这个号上的用速 · 本会话守的号 ·
/// 现在换过去当场重写的量 · 整段重写的量 · 每分钟多出的重写量 · 本会话在各号上的缓存到几点 · 缓存留几秒 · 此刻 · 视野末尾。
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
}

/// 推完的几份原始量（折合输入 token）。`left` ＝ 推完还剩的每一份：号 · 剩多少 · 几点作废。
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

/// 进池子的那几条：时刻编码 · 用速 · 带进来要先付的重写 · 是不是本会话。
#[derive(Default)]
struct Joins {
    at: Vec<i64>,
    rho: Vec<f64>,
    lump: Vec<f64>,
    me: Vec<bool>,
}

impl Joins {
    fn push(&mut self, t: i64, rho: f64, lump: f64, me: bool) {
        self.at.push(t * SLOT + self.rho.len() as i64);
        self.rho.push(rho);
        self.lump.push(lump);
        self.me.push(me);
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

/// ★ 推一遍：别的会话各守着自己那份，用完或作废的那一刻并进池子；本会话守着 `w.at` 那份，用完再并进；池子照最先作废的那份用
/// （延迟切入的最后、还没开始的到点才进）。本会话并进池子以后跟着池子走：池子每换一个号本会话就换一次，缓存期内用过的号只补写
/// 新长的那段，否则整段重写；重写的量也从那个号的额度里出。没有一份能用的那几段，池子要的量整份算卡住。
pub(crate) fn rollout(pieces: &[Piece], others: &[Guard], w: &Walk) -> Raw {
    let mut amt: Vec<f64> = pieces.iter().map(|p| p.amt).collect();
    let now = w.now;
    let t_end = w.end;
    let mut cache = w.cache.clone();
    let mut out = Raw {
        rewrite: w.lump,
        ..Raw::default()
    };
    let mut ev = Joins::default();
    for g in others {
        let r = g.rate * w.m;
        if g.acct == w.mine || r <= 0.0 {
            continue;
        }
        let mut tt = now;
        if let Some(z) = g.at {
            let dt = hold_out(amt[z], r, pieces[z].exp, now);
            if pieces[z].weekly {
                out.spend_weekly(&pieces[z].who, r * dt / 60.0);
            }
            amt[z] = 0.0;
            tt = now + dt as i64;
        }
        if tt < t_end {
            ev.push(tt, r, 0.0, false);
        }
    }
    let rho = w.rho + w.share.unwrap_or(0.0);
    match w.at {
        Some(k) if rho > 0.0 => {
            let need = amt[k] - w.lump;
            if pieces[k].weekly {
                out.spend_weekly(&pieces[k].who, w.lump);
            }
            amt[k] = 0.0;
            if need <= 0.0 {
                ev.push(now, w.rho, -need, true);
            } else {
                let dt = hold_out(need, rho, pieces[k].exp, now);
                if pieces[k].weekly {
                    out.spend_weekly(&pieces[k].who, w.rho * dt / 60.0);
                }
                let t = now + dt as i64;
                if t < t_end {
                    ev.push(t, w.rho, 0.0, true);
                }
            }
        }
        _ => ev.push(now, w.rho, w.lump, true),
    }
    let mut evt = ev.at.clone();
    evt.sort_unstable();
    let key = |i: usize| (if pieces[i].late { LATE } else { 0 }) + pieces[i].exp * SLOT + i as i64;
    let mut avail: Vec<i64> = Vec::new();
    let mut pend: Vec<i64> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        if amt[i] > 0.0 && p.exp > now {
            if p.start > now {
                pend.push(p.start * SLOT + i as i64);
            } else {
                avail.push(key(i));
            }
        }
    }
    pend.sort_unstable();
    avail.sort_unstable();
    let at_of = |code: i64| (code % SLOT) as usize;
    let (mut d, mut due, mut tau) = (0.0_f64, 0.0_f64, now);
    let (mut ei, mut pi) = (0usize, 0usize);
    let mut me_in = false;
    let mut my = w.mine.clone();
    // 每一步至少挪掉一样（并进一条 · 进一份 · 用完或作废一份 · 到下一个时刻），封顶照份数与条数。
    let cap = 4 * pieces.len() + 4 * evt.len() + 16;
    let mut steps = 0usize;
    while tau < t_end && steps < cap {
        steps += 1;
        while ei < evt.len() && evt[ei] / SLOT <= tau {
            let q = at_of(evt[ei]);
            d += ev.rho[q];
            due += ev.lump[q];
            me_in |= ev.me[q];
            ei += 1;
        }
        let mut moved = false;
        while pi < pend.len() && pend[pi] / SLOT <= tau {
            avail.push(key(at_of(pend[pi])));
            pi += 1;
            moved = true;
        }
        if moved {
            avail.sort_unstable();
        }
        while let Some(&c) = avail.first() {
            let j = at_of(c);
            if amt[j] > 0.0 && pieces[j].exp > tau {
                break;
            }
            avail.remove(0);
        }
        let mut nxt = if ei < evt.len() {
            evt[ei] / SLOT
        } else {
            t_end
        };
        if pi < pend.len() && pend[pi] / SLOT < nxt {
            nxt = pend[pi] / SLOT;
        }
        if d <= 0.0 && due <= 0.0 {
            tau = nxt;
            continue;
        }
        let Some(&head) = avail.first() else {
            out.stuck += d * (nxt - tau) as f64 / 60.0 + due;
            due = 0.0;
            tau = nxt;
            continue;
        };
        let k = at_of(head);
        let who = &pieces[k].who;
        if me_in && *who != my {
            let r = match cache.get(who) {
                Some(&c) if c > tau => w.grow * (tau - now) as f64 / 60.0,
                _ => w.kfull,
            };
            out.rewrite += r;
            due += r;
            cache.insert(my.clone(), tau + w.ttl);
            my.clone_from(who);
        }
        if due > 0.0 {
            let take = due.min(amt[k]);
            amt[k] -= take;
            due -= take;
            if pieces[k].weekly {
                out.spend_weekly(who, take);
            }
            continue;
        }
        let mut end = pieces[k].exp.min(nxt);
        let e2 = tau + (amt[k] / d * 60.0).ceil() as i64;
        if e2 < end {
            end = e2;
        }
        if end <= tau {
            end = tau + 1;
        }
        let used = (d * (end - tau) as f64 / 60.0).min(amt[k]);
        amt[k] -= used;
        if pieces[k].weekly {
            out.spend_weekly(who, used);
        }
        tau = end;
    }
    out.left = pieces
        .iter()
        .zip(&amt)
        .filter(|(_, a)| **a > 0.0)
        .map(|(p, a)| (p.who.clone(), *a, p.exp))
        .collect();
    out
}

// ───────────── 挂进积木库（`lib::project_raw`）─────────────

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

/// 挂进积木库那一份模块：`lib::project_raw(amt, st, ex, who, wk, late, oev, o)`（参数同 `lib::project`）。
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
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/rollout_tests.rs"]
mod tests;
