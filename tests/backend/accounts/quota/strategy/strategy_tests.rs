//! 策略文件：沙箱（S1）· 确定性（S2）· 内置三份试跑（S3）· 共用小函数（S4）· 声明与表单（S9）· 加载路径（S12）· 字（S13 的策略那一半）。

use super::ctx::{self, Facts, Input, Stint, Win};
use super::form::{self, Code, Type};
use super::run::{self, Priorities};
use super::words::Speak;
use super::*;
use crate::accounts::quota::decide::Kind;
use serde_json::{json, Map, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/__fixtures__/");

/// 十四组样例（设计稿 `轮换-v3-策略/样例/c1–c13` ＋ c14：10-11 那一幕），文件名逐个写出来。
const SAMPLES: [&str; 14] = [
    "rotation-sample-c1.json",
    "rotation-sample-c2.json",
    "rotation-sample-c3.json",
    "rotation-sample-c4.json",
    "rotation-sample-c5.json",
    "rotation-sample-c6.json",
    "rotation-sample-c7.json",
    "rotation-sample-c8.json",
    "rotation-sample-c9.json",
    "rotation-sample-c10.json",
    "rotation-sample-c11.json",
    "rotation-sample-c12.json",
    "rotation-sample-c13.json",
    "rotation-sample-c14.json",
];

/// 一份手写的策略：`priority` 的身子照给的，`meta` / `texts` / `describe` 是最小的一套。
fn tiny(priority_body: &str) -> String {
    format!(
        r#"
fn meta() {{ #{{ format: 1, params: [], acct_params: [] }} }}
fn texts() {{ #{{ zh: #{{ w: "字" }}, en: #{{ w: "word" }} }} }}
fn describe(p, accounts) {{ ["w"] }}
fn priority(ctx) {{
{priority_body}
}}
"#
    )
}

fn local(id: &str, src: &str) -> Strategy {
    load(
        vec![File {
            name: format!("{id}.rhai"),
            path: None,
            text: Ok(src.to_string()),
        }],
        false,
        &["1".into(), "2".into(), "3".into()],
    )
    .remove(0)
}

fn sample(name: &str) -> Value {
    let file = SAMPLES
        .iter()
        .find(|f| **f == format!("rotation-sample-{name}.json"))
        .expect("样例名");
    let t = std::fs::read_to_string(format!("{FIXTURES}{file}")).expect("样例");
    serde_json::from_str(&t).expect("样例是 JSON")
}

fn facts_of(a: &Value) -> Facts {
    let win = |w: &Value| Win {
        open: w["open"].as_bool().unwrap_or(false),
        used: w["used"].as_f64().unwrap_or(0.0),
        resets: w["resets"].as_u64(),
    };
    Facts {
        id: a["id"].as_str().unwrap_or("").to_string(),
        kind: if a["kind"] == "api" {
            Kind::Api
        } else {
            Kind::Sub
        },
        ok: a["ok"].as_bool().unwrap_or(false),
        why_not: match a["why_not"].as_str() {
            Some("refused") => Some(ctx::WhyNot::Refused),
            Some("full") => Some(ctx::WhyNot::Full),
            Some("overage") => Some(ctx::WhyNot::Overage),
            Some("login") => Some(ctx::WhyNot::Login),
            _ => None,
        },
        back: a["back"].as_u64(),
        w5h: win(&a["w5h"]),
        w7d: win(&a["w7d"]),
        others: a["others"].as_u64().unwrap_or(0) as u32,
    }
}

/// 照一组样例跑一份策略：参数按声明补齐，账号先后按引擎的规矩（`ordered` 照写下的，别的按号名）。
fn run_sample(s: &Strategy, c: &Value) -> Result<Priorities, Fail> {
    let params = ctx::whole_params(&s.params, c["params"].as_object().unwrap_or(&Map::new()));
    let accounts: Vec<(Facts, Map<String, Value>)> = c["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| {
            (
                facts_of(a),
                ctx::acct_params(
                    &s.acct_params,
                    a["params"].as_object().unwrap_or(&Map::new()),
                    None,
                ),
            )
        })
        .collect();
    let ids: Vec<String> = accounts.iter().map(|(f, _)| f.id.clone()).collect();
    let memo = c["memo"].clone();
    let i = Input {
        now: c["now"].as_u64().unwrap_or(0),
        clock: c["clock"].as_u64().unwrap_or(0) as u32,
        current: c["current"].as_str().unwrap_or(""),
        stint: Stint {
            since: c["stint"]["since"].as_u64().unwrap_or(0),
            used: c["stint"]["used"].as_f64().unwrap_or(0.0),
        },
        rate: c["rate"].as_f64(),
        memo: &memo,
        params: &params,
        accounts: &accounts,
    };
    let d = ctx::build(s, &i).expect("ctx");
    run::priority(s, d, &ids)
}

/// 试跑器那一行里的「为什么」（照设计稿的试跑器 rh 原样写：`@` 键写键 ＋ 参数，自己的键照 zh 填参数的 JSON 样子）。
fn rh_why(w: &Value, s: &Strategy) -> String {
    let (k, args) = match w {
        Value::String(k) => (k.clone(), Value::Null),
        Value::Object(m) => (m["k"].as_str().unwrap_or("").to_string(), w.clone()),
        _ => return "—".into(),
    };
    if k.starts_with('@') {
        let tail: String = match &args {
            Value::Object(m) => m
                .iter()
                .filter(|(k, _)| *k != "k")
                .map(|(k, v)| {
                    format!(
                        " {k}={}",
                        if k == "then" {
                            rh_why(v, s)
                        } else {
                            v.to_string()
                        }
                    )
                })
                .collect(),
            _ => String::new(),
        };
        return format!("{k}{tail}");
    }
    let mut t = s
        .texts
        .zh
        .get(&k)
        .cloned()
        .unwrap_or_else(|| "??缺字".into());
    if let Value::Object(m) = &args {
        for (ak, av) in m {
            t = t.replace(&format!("{{{ak}}}"), &av.to_string());
        }
    }
    t
}

/// 一组样例写成试跑结果里的那几行。
fn rh_block(s: &Strategy, name: &str, c: &Value) -> String {
    let title = c["_title"].as_str().unwrap_or("");
    let out = match run_sample(s, c) {
        Ok(o) => o,
        Err(e) => return format!("{name}\tERR {}\n", e.msg),
    };
    let ok = |id: &str| {
        c["accounts"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x["id"] == id && x["ok"] == true))
    };
    let mut rows: Vec<(String, Option<f64>, Value)> = c["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| {
            let id = a["id"].as_str().unwrap_or("").to_string();
            let r = &out.rows[&id];
            (id, r.p, r.why.clone())
        })
        .collect();
    rows.sort_by(|x, y| match (x.1, y.1) {
        (Some(a), Some(b)) => b.total_cmp(&a).then(x.0.cmp(&y.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => x.0.cmp(&y.0),
    });
    let pick = rows
        .iter()
        .find(|r| r.1.is_some() && ok(&r.0))
        .map_or_else(|| "（停发）".to_string(), |r| r.0.clone());
    let line: Vec<String> = rows
        .iter()
        .map(|(id, p, why)| {
            format!(
                "{id}{}[{}]",
                p.map_or_else(|| " ()".to_string(), |p| format!(" {p}")),
                rh_why(why, s)
            )
        })
        .collect();
    let mut t = format!(
        "{name}\t{title}\n  挑 {pick} · {}\n  备忘 {}\n",
        line.join(" > "),
        out.memo
            .as_ref()
            .map_or_else(|| "—".to_string(), Value::to_string)
    );
    if name == "c1" {
        let params = ctx::whole_params(&s.params, &Map::new());
        let accts: Vec<Value> = c["accounts"]
            .as_array()
            .expect("accounts")
            .iter()
            .map(|a| {
                let p = ctx::acct_params(
                    &s.acct_params,
                    a["params"].as_object().unwrap_or(&Map::new()),
                    None,
                );
                json!({"id": a["id"], "kind": a["kind"], "params": p})
            })
            .collect();
        let (d, _) = call(
            s.ast.as_deref().expect("ast"),
            "describe",
            (
                rhai::serde::to_dynamic(Value::Object(params)).expect("p"),
                rhai::serde::to_dynamic(Value::Array(accts)).expect("a"),
            ),
        );
        let d = rhai::serde::from_dynamic::<Value>(&d.expect("describe")).expect("json");
        t.push_str(&format!("  描述 {d}\n"));
    }
    t
}

/// ★ S3：十三组样例 × 内置三份，排出的先后、`()`、`@memo` 与设计稿的试跑结果逐字节相同。
#[test]
fn the_three_builtins_reproduce_the_trial_run_byte_for_byte() {
    let want =
        std::fs::read_to_string(format!("{FIXTURES}rotation-trial-run.txt")).expect("试跑结果");
    let mut got = String::from(
        "# 试跑结果：rh <lib.rhai> <策略> 样例/c1–c13（Rhai 1.26.1）。@ 开头的是引擎的字（beRotation.lib.*），引擎写字时换成人话；时刻是 unix 秒。\n",
    );
    for s in builtins() {
        assert!(s.usable(), "内置策略 {} 读不懂：{:?}", s.id, s.error);
        got.push_str(&format!("\n## 内置策略 {}\n", s.id));
        for i in 1..=13 {
            let name = format!("c{i}");
            got.push_str(&rh_block(s, &name, &sample(&name)));
        }
    }
    assert_eq!(got, want);
}

/// 10-11 那一幕：在用的号还在线下（只剩 2 点）、别的号的优先级升上来了 ⇒ 三份内置都不当场把它换走；
/// 内置 2 靠延迟 ＋ 备忘顶着，备忘交回、满 10 分以后才让过去。
#[test]
fn a_usable_current_account_is_not_taken_away_at_once() {
    let c = sample("c14");
    let pick = |o: &Priorities| {
        let mut v: Vec<(&String, f64)> = o
            .rows
            .iter()
            .filter_map(|(id, r)| r.p.map(|p| (id, p)))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        v.first().map(|x| x.0.clone())
    };
    for s in builtins() {
        let o = run_sample(s, &c).expect("跑得通");
        assert_eq!(
            pick(&o).as_deref(),
            Some("team"),
            "内置策略 {} 当场换走了在用的号",
            s.id
        );
    }
    let two = find(builtins(), "2").expect("内置 2");
    let o = run_sample(&two, &c).expect("跑得通");
    assert_eq!(
        o.memo,
        Some(json!({"delay": {"to": "personal", "since": 1760187780u64}}))
    );
    // 十分钟以后、备忘交回 ⇒ 让过去。
    let mut later = c.clone();
    later["now"] = json!(1760187780u64 + 600);
    later["memo"] = o.memo.clone().expect("备忘");
    let o2 = run_sample(&two, &later).expect("跑得通");
    assert_eq!(pick(&o2).as_deref(), Some("personal"));
    assert_eq!(o2.memo, None);
}

/// S3 后半：每份内置的 `texts` 覆盖它 `meta` / `describe` / `priority` 用到的全部键，zh · en 都有。
#[test]
fn every_builtin_has_words_for_every_key_it_uses_in_both_languages() {
    for s in builtins() {
        for d in s.params.iter().chain(&s.acct_params) {
            assert!(s.texts.has(&d.label), "{} {}", s.id, d.label);
            if let Some(h) = &d.hint {
                assert!(s.texts.has(h), "{} {h}", s.id);
            }
        }
        for i in 1..=14 {
            let c = sample(&format!("c{i}"));
            let o = run_sample(s, &c).expect("跑得通");
            for r in o.rows.values() {
                run::why_ok(s, &r.why).unwrap_or_else(|m| panic!("内置 {} c{i}：{m}", s.id));
                let tz = crate::common::time::Tz::default();
                let speak = Speak::of(s, Lang::En, 1760187780, &tz);
                assert!(!speak.why(&r.why).contains('〔'), "{} c{i}", s.id);
            }
        }
    }
}

/// ★ S1：死循环 ⇒ 超时（操作数），同一份上下文跑两次报在同一处。
#[test]
fn an_endless_loop_stops_at_the_same_place_every_time() {
    let s = local("spin", &tiny("let i = 0;\nloop { i += 1; }"));
    assert!(s.usable(), "{:?}", s.error);
    let c = sample("c1");
    let a = run_sample(&s, &c).expect_err("停");
    let b = run_sample(&s, &c).expect_err("停");
    assert_eq!(
        (a.kind.clone(), b.kind.clone()),
        (FailKind::Timeout, FailKind::Timeout)
    );
    // 两次都是操作数掐的 ⇒ 停在同一处（机器忙时可能先撞上 50 ms 墙钟，那一种只保证是超时）。
    if a.msg == b.msg {
        assert_eq!(a, b, "同一份上下文停在同一处");
    }
}

/// ★ S1：没有读文件 · 取时间 · 随机数 · `import` —— 各写一份调它的脚本，编或跑必错；`print` 不出东西也不出错。
#[test]
fn the_sandbox_has_no_files_no_clock_no_dice_and_no_import() {
    let c = sample("c1");
    for body in [
        r#"open_file("/etc/passwd")"#,
        "read_string(\"x\")",
        "timestamp()",
        "rand()",
        "random()",
        r#"import "x" as y; #{}"#,
        r#"eval("1")"#,
    ] {
        let s = local("bad", &tiny(body));
        let r = if s.usable() {
            run_sample(&s, &c).map(|_| ())
        } else {
            Err(s.error.clone().expect("错"))
        };
        assert!(r.is_err(), "这一句不该能跑：{body}");
    }
    let s = local("talk", &tiny("print(\"hi\"); debug(1); #{}"));
    assert!(run_sample(&s, &c).is_ok());
}

/// ★ S1：`lib` 不能被策略改写：自己写一个同名函数不改 `lib::` 的；给 `lib::` 赋值编不过。
#[test]
fn a_strategy_cannot_rewrite_lib() {
    let src = tiny("let u = lib::usable(ctx.accounts[0], ctx);\n#{ work: #{ p: if type_of(u) == \"map\" { 1 } else { 2 }, why: \"w\" } }")
        + "\nfn usable(a, ctx) { 42 }\n";
    let s = local("shadow", &src);
    let o = run_sample(&s, &sample("c1")).expect("跑得通");
    assert_eq!(o.rows["work"].p, Some(1.0));
    let s = local("assign", &tiny("lib::usable = 1; #{}"));
    assert!(
        !s.usable() || run_sample(&s, &sample("c1")).is_err(),
        "给 lib:: 赋值编不过或跑不过"
    );
    let again = local("shadow2", &src);
    assert_eq!(
        run_sample(&again, &sample("c1")).expect("跑得通").rows["work"].p,
        Some(1.0),
        "lib 照旧"
    );
}

/// ★ S2：同一份上下文（含 `memo`）跑 100 次，回的表与 `@memo` 逐字节相同。
#[test]
fn the_same_context_gives_the_same_answer_a_hundred_times() {
    for s in builtins() {
        let c = sample("c10");
        let first = run_sample(s, &c).expect("跑得通");
        for _ in 0..100 {
            assert_eq!(run_sample(s, &c).expect("跑得通"), first, "内置 {}", s.id);
        }
    }
}

/// S5 的形状那一半：`p` 不是数也不是 `()` · `why` 键不在 `texts` · `back` 不是时刻 ⇒ 形状不对。
#[test]
fn a_bad_shape_is_named() {
    let c = sample("c1");
    for body in [
        r#"#{ work: #{ p: "high", why: "w" } }"#,
        r#"#{ work: #{ p: 1, why: "nope" } }"#,
        r#"#{ work: #{ p: (), why: "w", back: "soon" } }"#,
        r#"#{ work: #{ p: 1, why: #{ k: "@bogus" } } }"#,
        "42",
    ] {
        let s = local("shape", &tiny(body));
        let e = run_sample(&s, &c).expect_err(body);
        assert_eq!(e.kind, FailKind::BadShape, "{body}");
    }
    let s = local("throw", &tiny("throw \"x\";"));
    assert_eq!(run_sample(&s, &c).expect_err("出错").kind, FailKind::Broke);
}

fn texts_for(keys: &[&str]) -> String {
    let m: String = keys.iter().map(|k| format!("{k}: \"x\", ")).collect();
    format!("#{{ zh: #{{ {m} }}, en: #{{ {m} }} }}")
}

fn with_meta(params: &str, acct: &str, keys: &[&str]) -> String {
    format!(
        "fn meta() {{ #{{ format: 1, params: [{params}], acct_params: [{acct}] }} }}\nfn texts() {{ {} }}\nfn describe(p, accounts) {{ [] }}\nfn priority(ctx) {{ #{{}} }}\n",
        texts_for(keys)
    )
}

/// ★ S9：每种 `type` 一份声明 ⇒ 写好字的那一份对；`choice` 缺选项字 · 约定名类型不对 · `min > max` ⇒ 加载报错带行。
#[test]
fn every_declaration_type_turns_into_a_form_cell() {
    let src = with_meta(
        r#"#{ k: "b", type: "bool", init: false, label: "l_b" },
           #{ k: "m", type: "minutes", init: 5, min: 0, max: 60, label: "l_m", hint: "h_m" },
           #{ k: "pt", type: "points", init: (), min: 1, max: 100, label: "l_p" },
           #{ k: "pc", type: "percent", init: 50, label: "l_pc" },
           #{ k: "c", type: "choice", of: ["a", "b"], init: "a", label: "l_c" }"#,
        r#"#{ k: "line", type: "line", init: 90, label: "l_l" },
           #{ k: "slots", type: "slots", init: [], label: "l_s" }"#,
        &[
            "l_b", "l_m", "h_m", "l_p", "l_pc", "l_c", "l_c_a", "l_c_b", "l_c_a_h", "l_l", "l_s",
        ],
    );
    let s = local("form", &src);
    assert!(s.usable(), "{:?}", s.error);
    let types: Vec<Type> = s
        .params
        .iter()
        .chain(&s.acct_params)
        .map(|d| d.ty)
        .collect();
    assert_eq!(
        types,
        [
            Type::Bool,
            Type::Minutes,
            Type::Points,
            Type::Percent,
            Type::OneOf,
            Type::Line,
            Type::Slots
        ]
    );
    let c = form::shown(&s.params[4], &s.texts, Lang::Zh);
    assert_eq!(c.options.len(), 2);
    assert_eq!(c.options[0].hint.as_deref(), Some("x"));
    assert_eq!(c.options[1].hint, None);
    let m = serde_json::to_value(form::shown(&s.params[1], &s.texts, Lang::En)).expect("json");
    assert_eq!(
        m,
        json!({"k": "m", "type": "minutes", "init": 5, "min": 0.0, "max": 60.0, "label": "x", "hint": "x"})
    );

    for (src, needle) in [
        (
            with_meta(
                r#"#{ k: "c", type: "choice", of: ["a", "zz"], init: "a", label: "l" }"#,
                "",
                &["l", "l_a"],
            ),
            "\"c\"",
        ),
        (
            with_meta(
                "",
                r#"#{ k: "line", type: "points", init: 90, label: "l" }"#,
                &["l"],
            ),
            "\"line\"",
        ),
        (
            with_meta(
                r#"#{ k: "m", type: "minutes", init: 5, min: 9, max: 3, label: "l" }"#,
                "",
                &["l"],
            ),
            "\"m\"",
        ),
        (
            with_meta(
                r#"#{ k: "all_out", type: "choice", of: ["x"], init: "x", label: "l" }"#,
                "",
                &["l", "l_x"],
            ),
            "\"all_out\"",
        ),
    ] {
        let s = local("bad", &src);
        let e = s.error.clone().expect("该报错");
        let want = src[..src.find(needle).expect("needle")]
            .matches('\n')
            .count()
            + 1;
        assert_eq!(e.line, Some(want), "{}", e.msg);
    }
}

/// ★ S9：参数值越界 · 类型不对 · 不在 `of` ⇒ 逐格的码；`line` 两窗同值并成一个数。
#[test]
fn a_value_is_checked_against_its_declaration() {
    let one = find(builtins(), "1").expect("内置 1");
    let d = |k: &str| {
        one.params
            .iter()
            .chain(&one.acct_params)
            .find(|d| d.k == k)
            .cloned()
            .expect(k)
    };
    assert_eq!(form::check(&d("delay"), &json!(121)), Err(Code("range")));
    assert_eq!(form::check(&d("delay"), &json!("x")), Err(Code("type")));
    assert_eq!(form::check(&d("all_out"), &json!("maybe")), Err(Code("of")));
    assert_eq!(form::check(&d("stint"), &Value::Null), Ok(Value::Null));
    assert_eq!(form::check(&d("delay"), &Value::Null), Err(Code("type")));
    assert_eq!(
        form::check(&d("line"), &json!({"5h": 80, "7d": 80})),
        Ok(json!(80))
    );
    assert_eq!(
        form::check(&d("line"), &json!({"7d": 80})),
        Ok(json!({"7d": 80}))
    );
    assert_eq!(form::check(&d("line"), &json!(100)), Err(Code("range")));
    assert_eq!(
        form::check_cell(
            &d("slots"),
            &json!([{"at": "17:00-02:00", "line": 80}, {"at": "01:00-03:00", "line": 0}])
        ),
        Err((Code("overlap"), "[1].at".into()))
    );
    assert_eq!(
        form::check_cell(&d("slots"), &json!([{"at": "25:00-02:00", "line": 80}])),
        Err((Code("time"), "[0].at".into()))
    );
}

/// ★ S12：内置与本机同一函数读；本机同 id ⇒ `dupId`；编不过的那份读得出行列；`lib.rhai` 不在列表里。
#[test]
fn builtins_and_local_files_load_the_same_way() {
    let dir = std::env::temp_dir().join(format!("ccm-strategies-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("2.rhai"), BUILTIN[1].1).expect("w");
    std::fs::write(dir.join("broken.rhai"), "fn meta() {\n  #{ format: 1 \n}\n").expect("w");
    std::fs::write(
        dir.join("mine.rhai"),
        BUILTIN[2]
            .1
            .replace("format: 1,", "format: 1, name: \"我的\","),
    )
    .expect("w");
    std::fs::write(dir.join("lib.rhai"), "// 不是策略").expect("w");
    let all = all(Some(&dir));
    let ids: Vec<&str> = all.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "3", "2", "broken", "lib", "mine"]);
    assert_eq!(all[3].error.as_ref().map(|e| e.msg.as_str()), Some(DUP_ID));
    let broken = &all[4];
    assert!(!broken.usable());
    let e = broken.error.as_ref().expect("错");
    assert!(e.line.is_some() && e.col.is_some(), "{e:?}");
    assert!(!all[5].usable(), "lib.rhai 当策略读不出来");
    assert_eq!(all[6].name, "我的");
    assert!(all[6].usable());
    assert_eq!(
        all[0].name,
        copy_core::copy_text("beRotation.strategy.builtinName", &[("id", "1")])
    );
    // 改了文件 ⇒ 重读。
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(dir.join("broken.rhai"), BUILTIN[0].1).expect("w");
    let again = super::all(Some(&dir));
    assert!(again[4].usable());
    let _ = std::fs::remove_dir_all(&dir);
}

/// S13 的策略那一半：`why` / 描述写成字 —— `@` 键走引擎的字，时刻按时区写，号换号名，`then` 递归。
#[test]
fn whys_and_descriptions_turn_into_words() {
    let one = find(builtins(), "1").expect("内置 1");
    let tz = crate::common::time::Tz::iana("Asia/Shanghai").expect("tz");
    let now = 1760187780; // 21:03 上海
    let sp = Speak::of(&one, Lang::Zh, now, &tz);
    let pending =
        json!({"k": "@pending", "at": now + 600, "n": 10, "then": {"k": "w_order", "n": 1}});
    assert_eq!(
        sp.why(&pending),
        copy_core::copy_text(
            "beRotation.lib.pending",
            &[("at", "21:13"), ("n", "10"), ("then", "写在第 1")]
        )
    );
    assert_eq!(
        sp.why(&json!({"k": "w_stint", "n": 12.4, "m": 10})),
        "这次已连用 12 点（上限 10），让给别的号"
    );
    assert_eq!(
        sp.why(&json!({"k": "@refused"})),
        copy_core::copy_text("beRotation.lib.refusedOpen", &[])
    );
    let d = sp.describe(&json!(["d_list", {"k": "@d_n", "n": 5}, {"k": "@d_acctSlot", "acct": "lab", "at": "17:00-02:00", "n": 80}]));
    assert_eq!(
        d,
        [
            "按写下的先后用".to_string(),
            copy_core::copy_text("beRotation.desc.n", &[("n", "5")]),
            copy_core::copy_text(
                "beRotation.desc.acctSlot",
                &[("acct", "lab"), ("at", "17:00-02:00"), ("n", "80")]
            ),
        ]
        .join(&copy_core::copy_text("beRotation.desc.sep", &[]))
    );
}

/// S4：共用小函数的几处边界（直接调 `lib::`）。
#[test]
fn lib_usable_reads_facts_lines_and_slots() {
    let src = tiny("let out = #{}; for a in ctx.accounts { let u = lib::usable(a, ctx); out[a.id] = #{ p: if u.ok { 1 } else { () }, why: if u.why == () { \"w\" } else { u.why }, back: u.back }; } out")
        .replace(
            "acct_params: []",
            "acct_params: [#{ k: \"line\", type: \"line\", init: 90, label: \"w\" }, #{ k: \"slots\", type: \"slots\", init: [], label: \"w\" }]",
        );
    let s = local("probe", &src);
    assert!(s.usable(), "{:?}", s.error);
    // c13：lab 17:00–23:00 取 0 ⇒ @off、回来 ＝ 时段止；work 5h 到 90 ⇒ @line 5h；按量号恒能用。
    let o = run_sample(&s, &sample("c13")).expect("跑得通");
    assert_eq!(o.rows["lab"].why["k"], "@off");
    assert_eq!(
        o.rows["lab"].back,
        Some(1760187780 + (23 * 60 - 1050) as u64 * 60)
    );
    assert_eq!(
        o.rows["work"].why,
        json!({"k": "@line", "w": "5h", "n": 90, "at": 1760193600u64})
    );
    assert_eq!(o.rows["api"].p, Some(1.0));
    // c8：被拒 ⇒ @refused 带回来的时刻。
    let o = run_sample(&s, &sample("c8")).expect("跑得通");
    assert_eq!(o.rows["work"].why["k"], "@refused");
    assert_eq!(o.rows["work"].back, Some(1760188980));
    // c12：7d 用满 ⇒ @full。
    let o = run_sample(&s, &sample("c12")).expect("跑得通");
    assert_eq!(o.rows["personal"].why["k"], "@full");
}
