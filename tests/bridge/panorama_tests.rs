/// ★★ **写 doc-link 那条路的路径安全，第一道压在上游的 `guard_doc_rel` 上**
/// 〔audit-0805 08-08，Phase G 第 87 件；〔RM1d〕09-24 随「引擎只算、文件管理来写」改射程〕。
///
/// # 先核的结果（三段，别只读结论）
///
/// 1. 批注的 `file` **不进路径** —— 上游把它当**数据**存进 `Annotation`，落点文件名是内容哈希 ⇒
///    无穿越面，**刻意不给它加守卫**（加了是安慰剂）。
/// 2. 文档关联的 `doc` **真的进路径**（计划里的 `rel` 就是它）。上游「算」那一层自己有围栏：
///    `edits::plan_write_doc_link` / `plan_remove_doc_link`（以及纯的 `next_*`）先过 `docs::guard_doc_rel`，
///    它拒绝绝对路径与 `..`。〔RM1d〕第二道在落盘那一侧：那台机器后端的 `files-put` / `files-delete`
///    以 `root = 仓` 过写面围栏（后端判据管，不在这里判）。
/// 3. `repo` 本身**刻意不设围栏**：全景的功能就是「索引任意一个项目目录」。
///
/// # 本条钉什么
///
/// 第一道是**别人家的几行守卫**，而 vendor 是冻结副本、会被整份换新 ⇒ 前提触发器：那四个「算」函数
/// 必须仍然调 `guard_doc_rel`，且它必须仍然同时拒**绝对路径**与 `..`。**只读 vendor，不改它一个字节。**
#[test]
fn the_doc_link_writes_still_go_through_the_vendor_guard() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/code-picture-core/src");
    let src = ["edits.rs", "docs.rs"]
        .map(|f| {
            let p = dir.join(f);
            std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— vendor 布局变了就把本条一起改"))
        })
        .join("\n");
    // 我们真正调到的「算」函数（本机 `panorama.rs::plan_local`、远端小程序各调一次）＋ 它们的纯内核。
    for m in [
        "plan_write_doc_link",
        "plan_remove_doc_link",
        "next_write_doc_link",
        "next_remove_doc_link",
    ] {
        let at = src.find(&format!("pub fn {m}(")).unwrap_or_else(|| {
            panic!("vendor 里找不到 `{m}` —— 换版了，本条与 `panorama.rs` 一起复核")
        });
        // ⚠ 两件事一起做对，缺一个就白写（08-08 变异实测过）：
        // ① **按 char 取，别按字节切** —— vendor 源码里全是中文注释；
        // ② **切到函数真正的结尾**（大括号配平），不是「起点后 N 个字符」—— 定长窗口会吃进下一个函数。
        let body: String = {
            let rest: Vec<char> = src[at..].chars().take(4000).collect();
            let mut depth = 0i32;
            let mut end = rest.len();
            for (i, c) in rest.iter().enumerate() {
                if *c == '{' {
                    depth += 1;
                } else if *c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            rest[..end].iter().collect()
        };
        let body = body.as_str();
        assert!(
            body.len() > 40 && body.lines().count() < 40,
            "从 `{m}` 切出 {} 行，不像一个函数体（配平切错了，本条会零命中地绿）",
            body.lines().count()
        );
        assert!(
            guard_core::contains_word(body, "guard_doc_rel"),
            "vendor 的 `{m}` 不再调 `guard_doc_rel` 了。\n\
                 ★ 那一行是**算那一侧唯一挡着路径穿越的东西**：`doc` 来自 webview，\n\
                 下游是计划里的 `rel`（本机那条读 `repo.join(doc)`）。\n\
                 换版后要么确认新版另有等价围栏，要么在 `panorama.rs` 这侧自己加一道。\n\
                 实得这一段：{body:?}"
        );
    }
    // 守卫本身还得真守：绝对路径与 `..` 两件都要拒。
    let g = src
        .find("fn guard_doc_rel(")
        .expect("vendor 里找不到 `guard_doc_rel` 定义 —— 上面那几条此刻在比一个不存在的东西");
    let gbody: String = src[g..].chars().take(600).collect();
    let gbody = gbody.as_str();
    for needle in ["starts_with", "\"..\""] {
        assert!(
            guard_core::contains_word(gbody, needle),
            "`guard_doc_rel` 里找不到 `{needle}` —— 它可能只剩半道围栏了。\n\
                 两件缺一不可：绝对路径（`/etc/x.md` 直接跳出 repo）与 `..`（逐级爬出去）。\n\
                 实得：{gbody:?}"
        );
    }
}

/// ★〔RM1d · V110〕monitor 生产段（`panorama.rs` ＋ `panorama_call.rs`）**自己一个字节都不写**批注 /
/// 文档关联：引擎写方法的调用形与上游写盘那一层零命中；落盘只经 `user_files::Door`。
///
/// 针与小程序那条（`tests/panorama-engine/cli_tests.rs::the_program_never_calls_…`）同一张；
/// 反空真：「算」那一层与写口确实在用。
#[test]
fn the_monitor_never_writes_annotations_or_doc_links_itself() {
    let prod = [
        include_str!("../../src/bridge/src/panorama.rs"),
        include_str!("../../src/bridge/src/panorama_call.rs"),
    ]
    .map(guard_core::production_code)
    .join("\n");
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
    let hits: Vec<&str> = needles
        .iter()
        .copied()
        .filter(|n| prod.contains(n))
        .collect();
    assert!(
        hits.is_empty(),
        "monitor 自己写了批注 / 文档关联：{hits:?} —— 用户文件只许那台机器后端的文件管理写（V88 · V110）"
    );
    for must in ["edits::plan_add_annotation(", ".put(", ".delete("] {
        assert!(
            prod.contains(must),
            "生产段里没有 `{must}` —— 「算 · 交」那条路不在了，零命中不作数"
        );
    }
    // 正控：尺子对合成的一行真调用必须命中。
    let synthetic =
        guard_core::production_code("fn x(e: &Engine) { e.approve_annotation(\"1\").ok(); }\n");
    assert!(needles.iter().any(|n| synthetic.contains(n)), "尺子瞎了");
}

/// 测试里代替「那台机器后端的文件管理」把一份计划落盘（**只在判据里**；生产只经 `user_files::Door`）。
fn land<T>(repo: &std::path::Path, p: code_picture_core::edits::Planned<T>) -> T {
    if let Some(e) = &p.edit {
        let at = repo.join(&e.rel);
        match &e.after {
            Some(t) => {
                if e.parents {
                    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                }
                std::fs::write(&at, t).unwrap();
            }
            None => std::fs::remove_file(&at).unwrap(),
        }
    }
    p.value
}

use super::*;

#[test]
fn engine_pool_same_repo_same_arc_distinct_repo_distinct_arc() {
    // 池 get-or-create 语义:同 repo 返同一 Arc（命中,不重复 open）;异 repo 返不同 Arc。
    // 用显式临时 store（`engine_for_with_store`）——不走 panorama_store_dir 免污染真实数据目录。
    let base = std::env::temp_dir();
    let r1 = base.join("cc-monitor-b15-pool-a");
    let r2 = base.join("cc-monitor-b15-pool-b");
    let store = base.join("cc-monitor-b15-pool-store");
    std::fs::create_dir_all(&r1).ok();
    std::fs::create_dir_all(&r2).ok();
    let a1 = engine_for_with_store(r1.to_str().unwrap(), Some(store.clone())).expect("open r1");
    let a1b =
        engine_for_with_store(r1.to_str().unwrap(), Some(store.clone())).expect("open r1 again");
    let a2 = engine_for_with_store(r2.to_str().unwrap(), Some(store.clone())).expect("open r2");
    assert!(
        Arc::ptr_eq(&a1, &a1b),
        "同 repo → 同一 Arc（池命中,不重复 open）"
    );
    assert!(!Arc::ptr_eq(&a1, &a2), "异 repo → 不同 Arc");
}

#[test]
fn store_dir_some_writes_to_store_not_user_repo() {
    // F69/D20 回归防线:store_dir=Some → 索引落 store 目录,**用户仓不被建 .codepicture**
    // （消灭「点🗺就在你仓里凭空建目录」灰区）。防有人把 panorama.rs 的 store_dir 改回 None。
    let base = std::env::temp_dir();
    let repo = base.join("cc-monitor-f69-d20-repo");
    let store = base.join("cc-monitor-f69-d20-store");
    std::fs::remove_dir_all(repo.join(".codepicture")).ok(); // 干净起点
    std::fs::remove_dir_all(&store).ok();
    std::fs::create_dir_all(&repo).ok();
    let _e = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open repo");
    assert!(
        !repo.join(".codepicture").exists(),
        "store_dir=Some 时用户仓不该凭空出现 .codepicture（D20）"
    );
    assert!(
        store.join(".codepicture").exists(),
        "索引应落到 store_dir 下（core 在其下建 .codepicture/<name>-<hash>/）"
    );
    std::fs::remove_dir_all(&store).ok();
}

#[test]
fn symbols_in_file_lists_that_files_symbols() {
    // F71：索引一个含两个函数的临时仓 → collect_symbols_in_file 列出该文件的符号。
    // 显式临时 store（免污染真实数据目录）。走真 tree-sitter（rust grammar）索引。
    let base = std::env::temp_dir();
    let repo = base.join("cc-monitor-f71-symfile-repo");
    let store = base.join("cc-monitor-f71-symfile-store");
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
    std::fs::create_dir_all(&repo).ok();
    std::fs::write(repo.join("lib.rs"), "pub fn alpha() {}\npub fn beta() {}\n").unwrap();
    let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
    {
        let mut g = arc.lock().unwrap();
        g.index().expect("index");
        let names: std::collections::HashSet<String> = collect_symbols_in_file(&g, "lib.rs")
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert!(names.contains("alpha"), "应列出 alpha，实得 {names:?}");
        assert!(names.contains("beta"), "应列出 beta，实得 {names:?}");
        // 不存在的文件 → 空。
        assert!(collect_symbols_in_file(&g, "nope.rs").is_empty());
    }
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
}

#[test]
fn annotation_writes_to_repo_not_store_after_split() {
    // F72:re-vendor 后批注分家——store_dir=Some 时批注落 <repo>/.codepicture/annotations/、
    // 不落 store;写批注前 repo 内无 .codepicture(D20:批注 lazy)。验 re-vendor 的 core 行为。
    let base = std::env::temp_dir();
    let repo = base.join("cc-monitor-f72-ann-repo");
    let store = base.join("cc-monitor-f72-ann-store");
    std::fs::remove_dir_all(repo.join(".codepicture")).ok();
    std::fs::remove_dir_all(&store).ok();
    std::fs::create_dir_all(&repo).ok();
    std::fs::write(repo.join("lib.rs"), "pub fn f() {}\n").unwrap();
    let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
    {
        let mut g = arc.lock().unwrap();
        g.index().expect("index");
        assert!(
            !repo.join(".codepicture").exists(),
            "写批注前 repo 内不该有 .codepicture（D20 lazy）"
        );
        // 〔RM1d〕算（上游 `edits`，只读）＋ 落盘（判据里的替身，生产是那台后端的 `files-put`）。
        let planned =
            code_picture_core::edits::plan_add_annotation(&repo, "lib.rs", Some("f"), "note", "me")
                .expect("plan_add_annotation");
        assert!(
            !repo.join(".codepicture").exists(),
            "算那一步建了批注目录（D20 lazy）"
        );
        let id = land(&repo, planned);
        assert!(
            repo.join(".codepicture")
                .join("annotations")
                .join(format!("{id}.json"))
                .is_file(),
            "批注应落 <repo>/.codepicture/annotations/"
        );
        let store_has_ann = std::fs::read_dir(store.join(".codepicture"))
            .map(|rd| rd.flatten().any(|e| e.path().join("annotations").exists()))
            .unwrap_or(false);
        assert!(!store_has_ann, "批注不该落 store（已分家回仓）");
        assert_eq!(
            g.annotations_for(&"lib.rs#f".to_string()).len(),
            1,
            "应读回批注"
        );
    }
    std::fs::remove_dir_all(repo.join(".codepicture")).ok();
    std::fs::remove_dir_all(&store).ok();
}

/// `P8b-Y1`：`#79` 那半的边界必须留在本文件的头注上，且说的是
/// **「上游还没有」**而不是**「我们还没做」**。
///
/// 为什么值得立一条判据钉一段散文：它省的是**几天** —— 下一个被指派这件事的人
/// 若不知道上游零实现，会先花时间去找「code-picture 的 test 那块 API 在哪」。
///
/// ⚠ 读 `production_source` 而**不是** `production_code`：后者连 `//` 注释一起剥，
/// 而本条钉的**恰恰就是一段注释** —— 用错那个的话判据当场瞎（首跑实测：红在
/// 「头注里少了『上游』」，而那段字明明在）。
/// ⚠ 仍必须剥测试段：否则本条自己那几个字面量会把自己喂绿 —— 那是本会话犯过
/// **四次**的同一种自伤（`P3s-Y2` / `P4d-Y4` / `P4b-Y3`，`P8a` 时刻意绕开了一次）。
#[test]
fn the_upstream_gap_for_issue_79_is_written_down_here() {
    let prod = guard_core::production_source(include_str!("../../src/bridge/src/panorama.rs"));
    // ⚠ `别新造第二份` 是**首跑变异补进来的**：原表只钉 `check_vendor_freshness`
    // 这个名字，而把「别新造第二份检查」那句话删掉**照样绿** —— 名字在、
    // **可操作的那半没了**，下一个人照样会去造第二条绊线。
    for needle in [
        "上游",
        "#79",
        "check_vendor_freshness",
        "别新造第二份",
        "U10e",
    ] {
        assert!(
            prod.contains(needle),
            "头注里少了「{needle}」—— 那段边界是 `P8b` 唯一的交付物，删了它\
                 下一个人就会以为这半只是「还没排期」"
        );
    }
    // ★ 钉**说法本身**，不只是关键词：把「上游还没有」改写成「我们还没做」
    // 是本件最可能的腐坏形态，而那两句话的排期含义差一个量级。
    assert!(
        prod.contains("不是「我们还没做」，是「上游还没有」"),
        "那句区分被改掉了 —— 它正是本件的正题"
    );
}

/// `设计/97` **CP5**：批注的状态必须是**数据**，不是文案 —— 审批队列的承重前提。
///
/// 全景页的「批注审批」按钮把 `propose` / `approve` / `list` 接上了人手；它只有在
/// core **真的**按 `status` 决定 agent 看不看得见时才有意义。本条在**真引擎**上走一遍：
///
/// 1. agent 提议（`propose_annotation`）→ 人那一侧 `list_annotations` 看得见、状态 `Proposed`；
///    agent 那一侧（`annotations_for` 与 `node().annotations`，后者就是全景详情读的那份）**看不见**。
/// 2. 人批准（`approve_annotation`）→ agent 那一侧恰好看见这一条。
/// 3. **CP5 的死值实验本身**：人写一条（Active，agent 看得见）→ 直接把它的侧车文件改成
///    `Proposed` → agent 那一侧必须**立刻看不见**。`设计/97` 预言「今天做会发现看得到」——
///    本条现打证明**看不到**（状态机住 `engine.rs`，不在 `annotations.rs`）。
///
/// 买到：「待审 = agent 看不见」这句按钮提示有真引擎兜底。
/// **买不到**：CP6 —— 批准之后它与人写的都是 `Active`，数据上分不开（只剩 `author` 自由文本）；
/// 那要上游加字段，本条不替它钉。改侧车文件走 `serde_json` 结构化改字段，不做子串替换。
#[test]
fn proposed_annotations_stay_invisible_to_agents_until_approved() {
    let base = std::env::temp_dir();
    let repo = base.join("cc-monitor-cp5-ann-repo");
    let store = base.join("cc-monitor-cp5-ann-store");
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("lib.rs"), "pub fn f() {}\npub fn g() {}\n").unwrap();
    let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
    {
        let mut g = arc.lock().unwrap();
        g.index().expect("index");
        let f = "lib.rs#f".to_string();
        let agent_sees = |g: &Engine, sym: &String| -> Vec<String> {
            let via_for: Vec<String> = g.annotations_for(sym).into_iter().map(|a| a.id).collect();
            let via_node: Vec<String> = g
                .node(sym)
                .expect("符号应在索引里")
                .annotations
                .into_iter()
                .map(|a| a.id)
                .collect();
            assert_eq!(
                via_for, via_node,
                "annotations_for 与 node().annotations 两条读路不该分叉"
            );
            via_for
        };

        // 1. 提议 → 人看得见（Proposed），agent 看不见。
        // 〔RM1d〕写走「算 ＋ 落盘」（生产里落盘是那台后端的 `files-put`，这里是判据替身 `land`）。
        use code_picture_core::edits;
        let pid = land(
            &repo,
            edits::plan_propose_annotation(
                &repo,
                "lib.rs",
                Some("f"),
                "agent 觉得这里要改",
                "agent-x",
            )
            .expect("propose"),
        );
        let listed: Vec<(String, model::AnnotationStatus)> = g
            .list_annotations()
            .into_iter()
            .map(|a| (a.id, a.status))
            .collect();
        assert_eq!(
            listed,
            vec![(pid.clone(), model::AnnotationStatus::Proposed)]
        );
        assert_eq!(
            agent_sees(&g, &f),
            Vec::<String>::new(),
            "待审的提议漏给了 agent"
        );

        // 2. 批准 → agent 恰好看见这一条。
        assert!(
            land(
                &repo,
                edits::plan_approve_annotation(&repo, &pid).expect("approve")
            ),
            "批准一条存在的提议应回 true"
        );
        assert_eq!(agent_sees(&g, &f), vec![pid.clone()]);
        assert!(
            !land(
                &repo,
                edits::plan_approve_annotation(&repo, "0000").expect("approve")
            ),
            "不存在的 id 应回 false"
        );

        // 3. 死值实验：人写的（Active）→ 侧车文件改成 Proposed → agent 立刻看不见。
        let gsym = "lib.rs#g".to_string();
        let hid = land(
            &repo,
            edits::plan_add_annotation(&repo, "lib.rs", Some("g"), "人写的", "me").expect("add"),
        );
        assert_eq!(agent_sees(&g, &gsym), vec![hid.clone()]);
        let side = repo
            .join(".codepicture")
            .join("annotations")
            .join(format!("{hid}.json"));
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&side).expect("读侧车"))
                .expect("侧车是 JSON");
        assert_eq!(
            v["status"],
            serde_json::json!("Active"),
            "侧车里的状态字段名/取值变了"
        );
        v["status"] = serde_json::json!("Proposed");
        std::fs::write(&side, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        assert_eq!(
            agent_sees(&g, &gsym),
            Vec::<String>::new(),
            "侧车改成未审之后 agent 仍看得见 —— 「需人审」只是文案"
        );
    }
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
}

/// PN1b（`设计/97 §7`）：两条选图命令**原样透出**上游 —— 本侧不改名、不重排、不丢字段。
///
/// 在真引擎上把注册表里的**每一种**图画一遍，经本侧的 `PanoramaDiagram` 序列化后：
/// ① 线上 `diagram.kind` == 注册表 id；② `diagram.body.shape` == 注册表声明的形状；
/// ③ `mermaid` == 上游 `to_mermaid` 逐字相等（本侧没有第二个渲染器）。
/// 另：注册表原样透出（`panorama_diagram_kinds` 的 JSON == 上游 `kinds()` 的 JSON）。
/// 买不到：前端怎么画 —— 那一半在 `tests/views/panorama-diagram*.vitest.ts`。
#[test]
fn the_diagram_commands_pass_the_upstream_through_untouched() {
    let base = std::env::temp_dir();
    let repo = base.join("cc-monitor-pn1b-diagram-repo");
    let store = base.join("cc-monitor-pn1b-diagram-store");
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
    std::fs::create_dir_all(repo.join("src/a")).unwrap();
    std::fs::create_dir_all(repo.join("src/b")).unwrap();
    std::fs::write(
        repo.join("src/a/x.rs"),
        "pub struct S { t: T }\npub fn f() { crate::b::y::g(); }\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("src/b/y.rs"),
        "pub struct T;\nimpl T { pub fn m(&self) {} }\npub fn g() {}\n",
    )
    .unwrap();
    let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
    let kinds_json = serde_json::to_value(
        tauri::async_runtime::block_on(panorama_diagram_kinds()).expect("注册表"),
    )
    .unwrap();
    assert_eq!(kinds_json, serde_json::to_value(diagram::kinds()).unwrap());
    {
        let mut g = arc.lock().unwrap();
        g.index().expect("index");
        for info in diagram::kinds() {
            let req = DiagramRequest {
                symbol: info.kind.needs_symbol().then(|| "src/a/x.rs#f".to_string()),
                ..Default::default()
            };
            // 走本侧那条路（`draw_view`），与直调上游的结果逐项比。
            let want_mermaid = diagram::to_mermaid(&g.draw(info.kind, &req).expect("画图"));
            let v = serde_json::to_value(draw_view(&g, info.kind.id(), &req).expect("本侧画图"))
                .unwrap();
            assert_eq!(v["diagram"]["kind"], serde_json::json!(info.kind.id()));
            assert_eq!(
                v["diagram"]["body"]["shape"],
                serde_json::to_value(info.shape).unwrap()
            );
            assert_eq!(v["mermaid"], serde_json::json!(want_mermaid));
            let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
            assert_eq!(
                keys,
                vec!["diagram", "mermaid"],
                "线上字段名变了，前端 types.ts 要跟"
            );
        }
        // 认不出的图种：上游的原话透出来，不回落
        let err = draw_view(&g, "modul", &DiagramRequest::default())
            .err()
            .expect("认不出的图种该报错");
        assert!(
            guard_core::contains_word(&err, "modul"),
            "错误里该说出是哪个：{err}"
        );
    }
    std::fs::remove_dir_all(&repo).ok();
    std::fs::remove_dir_all(&store).ok();
}

/// PN1b（`设计/97` CP2）：**本仓零处写死上游的图种名**。
///
/// 图种由上游注册表决定（`DiagramKind::ALL`），本仓的选择器从 `panorama_diagram_kinds` 现读。
/// 一旦有人在前端 / bridge 里写一个 `"<某个图种 id>"` 的字面量，上游改名或加图时那一处就会
/// 静默地指向错的东西 —— 那正是上游这一轮刚治掉的毛病（种类写两遍、已经漂了）。
///
/// 人群：`src/panorama/` 整棵（TS，**连注释一起扫**，更严）· `src/views/panorama.ts` ·
/// `src/bridge/src/panorama*.rs`（剥注释与测试段）。针：每个 id 带引号的三种写法
/// （`"id"` / `'id'` / 反引号）—— 引号就是边界，不会被 `modules` 这类更长的词撑大。
/// 正控：同一把尺子量 vendored 的 `registry.rs`（生产段），每个 id 恰好量到一次
/// （`DiagramKind::id` 那一臂）—— 尺子瞎了这条先红。
/// 买不到：不带引号的拼法（把 id 拆成两半拼）它认不出。
#[test]
fn no_upstream_diagram_kind_id_is_spelled_out_on_our_side() {
    let ids: Vec<&str> = DiagramKind::ALL.iter().map(|k| k.id()).collect();
    let root = crate::guard_support::repo_root();
    let hits_in = |src: &str| -> Vec<String> {
        let mut out = Vec::new();
        for id in &ids {
            for q in ['"', '\'', '`'] {
                let needle = format!("{q}{id}{q}");
                for _ in src.matches(needle.as_str()) {
                    out.push(needle.clone());
                }
            }
        }
        out
    };

    // 正控：尺子在注册表本身上量得到每个 id，且恰好一次
    let registry = std::fs::read_to_string(
        root.join("src/bridge/vendor/code-picture-core/src/diagram/registry.rs"),
    )
    .expect("读 vendored 注册表");
    let mut control = hits_in(&guard_core::production_code(&registry));
    control.sort();
    let mut want: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
    want.sort();
    assert_eq!(control, want, "尺子在注册表上没量准 —— 下面的零命中不作数");

    // 人群
    let mut corpus: Vec<(String, String)> = Vec::new();
    for (p, src) in guard_core::scan_tree_excluding(&root.join("src/panorama"), &["ts"], &[]) {
        corpus.push((p.to_string_lossy().replace('\\', "/"), src));
    }
    let view = root.join("src/views/panorama.ts");
    corpus.push((
        view.to_string_lossy().into(),
        std::fs::read_to_string(&view).expect("读 views/panorama.ts"),
    ));
    for (p, src) in guard_core::scan_tree_excluding(&root.join("src/bridge/src"), &["rs"], &[]) {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // 按「第一个 `_` / `.` 之前那一段」整段相等来认 `panorama*.rs`（不用前缀匹配：
        // `needle_anchor` 棘轮拦的正是语料变量上的裸前缀/子串匹配）。
        if name.split(['_', '.']).next() == Some("panorama") {
            corpus.push((
                p.to_string_lossy().replace('\\', "/"),
                guard_core::production_code(&src),
            ));
        }
    }
    // 人群自检：这几份必须在（任何一份掉出去，零命中就是空转）
    let mut present: Vec<&str> = [
        "src/panorama/diagram-view.ts",
        "src/panorama/diagram-render.ts",
        "src/panorama/types.ts",
        "src/views/panorama.ts",
        "src/bridge/src/panorama.rs",
    ]
    .into_iter()
    .filter(|want| corpus.iter().any(|(p, _)| p.ends_with(want)))
    .collect();
    present.sort();
    assert_eq!(
        present.len(),
        5,
        "人群里少了该扫的文件，实得 {present:?}（共 {} 份）",
        corpus.len()
    );

    let found: Vec<String> = corpus
        .iter()
        .flat_map(|(p, src)| hits_in(src).into_iter().map(move |h| format!("{p}: {h}")))
        .collect();
    assert_eq!(
        found,
        Vec::<String>::new(),
        "本仓写死了上游的图种名 —— 选项该从注册表现读（CP2）"
    );
}
