//! 策略文件与沙箱：头的读写不碰代码 · 沙箱掐得住 · 回值照形状收 · 随包三份都读得懂、后端不认它们的名字。

use super::*;
use crate::accounts::quota::rules::decide::{turn, Ctx, CtxAcct, Pick, Sigma, Strategy};
use crate::accounts::quota::rules::Acct;
use crate::accounts::quota::strategy::FailKind;

fn shipped_dir() -> std::path::PathBuf {
    crate::guard_support::repo_root().join("src/shared/rotation/strategies")
}

fn shipped() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = guard_core::scan_tree!(&shipped_dir(), &["rhai"])
        .into_iter()
        .map(|(p, t)| {
            (
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string(),
                t,
            )
        })
        .collect();
    v.sort();
    v
}

fn with_code(code: &str) -> String {
    format!("/*--- rotation\n{{\"format\": 5, \"name\": \"x\", \"line\": \"y\", \"settings\": {{\"accounts\": [{{\"start\": true}}]}}}}\n---*/\n{code}")
}

static SETTINGS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

fn ctx_of(now: u64) -> Ctx<'static> {
    Ctx {
        now,
        clock: 0,
        start: "a",
        current: "a",
        settings: SETTINGS.get_or_init(|| serde_json::json!({"accounts": [{"start": true}]})),
        memo: None,
        accounts: vec![CtxAcct {
            id: "a".into(),
            pos: 0,
            api: false,
            sendable: true,
            used: [40.0, 10.0],
            resets: [Some(now + 3600), None],
            refused_until: None,
            rate: [Some(0.5), None],
        }],
    }
}

/// ★ 随包三份：都读得懂、跑得通；`defaults.json` 指的那一份在。
#[test]
fn every_shipped_strategy_reads_and_the_default_points_at_one_of_them() {
    let all = shipped();
    assert_eq!(
        all.len(),
        3,
        "随包三份：{:?}",
        all.iter().map(|x| &x.0).collect::<Vec<_>>()
    );
    for (id, src) in &all {
        let f = parse(src).unwrap_or_else(|e| panic!("{id} 读不懂：{e:?}"));
        let v = f
            .decide(&ctx_of(4_000_000_000))
            .unwrap_or_else(|e| panic!("{id} 跑不通：{e:?}"));
        assert_eq!(v.order, ["a"], "{id}");
    }
    let d: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(shipped_dir().join("defaults.json")).expect("读"),
    )
    .expect("json");
    let want = d["forNew"].as_str().expect("forNew");
    assert!(
        all.iter().any(|(id, _)| id == want),
        "defaults.json 指的 {want} 不在"
    );
}

/// ★ 存设置只重写头：代码一个字节都不动；再读回来头就是新的那份。
#[test]
fn rewriting_the_head_leaves_the_code_byte_for_byte() {
    for (id, src) in shipped() {
        let f = parse(&src).expect("读");
        let mut h = f.head.clone();
        h.settings["accounts"]
            .as_array_mut()
            .expect("accounts")
            .push(serde_json::json!({"id": "lab", "delay": 30, "back": 20}));
        let out = with_head(&src, &h).expect("写");
        let code_of = |s: &str| s[s.find("\n---*/").expect("头尾")..].to_string();
        assert_eq!(code_of(&out), code_of(&src), "{id}：代码变了");
        assert_eq!(parse(&out).expect("读回").head, h);
    }
}

/// ★ 沙箱：死循环 ⇒ 超时；读文件 · 取时间 · 随机数 · import 都没有；print 吞掉。
#[test]
fn the_sandbox_stops_loops_and_offers_no_files_clock_or_randomness() {
    let f = parse(&with_code("fn decide(ctx) { loop { } }")).expect("编得过");
    assert_eq!(
        f.decide(&ctx_of(1)).expect_err("该掐住").kind,
        FailKind::Timeout
    );
    for code in [
        "fn decide(ctx) { let x = open_file(\"/etc/passwd\"); #{ order: [] } }",
        "fn decide(ctx) { let x = timestamp(); #{ order: [] } }",
        "fn decide(ctx) { let x = rand(); #{ order: [] } }",
        "fn decide(ctx) { import \"x\" as y; #{ order: [] } }",
    ] {
        let f = parse(&with_code(code)).expect("编得过（函数是跑的时候才找）");
        assert_eq!(
            f.decide(&ctx_of(1)).expect_err(code).kind,
            FailKind::Broke,
            "{code}"
        );
    }
    let f = parse(&with_code(
        "fn decide(ctx) { print(\"x\"); debug(\"y\"); #{ order: [] } }",
    ))
    .expect("编得过");
    assert!(f.decide(&ctx_of(1)).is_ok());
}

/// ★ 回值形状：没有 order · order 不是号 · why 太长 · 叫醒的窗口不认得 ⇒ 形状不对；同一份 ctx 跑两次一样。
#[test]
fn returns_are_shape_checked_and_deterministic() {
    for (code, bad) in [
        ("fn decide(ctx) { #{} }", true),
        ("fn decide(ctx) { #{ order: [1] } }", true),
        ("fn decide(ctx) { #{ order: [], why: #{ a: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" } } }", true),
        ("fn decide(ctx) { #{ order: [], wake: #{ when: [#{ acct: \"a\", w: \"1d\", used_at_least: 90 }] } } }", true),
        ("fn decide(ctx) { #{ order: [\"a\"], wake: #{ at: ctx.now + 60, when: [#{ acct: \"a\", w: \"5h\", used_at_least: 90 }] } } }", false),
    ] {
        let r = parse(&with_code(code)).expect("编得过").decide(&ctx_of(1));
        assert_eq!(r.as_ref().err().map(|e| e.kind.clone()) == Some(FailKind::BadShape), bad, "{code}：{r:?}");
    }
    let f = parse(
        &shipped()
            .into_iter()
            .find(|(i, _)| i == "most-left")
            .expect("most-left")
            .1,
    )
    .expect("读");
    assert_eq!(f.decide(&ctx_of(77)).ok(), f.decide(&ctx_of(77)).ok());
}

/// ★ 读不懂的几种：没有头 · 头的形状不对 · 编不过（带行）· 没有 decide。
#[test]
fn unreadable_files_say_why_and_where() {
    assert!(parse("fn decide(ctx) { #{} }").is_err());
    assert!(parse("/*--- rotation\n{\"format\": 5}\n---*/\nfn decide(ctx) { #{} }").is_err());
    let e = parse(&with_code("\nfn decide(ctx) { let = ; }")).expect_err("编不过");
    assert_eq!((e.kind, e.line), (FailKind::Broke, Some(5)));
    assert!(parse(&with_code("fn other(ctx) { #{} }")).is_err());
}

/// ★ 后端不认随包策略：生产源码里搜不到它们的文件名与名字（一次性的升级那两份 · 打包那一层的文件清单除外）；三份全部改 `name`，判出来一个不变。
/// 积木库里那几项设置的名字（`capKind` · `margin`）也不在后端源码里（`back` · `delay` 这种常见词别处另有用处，不搜）。
#[test]
fn the_backend_knows_no_shipped_strategy_and_no_rule() {
    let root = crate::guard_support::repo_root().join("src/backend");
    let mut needles: Vec<String> = vec!["\"capKind\"".into(), "\"margin\"".into()];
    for (id, src) in shipped() {
        needles.push(format!("{id}.rhai"));
        needles.push(format!("\"{id}\""));
        if let Ok(f) = parse(&src) {
            needles.push(f.head.name.pick(false).to_string());
        }
    }
    let hits: Vec<String> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .filter(|(p, _)| {
            // 一次性的升级那两份（认第三版编号 1 2 3 对哪几份）· 打包那一层的文件清单（只把文件原样带上）。
            !p.ends_with("rotation/migrate.rs")
                && !p.ends_with("rules/upgrade.rs")
                && !p.ends_with("rules/shipped.rs")
        })
        .flat_map(|(p, t)| {
            let t = crate::guard_support::production_code(&t);
            needles
                .iter()
                .filter(|n| guard_core::contains_word(&t, n))
                .map(|n| format!("{} · {n}", p.display()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(hits.is_empty(), "后端源码点了随包策略或规则的名：{hits:?}");
    let f = [
        Acct {
            id: "a".into(),
            api: false,
            used: [40.0, 0.0],
            resets: [Some(4_000_003_600), None],
            refused_until: None,
            barred: None,
            rate: [Some(0.5), None],
        },
        Acct {
            id: "b".into(),
            api: false,
            used: [95.0, 0.0],
            resets: [Some(4_000_001_000), None],
            refused_until: None,
            barred: None,
            rate: [None, None],
        },
        Acct {
            id: "c".into(),
            api: false,
            used: [10.0, 0.0],
            resets: [Some(4_000_009_000), None],
            refused_until: None,
            barred: None,
            rate: [None, None],
        },
    ];
    for (_, src) in shipped() {
        let mut head = parse(&src).expect("读").head;
        head.settings["accounts"] = serde_json::json!([{"id": "a"}, {"id": "b"}, {"id": "c"}]);
        head.settings["caps"] = serde_json::json!({"5h": [{"from": 0, "to": 1440, "v": 90}]});
        let src = with_head(&src, &head).expect("写");
        let mut renamed = head.clone();
        renamed.name = Text::One("改过的名字".into());
        let other = with_head(&src, &renamed).expect("写");
        let sigma = Sigma {
            start: "a".into(),
            current: "b".into(),
            ..Sigma::default()
        };
        let run = |s: &str| {
            let file = parse(s).expect("读");
            let d = turn(
                &sigma,
                &f,
                &file.head.settings,
                &file,
                0,
                4_000_000_000,
                &[],
            );
            (d.pick.clone(), d.order.clone())
        };
        let (a, b) = (run(&src), run(&other));
        assert_eq!(a, b);
        assert!(matches!(a.0, Pick::Use(_)));
    }
}
