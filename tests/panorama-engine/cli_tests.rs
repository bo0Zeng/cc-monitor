//! 〔RM1c · 第四波〕`cc-monitor-panorama` 自己的判据。
//!
//! 夹具仓是**合成的**两个小源文件（只有结构：两个函数、一次调用），不含任何会话正文。
//! 引擎是真的（vendored `code-picture-core`，真解析、真 SQLite），一个 mock 都没有。

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 本文件的生产段（剥掉注释与 `#[cfg(test)]` 块）。
fn prod() -> String {
    guard_core::production_code(include_str!("../../src/panorama-engine/main.rs"))
}

/// 一个私有的临时目录（进程号 ＋ 序号，互不相撞；用完删）。
struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "ccm-panorama-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 合成夹具仓：`src/lib.rs` 里 `alpha` 调 `beta`。
fn fixture_repo() -> Tmp {
    let t = Tmp::new("repo");
    std::fs::create_dir_all(t.0.join("src")).unwrap();
    std::fs::write(
        t.0.join("src/lib.rs"),
        "pub fn alpha() -> u32 {\n    beta() + 1\n}\n\npub fn beta() -> u32 {\n    41\n}\n",
    )
    .unwrap();
    t
}

fn call(op: &str, repo: &Path, store: &Path, args: Value) -> Result<Value, Fail> {
    run(op, Some(repo), Some(store), args)
}

/// 从某个函数体里抽 `match` 臂上的字符串字面量（`"xxx" =>`）。
fn arms_of(src: &str, func: &str) -> Vec<String> {
    let at = src
        .find(&format!("fn {func}("))
        .unwrap_or_else(|| panic!("生产段里找不到 `fn {func}(` —— 分派改名了，本条跟着改"));
    // 函数体：从第一个 `{` 起按大括号配平。
    let open = at + src[at..].find('{').expect("函数没有体");
    let mut depth = 0i32;
    let mut end = src.len();
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &src[open..end];
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('"') {
            if let Some(q) = rest.find('"') {
                if rest[q + 1..].trim_start().starts_with("=>") {
                    out.push(rest[..q].to_string());
                }
            }
        }
    }
    out
}

/// ★ op 表 == 两个分派函数的臂（**两向集合相等**，异源：一侧是常量表，一侧从源码抽）。
///
/// 多一条臂没进表 ⇒ `--probe` 不报它、后端永远不会问它（死代码）；
/// 表里多一个 op 没有臂 ⇒ 探测说「会」、一问就是「不在分派里」（假能力）。
#[test]
fn the_op_table_equals_the_dispatch_arms() {
    let src = prod();
    let mut arms = arms_of(&src, "dispatch");
    arms.extend(arms_of(&src, "dispatch_registry"));
    arms.extend(arms_of(&src, "dispatch_plan"));
    arms.sort();
    let mut table: Vec<String> = OPS.iter().map(|(n, _)| n.to_string()).collect();
    table.sort();
    // 反空真：抽臂的尺子坏了会抽出空集，而空集与空表「相等」。
    assert!(arms.len() >= 10, "只抽到 {} 条臂 —— 抽取坏了", arms.len());
    assert_eq!(
        arms, table,
        "op 表与分派臂对不上 —— 加一个 op 要两处同拍改（表里一行 ＋ 分派一条臂）"
    );
    // 表里没有重名。
    let mut dedup = table.clone();
    dedup.dedup();
    assert_eq!(dedup, table, "op 表里有重名");
}

/// 抽臂那把尺子的正控：合成一个多臂的分派必须抽得出来。
#[test]
fn the_arm_ruler_sees_a_synthetic_arm() {
    let src = "fn dispatch(op: &str) {\n    match op {\n        \"a\" => 1,\n        \"b_c\" => {\n            2\n        }\n        other => 3,\n    }\n}\n";
    assert_eq!(arms_of(src, "dispatch"), vec!["a", "b_c"]);
}

/// ★ 每个 op 在**真引擎**上真跑一遍：没有一个落进「不在分派里」，而且读数是对的。
#[test]
fn every_op_runs_on_a_real_engine_over_a_synthetic_repo() {
    let repo = fixture_repo();
    let store = Tmp::new("store");
    let (r, s) = (repo.0.as_path(), store.0.as_path());

    // 建索引之前：一个符号都没有（F69 / D20：开面板不自动扫）。
    let st = call("status", r, s, json!({})).unwrap();
    assert_eq!(st["symbols"], json!(0));
    assert_eq!(st["indexedAt"], Value::Null);

    let stats = call("index", r, s, json!({})).unwrap();
    assert_eq!(stats["files"], json!(1), "夹具仓恰好一个源文件：{stats}");
    assert_eq!(stats["symbols"], json!(2), "夹具仓恰好两个函数：{stats}");

    let st = call("status", r, s, json!({})).unwrap();
    assert_eq!(st["symbols"], json!(2));
    assert!(st["indexedAt"].is_u64(), "建过索引就该有时刻：{st}");
    // status 三格的键集 == monitor `PanoramaStatus` 的三格（下一条判据对拍 TS 那一侧）。
    let mut keys: Vec<&String> = st.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["indexedAt", "stale", "symbols"]);

    let found = call("search", r, s, json!({"query": "alpha"})).unwrap();
    let alpha = found[0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("搜不到 alpha：{found}"))
        .to_string();
    let beta_id = call("search", r, s, json!({"query": "beta", "limit": 5})).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let callees = call("callees", r, s, json!({"symbol": alpha, "depth": 1})).unwrap();
    let callee_ids: Vec<&str> = callees
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["to"].as_str())
        .collect();
    assert_eq!(
        callee_ids,
        vec![beta_id.as_str()],
        "alpha 恰好调 beta：{callees}"
    );
    let callers = call("callers", r, s, json!({"symbol": beta_id, "depth": 1})).unwrap();
    assert_eq!(callers.as_array().unwrap().len(), 1, "{callers}");

    let file_syms = call("symbols_in_file", r, s, json!({"file": "src/lib.rs"})).unwrap();
    assert_eq!(file_syms.as_array().unwrap().len(), 2, "{file_syms}");
    let touching = call(
        "touching",
        r,
        s,
        json!({"files": ["src/lib.rs"], "ranges": []}),
    )
    .unwrap();
    assert_eq!(touching.as_array().unwrap().len(), 2, "{touching}");

    // 其余 op：只要求「跑得通、不落进分派兜底」—— 读数的语义归上游（CP1：我们这侧不算）。
    let mut ran: Vec<&str> = vec![
        "status",
        "index",
        "search",
        "callees",
        "callers",
        "symbols_in_file",
        "touching",
    ];
    for (op, args) in [
        ("reindex", json!({})),
        ("overview", json!({"budget": 2000})),
        ("node", json!({"symbol": alpha})),
        ("neighborhood", json!({"symbol": alpha, "depth": 1})),
        ("impact", json!({"symbol": beta_id})),
        ("docs_for", json!({"symbol": alpha})),
        ("drift", json!({})),
        ("list_annotations", json!({})),
        (
            "diagram",
            json!({"kind": "calls", "request": {"symbol": alpha}}),
        ),
    ] {
        let got = call(op, r, s, args);
        assert!(got.is_ok(), "op `{op}` 在真引擎上失败：{got:?}");
        ran.push(op);
    }
    // 〔RM1d〕只算不写的那几个：跑得通、回的是一份计划（落盘不归本程序）。
    for (op, args) in [
        (
            "plan_add_annotation",
            json!({"file": "src/lib.rs", "symbol": "alpha", "body": "b", "author": "me"}),
        ),
        (
            "plan_propose_annotation",
            json!({"file": "src/lib.rs", "body": "p", "author": "agent"}),
        ),
        ("plan_approve_annotation", json!({"id": "abc"})),
        ("plan_remove_annotation", json!({"id": "abc"})),
        (
            "plan_write_doc_link",
            json!({"doc": "README.md", "target": "src/lib.rs#alpha"}),
        ),
        (
            "plan_remove_doc_link",
            json!({"doc": "README.md", "target": "src/lib.rs#alpha"}),
        ),
    ] {
        let got = call(op, r, s, args).unwrap_or_else(|f| panic!("op `{op}` 失败：{f:?}"));
        assert!(
            got.get("value").is_some() && got.get("edit").is_some(),
            "{op} ⇒ {got}"
        );
        ran.push(op);
    }
    call("refresh_doc_links", r, s, json!({})).unwrap();
    ran.push("refresh_doc_links");
    let kinds = run("diagram_kinds", None, None, json!({})).unwrap();
    assert!(kinds.as_array().is_some_and(|a| !a.is_empty()), "{kinds}");
    ran.push("diagram_kinds");
    // 图带 Mermaid 文本（「复制给 agent」与「画不出这种形状」兜底要它）。
    let d = call(
        "diagram",
        r,
        s,
        json!({"kind": "calls", "request": {"symbol": alpha}}),
    )
    .unwrap();
    assert!(d["mermaid"].as_str().is_some_and(|m| !m.is_empty()), "{d}");
    assert!(d["diagram"].is_object(), "{d}");

    // ★ 覆盖面相等：本条真跑过的 op == op 表（漏跑一个就红，别让新 op 躲在「没人跑过」里）。
    ran.sort();
    ran.dedup();
    let mut table: Vec<&str> = OPS.iter().map(|(n, _)| *n).collect();
    table.sort();
    assert_eq!(ran, table, "有 op 没在真引擎上跑过");
}

/// 要求住址：`设计/97 §1` 判据 CP1「人那一侧**不许出现任何图分析**……只许三件事：取 · 画 · 收」· `99 §1` V158「前端不算图（CP1）」。
///
/// ★ 「距根几跳」由本程序给（前端只按它分组）：链 `a → b → c` 上，`a` 两跳内 = `b`@1 · `c`@2；
/// `c` 一跳内只有调它的 `b`（调用方那一侧也算邻域）；根不进结果。真引擎、合成夹具。
#[test]
fn the_neighborhood_says_how_many_hops_each_symbol_is() {
    let t = Tmp::new("chain");
    std::fs::create_dir_all(t.0.join("src")).unwrap();
    std::fs::write(
        t.0.join("src/lib.rs"),
        "pub fn a() -> u32 {\n    b()\n}\n\npub fn b() -> u32 {\n    c()\n}\n\npub fn c() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    let store = Tmp::new("store");
    let (r, s) = (t.0.as_path(), store.0.as_path());
    call("index", r, s, json!({})).unwrap();
    let id = |n: &str| format!("src/lib.rs#{n}");
    let hops = |sym: &str, depth: u32| -> Vec<(String, u64)> {
        let v = call(
            "neighborhood",
            r,
            s,
            json!({"symbol": id(sym), "depth": depth}),
        )
        .unwrap();
        assert_eq!(v["root"], json!(id(sym)), "{v}");
        v["reached"]
            .as_array()
            .unwrap_or_else(|| panic!("没有 reached：{v}"))
            .iter()
            .map(|x| {
                (
                    x["id"].as_str().unwrap().to_string(),
                    x["depth"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(hops("a", 2), vec![(id("b"), 1), (id("c"), 2)]);
    assert_eq!(hops("a", 1), vec![(id("b"), 1)], "一跳就只到一跳");
    assert_eq!(hops("c", 1), vec![(id("b"), 1)], "调它的那一侧也算");
}

/// ★ `status` 的三格 == 前端 `PanoramaStatus`（`src/frontend/ui/panorama/types.ts` 那个手写接口 —— 异源：前端按它收）。
///
/// 〔RM1f · V108 后半句〕对拍的另一侧从 ts-rs 生成物（`src/frontend/ui/generated/PanoramaStatus.ts`，源是 monitor 那份 Rust
/// 类型）换成前端手写的那一份：monitor 那份 Rust 源随内嵌引擎删了，**本程序就是这个形状今天唯一的产出方**，
/// 前端认的那一份是唯一的消费方 —— 本条钉两者逐格相等。
///
/// ⚠ 跨树运行时读（不走 `include_str!`）：同 `panorama_locus_guard` 的取舍 —— 文件挪了只有本条红。
#[test]
fn the_status_shape_matches_the_monitor_dto() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/ui/panorama/types.ts");
    let ts = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    let at = ts.find("export interface PanoramaStatus {").expect(
        "前端 types.ts 里找不到 `export interface PanoramaStatus {` —— 改了写法，本条跟着改",
    );
    let body = &ts[at..at + ts[at..].find('}').expect("接口没收尾")];
    let body = &body[body.find('{').unwrap() + 1..];
    let mut fields: Vec<String> = body
        .lines()
        .filter_map(|l| {
            let (k, _) = l.trim().split_once(':')?;
            let k = k.trim();
            (!k.is_empty() && !k.starts_with('/') && !k.starts_with('*')).then(|| k.to_string())
        })
        .collect();
    fields.sort();
    assert_eq!(
        fields,
        vec!["indexedAt", "stale", "symbols"],
        "TS 那侧：{body}"
    );
    let src = prod();
    for f in &fields {
        assert!(
            src.contains(&format!("\"{f}\":")),
            "本程序的 status 应答里没有 `{f}` 这一格"
        );
    }
}

/// ★ 写用户文件的那几样，本程序生产段**零调用**（只算不写，理由见头注）。
///
/// 针 =（a）`Engine` 那六个写方法的**方法调用形**（`.add_annotation(` …）——〔RM1d〕只认方法形，
/// 因为「算」那一层的名字 `edits::plan_add_annotation(` 里含着裸名；（b）上游**写盘那一层**的
/// 模块路径（`annotations::apply` · `annotations::write` · `annotations::remove` · `docs::apply` ·
/// `docs::write_doc_link` · `docs::remove_doc_link`，不带括号 ⇒ `use` 进来改名也逮得住）。
/// 人群 = 本程序生产段（剥注释、剥测试块）。
#[test]
fn the_program_never_calls_an_engine_method_that_writes_user_files() {
    let needles = [
        ".add_annotation(",
        ".propose_annotation(",
        ".approve_annotation(",
        ".remove_annotation(",
        ".write_doc_link(",
        ".remove_doc_link(",
        "annotations::apply",
        "annotations::write",
        "annotations::remove",
        "docs::apply",
        "docs::write_doc_link",
        "docs::remove_doc_link",
    ];
    let src = prod();
    // 反空真：人群得是真的那份（读到空串会零命中地绿）。
    assert!(src.contains("fn dispatch("), "生产段没读到真文件");
    let hits: Vec<&str> = needles
        .iter()
        .copied()
        .filter(|n| src.contains(n))
        .collect();
    assert!(
        hits.is_empty(),
        "本程序调了写用户文件的引擎方法：{hits:?}\n\
         ⇒ 远端仓的批注 / 文档关联是远端的**用户文件**，按用户 09-24「只允许后端的文件管理部分写用户文件」\
         要走后端写面那一路（RW1），不在本程序里开第二条写路。"
    );
    // 反空真：「算」那一层确实在用（不然零命中可能只是因为这几样整个没做）。
    assert!(
        src.contains("edits::plan_add_annotation("),
        "生产段里没有「算」那一层"
    );
    // 正控：同一把尺子对合成的真调用必须逐条命中（尺子没瞎）—— 方法形与写盘层各一行。
    for (line, want) in [
        (
            "fn x(e: &mut Engine) { e.write_doc_link(\"a.md\", \"s\").unwrap(); }\n",
            ".write_doc_link(",
        ),
        (
            "fn x(r: &Path, p: &FileEdit) { code_picture_core::docs::apply(r, p).unwrap(); }\n",
            "docs::apply",
        ),
        (
            "use code_picture_core::annotations::apply as put;\n",
            "annotations::apply",
        ),
    ] {
        let synthetic = guard_core::production_code(line);
        let hit: Vec<&str> = needles
            .iter()
            .copied()
            .filter(|n| synthetic.contains(n))
            .collect();
        assert_eq!(hit, vec![want], "尺子对 {line:?} 没认对");
    }
}

/// ★〔RM1d〕「算」那几个 op 对被分析的仓**一个字节都不写**，而且交回的 `before` 就是盘上原样
/// （起它的那一侧拿它当 `files-put` 的 CAS 期望 —— 不是原样，写口就会恒 `stale`）。
///
/// ⚠ 不裸遍历目录（`scanning_guard_registry` 那条元判据）：比的是两份具体的文件 ＋ 批注目录在不在。
#[test]
fn planning_ops_leave_the_repo_byte_identical() {
    let repo = fixture_repo();
    let store = Tmp::new("store");
    let (r, s) = (repo.0.as_path(), store.0.as_path());
    let md = "---\ncovers: [src/lib.rs#beta]\n---\n# 说明\n";
    std::fs::write(r.join("NOTES.md"), md).unwrap();
    let lib = std::fs::read(r.join("src/lib.rs")).unwrap();

    let add = call(
        "plan_add_annotation",
        r,
        s,
        json!({"file": "src/lib.rs", "symbol": "alpha", "body": "热路径", "author": "me"}),
    )
    .unwrap();
    let e = &add["edit"];
    assert_eq!(
        e["before"],
        Value::Null,
        "批注还不存在 ⇒ before 必须是 null（CAS：必须不存在）"
    );
    assert!(
        e["after"].as_str().is_some_and(|t| t.contains("热路径")),
        "{add}"
    );
    assert_eq!(e["parents"], json!(true), "批注首写要建目录");
    let id = add["value"].as_str().unwrap();
    assert!(
        e["rel"]
            .as_str()
            .is_some_and(|x| x.ends_with(&format!("{id}.json"))),
        "{add}"
    );

    let link = call(
        "plan_write_doc_link",
        r,
        s,
        json!({"doc": "NOTES.md", "target": "src/lib.rs#alpha"}),
    )
    .unwrap();
    assert_eq!(link["edit"]["before"], json!(md), "before 必须是盘上原样");
    assert_eq!(link["edit"]["rel"], json!("NOTES.md"));
    let unlink = call(
        "plan_remove_doc_link",
        r,
        s,
        json!({"doc": "NOTES.md", "target": "src/lib.rs#beta"}),
    )
    .unwrap();
    assert_eq!(unlink["value"], json!(true), "{unlink}");
    let none = call(
        "plan_remove_doc_link",
        r,
        s,
        json!({"doc": "NOTES.md", "target": "src/nope.rs"}),
    )
    .unwrap();
    assert_eq!(
        none,
        json!({"value": false, "edit": null}),
        "没有要写的 ⇒ edit 是 null"
    );

    assert!(!r.join(".codepicture").exists(), "算的那一步建了批注目录");
    assert_eq!(
        std::fs::read_to_string(r.join("NOTES.md")).unwrap(),
        md,
        "算的那一步改了 .md"
    );
    assert_eq!(std::fs::read(r.join("src/lib.rs")).unwrap(), lib);
    assert!(!s.join(".codepicture").exists(), "算的那一步碰了索引根");
    // 给错了东西 ⇒ bad_args（越界路径不许算出落点）。
    assert!(matches!(
        call(
            "plan_write_doc_link",
            r,
            s,
            json!({"doc": "../x.md", "target": "t"})
        ),
        Err(Fail::BadArgs(_))
    ));
}

/// ★ 建索引与全部读 op 之后，被分析的仓里**没有长出索引侧车**（索引只落 `--store`）。
///
/// ⚠ 不裸遍历目录（`scanning_guard_registry` 那条元判据）：看的是两件具体的事 ——
/// 仓里那个侧车目录在不在、仓里的源文件集合变没变。
#[test]
fn nothing_lands_inside_the_analysed_repo() {
    let repo = fixture_repo();
    let store = Tmp::new("store");
    let sources = |root: &Path| -> Vec<PathBuf> {
        guard_core::scan_tree_excluding(root, &["rs", "json", "md", "db"], &[])
            .into_iter()
            .map(|(p, _)| p)
            .collect()
    };
    let before = sources(&repo.0);
    call("index", &repo.0, &store.0, json!({})).unwrap();
    call("overview", &repo.0, &store.0, json!({})).unwrap();
    call("drift", &repo.0, &store.0, json!({})).unwrap();
    call("list_annotations", &repo.0, &store.0, json!({})).unwrap();
    assert!(
        !repo.0.join(".codepicture").exists(),
        "被分析的仓里长出了索引侧车目录"
    );
    assert_eq!(sources(&repo.0), before, "仓里多了文件");
    // 反空真：索引真落在了 store 里（不然「仓里没多」可能只是因为根本没建）——
    // 换一个全新的进程视角（再开一次引擎）问符号数，得是夹具那两个。
    assert!(
        store.0.join(".codepicture").is_dir(),
        "索引根里没有侧车目录"
    );
    let st = call("status", &repo.0, &store.0, json!({})).unwrap();
    assert_eq!(st["symbols"], json!(2), "索引没落进 store：{st}");
}

/// ★ 失败两类各自落到对的退出码与 `code`，而且应答**恰一行**。
#[test]
fn failures_land_on_the_right_exit_code_in_exactly_one_line() {
    let repo = fixture_repo();
    let store = Tmp::new("store");
    let r = repo.0.display().to_string();
    let s = store.0.display().to_string();
    let argv = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let cases: Vec<(Vec<String>, i32, &str)> = vec![
        (argv(&[]), EXIT_BAD_ARGS, "bad_args"),
        (
            argv(&["nope", "--repo", &r, "--store", &s]),
            EXIT_BAD_ARGS,
            "bad_args",
        ),
        (argv(&["status", "--repo", &r]), EXIT_BAD_ARGS, "bad_args"),
        (argv(&["status", "--store", &s]), EXIT_BAD_ARGS, "bad_args"),
        (
            argv(&["status", "--repo", "rel/path", "--store", &s]),
            EXIT_BAD_ARGS,
            "bad_args",
        ),
        (
            argv(&["status", "--repo", &r, "--store", &s, "--bogus", "1"]),
            EXIT_BAD_ARGS,
            "bad_args",
        ),
        (
            argv(&["status", "--repo", &r, "--store", &s, "--args", "{"]),
            EXIT_BAD_ARGS,
            "bad_args",
        ),
        // 拼错的字段名不许被静默忽略。
        (
            argv(&[
                "search",
                "--repo",
                &r,
                "--store",
                &s,
                "--args",
                "{\"qeury\":\"a\"}",
            ]),
            EXIT_BAD_ARGS,
            "bad_args",
        ),
        (argv(&["--probe", "x"]), EXIT_BAD_ARGS, "bad_args"),
        (
            argv(&[
                "status",
                "--repo",
                "/definitely/not/here/ccm",
                "--store",
                &s,
            ]),
            EXIT_FAILED,
            "failed",
        ),
    ];
    for (a, want_exit, want_code) in cases {
        let (out, err, exit) = answer(&a);
        assert_eq!(exit, want_exit, "{a:?} ⇒ {out}");
        assert_eq!(out.matches('\n').count(), 1, "应答不是恰一行：{out:?}");
        let v: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(v["ok"], json!(false), "{a:?}");
        assert_eq!(v["code"], json!(want_code), "{a:?} ⇒ {out}");
        assert_eq!(
            err.trim_end(),
            v["message"].as_str().unwrap(),
            "stderr 那一行 == message"
        );
    }
    // 成功那一形：恰一行、退出码 0。
    let (out, err, exit) = answer(&argv(&["status", "--repo", &r, "--store", &s]));
    assert_eq!((exit, err.as_str()), (0, ""));
    assert_eq!(out.matches('\n').count(), 1, "{out:?}");
    assert_eq!(
        serde_json::from_str::<Value>(out.trim_end()).unwrap()["ok"],
        json!(true)
    );
}

/// `--probe` 说插件口那套方言：首行身份逐字、能力行是 op 表。
#[test]
fn the_probe_speaks_the_plugin_dialect() {
    let (out, err, exit) = answer(&["--probe".to_string()]);
    assert_eq!((exit, err.as_str()), (0, ""));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], format!("name={NAME}"), "首行必须是身份行");
    let caps = lines
        .iter()
        .find_map(|l| l.strip_prefix("capabilities="))
        .expect("没有能力行");
    let mut caps: Vec<&str> = caps.split(',').collect();
    caps.sort();
    let mut table: Vec<&str> = OPS.iter().map(|(n, _)| *n).collect();
    table.sort();
    assert_eq!(caps, table);
    let mut long: Vec<&str> = lines
        .iter()
        .find_map(|l| l.strip_prefix("long="))
        .expect("没有长活档那一行")
        .split(',')
        .collect();
    long.sort();
    let mut built: Vec<&str> = OPS
        .iter()
        .filter(|(_, n)| *n == Need::Build)
        .map(|(o, _)| *o)
        .collect();
    built.sort();
    assert_eq!(long, built, "长活档 == 独占写（建索引）那几个");
    let shape = lines
        .iter()
        .find_map(|l| l.strip_prefix("shape="))
        .expect("没有形状行");
    assert_eq!(shape, shape_code());
}

/// 生成物 `engine-contract.json` 的全文：op 表（名 → 档）＋ 形状代号。
fn contract_json() -> String {
    let ops: serde_json::Map<String, Value> = OPS
        .iter()
        .map(|(n, need)| {
            (
                n.to_string(),
                json!(if need.is_long() { "long" } else { "quick" }),
            )
        })
        .collect();
    let v = json!({
        "_generated": "由 src/panorama-engine/main.rs 的 OPS 与 shape_code() 生成（tests/panorama-engine/cli_tests.rs::the_frontend_contract_is_generated_from_this_program），不许手改",
        "shape": shape_code(),
        "ops": ops,
    });
    format!("{}\n", serde_json::to_string_pretty(&v).unwrap())
}

/// 要求住址：`99 §1 V158`「本仓只管两件：**解耦**（线上契约由上游给、本仓不手抄 · 前端不算图（CP1）· 后端不带引擎知识）」。
///
/// ★ 前端与判据读的那份生成物 == 本程序的 op 表（名 ＋ 档）＋ 真算出来的形状代号（`97 §6.5` FIX2 那一代）。
/// 前端每问把 `shape` 带上、按档给期限；后端只拿它与 `--probe` 报的比，自己不存（`control/panorama.rs` 头注）。
/// 漂了 ⇒ 当场按本程序重写并红一次（重跑即绿，把它一起提交）—— 同 ts-rs 生成物「改源不重生成就红」那条纪律。
#[test]
fn the_frontend_contract_is_generated_from_this_program() {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/ui/panorama/engine-contract.json");
    let want = contract_json();
    let have = std::fs::read_to_string(&p).unwrap_or_default();
    if have != want {
        std::fs::write(&p, &want).unwrap();
        panic!("生成物 {p:?} 与本程序的 op 表 / 形状代号对不上，已按本程序重写 —— 重跑即绿，把它一起提交");
    }
    assert_eq!(shape_code().len(), 16);
    assert!(vendor_pin().len() >= 7, "pin 没读到：{:?}", vendor_pin());
}

/// 索引根的锁：建索引独占、读共享（两个进程并发时进程内的锁管不到，只能靠它）。
#[test]
fn building_holds_the_store_exclusively_and_reads_share_it() {
    let store = Tmp::new("lock");
    let r1 = lock_store(&store.0, Need::Read).unwrap();
    let r2 = lock_store(&store.0, Need::Read).unwrap();
    // 有读在 ⇒ 独占拿不到（用 try，别把测试挂死）。
    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(store.0.join(".lock"))
        .unwrap();
    assert!(probe.try_lock().is_err(), "读锁在的时候独占锁居然拿到了");
    drop((r1, r2));
    assert!(probe.try_lock().is_ok(), "读锁都放了，独占锁该拿得到");
    drop(probe);
    let w = lock_store(&store.0, Need::Build).unwrap();
    let other = std::fs::OpenOptions::new()
        .write(true)
        .open(store.0.join(".lock"))
        .unwrap();
    assert!(other.try_lock_shared().is_err(), "建索引时读锁居然拿到了");
    drop(w);
}

/// 〔W5-AUX · 97 #5〕**判据 CP5**：批注的状态是数据 —— **提议的批注 agent 看不见，批准之后才看得见**。
///
/// 要求住址：`设计/97 §2.3` 逐字「**判据 CP5**：批注的**状态**（谁写的 · 审没审 · agent 看不看得见）**必须是数据，不是文案**。
/// 状态机住上游 `engine`（`propose_annotation` 写 `Proposed`、`approve_annotation` 改 `Active`、`annotations_for` 只回 `Active`）」；
/// `§8` 逐字「CP5 在仓内没有执行链上的判据 —— 原先那条……真引擎判据住 monitor 的 `panorama_tests.rs`，随 monitor 摘引擎一起删了；……
/// 要在小程序那一侧补一条（例如经 `list_annotations` / `node` 读回），还是认上游那条，没定」。本条就是「小程序那一侧补一条」
/// （题面裁：W5-AUX「97 CP5 仓内判据」）。
///
/// 读回走的是**小程序自己的 op**（真引擎、真侧车文件，零 mock）：人那一侧 = `list_annotations`（审批队列读它，含 `Proposed`）；
/// agent 那一侧 = `node` 的 `annotations`（上游 `Engine::node` 里就是 `annotations_for`，只回 `Active`）。
/// 写由测试照计划原样落盘 —— 同生产里「引擎只算、文件管理来写」（V110）那一步，小程序自己一个字节不写。
///
/// 反空真：同一个 `node` 查询在批准之后**必须看得见**它 —— 否则「看不见」可能只是那一格恒空；
/// 另加一条人写的（`plan_add_annotation`，直接 `Active`）当场就要看得见。
#[test]
fn a_proposed_annotation_stays_invisible_to_the_agent_until_it_is_approved() {
    let repo = fixture_repo();
    let store = Tmp::new("store");
    let (r, s) = (repo.0.as_path(), store.0.as_path());
    call("index", r, s, json!({})).unwrap();
    let alpha = call("search", r, s, json!({"query": "alpha"})).unwrap()[0]["id"]
        .as_str()
        .expect("搜得到 alpha")
        .to_string();
    // 文件管理那一步：照计划原样落盘（before 就是 CAS 期望 —— 这里是独占夹具，逐字核一下即可）。
    let apply = |planned: &Value| {
        let e = &planned["edit"];
        let p = r.join(e["rel"].as_str().expect("计划里有落点"));
        let now = std::fs::read_to_string(&p).ok();
        assert_eq!(
            now.as_deref(),
            e["before"].as_str(),
            "计划的 before 与盘上不符：{planned}"
        );
        if e["parents"] == json!(true) {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        }
        match e["after"].as_str() {
            Some(t) => std::fs::write(&p, t).unwrap(),
            None => std::fs::remove_file(&p).unwrap(),
        }
    };
    let agent_sees = |id: &str| -> bool {
        let node = call("node", r, s, json!({"symbol": alpha})).unwrap();
        node["annotations"]
            .as_array()
            .unwrap_or_else(|| panic!("node 应答里没有 annotations 数组：{node}"))
            .iter()
            .any(|a| a["id"] == json!(id))
    };
    let queue_status = |id: &str| -> Value {
        let all = call("list_annotations", r, s, json!({})).unwrap();
        all.as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == json!(id))
            .map(|a| a["status"].clone())
            .unwrap_or(Value::Null)
    };

    // ① agent 提议 ⇒ 人那一侧的队列里有它（Proposed），agent 那一侧看不见。
    let prop = call(
        "plan_propose_annotation",
        r,
        s,
        json!({"file": "src/lib.rs", "symbol": "alpha", "body": "这里可以缓存", "author": "agent-x"}),
    )
    .unwrap();
    apply(&prop);
    let pid = prop["value"].as_str().expect("提议回了 id").to_string();
    assert_eq!(
        queue_status(&pid),
        json!("Proposed"),
        "审批队列里读不到这条提议，或状态不是数据里的 Proposed"
    );
    assert!(
        !agent_sees(&pid),
        "🔴 CP5：还没审的提议 agent 已经看得见了 —— agent 会读到自己的话并当成人的指示"
    );

    // ② 人批准 ⇒ 同一个 `node` 查询看得见了（反空真：「看不见」不是那一格恒空）。
    let appr = call("plan_approve_annotation", r, s, json!({"id": pid})).unwrap();
    apply(&appr);
    assert_eq!(
        queue_status(&pid),
        json!("Active"),
        "批准之后状态没变成 Active"
    );
    assert!(agent_sees(&pid), "批准之后 agent 仍看不见 —— 要么状态机坏了，要么 `node` 那一格恒空（那上面「看不见」就证明不了任何事）");

    // ③ 人直接写的 ⇒ 当场 Active、当场看得见（两条路都走一遍，不只靠批准那一条）。
    let add = call(
        "plan_add_annotation",
        r,
        s,
        json!({"file": "src/lib.rs", "symbol": "alpha", "body": "热路径", "author": "me"}),
    )
    .unwrap();
    apply(&add);
    let hid = add["value"].as_str().unwrap().to_string();
    assert_eq!(queue_status(&hid), json!("Active"));
    assert!(agent_sees(&hid), "人写的批注 agent 看不见");
}
