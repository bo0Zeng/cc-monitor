//! 回放台架：拿一段真需求轨迹闭环跑策略（[`replay`]），量卡住 · 换号 · 作废。
//! 夹具 `night-1029.json`：10-29 22:00 到 10-30 03:10（PDT）本会话每分钟的用量与上下文（只抄数）· 那一刻各号的初态 ·
//! 策略管不着的事（别的会话压在号上的用速 · 被拒的时段 · 用户「现在用」）· 那一夜的真轮换日志。号名换成代号 a1 … a9。
//! 初态是倒推的：照轮换日志里每段在号上的时刻、10-30 03:13 的账（用量 · 重置）与每段本会话用了多少，拼成一份自洽的；
//! 「1% ≈ 多少」取让每段在号上的用量正好在日志那一刻打到上限的那个数。

use super::*;
use crate::accounts::quota::rules::code;
use crate::accounts::quota::rules::decide::{Ctx, Sigma, Strategy, Verdict};
use crate::accounts::quota::rules::Acct;
use crate::accounts::quota::setup::script::{self, File};
use crate::accounts::quota::strategy::Fail;
use serde_json::{json, Value};
use std::sync::OnceLock;

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
