//! 编辑器那一份：读得懂 · 新建出来的还认得是它 · 改了代码就是手写；它的代码照声明的意思判（用量上限 · 切入 · 切出），在后端的一判里跑。

use super::*;
use crate::accounts::quota::rules::decide::{turn, Ctx, Decision, Pick, Sigma, Strategy, Verdict};
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
