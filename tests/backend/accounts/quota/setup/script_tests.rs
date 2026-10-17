//! 策略文件与沙箱：头的读写不碰代码 · 沙箱掐得住 · 回值照形状收 · 判据夹具都读得懂、后端不认任何一份策略的名字。

use super::*;
use crate::accounts::quota::rules::decide::{
    turn, Ctx, CtxAcct, Me, Pick, PromptCache, Sigma, Strategy,
};
use crate::accounts::quota::rules::Acct;
use crate::accounts::quota::strategy::FailKind;

/// 判据用的几份策略：三份升级材料（材料只编进升级那一处，判据照原文跑）。
fn fixtures() -> Vec<(String, String)> {
    (0..3)
        .map(|n| {
            (
                format!("material-{}", n + 1),
                super::super::upgrade::material(n).to_string(),
            )
        })
        .collect()
}

fn with_code(code: &str) -> String {
    format!("/*--- rotation\n{{\"format\": 5, \"settings\": {{\"accounts\": [{{\"start\": true}}]}}}}\n---*/\n{code}")
}

static SETTINGS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
static ME: Me = Me {
    pace: None,
    spread: None,
    context: None,
    write_extra: None,
    ttl: None,
    grow: None,
    cached: Vec::new(),
};

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
            per_pct: [None, None],
            weekly: false,
            off: false,
            others: None,
        }],
        me: &ME,
        pinned: false,
        temp: &[],
    }
}

/// ★ 三份升级材料都读得懂、跑得通（只有一个号 ⇒ 就用它）。
#[test]
fn every_upgrade_material_reads() {
    for (id, src) in &fixtures() {
        let f = parse(src).unwrap_or_else(|e| panic!("{id} 读不懂：{e:?}"));
        let v = f
            .code
            .decide(&ctx_of(4_000_000_000))
            .unwrap_or_else(|e| panic!("{id} 跑不通：{e:?}"));
        assert_eq!(v.order, ["a"], "{id}");
    }
}

/// ★ 策略读得到每号「1% ≈ 多少用量」：量到的那一窗是那个数，量不到的那一窗是 `()`（不给假数）。
#[test]
fn a_strategy_reads_the_size_of_one_percent_and_nothing_where_unmeasured() {
    let f = parse(&with_code(
        r#"fn decide(ctx) { let p = ctx.accounts[0].tokensPerPct; #{ order: ["a"], why: #{ a: `${p.w5h}|${type_of(p.w7d)}` } } }"#,
    ))
    .expect("编得过");
    let mut c = ctx_of(1);
    c.accounts[0].per_pct = [Some(25_000), None];
    let v = f.code.decide(&c).expect("跑得通");
    assert_eq!(v.why, [("a".to_string(), "25000|()".to_string())]);
}

/// ★ 策略读得到智能切换要的事实：每号有没有周上限 · 本会话先不用 · 别的会话压在上面的用速；本会话的用速 · 起伏 · 上下文 ·
/// 写缓存倍数 · 缓存留多久 · 上下文每分钟长多少 · 各号上的缓存；在用的号是不是手动选的 · 手动排的先后。量不到的格是 `()`。
#[test]
fn a_strategy_reads_the_facts_smart_switching_needs_and_nothing_where_unmeasured() {
    let f = parse(&with_code(
        r#"fn decide(ctx) {
  let a = ctx.accounts[0];
  let s = ctx.session;
  let c = if s.cached.b != () { `${s.cached.b["until"]}/${s.cached.b.tokens}` } else { "-" };
  let o = if a.others != () { `${a.others.w5h}` } else { type_of(a.others) };
  #{ order: ["a"], why: #{ a: `${a.weekly}|${a.off}|${o}|${s.pace}|${s.spread}|${s.context}|${s.writeExtra}|${s.ttl}|${s.grow}|${c}|${ctx.pinned}|${ctx.temp}` } }
}"#,
    ))
    .expect("编得过");
    let bare = f.code.decide(&ctx_of(1)).expect("跑得通");
    assert_eq!(
        bare.why,
        [(
            "a".to_string(),
            "false|false|()|||||||-|false|[]".to_string()
        )],
        "量不到的都是 ()"
    );
    let me = Me {
        pace: Some(1500.0),
        spread: Some(0.5),
        context: Some(120_000),
        write_extra: Some(1.9),
        ttl: Some(3600),
        grow: Some(800.0),
        cached: vec![PromptCache {
            account: "b".into(),
            until: 5000,
            tokens: 90_000,
        }],
    };
    let temp = ["a".to_string(), "b".to_string()];
    let mut c = ctx_of(1);
    c.accounts[0].weekly = true;
    c.accounts[0].off = true;
    c.accounts[0].others = Some(0.25);
    c.me = &me;
    c.pinned = true;
    c.temp = &temp;
    let v = f.code.decide(&c).expect("跑得通");
    assert_eq!(
        v.why,
        [(
            "a".to_string(),
            "true|true|0.25|1500.0|0.5|120000|1.9|3600|800.0|5000/90000|true|[\"a\", \"b\"]"
                .to_string()
        )]
    );
}

/// ★ 号到 100 个时 ctx 照样放得进沙箱：改 ctx 里一格（整个 ctx 按嵌套合计查一遍）· 照号摊平的几张表 · 每号一行的表都放得下。
#[test]
fn the_sandbox_holds_a_hundred_accounts() {
    let f = parse(&with_code(
        r#"fn decide(ctx) {
  let all = ctx;
  let amt = #{}; let exp = #{}; let wk = #{}; let has = #{}; let tpp = #{}; let oth = #{}; let tier = #{}; let open = #{}; let cache = #{};
  let col = []; let ids = []; let rows = [];
  for a in all.accounts {
    amt[a.id] = 100.0 - a.used.w5h; exp[a.id] = a.resets.w5h; wk[a.id] = a.weekly; has[a.id] = a.sendable;
    tpp[a.id] = a.tokensPerPct.w5h; oth[a.id] = a.others.w5h; tier[a.id] = "default"; open[a.id] = true;
    cache[a.id] = all.session.cached[a.id]["until"];
    col.push(a.pos); col.push(a.pos); col.push(a.pos); col.push(a.pos); col.push(a.pos); col.push(a.pos);
    ids.push(a.id);
    rows.push(#{ id: a.id, amt: amt[a.id], exp: exp[a.id], wk: wk[a.id], has: has[a.id], tpp: tpp[a.id], oth: oth[a.id], cache: cache[a.id] });
  }
  all.accounts[0].sendable = false;
  #{ order: [rows[0].id] }
}"#,
    ))
    .expect("编得过");
    let ids: Vec<String> = (0..100).map(|i| format!("account-{i:03}")).collect();
    let me = Me {
        pace: Some(1000.0),
        context: Some(100_000),
        cached: ids
            .iter()
            .map(|id| PromptCache {
                account: id.clone(),
                until: 5000,
                tokens: 100_000,
            })
            .collect(),
        ..Me::default()
    };
    let mut c = ctx_of(1);
    let one = c.accounts[0].clone();
    c.accounts = ids
        .iter()
        .enumerate()
        .map(|(pos, id)| CtxAcct {
            id: id.clone(),
            pos,
            per_pct: [Some(150_000), Some(1_000_000)],
            others: Some(0.1),
            weekly: true,
            ..one.clone()
        })
        .collect();
    c.me = &me;
    let v = f
        .code
        .decide(&c)
        .unwrap_or_else(|e| panic!("100 个号放不下：{e:?}"));
    assert_eq!(v.order, ["account-000"]);
}

/// ★ 存设置只重写头：代码一个字节都不动；再读回来头就是新的那份。
#[test]
fn rewriting_the_head_leaves_the_code_byte_for_byte() {
    for (id, src) in fixtures() {
        let f = parse(&src).expect("读");
        let mut h = f.head.clone();
        h.settings["accounts"]
            .as_array_mut()
            .expect("accounts")
            .push(serde_json::json!({"id": "lab", "fallback": true, "delay": 30}));
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
        f.code.decide(&ctx_of(1)).expect_err("该掐住").kind,
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
            f.code.decide(&ctx_of(1)).expect_err(code).kind,
            FailKind::Broke,
            "{code}"
        );
    }
    let f = parse(&with_code(
        "fn decide(ctx) { print(\"x\"); debug(\"y\"); #{ order: [] } }",
    ))
    .expect("编得过");
    assert!(f.code.decide(&ctx_of(1)).is_ok());
}

/// ★ 跑出错 ⇒ 是哪一种（照 Rhai 的错归成几种常见的，提示照它写）· 第几行（文件里的行；出在自己写的函数里 ⇒ 报那一行，不报调用处；
/// 操作数超了说不出在哪一行）。
#[test]
fn run_errors_say_which_kind_and_on_which_line() {
    use crate::accounts::quota::strategy::FailWhy;
    // 头占 1–3 行，代码从第 4 行起。
    for (code, why, line) in [
        (
            "fn decide(ctx) {\n  let x = 1 / 0;\n  #{ order: [] }\n}",
            FailWhy::DivZero,
            Some(5),
        ),
        (
            "fn decide(ctx) {\n  let x = 1 % 0;\n  #{ order: [] }\n}",
            FailWhy::DivZero,
            Some(5),
        ),
        (
            "fn half(x) {\n  x / 0\n}\nfn decide(ctx) {\n  half(1);\n  #{ order: [] }\n}",
            FailWhy::DivZero,
            Some(5),
        ),
        (
            "fn decide(ctx) {\n\n  nope(1);\n  #{ order: [] }\n}",
            FailWhy::NoFn,
            Some(6),
        ),
        (
            "fn decide(ctx) {\n  let a = ctx.accounts;\n  let x = a * 2;\n  #{ order: [] }\n}",
            FailWhy::Type,
            Some(6),
        ),
        (
            "fn decide(ctx) {\n  let x = \"a\" - 1;\n  #{ order: [] }\n}",
            FailWhy::Type,
            Some(5),
        ),
        (
            "fn decide(ctx) {\n  let a = [1];\n  let x = a[5];\n  #{ order: [] }\n}",
            FailWhy::Index,
            Some(6),
        ),
        ("fn decide(ctx) {\n  loop { }\n}", FailWhy::Limit, None),
        (
            "fn decide(ctx) {\n  throw \"boom\";\n}",
            FailWhy::Other,
            Some(5),
        ),
    ] {
        let e = parse(&with_code(code))
            .expect("编得过")
            .code
            .decide(&ctx_of(1))
            .expect_err(code);
        assert_eq!((e.why, e.line), (why, line), "{code}：{e:?}");
    }
    let e = parse(&with_code("\nfn decide(ctx) { let = ; }")).expect_err("编不过");
    assert_eq!(e.why, FailWhy::Other, "编不过 ⇒ 代码出错");
    // 推演里累计耗时用完 ⇒ 超时（不是超出运算上限）。
    let slow = parse(&with_code("fn decide(ctx) {\n  loop { }\n}")).expect("编得过");
    let slow = super::super::adopted_of_file("", "", &slow, None);
    let b = crate::accounts::quota::rules::code::Budgeted::new(&slow);
    while !b.exhausted() {
        let _ = b.decide(&ctx_of(1));
    }
    assert_eq!(
        b.decide(&ctx_of(1)).expect_err("用完了").why,
        FailWhy::Timeout
    );
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
        let r = parse(&with_code(code)).expect("编得过").code.decide(&ctx_of(1));
        assert_eq!(r.as_ref().err().map(|e| e.kind.clone()) == Some(FailKind::BadShape), bad, "{code}：{r:?}");
    }
    let f = parse(
        &fixtures()
            .into_iter()
            .find(|(i, _)| i == "material-2")
            .expect("material-2")
            .1,
    )
    .expect("读");
    assert_eq!(
        f.code.decide(&ctx_of(77)).ok(),
        f.code.decide(&ctx_of(77)).ok()
    );
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

/// ★ 后端不认任何一份策略：生产源码里搜不到判据用的那几份的文件名（一次性的升级那两份除外）；说明全部换掉，判出来一个不变。
/// 积木库里那几项设置的名字（`capKind`）也不在后端源码里（`row` · `delay` 这种常见词别处另有用处，不搜）。
#[test]
fn the_backend_knows_no_strategy_and_no_rule() {
    let root = guard_core::addr::repo_root().join("src/backend");
    let mut needles: Vec<String> = vec!["\"capKind\"".into()];
    for (id, _) in fixtures() {
        needles.push(format!("{id}.rhai"));
        needles.push(format!("\"{id}\""));
    }
    let hits: Vec<String> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .filter(|(p, _)| {
            // 一次性的升级那两份（认第二版的编号 1 2 3、写出升级材料认的设置）。
            !p.ends_with("setup/migrate.rs") && !p.ends_with("setup/upgrade.rs")
        })
        .flat_map(|(p, t)| {
            let t = guard_core::production_code(&t);
            needles
                .iter()
                .filter(|n| guard_core::contains_word(&t, n))
                .map(|n| format!("{} · {n}", p.display()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(hits.is_empty(), "后端源码点了策略或规则的名：{hits:?}");
    let f = [
        Acct {
            id: "a".into(),
            api: false,
            used: [40.0, 0.0],
            resets: [Some(4_000_003_600), None],
            refused_until: None,
            barred: None,
            rate: [Some(0.5), None],
            per_pct: [None, None],
            weekly: false,
            others: None,
        },
        Acct {
            id: "b".into(),
            api: false,
            used: [95.0, 0.0],
            resets: [Some(4_000_001_000), None],
            refused_until: None,
            barred: None,
            rate: [None, None],
            per_pct: [None, None],
            weekly: false,
            others: None,
        },
        Acct {
            id: "c".into(),
            api: false,
            used: [10.0, 0.0],
            resets: [Some(4_000_009_000), None],
            refused_until: None,
            barred: None,
            rate: [None, None],
            per_pct: [None, None],
            weekly: false,
            others: None,
        },
    ];
    assert_eq!(fixtures().len(), 3, "材料没读到");
    for (_, src) in fixtures() {
        let mut head = parse(&src).expect("读").head;
        head.settings["accounts"] = serde_json::json!([{"id": "a"}, {"id": "b"}, {"id": "c"}]);
        head.settings["caps"] = serde_json::json!({"5h": [{"from": 0, "to": 1440, "v": 90}]});
        let src = with_head(&src, &head).expect("写");
        let other = format!(
            "// 换过的说明\n\n{}",
            &src[src.find("/*--- rotation").expect("头")..]
        );
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
                &super::super::adopted_of_file("", "", &file, None),
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

/// ★ 说明 ＝ 文件开头连着的 `//` 行：去行首 `// `，空的 `//` 行分段；没有说明 ⇒ 空。
#[test]
fn the_about_text_is_read_from_the_leading_comments() {
    let src = format!(
        "// 第一行\n//第二行\n//\n//\n// 第二段\n\n{}",
        with_code("fn decide(ctx) { #{ order: [] } }")
    );
    assert_eq!(parse(&src).expect("读").about, "第一行\n第二行\n\n第二段");
    assert_eq!(
        parse(&with_code("fn decide(ctx) { #{ order: [] } }"))
            .expect("读")
            .about,
        ""
    );
    for (id, src) in fixtures() {
        let f = parse(&src).expect("读");
        assert!(
            !f.about.is_empty() && !f.about.contains("//"),
            "{id}：{:?}",
            f.about
        );
    }
    // 说明和头之间夹了代码 ⇒ 读不懂（说明只能是开头连着的注释）。
    let e = parse(&format!(
        "// 说明\nlet x = 1;\n{}",
        with_code("fn decide(ctx) { #{ order: [] } }")
    ))
    .expect_err("读不懂");
    assert_eq!((e.kind, e.line), (FailKind::BadShape, Some(2)));
}

/// ★ 存设置后说明和代码逐字节不变（只换头那一块）；第三版那种没有头的脚本认得出来。
#[test]
fn saving_settings_leaves_the_about_text_and_the_code_byte_for_byte() {
    let src = format!(
        "// 说明 · 第一行\r\n//\n// 第二段\n\n\n{}",
        with_code("\n// 代码里的注释\nfn decide(ctx) { #{ order: [] } }\n")
    );
    let f = parse(&src).expect("读");
    let mut h = f.head.clone();
    h.settings = serde_json::json!({"accounts": [{"start": true}, {"id": "lab"}], "x": [1, 2]});
    let out = with_head(&src, &h).expect("写");
    let cut = |s: &str| {
        let a = s.find("/*--- rotation").expect("头");
        let b = s.find("\n---*/").expect("头尾");
        (s[..a].to_string(), s[b..].to_string())
    };
    assert_eq!(cut(&out), cut(&src));
    assert_eq!(parse(&out).expect("读回").head, h);
    assert!(has_head(&src));
    assert!(!has_head("// 第三版\nfn decide(ctx) { [] }"));
}

/// ★ 头里带 `name` · `line`（旧格式）⇒ 形状不对，报出来（标题是文件名、说明是开头的注释）。
#[test]
fn a_head_with_name_or_line_is_the_wrong_shape() {
    for extra in ["\"name\": \"x\", ", "\"line\": \"y\", "] {
        let src = format!("// 说明\n/*--- rotation\n{{\"format\": 5, {extra}\"settings\": {{\"accounts\": [{{\"start\": true}}]}}}}\n---*/\nfn decide(ctx) {{ #{{ order: [] }} }}");
        let e = parse(&src).expect_err(extra);
        assert_eq!(e.kind, FailKind::BadShape, "{extra}");
        assert_eq!(e.line, Some(3), "{extra}：行号接着说明数");
        assert!(e.msg.contains("unknown field"), "{}", e.msg);
    }
}
