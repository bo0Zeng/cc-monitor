//! 回放台架：拿一段真需求轨迹闭环跑策略（[`replay`]），量卡住 · 换号 · 作废。
//! 夹具 `night-1029.json`：10-29 22:00 到 10-30 03:10（PDT）本会话每分钟的用量与上下文（只抄数）· 那一刻各号的初态 ·
//! 策略管不着的事（别的会话压在号上的用速 · 被拒的时段 · 用户「现在用」）· 那一夜的真轮换日志。号名换成代号 a1 … a9。
//! 初态是倒推的：照轮换日志里每段在号上的时刻、10-30 03:13 的账（用量 · 重置）与每段本会话用了多少，拼成一份自洽的；
//! 「1% ≈ 多少」取让每段在号上的用量正好在日志那一刻打到上限的那个数。

use super::*;
use crate::accounts::quota::ledger::EST_FRESH as PACE_SPAN;
use crate::accounts::quota::rules::code;
use crate::accounts::quota::rules::decide::{use_now, Ctx, Me, PromptCache, Sigma, Strategy, Verdict};
use crate::accounts::quota::rules::Acct;
use crate::accounts::quota::setup::script::{self, File};
use crate::accounts::quota::strategy::Fail;
use serde_json::{json, Value};
use std::sync::OnceLock;

/// 回放的需求：本会话从 `t0` 起每分钟用了多少（折合输入 token）· 那一分钟的上下文（token）；
/// 写缓存那一档：比读缓存多几倍 · 留几秒。
#[derive(Debug, Clone)]
struct Trace {
    t0: u64,
    mins: Vec<(f64, u64)>,
    extra: f64,
    ttl: u64,
}

/// 本会话的用速看最近 [`PACE_SPAN`] 秒 · 刚开头这么久不给（同额度账那一本的口径）。
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
        // 短窗用速 · 在途并发 · 6 小时分段：录下来的轨迹里没有这三样 ⇒ 交「量不到」。
        Me {
            pace: pace.filter(|p| *p > 0.0),
            pace_short: None,
            inflight: None,
            bins: Vec::new(),
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
struct Load {
    acct: String,
    from: u64,
    to: u64,
    rate: f64,
}

/// 回放里策略管不着的事：别的会话的用量 · 号被拒的时段（号 · 起 · 止）· 用户「现在用」（时刻 · 号）。
#[derive(Debug, Clone)]
struct World {
    others: Vec<Load>,
    refused: Vec<(String, u64, u64)>,
    use_now: Vec<(u64, String)>,
}

/// 一次换号为什么换：在用的号用不了了（到上限 · 被拒）· 在用的号还能用、策略要提前换 · 用户「现在用」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchCause {
    Forced,
    Early,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
struct Switch {
    at: u64,
    from: String,
    to: String,
    cause: SwitchCause,
}

/// 一份额度作废：哪个号的哪一窗 · 几点 · 作废时还剩几点（100 − 用量）。
#[derive(Debug, Clone, PartialEq)]
struct Expired {
    acct: String,
    w: usize,
    at: u64,
    left: f64,
}

/// 回放结果：换号 · 停发的时段 · 卡住的需求（折合输入 token）· 作废的每一份 · 跑完时各号的事实。
#[derive(Debug, Clone)]
struct Replay {
    switches: Vec<Switch>,
    holds: Vec<(u64, u64)>,
    stuck: f64,
    expired: Vec<Expired>,
    end: Vec<Acct>,
}

/// ★ 回放：从 `from` 到 `to` 每分钟判一次（和真用同一个 [`turn`]），需求照 `trace` 而不是用速。
/// 在用的号这一分钟涨 ＝ 本会话这一分钟的用量 ÷ 它「1% ≈ 多少」（换过去那一下另加整段重写：写缓存倍数 × 上下文，
/// 那个号上的缓存还没过期就不加），用到 100 那一刻起多出来的算卡住；停发的那几分钟整份卡住。
/// 别的号照 [`rate_of`] 涨（本会话不在上面 ⇒ 只有别的会话那一份）；窗口关着、别的会话开始用 ⇒ 从那一刻开窗。
#[allow(clippy::too_many_arguments)]
fn replay(
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
        switches: Vec::new(),
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
            if let Ok(next) = use_now(&s, &f, t, a) {
                if next.current != s.current {
                    out.switches.push(Switch {
                        at: t,
                        from: s.current.clone(),
                        to: next.current.clone(),
                        cause: SwitchCause::Manual,
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
                    out.switches.push(Switch {
                        at: t,
                        from: prev.clone(),
                        to: a.clone(),
                        cause: if still {
                            SwitchCause::Early
                        } else {
                            SwitchCause::Forced
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
                        .filter_map(|w| {
                            x.per_pct[w].map(|k| (100.0 - x.used[w]).max(0.0) * k as f64)
                        })
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

const NIGHT: &str = include_str!("../../../../__fixtures__/rotation-night-1029.json");

/// 判据这一层的沙箱：跟进程里那一份同一个造法，只是不按墙钟掐（同 `decide_tests`）。
fn test_engine() -> &'static rhai::Engine {
    static E: OnceLock<rhai::Engine> = OnceLock::new();
    E.get_or_init(|| {
        let mut e = crate::accounts::quota::strategy::fresh_engine();
        e.on_progress(|_| None);
        code::with_lib(e).expect("积木库编得过")
    })
}

/// 编辑器那一份策略（用户那份「max额度轮换」的代码与它一字不差）。
struct Editor(File);

impl Strategy for Editor {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        self.0.code.decide_in(test_engine(), ctx)
    }
    fn time_varying(&self) -> bool {
        self.0.head.time_varying
    }
}

fn editor() -> Editor {
    Editor(
        script::parse(crate::accounts::quota::setup::editor::SOURCE).expect("编辑器那一份读得懂"),
    )
}

fn u(v: &Value) -> u64 {
    v.as_u64().expect("整数")
}

fn s(v: &Value) -> String {
    v.as_str().expect("字").to_string()
}

/// 夹具拼成回放的输入。
struct Night {
    sigma: Sigma,
    facts: Vec<Acct>,
    settings: Value,
    trace: Trace,
    world: World,
    from: u64,
    to: u64,
    tz: i64,
    log: Vec<Value>,
}

fn night() -> Night {
    let n: Value = serde_json::from_str(NIGHT).expect("夹具是 JSON");
    let facts = n["accounts"]
        .as_array()
        .expect("号")
        .iter()
        .map(|a| Acct {
            id: s(&a["id"]),
            api: false,
            used: [
                a["used"][0].as_f64().expect("用量"),
                a["used"][1].as_f64().expect("用量"),
            ],
            resets: [a["resets"][0].as_u64(), a["resets"][1].as_u64()],
            refused_until: None,
            barred: None,
            rate: [None, None],
            per_pct: [a["perPct"][0].as_u64(), a["perPct"][1].as_u64()],
            weekly: a["weekly"].as_bool().expect("周上限"),
            others: None,
            past: Default::default(),
        })
        .collect();
    let trace = Trace {
        t0: u(&n["t0"]),
        mins: n["demand"]
            .as_array()
            .expect("需求")
            .iter()
            .map(|m| (m[0].as_f64().expect("用量"), u(&m[1])))
            .collect(),
        extra: n["tier"]["extra"].as_f64().expect("写缓存倍数"),
        ttl: u(&n["tier"]["ttl"]),
    };
    let world = World {
        others: n["others"]
            .as_array()
            .expect("别的会话")
            .iter()
            .map(|o| Load {
                acct: s(&o["acct"]),
                from: u(&o["from"]),
                to: u(&o["to"]),
                rate: o["rate"].as_f64().expect("用速"),
            })
            .collect(),
        refused: n["refused"]
            .as_array()
            .expect("被拒")
            .iter()
            .map(|r| (s(&r["acct"]), u(&r["from"]), u(&r["to"])))
            .collect(),
        use_now: n["useNow"]
            .as_array()
            .expect("现在用")
            .iter()
            .map(|r| (u(&r["at"]), s(&r["acct"])))
            .collect(),
    };
    Night {
        sigma: Sigma {
            start: s(&n["start"]),
            current: s(&n["current"]),
            ..Sigma::default()
        },
        facts,
        settings: n["settings"].clone(),
        trace,
        world,
        from: u(&n["from"]),
        to: u(&n["to"]),
        tz: n["tzOffsetMin"].as_i64().expect("时区") * 60,
        log: n["log"].as_array().expect("日志").clone(),
    }
}

fn run(n: &Night, p: &dyn Strategy) -> Replay {
    let tz = n.tz;
    let clock = move |t: u64| ((t as i64 + tz).rem_euclid(86_400) / 60) as u16;
    replay(
        &n.sigma,
        &n.facts,
        &n.settings,
        p,
        &clock,
        &n.trace,
        &n.world,
        n.from,
        n.to,
    )
}

fn hhmm(t: u64, tz: i64) -> String {
    let m = (t as i64 + tz).rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// 三个数照实印出来（`--nocapture` 看）：卡住的 tok · 换号次数（主动 / 到上限 / 手动）· 每份作废时剩的点数。
fn report(n: &Night, r: &Replay) {
    let count = |c: SwitchCause| r.switches.iter().filter(|h| h.cause == c).count();
    eprintln!(
        "卡住 {:.2}M tok · 换号 {} 次（主动 {} · 到上限 {} · 手动 {}）· 停发 {} 段 · 5 小时窗作废 {} 份共剩 {:.1} 点",
        r.stuck / 1e6,
        r.switches.len(),
        count(SwitchCause::Early),
        count(SwitchCause::Forced),
        count(SwitchCause::Manual),
        r.holds.len(),
        r.expired.iter().filter(|e| e.w == 0).count(),
        r.expired
            .iter()
            .filter(|e| e.w == 0)
            .map(|e| e.left)
            .sum::<f64>(),
    );
    for h in &r.switches {
        eprintln!("  {} {} → {} {:?}", hhmm(h.at, n.tz), h.from, h.to, h.cause);
    }
    for (a, b) in &r.holds {
        eprintln!("  停发 {}–{}", hhmm(*a, n.tz), hhmm(*b, n.tz));
    }
    for e in &r.expired {
        eprintln!(
            "  作废 {} {} 窗{} 剩 {:.1}",
            hhmm(e.at, n.tz),
            e.acct,
            e.w,
            e.left
        );
    }
}

#[test]
fn the_editor_strategy_on_night_1029_switches_where_the_facts_forced_the_real_log_to() {
    let n = night();
    let r = run(&n, &editor());
    report(&n, &r);
    // 真日志 22:22–03:06 一共 10 条：换号 7 次（手动 1 · 到上限 5 · 智能提前 1）· 停发后恢复 3 次。
    let real = |k: &str| n.log.iter().filter(|e| e["kind"] == k).count();
    assert_eq!(
        (real("manual"), real("forced"), real("early"), real("hold")),
        (1, 5, 1, 3),
        "夹具里的真日志"
    );
    let ours = |c: SwitchCause| r.switches.iter().filter(|h| h.cause == c).count();
    // 由事实定的那几样逐样比：手动 · 停发对得上，到上限换允许 ±1。
    // 智能提前换不比：那一夜 02:10 挑号与 02:28 提前换都出自量歪了的号的用速（两次读数的斜率），
    // 回放里别的会话的用速是夹具给的真数，不再有那一抖。
    assert_eq!(ours(SwitchCause::Manual), real("manual"), "手动换号");
    assert_eq!(r.holds.len(), real("hold"), "停发几段");
    assert!(
        ours(SwitchCause::Forced).abs_diff(real("forced")) <= 1,
        "到上限换 {} 次，真日志 {}（允许 ±1）",
        ours(SwitchCause::Forced),
        real("forced")
    );
    // 头两次到上限换的号与真日志一样（那时只有一个号能用，换到哪由事实定）。
    let first: Vec<(&str, &str)> = r
        .switches
        .iter()
        .filter(|h| h.cause != SwitchCause::Manual)
        .take(2)
        .map(|h| (h.from.as_str(), h.to.as_str()))
        .collect();
    assert_eq!(first, vec![("a9", "a2"), ("a2", "a3")]);
}

/// 只认号列表先后的假策略：此刻发得出去的照列表排，在用的号发得出去就留着。
struct ByList;

impl Strategy for ByList {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        let mut order: Vec<String> = ctx
            .accounts
            .iter()
            .filter(|a| a.sendable)
            .map(|a| a.id.clone())
            .collect();
        if let Some(i) = order.iter().position(|a| a == ctx.current) {
            let c = order.remove(i);
            order.insert(0, c);
        }
        Ok(Verdict {
            order,
            ..Verdict::default()
        })
    }
}

fn one(id: &str, used: f64, resets: Option<u64>, per_pct: u64) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used, 0.0],
        resets: [resets, None],
        refused_until: None,
        barred: None,
        rate: [None, None],
        per_pct: [Some(per_pct), None],
        weekly: false,
        others: None,
        past: Default::default(),
    }
}

const T0: u64 = 1_800_000_000 / 86_400 * 86_400;

fn flat(tok: f64, mins: usize) -> Trace {
    Trace {
        t0: T0,
        mins: vec![(tok, 100_000); mins],
        extra: 1.0,
        ttl: 300,
    }
}

fn quiet() -> World {
    World {
        others: vec![],
        refused: vec![],
        use_now: vec![],
    }
}

fn on(id: &str) -> Sigma {
    Sigma {
        start: id.into(),
        current: id.into(),
        ..Sigma::default()
    }
}

#[test]
fn demand_past_the_last_account_is_stuck_and_counted() {
    // 一个号 1% ≈ 1 万、已用 90：每分钟 2 万 ⇒ 5 分钟用到 100，之后每分钟整份卡住。
    let facts = vec![one("A", 90.0, Some(T0 + 5 * 3600), 10_000)];
    let r = replay(
        &on("A"),
        &facts,
        &json!({}),
        &ByList,
        &|_| 0,
        &flat(20_000.0, 10),
        &quiet(),
        T0,
        T0 + 600,
    );
    assert!((r.stuck - 100_000.0).abs() < 1.0, "卡住 {}", r.stuck);
    assert!(r.switches.is_empty());
    assert_eq!(r.holds, vec![(T0 + 300, T0 + 600)]);
}

#[test]
fn a_switch_is_counted_by_why_and_charges_the_rewrite_to_the_new_account() {
    // A 用到 100 ⇒ 换到 B（到上限）；换过去那一下 B 多一份重写（上下文 10 万 × 1 ＝ 10 点，1% ≈ 1 万）。
    let facts = vec![
        one("A", 99.0, Some(T0 + 5 * 3600), 10_000),
        one("B", 0.0, None, 10_000),
    ];
    let r = replay(
        &on("A"),
        &facts,
        &json!({}),
        &ByList,
        &|_| 0,
        &flat(10_000.0, 3),
        &quiet(),
        T0,
        T0 + 180,
    );
    assert_eq!(
        r.switches,
        vec![Switch {
            at: T0 + 60,
            from: "A".into(),
            to: "B".into(),
            cause: SwitchCause::Forced
        }]
    );
    assert!(r.stuck.abs() < 1.0, "卡住 {}", r.stuck);
    // B：重写 10 点 ＋ 两分钟各 1 点。
    let b = r.end.iter().find(|x| x.id == "B").expect("B");
    assert!((b.used[0] - 12.0).abs() < 1e-6, "B 用到 {}", b.used[0]);
}

#[test]
fn a_window_that_expires_reports_what_was_left() {
    // A 开着、剩 30 点，一分钟后作废；本会话在 B 上（不碰 A）。
    let facts = vec![
        one("B", 0.0, Some(T0 + 5 * 3600), 10_000),
        one("A", 70.0, Some(T0 + 60), 10_000),
    ];
    let r = replay(
        &on("B"),
        &facts,
        &json!({}),
        &ByList,
        &|_| 0,
        &flat(1_000.0, 3),
        &quiet(),
        T0,
        T0 + 180,
    );
    assert_eq!(
        r.expired,
        vec![Expired {
            acct: "A".into(),
            w: 0,
            at: T0 + 60,
            left: 30.0
        }]
    );
}
