//! 策略文件与沙箱：头的读写不碰代码 · 沙箱掐得住 · 回值照形状收 · 随包三份都读得懂、后端不认它们的名字。

use super::*;
use crate::accounts::quota::rules::decide::{decide, Ctx, CtxAcct, Pick, Score, Sigma};
use crate::accounts::quota::rules::settings::Rules;
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

fn ctx_of(now: u64) -> Ctx<'static> {
    Ctx {
        now,
        clock: 0,
        current: "a",
        temp: &[],
        accounts: vec![CtxAcct {
            id: "a".into(),
            pos: 0,
            api: false,
            ok: true,
            h: 50.0,
            used: [40.0, 10.0],
            resets: [Some(now + 3600), None],
            cap: [90.0, 90.0],
            refused_until: None,
            rate: [Some(0.5), None],
        }],
    }
}

/// ★ 随包三份：都读得懂；`defaults.json` 指的那一份在。
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
        f.score(&ctx_of(4_000_000_000))
            .unwrap_or_else(|e| panic!("{id} 跑不通：{e:?}"));
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
        h.settings
            .accounts
            .push(crate::accounts::quota::rules::settings::Listed {
                id: Some("lab".into()),
                delay: 30,
                back: Some(20),
                ..Default::default()
            });
        let out = with_head(&src, &h).expect("写");
        let code_of = |s: &str| s[s.find("\n---*/").expect("头尾")..].to_string();
        assert_eq!(code_of(&out), code_of(&src), "{id}：代码变了");
        assert_eq!(parse(&out).expect("读回").head, h);
    }
}

/// ★ 沙箱：死循环 ⇒ 超时（操作数，同样的输入停在同一处）；读文件 · 取时间 · 随机数 · import 都没有；print 吞掉。
#[test]
fn the_sandbox_stops_loops_and_offers_no_files_clock_or_randomness() {
    let f = parse(&with_code("fn score(ctx) { loop { } }")).expect("编得过");
    let e = f.score(&ctx_of(1)).expect_err("该掐住");
    assert_eq!(e.kind, FailKind::Timeout, "{e:?}");
    for code in [
        "fn score(ctx) { let x = open_file(\"/etc/passwd\"); #{} }",
        "fn score(ctx) { let x = timestamp(); #{} }",
        "fn score(ctx) { let x = rand(); #{} }",
    ] {
        let f = parse(&with_code(code)).expect("编得过（函数是跑的时候才找）");
        assert_eq!(
            f.score(&ctx_of(1)).expect_err(code).kind,
            FailKind::Broke,
            "{code}"
        );
    }
    // 没有模块解析器：`import` 一跑就错。
    let f = parse(&with_code("fn score(ctx) { import \"x\" as y; #{} }")).expect("编得过");
    assert_eq!(
        f.score(&ctx_of(1)).expect_err("import").kind,
        FailKind::Broke
    );
    let f = parse(&with_code(
        "fn score(ctx) { print(\"x\"); debug(\"y\"); #{ scores: #{} } }",
    ))
    .expect("编得过");
    assert!(f.score(&ctx_of(1)).is_ok());
}

/// ★ 回值形状：分不是数 · why 不是字 · why 太长 ⇒ 形状不对；没给 score ⇒ 缺；同一份 ctx 跑两次一样。
#[test]
fn returns_are_shape_checked_and_deterministic() {
    for (code, bad) in [
        ("fn score(ctx) { #{ scores: #{ a: #{ score: \"x\", why: \"\" } } } }", true),
        ("fn score(ctx) { #{ scores: #{ a: #{ score: 1, why: 3 } } } }", true),
        ("fn score(ctx) { #{ scores: #{ a: #{ why: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" } } } }", true),
        ("fn score(ctx) { 3 }", true),
        ("fn score(ctx) { #{ scores: #{ a: #{ why: \"没给分\" } } } }", false),
    ] {
        let f = parse(&with_code(code)).expect("编得过");
        let r = f.score(&ctx_of(1));
        assert_eq!(r.as_ref().err().map(|e| e.kind.clone()) == Some(FailKind::BadShape), bad, "{code}：{r:?}");
        if !bad {
            assert_eq!(r.expect("ok").scores["a"].0, None);
        }
    }
    let f = parse(
        &shipped()
            .into_iter()
            .find(|(i, _)| i == "most-left")
            .expect("most-left")
            .1,
    )
    .expect("读");
    assert_eq!(f.score(&ctx_of(77)).ok(), f.score(&ctx_of(77)).ok());
}

/// ★ 读不懂的几种：没有头 · 头的形状不对 · 编不过（带行）· 没有 score。
#[test]
fn unreadable_files_say_why_and_where() {
    assert!(parse("fn score(ctx) { #{} }").is_err());
    assert!(parse("/*--- rotation\n{\"format\": 5}\n---*/\nfn score(ctx) { #{} }").is_err());
    let e = parse(&with_code("\nfn score(ctx) { let = ; }")).expect_err("编不过");
    assert_eq!(e.kind, FailKind::Broke);
    assert_eq!(e.line, Some(5), "行号与文件对得上：{e:?}");
    assert!(parse(&with_code("fn other(ctx) { #{} }")).is_err());
}

/// ★ 后端不认随包策略：生产源码里搜不到它们的文件名与名字（一次性的升级那一份除外）；
/// 三份全部改名、改 `name`，同一组事实判出来一个不变。
#[test]
fn the_backend_knows_no_shipped_strategy_by_name() {
    let root = crate::guard_support::repo_root().join("src/backend");
    let mut needles: Vec<String> = Vec::new();
    for (id, src) in shipped() {
        needles.push(format!("{id}.rhai"));
        needles.push(format!("\"{id}\""));
        if let Ok(f) = parse(&src) {
            needles.push(f.head.name.pick(false).to_string());
        }
    }
    let hits: Vec<String> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .filter(|(p, _)| !p.ends_with("rotation/migrate.rs"))
        .flat_map(|(p, t)| {
            let t = crate::guard_support::production_code(&t);
            needles
                .iter()
                .filter(|n| guard_core::contains_word(&t, n))
                .map(|n| format!("{} · {n}", p.display()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(hits.is_empty(), "后端源码点了随包策略的名：{hits:?}");
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
        head.settings.accounts = ["a", "b", "c"]
            .iter()
            .map(|a| crate::accounts::quota::rules::settings::Listed {
                id: Some(a.to_string()),
                ..Default::default()
            })
            .collect();
        let src = with_head(&src, &head).expect("写");
        let mut renamed = head.clone();
        renamed.name = Text::One("改过的名字".into());
        let other = with_head(&src, &renamed).expect("写");
        let sigma = Sigma {
            start: "a".into(),
            current: "b".into(),
            temp: vec![],
            since: Default::default(),
        };
        let run = |s: &str| {
            let file = parse(s).expect("读");
            let d = decide(
                &sigma,
                &f,
                &Rules::utc(file.head.settings.clone()),
                &file,
                4_000_000_000,
                &[],
            );
            (
                d.pick.clone(),
                d.order
                    .iter()
                    .map(|r| r.account.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let (a, b) = (run(&src), run(&other));
        assert_eq!(a, b);
        assert!(matches!(a.0, Pick::Use(_)));
    }
}
