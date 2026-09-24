/// ★★ **写 doc-link 那条路的安全性整个压在 vendor 的 `guard_rel` 上**
/// 〔audit-0805 08-08，Phase G 第 87 件〕。
///
/// # 先核的结果（三段，别只读结论）
///
/// 08-08 横扫「收路径参数的 `#[tauri::command]`」得 22 条，逐条分档：
/// 远端那批（`sftp_pool` / `sftp` / `acct_iso_deploy` / `tmux`）操作的是**用户自己的
/// 远端机器**，收任意路径是功能本身；本机那侧除上一轮刚围栏的三条 `cc_integration_*`，
/// 只剩 panorama 这五条。
///
/// 1. `add_annotation` / `propose_annotation` 收的 `file` **不进路径** ——
///    vendor 把它当**数据**存进 `Annotation`，落盘文件名是内容哈希 ⇒ 无穿越面，
///    **刻意不给它们加守卫**（加了是安慰剂）。
/// 2. `write_doc_link` / `remove_doc_link` 的 `doc` **真的进路径**（`repo.join(doc_rel)`），
///    而 vendor **自己有围栏**：`guard_rel` / `guard_doc_rel` 拒绝绝对路径与 `..`。
/// 3. `repo` 本身**刻意不设围栏**：panorama 的功能就是「索引任意一个项目目录」，
///    home 围栏会砍掉 `/srv/work` 这类正当用法。⇒ 登记进 `ROADMAP §5` 当诚实边界，
///    而不是装一道假围栏。
///
/// # 本条钉什么
///
/// 我们这侧的安全性**整个压在别人家的两行守卫上**，而 vendor 是**冻结的副本**
///（红线：一字节不动）——它会被**整份换新**（`build.rs` 有 `.vendor_id` 新鲜度自检，
/// 但那只说「副本旧了」，不说「那两行还在不在」）。
/// ⇒ 前提触发器：那两个方法必须仍然调 `guard_rel`，且 `guard_rel` 必须仍然
/// 同时拒**绝对路径**与 `..`。**只读 vendor，不改它一个字节。**
#[test]
fn the_doc_link_writes_still_go_through_the_vendor_guard() {
    // 〔PN1b 09-24 re-vendor〕上游把 `engine.rs` 按职责拆成了 `engine/` 目录：
    // 两个写路径方法住 `docs_anchors.rs`，`guard_rel` 住 `mod.rs` ⇒ 两份拼起来读。
    // 读不到任何一份都当场 panic（不许退成空串 —— 那会让下面每条都「找不到」而不是「判过」）。
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("vendor/code-picture-core/src/engine");
    let src = ["docs_anchors.rs", "mod.rs"]
        .map(|f| {
            let p = dir.join(f);
            std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— vendor 布局变了就把本条一起改"))
        })
        .join("\n");
    // 我们真正调到的**写路径**方法（`panorama.rs` 里各调一次）。
    for m in ["write_doc_link", "remove_doc_link"] {
        let at = src.find(&format!("pub fn {m}(")).unwrap_or_else(|| {
            panic!("vendor 里找不到 `{m}` —— 换版了，本条与 `panorama.rs` 一起复核")
        });
        // ⚠ 两件事一起做对，缺一个就白写：
        // ① **按 char 取，别按字节切** —— vendor 源码里全是中文注释，
        //    `&src[at..at+400]` 会落在汉字中间当场 panic（`digit_after` 头注记过）；
        // ② **切到方法真正的结尾**，不是「起点后 N 个字符」——
        //    ⚠ 08-08 变异实测：定长窗口把守卫删掉后**照样绿**，因为窗口
        //    一路吃进了下一个方法 `remove_doc_link`，那里还有一句 `guard_rel`。
        //    同一个缺陷两轮前刚在 `structural_scan` 修过，这次犯在自己新写的判据上。
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
            "从 `{m}` 切出 {} 行，不像一个方法体（配平切错了，本条会零命中地绿）",
            body.lines().count()
        );
        assert!(
            guard_core::contains_word(body, "guard_rel"),
            "vendor 的 `{m}` 不再调 `guard_rel` 了。\n\
                 ★ 那两行是**我们这侧唯一挡着路径穿越的东西**：`doc` 来自 webview，\n\
                 下游是 `repo.join(doc_rel)` 然后 `fs::write`。\n\
                 ⚠ vendor 是冻结副本、会被整份换新，而 `.vendor_id` 新鲜度自检只说\n\
                 「副本旧了」，不说「那两行还在不在」。\n\
                 换版后要么确认新版另有等价围栏，要么在 `panorama.rs` 这侧自己加一道。\n\
                 实得这一段：{body:?}"
        );
    }
    // 守卫本身还得真守：绝对路径与 `..` 两件都要拒。
    let g = src
        .find("fn guard_rel(")
        .expect("vendor 里找不到 `guard_rel` 定义 —— 上面那两条此刻在比一个不存在的东西");
    let gbody: String = src[g..].chars().take(300).collect();
    let gbody = gbody.as_str();
    // ⚠ 用 `contains_word` 而不是裸 `contains`：`needle_anchor` 棘轮当场拦过
    // （33→34），而它指出的不只是写法 —— 裸子串在事实被撑大时照样绿。
    for needle in ["starts_with", "\"..\""] {
        assert!(
            guard_core::contains_word(gbody, needle),
            "`guard_rel` 里找不到 `{needle}` —— 它可能只剩半道围栏了。\n\
                 两件缺一不可：绝对路径（`/etc/x.md` 直接跳出 repo）与 `..`（逐级爬出去）。\n\
                 实得：{gbody:?}"
        );
    }
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
        let id = g
            .add_annotation("lib.rs", Some("f"), "note", "me")
            .expect("add_annotation");
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
        let pid = g
            .propose_annotation("lib.rs", Some("f"), "agent 觉得这里要改", "agent-x")
            .expect("propose");
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
            g.approve_annotation(&pid).expect("approve"),
            "批准一条存在的提议应回 true"
        );
        assert_eq!(agent_sees(&g, &f), vec![pid.clone()]);
        assert!(
            !g.approve_annotation("0000").expect("approve"),
            "不存在的 id 应回 false"
        );

        // 3. 死值实验：人写的（Active）→ 侧车文件改成 Proposed → agent 立刻看不见。
        let gsym = "lib.rs#g".to_string();
        let hid = g
            .add_annotation("lib.rs", Some("g"), "人写的", "me")
            .expect("add");
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
