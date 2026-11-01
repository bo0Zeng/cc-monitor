//! 编辑器那一份：读得懂 · 新建出来的还认得是它 · 改了代码就是手写；它的代码照声明的意思判（用量上限 · 切入 · 切出 · 智能切换），在后端的一判里跑。

use super::*;
use crate::accounts::quota::rules::decide::{
    turn, Ctx, CtxAcct, Decision, Me, Pick, PromptCache, Sigma, Strategy, Verdict,
};
use crate::accounts::quota::rules::{code, Acct};
use crate::accounts::quota::strategy::Fail;
use serde_json::{json, Value};
use std::sync::OnceLock;

const DAY: u64 = 1_800_000_000 / 86_400 * 86_400;
/// 此刻 ＝ 当天 14:30（UTC）。
const T: u64 = DAY + 14 * 3600 + 30 * 60;
const MIN: u64 = 60;

/// 判据这一层的沙箱：同进程里那一份的造法，不按墙钟掐（测的是运算量）。
fn test_engine() -> &'static rhai::Engine {
    static E: OnceLock<rhai::Engine> = OnceLock::new();
    E.get_or_init(|| {
        let mut e = crate::accounts::quota::strategy::fresh_engine();
        e.on_progress(|_| None);
        code::with_lib(e).expect("积木库编得过")
    })
}

struct Ed(script::File);

impl Strategy for Ed {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        self.0.code.decide_in(test_engine(), ctx)
    }
}

fn ed() -> &'static Ed {
    static F: OnceLock<Ed> = OnceLock::new();
    F.get_or_init(|| Ed(script::parse(SOURCE).expect("编辑器那一份读得懂")))
}

fn acct(id: &str, used5h: f64) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used5h, 0.0],
        resets: [Some(T + 3 * 3600), None],
        refused_until: None,
        barred: None,
        rate: [None, None],
        per_pct: [None, None],
        weekly: false,
        others: None,
        past: Default::default(),
    }
}

fn resets_in(mut a: Acct, min: u64) -> Acct {
    a.resets[0] = Some(T + min * MIN);
    a
}

/// 号列表 A B C，每号的设置照 `rows` 补。
fn settings(rows: &[(&str, Value)]) -> Value {
    let list: Vec<Value> = ["A", "B", "C"]
        .iter()
        .map(|id| {
            let mut r = json!({"id": id});
            for (k, v) in rows.iter().filter(|(k, _)| k.starts_with(id)) {
                r[&k[2..]] = v.clone();
            }
            r
        })
        .collect();
    json!({"accounts": list, "smart": true})
}

fn run(s: &Value, current: &str, memo: Option<Value>, f: &[Acct]) -> Decision {
    let sigma = Sigma {
        start: "A".into(),
        current: current.into(),
        memo,
        ..Sigma::default()
    };
    let d = turn(&sigma, f, s, ed(), ((T % 86_400) / 60) as u16, T, &[]);
    assert!(d.strategy_error.is_none(), "编辑器的代码跑出错：{d:?}");
    d
}

fn used(d: &Decision) -> &str {
    match &d.pick {
        Pick::Use(a) => a,
        Pick::Hold { .. } => panic!("该挑一个号，实得停发：{d:?}"),
    }
}

#[test]
fn reads_and_new_ones_are_its_own() {
    let e = script::parse(SOURCE).expect("读得懂");
    assert!(made_by(&e));
    assert!(
        e.about.contains("智能切换"),
        "说明写明智能切换此刻怎么走：{}",
        e.about
    );
    let src = fresh(
        &["work".into(), "team".into()],
        Some("Asia/Shanghai".into()),
    );
    let f = script::parse(&src).expect("新建的读得懂");
    assert!(made_by(&f), "新建的就是编辑器那一份");
    assert_eq!(f.about, e.about);
    assert_eq!(f.head.tz.as_deref(), Some("Asia/Shanghai"));
    assert_eq!(
        f.head.settings["accounts"],
        json!([{"id": "work"}, {"id": "team"}])
    );
    assert_eq!(f.head.settings["smart"], json!(true));
    let none = script::parse(&fresh(&[], None)).expect("读得懂");
    assert_eq!(none.head.settings["accounts"], json!([{"start": true}]));
    let hand = script::parse(&src.replace("90.0\n}", "80.0\n}")).expect("改过的也读得懂");
    assert!(!made_by(&hand), "代码改过 ⇒ 手写");
}

#[test]
fn default_keeps_the_current_until_its_cap_then_goes_by_the_list() {
    let s = settings(&[]);
    assert_eq!(
        used(&run(
            &s,
            "B",
            None,
            &[acct("A", 10.0), acct("B", 50.0), acct("C", 0.0)]
        )),
        "B"
    );
    // 没设上限 ⇒ 90：B 到 90 就换，照号牌是 A。
    assert_eq!(
        used(&run(
            &s,
            "B",
            None,
            &[acct("A", 10.0), acct("B", 90.0), acct("C", 0.0)]
        )),
        "A"
    );
    // 7 天没另设同 5 小时：A 的 5 小时上限 50，7 天用了 60 ⇒ A 也到了上限。
    let s = settings(&[("A.caps", json!({"5h": [{"from": 0, "to": 1440, "v": 50}]}))]);
    let mut a = acct("A", 10.0);
    a.weekly = true;
    a.used[1] = 60.0;
    assert_eq!(
        used(&run(&s, "B", None, &[a, acct("B", 95.0), acct("C", 0.0)])),
        "C"
    );
}

#[test]
fn force_in_takes_over_by_the_list() {
    let s = settings(&[("B.in", json!("force")), ("C.in", json!("force"))]);
    let f = [acct("A", 10.0), acct("B", 20.0), acct("C", 20.0)];
    assert_eq!(
        used(&run(&s, "A", None, &f)),
        "B",
        "在用默认切入的号 ⇒ 换到强制切入的"
    );
    assert_eq!(
        used(&run(&s, "C", None, &f)),
        "B",
        "两个强制切入的 ⇒ 号牌靠前的有额度就切回"
    );
}

#[test]
fn delay_in_waits_for_the_rest_to_run_out() {
    let s = settings(&[("B.in", json!("delay"))]);
    assert_eq!(
        used(&run(
            &s,
            "A",
            None,
            &[acct("A", 95.0), acct("B", 0.0), acct("C", 20.0)]
        )),
        "C"
    );
    let d = run(
        &s,
        "A",
        None,
        &[acct("A", 95.0), acct("B", 0.0), acct("C", 95.0)],
    );
    assert_eq!(used(&d), "B", "其余都用完 ⇒ 切到它");
    // 会话从它开、其余有额度 ⇒ 换走；其余都用完才切进来的 ⇒ 留着（切出是默认）。
    let f = [acct("A", 20.0), acct("B", 0.0), acct("C", 95.0)];
    assert_eq!(used(&run(&s, "B", None, &f)), "A");
    let memo = d.sigma.memo.clone();
    assert_eq!(used(&run(&s, "B", memo, &f)), "B");
    // 延迟 30 分钟：20 分钟后 A 重置 ⇒ 停到那时；40 分钟后才重置 ⇒ 马上切入。
    let s = settings(&[("B.in", json!("delay")), ("B.wait", json!(30))]);
    let soon = [
        resets_in(acct("A", 95.0), 20),
        acct("B", 0.0),
        acct("C", 95.0),
    ];
    assert!(
        matches!(run(&s, "A", None, &soon).pick, Pick::Hold { until: Some(t), .. } if t == T + 20 * MIN)
    );
    let late = [
        resets_in(acct("A", 95.0), 40),
        acct("B", 0.0),
        acct("C", 95.0),
    ];
    assert_eq!(used(&run(&s, "A", None, &late)), "B");
}

#[test]
fn force_out_leaves_once_the_rest_have_enough() {
    let base = [("B.in", json!("delay")), ("B.out", json!("force"))];
    let memo = Some(json!({"late": "B"}));
    let f = [acct("A", 20.0), acct("B", 0.0), acct("C", 95.0)];
    // 没设条件 ⇒ 其余合计 400 才走：A 只剩 70 ⇒ 留着。
    assert_eq!(used(&run(&settings(&base), "B", memo.clone(), &f)), "B");
    let mut any = base.to_vec();
    any.push(("B.outTotal", Value::Null));
    assert_eq!(
        used(&run(&settings(&any), "B", memo.clone(), &f)),
        "A",
        "一有额度 ⇒ 换走"
    );
    // 默认切入的号强制切出：其余合计到 50 ⇒ 换走，排到其余号后面。
    let s = settings(&[("A.out", json!("force")), ("A.outTotal", json!(50))]);
    let d = run(
        &s,
        "A",
        None,
        &[acct("A", 0.0), acct("B", 20.0), acct("C", 95.0)],
    );
    assert_eq!(used(&d), "B");
    assert_eq!(d.order, ["B", "A"]);
}

#[test]
fn a_zero_segment_stops_it_and_wakes_at_the_edge() {
    // A 在 14:00–16:00 是 0%：此刻用不了，16:00 那一刻再来问。
    let s = settings(&[(
        "A.caps",
        json!({"5h": [{"from": 0, "to": 840, "v": 90}, {"from": 840, "to": 960, "v": 0}, {"from": 960, "to": 1440, "v": 90}]}),
    )]);
    let d = run(
        &s,
        "A",
        None,
        &[acct("A", 10.0), acct("B", 10.0), acct("C", 10.0)],
    );
    assert_eq!(used(&d), "B");
    assert_eq!(d.wake_at, Some(DAY + 16 * 3600));
}

// ───────────── 智能切换 ─────────────

/// rotsim 跑智能切换那一份（算法定稿时的那份）留下的判：ctx 原样（号名换成了字母）＋ 那一份对它排出的先后。
const CASES: &str = include_str!("editor_smart_cases.json");

fn cases() -> Vec<Value> {
    serde_json::from_str(CASES).expect("夹具读得懂")
}

fn case(name: &str) -> Value {
    cases()
        .into_iter()
        .find(|c| c["case"] == name)
        .unwrap_or_else(|| panic!("夹具里有「{name}」"))
}

/// 一判的 ctx 照夹具拼回来，`settings` 换成给的那一份，调编辑器的代码。
fn decide_case(c: &Value, settings: &Value) -> Verdict {
    decide_case_in(test_engine(), c, settings)
}

/// 同 [`decide_case`]，在给的沙箱里判。
fn decide_case_in(e: &rhai::Engine, c: &Value, settings: &Value) -> Verdict {
    let pair = |v: &Value| [v["w5h"].as_f64(), v["w7d"].as_f64()];
    let int = |v: &Value| [v["w5h"].as_u64(), v["w7d"].as_u64()];
    let accounts: Vec<CtxAcct> = c["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| CtxAcct {
            id: a["id"].as_str().expect("id").into(),
            pos: a["pos"].as_u64().unwrap_or(0) as usize,
            api: a["kind"] == "api",
            sendable: a["sendable"] == true,
            used: pair(&a["used"]).map(|u| u.unwrap_or(0.0)),
            resets: int(&a["resets"]),
            refused_until: a["refusedUntil"].as_u64(),
            rate: pair(&a["rate"]),
            per_pct: int(&a["tokensPerPct"]),
            weekly: a["weekly"] == true,
            off: false,
            others: a["others"]["w5h"].as_f64(),
            past: Default::default(),
        })
        .collect();
    let s = &c["session"];
    let me = Me {
        pace: s["pace"].as_f64(),
        pace_short: s["paceShort"].as_f64(),
        inflight: s["inflight"].as_f64(),
        bins: s["bins"].as_array().map_or(Vec::new(), |b| {
            b.iter().map(serde_json::Value::as_f64).collect()
        }),
        spread: s["spread"].as_f64(),
        context: s["context"].as_u64(),
        write_extra: s["writeExtra"].as_f64(),
        ttl: s["ttl"].as_u64(),
        grow: s["grow"].as_f64(),
        cached: s["cached"].as_object().map_or(Vec::new(), |m| {
            m.iter()
                .map(|(k, v)| PromptCache {
                    account: k.clone(),
                    until: v["until"].as_u64().unwrap_or(0),
                    tokens: v["tokens"].as_u64().unwrap_or(0),
                })
                .collect()
        }),
    };
    let ctx = Ctx {
        now: c["now"].as_u64().expect("now"),
        clock: c["clock"].as_u64().expect("clock") as u16,
        start: "",
        current: c["current"].as_str().unwrap_or(""),
        settings,
        memo: c.get("memo").filter(|m| !m.is_null()),
        accounts,
        me: &me,
        pinned: c["pinned"] == true,
        temp: &[],
    };
    ed().0
        .code
        .decide_in(e, &ctx)
        .unwrap_or_else(|e| panic!("编辑器的代码跑出错（{}）：{e:?}", c["case"]))
}

fn why_of<'a>(v: &'a Verdict, id: &str) -> &'a str {
    v.why
        .iter()
        .find(|(a, _)| a == id)
        .map_or("", |(_, w)| w.as_str())
}

/// ★ 智能切换开着：额度紧时在用的 C 还剩不少、5 小时后才作废，B 剩 40%、半小时后作废 ⇒ 照推演当场换去 B；
/// 用满换号时也先用最先作废的（B），不照号牌（A）。
#[test]
fn smart_on_switches_early_when_it_pays_and_takes_the_first_to_expire() {
    let c = case("b · 场景 60 · 智能切换");
    let s = &c["ctx"]["settings"];
    let v = decide_case(&c["ctx"], s);
    assert_eq!(v.order.first().map(String::as_str), Some("B"), "{v:?}");
    assert!(why_of(&v, "B").starts_with("智能切换"), "{v:?}");
    let mut full = c["ctx"].clone();
    full["accounts"][2]["used"]["w5h"] = json!(95.0);
    let v = decide_case(&full, s);
    assert_eq!(
        v.order.first().map(String::as_str),
        Some("B"),
        "用满换号先用最先作废的：{v:?}"
    );
}

/// ★ 智能切换关着：同一组事实，在用的 C 没到上限就留着，其余照号牌；用满时照号牌换（A）。
#[test]
fn smart_off_keeps_the_current_and_goes_by_the_list() {
    let c = case("b · 场景 60 · 智能切换");
    let mut s = c["ctx"]["settings"].clone();
    s["smart"] = json!(false);
    let v = decide_case(&c["ctx"], &s);
    assert_eq!(v.order, ["C", "A", "B", "D", "E", "F", "G"]);
    let mut full = c["ctx"].clone();
    full["accounts"][2]["used"]["w5h"] = json!(95.0);
    let v = decide_case(&full, &s);
    assert_eq!(v.order.first().map(String::as_str), Some("A"), "{v:?}");
}

/// 夹具里改了期望的几判：照推演 ＋ 定价排出来的先后与「最先作废先用 ＋ 差 1 tok 就换」那一版不同，逐条写为什么。
/// 夹具是算法定稿时 rotsim 导出的一判一份（没有重打的路子），一份一行、十万字，期望照这张表改，不改夹具。
const CHANGED: &[(&str, &[&str], &str)] = &[
    (
        "h · 场景 42 · 智能切换",
        &["W", "C", "B"],
        "额度宽裕、哪种走法都不卡；C（20 分钟后作废）等不到 W 用完，是提前换的候选，但换过去不少卡（C 那 40 点作废不卡，不算代价）；\
         B 那份等得到 W 用完，用完再换过去一样用得上，不提前换 ⇒ 留着 W。旧的照最先作废提前换去 C，白换一次",
    ),
    (
        "h · 场景 43 · 智能切换",
        &["W", "C", "B"],
        "同场景 42（C 60 分钟后作废）：换去 C 不少卡，B 等得到，留着 W",
    ),
    (
        "h · 场景 48 · 智能切换",
        &["W", "C", "B"],
        "同场景 42：换去 C 不少卡，B 等得到，留着 W",
    ),
    (
        "h · 场景 54 · 智能切换",
        &["W", "B"],
        "额度宽裕：W 剩 60 点，够用到 A 4 小时后重置，不卡；换去 B（25 分钟后作废）视野里也是换两次，两种走法期望代价一样 ⇒ 不到门槛，留着。\
         旧的照最先作废提前换去 B，白换一次",
    ),
    (
        "h · 场景 55 · 智能切换",
        &["W", "B"],
        "同场景 54：留着与换去 B 期望代价一样，不换",
    ),
];

/// ★ 编辑器那一份与算法定稿时的那一份对同一组事实排出同样的先后（rotsim 的几十判：提前换 · 不值得换 · 用满换号 ·
/// 延迟切入 · 强制切出 · 停发 · 手动选的 · 量不到用速 · 30 个号），[`CHANGED`] 里那几判照它写的。
#[test]
fn it_orders_the_same_as_the_algorithm_on_the_simulated_facts() {
    let all = cases();
    assert!(all.len() >= 30, "夹具够多：{}", all.len());
    for (name, _, _) in CHANGED {
        assert!(
            all.iter().any(|c| c["case"] == *name),
            "改了期望的「{name}」在夹具里"
        );
    }
    let mut off = Vec::new();
    for c in &all {
        let v = decide_case(&c["ctx"], &c["ctx"]["settings"]);
        let want: Vec<String> = match CHANGED.iter().find(|(n, _, _)| c["case"] == *n) {
            Some((_, o, _)) => o.iter().map(|s| (*s).to_string()).collect(),
            None => serde_json::from_value(c["order"].clone()).expect("order"),
        };
        if v.order != want {
            off.push(format!("{}：得 {:?}，应 {:?}", c["case"], v.order, want));
        }
    }
    assert!(off.is_empty(), "排法不一样：\n{}", off.join("\n"));
}

/// 合成的 `n` 个号（号名不相干）：都开着窗、用了一半、重置时刻错开，除在用的 n03（剩得多、5 小时后重置）与 n05（半小时后作废）外
/// 都压着别的会话；本会话用速量得到，起伏 `spread`。
fn many(n: usize, spread: f64) -> Value {
    const T0: u64 = 1_791_907_200;
    let ids: Vec<String> = (0..n).map(|i| format!("n{i:02}")).collect();
    let accounts: Vec<Value> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let (u, r) = match i {
                3 => (20.0, T0 + 299 * 60),
                5 => (50.0, T0 + 30 * 60),
                _ => (50.0, T0 + (20 + (i as u64 * 37) % 280) * 60),
            };
            json!({"id": id, "pos": i, "kind": "sub", "sendable": true,
                   "used": {"w5h": u, "w7d": 0.0}, "resets": {"w5h": r, "w7d": T0 + 6 * 86_400},
                   "rate": {"w5h": 0.3}, "tokensPerPct": {"w5h": 150_000, "w7d": 150_000},
                   "others": {"w5h": if i == 3 || i == 5 { 0.0 } else { 0.3 }}, "weekly": false})
        })
        .collect();
    json!({"case": format!("{n} 个号 · 起伏 {spread}"), "now": T0, "clock": 540, "current": "n03",
           "settings": {"accounts": ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(), "smart": true},
           "accounts": accounts,
           "session": {"pace": 150_000.0, "spread": spread, "context": 150_000, "writeExtra": 1.9, "grow": 0.0, "cached": {}},
           "pinned": false})
}

/// ★ 号多了一判也在操作数上限里判完、不走兜底：一次推几种走法由策略照号数封顶（积木交回的原始量至多 6000 个值），
/// 定价的循环跟着原始量走。9 个号到 100 个号都推得起、比了候选（至少留着 ＋ 一个），一判不到上限（20 万）的一半。
/// 30 个号、没起伏、饱和时照推演提前换走（换去哪个看推演：别的会话都压着，不再是「最先作废」的 n00）。
#[test]
fn many_accounts_judge_within_the_limit() {
    for (n, spread) in [
        (9, 0.6),
        (15, 0.0),
        (15, 0.6),
        (30, 0.0),
        (30, 0.6),
        (50, 0.6),
        (100, 0.6),
    ] {
        let c = many(n, spread);
        let (_, ops, _) = probe(&c);
        let walks = WALKS.with(|w| w.take());
        eprintln!("{n} 个号 · 起伏 {spread}：{ops} 次操作 · 定价 {walks:?}（走法数, 路径数）");
        assert!(ops < 100_000, "{n} 个号 · 起伏 {spread} 一判 {ops} 次操作");
        assert!(
            walks.first().is_some_and(|(w, p)| *w >= 2 && *p >= 1),
            "{n} 个号 · 起伏 {spread} 推不起、没比候选：{walks:?}"
        );
    }
    let c = many(30, 0.0);
    let v = decide_case(&c, &c["settings"]);
    let first = v.order.first().cloned().unwrap_or_default();
    assert_ne!(first, "n03", "{v:?}");
    assert!(why_of(&v, &first).starts_with("智能切换"), "{v:?}");
}

// ───────────── 推演在原生内核里：与 Rhai 那一版逐个相等 ─────────────

thread_local! {
    /// 量具那一份沙箱：这一判到此用了几次操作 · 每次喂给 `lib::project` 的八个参数。
    static OPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CALLS: std::cell::RefCell<Vec<Vec<rhai::Dynamic>>> = const { std::cell::RefCell::new(Vec::new()) };
    /// 每次调 `lib::price` 时那一份原始量有几种走法 · 几条路径。
    static WALKS: std::cell::RefCell<Vec<(i64, i64)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 量具：数操作数，在 `lib::project` 开头录下它的八个参数、在 `lib::price` 开头录下几种走法几条路径（积木库别的不动）。
fn probe_engine() -> &'static rhai::Engine {
    static E: OnceLock<rhai::Engine> = OnceLock::new();
    E.get_or_init(|| {
        let mut e = crate::accounts::quota::strategy::fresh_engine();
        e.on_progress(|n| {
            OPS.with(|c| c.set(n));
            None
        });
        e.register_fn(
            "rec",
            |a: rhai::Array,
             b: rhai::Array,
             c: rhai::Array,
             d: rhai::Array,
             f: rhai::Array,
             g: rhai::Array,
             h: rhai::Map,
             o: rhai::Map| {
                let v: Vec<rhai::Dynamic> = vec![
                    a.into(),
                    b.into(),
                    c.into(),
                    d.into(),
                    f.into(),
                    g.into(),
                    h.into(),
                    o.into(),
                ];
                CALLS.with(|x| x.borrow_mut().push(v));
            },
        );
        e.register_fn("rec_walks", |w: i64, p: i64| {
            WALKS.with(|x| x.borrow_mut().push((w, p)));
        });
        let head = "fn project(amt, st, ex, who, wk, late, oev, o) {";
        assert!(code::LIB_SOURCE.contains(head), "积木库里有 project");
        let priced = "fn price(parts, ctx) {";
        assert!(code::LIB_SOURCE.contains(priced), "积木库里有 price");
        let src = code::LIB_SOURCE
            .replacen(
                head,
                &format!("{head}\n  rec(amt, st, ex, who, wk, late, oev, o);"),
                1,
            )
            .replacen(
                priced,
                &format!(
                    "{priced}\n  if parts != () {{ rec_walks(parts.runs.len(), parts.paths); }}"
                ),
                1,
            );
        code::with_lib_from(e, &src).expect("积木库编得过")
    })
}

/// 在量具里判一次：结论 · 操作数 · 录下的每次推演的参数。
fn probe(c: &Value) -> (Verdict, u64, Vec<Vec<rhai::Dynamic>>) {
    OPS.with(|x| x.set(0));
    CALLS.with(|x| x.borrow_mut().clear());
    WALKS.with(|x| x.borrow_mut().clear());
    let v = decide_case_in(probe_engine(), c, &c["settings"]);
    (v, OPS.with(std::cell::Cell::get), CALLS.with(|x| x.take()))
}

/// `lib::project` 用 Rhai 写的那一版（对照：同样的参数代价逐个相等）。
const RHAI_PROJECT: &str = r#"
fn project(amt, st, ex, who, wk, late, oev, o) {
  let now = o.now;
  let t_end = o.end;
  let cache = o.cache;
  let n = amt.len();
  let cost = o.lump;
  let evt = []; let erho = []; let elump = []; let eme = [];
  let q = 0;
  while q < oev.acc.len() {
    let r = oev.r[q] * o.m;
    if oev.acc[q] != o.mine && r > 0.0 {
      let z = oev.q[q];
      let tt = now;
      if z != () {
        let dt = amt[z] / r * 60.0;
        if ex[z] - now < dt { dt = (ex[z] - now).to_float(); }
        if wk[z] { cost += r * dt / 60.0; }
        amt[z] = 0.0;
        tt = now + dt.to_int();
      }
      if tt < t_end { evt.push(tt * 1024 + erho.len()); erho.push(r); elump.push(0.0); eme.push(false); }
    }
    q += 1;
  }
  let k = o.at;
  let rho = o.rho;
  if o.share != () { rho += o.share; }
  if k == () || rho <= 0.0 { evt.push(now * 1024 + erho.len()); erho.push(o.rho); elump.push(o.lump); eme.push(true); }
  else {
    let need = amt[k] - o.lump;
    if wk[k] { cost += o.lump; }
    if need <= 0.0 { amt[k] = 0.0; evt.push(now * 1024 + erho.len()); erho.push(o.rho); elump.push(-need); eme.push(true); }
    else {
      let dt = need / rho * 60.0;
      if ex[k] - now < dt { dt = (ex[k] - now).to_float(); }
      if wk[k] { cost += o.rho * dt / 60.0; }
      amt[k] = 0.0;
      let t = now + dt.to_int();
      if t < t_end { evt.push(t * 1024 + erho.len()); erho.push(o.rho); elump.push(0.0); eme.push(true); }
    }
  }
  evt.sort();
  let avail = []; let pend = [];
  let i = 0;
  while i < n {
    if amt[i] > 0.0 && ex[i] > now {
      if st[i] > now { pend.push(st[i] * 1024 + i); } else { avail.push((if late[i] { 4000000000000000 } else { 0 }) + ex[i] * 1024 + i); }
    }
    i += 1;
  }
  pend.sort();
  avail.sort();
  let d = 0.0; let due = 0.0; let tau = now; let ei = 0; let pi = 0;
  let me_in = false; let my = o.mine; let guard = 0;
  while tau < t_end && guard < 4 * n + 4 * evt.len() + 16 {
    guard += 1;
    while ei < evt.len() && evt[ei] / 1024 <= tau {
      let q = evt[ei] % 1024;
      d += erho[q]; due += elump[q];
      if eme[q] { me_in = true; }
      ei += 1;
    }
    let moved = false;
    while pi < pend.len() && pend[pi] / 1024 <= tau {
      let j = pend[pi] % 1024;
      avail.push((if late[j] { 4000000000000000 } else { 0 }) + ex[j] * 1024 + j);
      pi += 1; moved = true;
    }
    if moved { avail.sort(); }
    while avail.len() > 0 && (amt[avail[0] % 1024] <= 0.0 || ex[avail[0] % 1024] <= tau) { avail.remove(0); }
    let nxt = if ei < evt.len() { evt[ei] / 1024 } else { t_end };
    if pi < pend.len() && pend[pi] / 1024 < nxt { nxt = pend[pi] / 1024; }
    if d <= 0.0 && due <= 0.0 { tau = nxt; continue; }
    if avail.len() == 0 { cost += d * (nxt - tau).to_float() / 60.0 + due; due = 0.0; tau = nxt; continue; }
    let k = avail[0] % 1024;
    if me_in && who[k] != my {
      let c = cache[who[k]];
      let r = if c != () && c > tau { o.grow * (tau - now).to_float() / 60.0 } else { o.kfull };
      cost += r; due += r;
      cache[my] = tau + o.ttl;
      my = who[k];
    }
    if due > 0.0 {
      let take = if due < amt[k] { due } else { amt[k] };
      amt[k] -= take; due -= take;
      if wk[k] { cost += take; }
      continue;
    }
    let end = if ex[k] < nxt { ex[k] } else { nxt };
    let e2 = tau + (amt[k] / d * 60.0).ceiling().to_int();
    if e2 < end { end = e2; }
    if end <= tau { end = tau + 1; }
    let used = d * (end - tau).to_float() / 60.0;
    if used > amt[k] { used = amt[k]; }
    amt[k] -= used;
    if wk[k] { cost += used; }
    tau = end;
  }
  cost
}
"#;

/// 一组参数各调一遍：Rhai 原文那一版 · 积木库此刻的 `lib::project`。
fn both(args: &[rhai::Dynamic]) -> (f64, f64) {
    static OLD: OnceLock<(rhai::Engine, rhai::AST)> = OnceLock::new();
    static NEW: OnceLock<rhai::AST> = OnceLock::new();
    let (oe, oa) = OLD.get_or_init(|| {
        let mut e = crate::accounts::quota::strategy::fresh_engine();
        e.on_progress(|_| None);
        e.set_max_operations(0);
        let a = e.compile(RHAI_PROJECT).expect("对照那一版编得过");
        (e, a)
    });
    let na = NEW.get_or_init(|| {
        test_engine()
            .compile("fn via_lib(a, b, c, d, e, f, g, h) { lib::project(a, b, c, d, e, f, g, h) }")
            .expect("编得过")
    });
    let call = |e: &rhai::Engine, ast: &rhai::AST, name: &str| {
        let a: Vec<rhai::Dynamic> = args.to_vec();
        let r = e
            .call_fn::<rhai::Dynamic>(
                &mut rhai::Scope::new(),
                ast,
                name,
                (
                    a[0].clone(),
                    a[1].clone(),
                    a[2].clone(),
                    a[3].clone(),
                    a[4].clone(),
                    a[5].clone(),
                    a[6].clone(),
                    a[7].clone(),
                ),
            )
            .unwrap_or_else(|x| panic!("{name} 跑出错：{x}"));
        r.as_float()
            .unwrap_or_else(|_| panic!("{name} 交的不是数：{r:?}"))
    };
    (call(oe, oa, "project"), call(test_engine(), na, "via_lib"))
}

/// 照旧签名调 `lib::project` 的那种写法（用户的策略文件里还这样调）：每号此刻那份 ＋ `refills` 补的几份按列放，别的会话进 `oev`，
/// 留着与换到每一个号各推一遍、每档用速（`pace_bands`）一次。上限一律 90。
const PACK: &str = r#"
fn pack(ctx) {
  let now = ctx.now; let clock = ctx.clock; let cur = ctx.current; let sess = ctx.session;
  let horizon = 2 * 18000;
  let late = #{};
  for r in ctx.settings.accounts { if r.id != () && r["in"] == "delay" { late[r.id] = true; } }
  let pa = []; let pst = []; let pex = []; let pid = []; let pwk = []; let plt = [];
  let tpp = #{}; let oth = #{};
  for a in ctx.accounts {
    if a.tokensPerPct == () || a.tokensPerPct.w5h == () { continue; }
    let tk = a.tokensPerPct.w5h.to_float();
    tpp[a.id] = tk;
    oth[a.id] = if a.others != () && a.others.w5h != () { a.others.w5h.to_float() } else { () };
    let q = lib::piece(a, 90.0, 90.0, now);
    let start = now;
    if a.refusedUntil != () && a.refusedUntil > now { start = a.refusedUntil; }
    if a.sendable != true && start == now { continue; }
    let left = if q.amt > 0.0 { q.amt } else { 0.0 };
    let lt = late[a.id] == true;
    let e = if q.exp == () { now + 2 * horizon } else { q.exp };
    pa.push(left * tk); pst.push(start); pex.push(e); pid.push(a.id); pwk.push(q.wk); plt.push(lt);
    for x in lib::refills(a.resets.w5h, if q.l7 != () { q.l7 - left } else { () }, q.wk, e, (), 90.0, clock, now, horizon) {
      pa.push(x[0] * tk); pst.push(x[1]); pex.push(x[2]); pid.push(a.id); pwk.push(q.wk); plt.push(lt);
    }
  }
  let at = #{};
  let k = 0;
  while k < pa.len() { if pst[k] <= now && at[pid[k]] == () { at[pid[k]] = k; } k += 1; }
  let full = lib::write_extra(sess) * (if sess.context != () { sess.context.to_float() } else { 0.0 });
  let grow = lib::write_extra(sess) * (if sess.grow != () { sess.grow.to_float() } else { 0.0 });
  let ttl = if sess.ttl != () { sess.ttl } else { 3600 };
  let cache = #{};
  if sess.cached != () { for c in sess.cached.keys() { let u = sess.cached[c]["until"]; if u != () { cache[c] = u; } } }
  cache[cur] = now + ttl;
  let oev = #{ acc: [], q: [], r: [] };
  for id in tpp.keys() {
    let o = oth[id];
    if o == () || o <= 0.0 || id == cur { continue; }
    oev.acc.push(id); oev.q.push(at[id]); oev.r.push(o * tpp[id]);
  }
  let pace = if sess.pace != () { sess.pace.to_float() } else { return; };
  for to in at.keys() {
    let lump = if to == cur { 0.0 } else { lib::switch_cost(sess, to, now) };
    for b in lib::pace_bands(sess.spread) {
      let share = if to != cur && oth[to] != () && oth[to] > 0.0 { oth[to] * tpp[to] * b[0] } else { () };
      lib::project(pa, pst, pex, pid, pwk, plt, oev,
        #{ m: b[0], at: at[to], rho: pace * b[0], share: share, mine: to, lump: lump, kfull: full, grow: grow, cache: cache, ttl: ttl, now: now, end: now + horizon });
    }
  }
}
"#;

/// 在量具里照 [`PACK`] 那种写法调 `lib::project`，回录下的每次参数。
fn packed_calls(c: &Value) -> Vec<Vec<rhai::Dynamic>> {
    static AST: OnceLock<rhai::AST> = OnceLock::new();
    let ast = AST.get_or_init(|| probe_engine().compile(PACK).expect("编得过"));
    CALLS.with(|x| x.borrow_mut().clear());
    let ctx = rhai::serde::to_dynamic(c).expect("ctx");
    probe_engine()
        .call_fn::<rhai::Dynamic>(&mut rhai::Scope::new(), ast, "pack", (ctx,))
        .unwrap_or_else(|e| panic!("跑出错（{}）：{e}", c["case"]));
    CALLS.with(|x| x.take())
}

/// ★ 照旧签名调 `lib::project` 的写法（[`PACK`]）对夹具里每一判 ·（加上合成的 15 · 30 个号）喂的参数：Rhai 原文那一版与此刻的积木库
/// 代价逐个相等（相对差 ＜ 1e-6）。编辑器自己改调 `lib::rollout`，`lib::project` 留给用户的策略文件。
#[test]
fn project_costs_the_same_as_the_rhai_version_on_every_recorded_call() {
    let mut inputs: Vec<(String, Value)> = cases()
        .into_iter()
        .map(|c| {
            (
                c["case"].as_str().unwrap_or("").to_string(),
                c["ctx"].clone(),
            )
        })
        .collect();
    for (n, spread) in [(15, 0.6), (30, 0.0), (30, 0.6)] {
        let c = many(n, spread);
        inputs.push((c["case"].as_str().unwrap_or("").to_string(), c));
    }
    let mut seen = 0;
    let mut off = Vec::new();
    for (name, c) in &inputs {
        for args in packed_calls(c) {
            seen += 1;
            let (old, new) = both(&args);
            if (old - new).abs() > 1e-6 * old.abs().max(1.0) {
                off.push(format!("{name}：Rhai {old}，此刻 {new}"));
            }
        }
    }
    eprintln!("录下的推演 {seen} 遍，逐个比过");
    assert!(seen >= 20, "录下的推演够多：{seen}");
    assert!(off.is_empty(), "代价不一样：\n{}", off.join("\n"));
}

// ───────────── 定价积木 `lib::price`：照 `lib::rollout` 交的原始量算代价 ─────────────

/// ★ 周上限兜底的号到周重置也用不完 ⇒ 它的周额度不贵：在用的 q 剩 40 点、4 小时后作废；y 剩 5 点、30 分钟后作废，之后这一天不再给
/// （分段上限到 15:00 为止）；q 也到 18:30 为止；池子最后靠 b（没开窗，周剩 62 点、18.6 小时后周重置，1% ≈ 150 万）。
/// 「换去 y」只省下 b 那 30 分钟的周额度：周额度按全价算（卡住 ＋ 重写 ＋ 周额度）差几十万、看着该换；
/// 照 `lib::price` 定价，b 的周额度影子价是 0，换过去只多一次换号代价，差 ＜ ε·K。
#[test]
fn price_does_not_pay_to_switch_when_the_weekly_backstop_cannot_be_used_up() {
    let src = format!(
        r#"
        let now = {T};
        let none = #{{ w5h: (), w7d: () }};
        let ctx = #{{ now: now, clock: 870, current: "q", start: "q",
          accounts: [
            #{{ id: "q", kind: "sub", sendable: true, used: #{{ w5h: 60.0, w7d: 0.0 }}, resets: #{{ w5h: now + 14400, w7d: () }},
               refusedUntil: (), tokensPerPct: #{{ w5h: 100000, w7d: () }}, weekly: false, others: (), windowGrid: #{{ up: () }} }},
            #{{ id: "y", kind: "sub", sendable: true, used: #{{ w5h: 95.0, w7d: 0.0 }}, resets: #{{ w5h: now + 1800, w7d: () }},
               refusedUntil: (), tokensPerPct: #{{ w5h: 1500000, w7d: () }}, weekly: false, others: (), windowGrid: #{{ up: () }} }},
            #{{ id: "b", kind: "sub", sendable: true, used: #{{ w5h: 0.0, w7d: 38.0 }}, resets: #{{ w5h: (), w7d: now + 66960 }},
               refusedUntil: (), tokensPerPct: #{{ w5h: 100000, w7d: 1500000 }}, weekly: true, others: (), windowGrid: #{{ up: () }} }} ],
          session: #{{ pace: 25000.0, paceShort: 25000.0, inflight: 1.0, bins: [], spread: (), context: 150000,
                      writeExtra: 1.9, ttl: 3600, grow: 0.0, cached: #{{}} }} }};
        for i in 0..36 {{ ctx.session.bins.push(250000.0); }}
        let plan = #{{ caps: #{{ q: #{{ "5h": [#{{ from: 0, to: 1110, v: 100 }}], "7d": 100 }},
                                y: #{{ "5h": [#{{ from: 0, to: 900, v: 100 }}], "7d": 100 }},
                                b: #{{ "5h": 100, "7d": 100 }} }},
                      dflt: 0, walks: ["q", "y"], paths: 8 }};
        let parts = lib::rollout(ctx, plan);
        let flat = [];
        for runs in parts.runs {{
          let r = runs[0];
          let c = r.stuck + r.rewrite;
          for x in r.weekly.values() {{ c += x; }}
          flat.push(c);
        }}
        let p = lib::price(parts, ctx);
        [flat[0], flat[1], p[0].j, p[1].j, p[1].diff, parts.paths]
    "#
    );
    let v: rhai::Array = test_engine().eval(&src).expect("跑得通");
    let f = |i: usize| v[i].as_float().unwrap_or(f64::NAN);
    let k = 1.9 * 150_000.0;
    let eps_k = 0.1 * k;
    assert_eq!(
        v[5].as_int().ok(),
        Some(1),
        "没有起伏、没有别的会话 ⇒ 一条路径就够"
    );
    assert!(
        f(0) - f(1) > eps_k,
        "周额度按全价算时看着该换：留着 {} · 换去 y {}",
        f(0),
        f(1)
    );
    assert!(
        f(2) - f(3) < eps_k,
        "照 lib::price 换过去省下的不到 ε·K：留着 {} · 换去 y {}",
        f(2),
        f(3)
    );
    assert!(
        (f(4) - (f(3) - f(2))).abs() < 1e-6 * k,
        "diff ＝ 这一走法 − 留着"
    );
}

// ───────────── 编辑器的决策规则：推演 ＋ 定价 ─────────────

/// 此刻（02:10 那一判）：一天里第几分钟 130。
const T1: u64 = 1_791_907_200;

/// 一个号（夹具的写法）：5 小时用量 · 离重置几分钟（空 ⇒ 窗关着）· 别的会话压在它上面的用速（点 / 分）。
fn fact(id: &str, pos: usize, used: f64, resets_in: Option<u64>, others: f64) -> Value {
    json!({"id": id, "pos": pos, "kind": "sub", "sendable": true,
           "used": {"w5h": used, "w7d": 0.0},
           "resets": {"w5h": resets_in.map(|m| T1 + m * 60), "w7d": T1 + 6 * 86_400},
           "rate": {"w5h": 0.3}, "tokensPerPct": {"w5h": 150_000, "w7d": 150_000},
           "others": {"w5h": others}, "weekly": false})
}

/// 一判的 ctx：号列表照 `accounts` 的先后，本会话用速 `pace`（折合输入 token / 分）· 10 分钟一格的用量 `bins`。
fn ctx_of(current: &str, accounts: Vec<Value>, pace: f64, bins: Vec<f64>) -> Value {
    let list: Vec<Value> = accounts.iter().map(|a| json!({"id": a["id"]})).collect();
    json!({"case": "合成", "now": T1, "clock": 130, "current": current,
           "settings": {"accounts": list, "smart": true},
           "accounts": accounts,
           "session": {"pace": pace, "paceShort": pace, "inflight": 1.0, "bins": bins, "spread": 0.0,
                       "context": 150_000, "writeExtra": 1.9, "ttl": 3600, "grow": 0.0,
                       "cached": {current: {"until": T1 + 1800, "tokens": 150_000}}},
           "pinned": false})
}

/// 反例 A 那一夜的号：j 用到上限（90）、50 分钟后重置；y 剩 9 点、190 分钟后（05:20）作废，别的会话在它上面量成 0.5 点 / 分（真值 0）；
/// r 剩 49 点、300 分钟后（07:10）作废。本会话饱和：每分钟 30 万，号都供不上。
fn night_a(j_used: f64) -> Value {
    ctx_of(
        "j",
        vec![
            fact("j", 0, j_used, Some(50), 0.0),
            fact("y", 1, 81.0, Some(190), 0.5),
            fact("r", 2, 41.0, Some(300), 0.0),
        ],
        300_000.0,
        vec![3_000_000.0; 36],
    )
}

/// ★ 反例 A：j 到上限要换号。y 剩的 9 点 05:20 作废、r 剩的 49 点 07:10 作废；本会话 24 分钟用完 r 再用 y，y 也赶得上，
/// 两种走法推出来只差一两成重写（一起守 y 的别的会话在 y 用完后照它们的用速并进池子，用量照算）⇒ 差不过门槛，用最先作废的。
/// 量到 y 上别的会话 0.5 点 / 分：到 05:20 会把 y 用完，最先作废的是 r ⇒ r；量到 0（真值）：最先作废的是 y ⇒ y。
/// 量歪了挑 r 也不亏：y 照样在作废前用上。why 写推演的数。
#[test]
fn a_used_up_account_hands_over_to_the_first_to_expire_when_the_rollout_cannot_tell_them_apart() {
    let c = night_a(90.0);
    let v = decide_case(&c, &c["settings"]);
    assert_eq!(v.order.first().map(String::as_str), Some("r"), "{v:?}");
    assert!(why_of(&v, "y").contains("万"), "why 写数：{v:?}");
    let mut c = night_a(90.0);
    c["accounts"][1]["others"]["w5h"] = json!(0.0);
    let v = decide_case(&c, &c["settings"]);
    assert_eq!(v.order.first().map(String::as_str), Some("y"), "{v:?}");
}

/// ★ 要换号时跟最先作废的那个比，同提前换一道门槛：反例 A 那组号、y 上别的会话量成 0（y 最先作废，是推演的基准），
/// 换号代价 K ＝ 1.9 × 15 万，ε·K ＝ 2.85 万。r 比 y 少 2 万、没有误差 ⇒ 不过门槛，用 y；少 10 万、标准误 0.1 万 ⇒ 用 r。
#[test]
fn a_forced_switch_takes_the_first_to_expire_unless_another_beats_it_past_the_gate() {
    let mut c = night_a(90.0);
    c["accounts"][1]["others"]["w5h"] = json!(0.0);
    let v = decide_priced(&c, 20_000.0, 0.0);
    assert_eq!(v.order.first().map(String::as_str), Some("y"), "{v:?}");
    let v = decide_priced(&c, 100_000.0, 1_000.0);
    assert_eq!(v.order.first().map(String::as_str), Some("r"), "{v:?}");
}

/// ★ 提前换只换去「等不到在用的号用完」的那份：j 用了 50，剩 40 点 × 15 万 ÷ 每分钟 30 万 ＝ 20 分钟用完。
/// 假定价说换去哪个都少 10 万、标准误 0.1 万（过了门槛）：
/// y 上别的会话 0.5 点 / 分，20 分钟用掉 10 点，y 剩的 9 点等不到 ⇒ 换去 y；
/// 量成 0 ⇒ y（190 分钟后作废）· r（300 分钟后）都等得到 j 用完，用完再换过去一样用得上 ⇒ 留着 j，不比。
#[test]
fn an_early_switch_only_goes_to_a_share_that_would_be_lost_before_the_current_runs_out() {
    let c = night_a(50.0);
    let v = decide_priced(&c, 100_000.0, 1_000.0);
    assert_eq!(v.order.first().map(String::as_str), Some("y"), "{v:?}");
    let mut c = night_a(50.0);
    c["accounts"][1]["others"]["w5h"] = json!(0.0);
    let v = decide_priced(&c, 100_000.0, 1_000.0);
    assert_eq!(v.order.first().map(String::as_str), Some("j"), "{v:?}");
    assert_eq!(why_of(&v, "j"), "未达用量上限，继续使用", "{v:?}");
}

/// 同 [`decide_case`]，`lib::price` 换成一份假定价：留着那一种期望代价 1 亿、差 0；别的每一种都比留着少 `gain`、标准误 `se`。
fn decide_priced(c: &Value, gain: f64, se: f64) -> Verdict {
    let head = "fn price(parts, ctx) {";
    assert!(code::LIB_SOURCE.contains(head), "积木库里有 price");
    let fake = format!(
        "{head}\n  let out = [];\n  let b = if parts.base != () {{ parts.base }} else {{ 0 }};\n  \
         for i in 0..parts.runs.len() {{ if i == b {{ out.push(#{{ j: 1e8, diff: 0.0, se: 0.0 }}); }} \
         else {{ out.push(#{{ j: 1e8 - {gain:.1}, diff: -{gain:.1}, se: {se:.1} }}); }} }}\n  return out;"
    );
    let mut e = crate::accounts::quota::strategy::fresh_engine();
    e.on_progress(|_| None);
    let e = code::with_lib_from(e, &code::LIB_SOURCE.replacen(head, &fake, 1)).expect("编得过");
    decide_case_in(&e, c, &c["settings"])
}

/// ★ 换的条件是 E[J_留] − E[J_换] ＞ max(ε·K, 2 × 标准误)：反例 A 那组号、j 还能用（用了 50），换号代价 K ＝ 1.9 × 15 万 ＝ 28.5 万，
/// ε·K ＝ 2.85 万。少 10 万、标准误 20 万（差在误差里）⇒ 留着；少 10 万、标准误 0.1 万 ⇒ 换去 y（等不到 j 用完的只有它）；
/// 少 2 万（不到 ε·K）、没有误差 ⇒ 留着。
#[test]
fn a_gain_inside_the_noise_or_under_a_tenth_of_the_switch_cost_keeps_the_current() {
    let c = night_a(50.0);
    let first = |gain: f64, se: f64| {
        let v = decide_priced(&c, gain, se);
        (
            v.order.first().cloned().unwrap_or_default(),
            why_of(&v, "j").to_string(),
            v,
        )
    };
    let (who, why, v) = first(100_000.0, 200_000.0);
    assert_eq!(who, "j", "差在两倍标准误以内不换：{v:?}");
    assert!(why.contains("误差"), "{v:?}");
    let (who, _, v) = first(100_000.0, 1_000.0);
    assert_eq!(who, "y", "差过了门槛就换：{v:?}");
    assert!(why_of(&v, "y").starts_with("智能切换"), "{v:?}");
    let (who, why, v) = first(20_000.0, 0.0);
    assert_eq!(who, "j", "不到换号代价的一成不换：{v:?}");
    assert!(why.contains("一成"), "{v:?}");
}

/// ★ 反例 C：两号都空着（5 小时窗都没在计时），本会话此刻用得少，用量记录里猛涨一来就是几个钟头（猛涨时每分钟 60 万，两号各 20 来分钟打满）。
/// 此刻给 an 开窗，猛涨来时它这一窗已经过了一截、早重置早再来一份；等用到才开就晚一截 ⇒ 交回 warm: ["an"]。在用的 q 不在里面。
#[test]
fn an_idle_window_is_warmed_when_later_bursts_would_be_stuck() {
    let mut bins = vec![100_000.0; 19];
    bins.extend(vec![6_000_000.0; 17]);
    let mut c = ctx_of(
        "q",
        vec![fact("q", 0, 0.0, None, 0.0), fact("an", 1, 0.0, None, 0.0)],
        1_000.0,
        bins,
    );
    c["session"]["paceShort"] = json!(1_000.0);
    c["session"]["inflight"] = json!(0.0);
    let v = decide_case(&c, &c["settings"]);
    assert_eq!(v.warm, ["an"], "{v:?}");
    assert!(why_of(&v, "an").starts_with("开窗"), "{v:?}");
    assert_eq!(v.order.first().map(String::as_str), Some("q"), "{v:?}");
}
