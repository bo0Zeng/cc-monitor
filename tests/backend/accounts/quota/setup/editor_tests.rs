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

/// ★ 编辑器那一份与算法定稿时的那一份对同一组事实排出同样的先后（rotsim 的几十判：提前换 · 不值得换 · 用满换号 ·
/// 延迟切入 · 强制切出 · 停发 · 手动选的 · 量不到用速 · 30 个号）。
#[test]
fn it_orders_the_same_as_the_algorithm_on_the_simulated_facts() {
    let all = cases();
    assert!(all.len() >= 30, "夹具够多：{}", all.len());
    let mut off = Vec::new();
    for c in &all {
        let v = decide_case(&c["ctx"], &c["ctx"]["settings"]);
        let want: Vec<String> = serde_json::from_value(c["order"].clone()).expect("order");
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

/// ★ 号多了一判也在操作数上限里判完、不走兜底：推演开销由算法自己封顶（推不起几遍就少比几个候选，一个都推不起就不提前换）。
/// 用速起伏大时低的那一档是 0，别的会话那一档不用、不并进池子（不拿 0 去除）。30 个号、没起伏时照推演提前换去快作废的 n00。
/// 推演在原生内核里（`lib::project` 一次调用一次操作）：15 个号 · 起伏 0.6 一判不到 3 万次操作；15 个号起直到 100 个号都推得起、比了候选。
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
        let (_, ops, calls) = probe(&c);
        eprintln!(
            "{n} 个号 · 起伏 {spread}：{ops} 次操作 · 推演 {} 遍",
            calls.len()
        );
        if n == 15 && spread > 0.0 {
            assert!(ops < 30_000, "15 个号 · 起伏 0.6 一判 {ops} 次操作");
        }
        if n >= 15 {
            assert!(
                !calls.is_empty(),
                "{n} 个号 · 起伏 {spread} 推不起、没比候选"
            );
        }
    }
    let c = many(30, 0.0);
    let v = decide_case(&c, &c["settings"]);
    assert_eq!(v.order.first().map(String::as_str), Some("n00"), "{v:?}");
    assert!(why_of(&v, "n00").starts_with("智能切换"), "{v:?}");
}

// ───────────── 推演在原生内核里：与 Rhai 那一版逐个相等 ─────────────

thread_local! {
    /// 量具那一份沙箱：这一判到此用了几次操作 · 每次喂给 `lib::project` 的八个参数。
    static OPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CALLS: std::cell::RefCell<Vec<Vec<rhai::Dynamic>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 量具：数操作数，并在 `lib::project` 开头录下它的八个参数（积木库别的不动）。
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
        let head = "fn project(amt, st, ex, who, wk, late, oev, o) {";
        assert!(code::LIB_SOURCE.contains(head), "积木库里有 project");
        let src = code::LIB_SOURCE.replacen(
            head,
            &format!("{head}\n  rec(amt, st, ex, who, wk, late, oev, o);"),
            1,
        );
        code::with_lib_from(e, &src).expect("积木库编得过")
    })
}

/// 在量具里判一次：结论 · 操作数 · 录下的每次推演的参数。
fn probe(c: &Value) -> (Verdict, u64, Vec<Vec<rhai::Dynamic>>) {
    OPS.with(|x| x.set(0));
    CALLS.with(|x| x.borrow_mut().clear());
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

/// ★ 夹具里每一判当时喂给 `lib::project` 的参数（录下来）·（加上合成的 15 · 30 个号）：Rhai 原文那一版与此刻的积木库代价逐个相等
/// （相对差 ＜ 1e-6）；排法也一样（见 [`it_orders_the_same_as_the_algorithm_on_the_simulated_facts`]）。
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
        let (_, _, calls) = probe(c);
        for args in calls {
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
