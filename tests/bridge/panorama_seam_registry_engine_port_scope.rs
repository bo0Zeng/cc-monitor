/// 这条性质的人群：**我们编的每一棵源码树**，按仓根相对路径登记。
///
/// `(标签, 仓根相对的源码树, 文件数地板, why——它凭什么在这条性质的射程里 + 现打读数)`
const TREES: &[(&str, &str, usize, &str)] = &[
    (
        "monitor",
        "src/bridge/src",
        80,
        "承载界面那一侧，今天唯一的取用口就在这棵树里。\
             ⚠ 〔`P4` 2026-09-21〕先前这里写着「两个数别混：09-04 盘上 105 份 `.rs`，\
             而判据的语料是 104 份 —— `scan_tree!` 按构造摘掉调用者自己那一份」——\
             那一刀在这一处不生效，而且本文件住 `tests/bridge/`、不在这棵树里\
             ⇒ **盘上几份，语料就是几份**。地板取 80",
    ),
    (
        "backend",
        "src/backend",
        55,
        "本件要换到的那一侧 —— 「解析搬到代码所在地」搬的就是往这棵树里搬\
             （09-04 现打 73 份 `.rs`，地板取 55）。今天它这一格是 0，\
             而**「0」只有在尺子接上了的时候才算数**，那一格由本模块第二条判据买",
    ),
    // 〔RM1f · V108 后半句〕**取用口今天住这棵树**：monitor 摘掉内嵌引擎之后，全体只剩全景小程序这一处。
    (
        "全景小程序",
        "src/panorama-engine",
        1,
        "只装代码全景引擎的独立小程序（V108 选 B）—— 本机与远端的全景都经后端插件口起它，\
             引擎取用口 `Engine::open` 与引擎类型的导入今天**只**在这里（09-25 现打 1 份 `.rs`：`main.rs`，地板取 1）",
    ),
    (
        "共享 crate",
        "src/bridge/crates",
        7,
        "两侧的清单都 `path` 依赖它们 ⇒ 谁在这里取用引擎，两侧都会被编进去，\
             而上面那两条老判据一份都看不见（09-04 现打 9 份 `.rs`，地板取 7）",
    ),
];

/// 登记在案的**排除**：`(仓根相对目录, 为什么它不进人群)`。
///
/// 排除要有住址、有理由、还要有幽灵检查 —— **一个没人知道的过滤器与一个没人知道的洞
/// 长得一模一样。**
const EXCLUDED_TREES: &[(&str, &str)] = &[(
    "src/bridge/vendor/code-picture-core",
    "引擎本体自己的家：取用口那个符号是在这里**定义**的，把它算进人群等于要求\
         「定义处也只许有一处取用」——那是另一件事。且 `C7` 逐字「vendor 不动」，\
         它进人群只会造出一条谁也不许修的红。monitor 清单的 `[workspace] exclude` \
         也逐字排除着它，两处口径一致。〔RM1f〕今天依赖它的只剩全景小程序那份清单（monitor 那一行删了）。",
)];

fn repo_root() -> std::path::PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 一棵树的绝对住址。**读不到就 `Err`**，不许退化成空语料。
fn tree_dir(rel: &str) -> Result<std::path::PathBuf, String> {
    let dir = repo_root().join(rel);
    if dir.is_dir() {
        return Ok(dir);
    }
    Err(format!(
        "登记在案的源码树 `{rel}` 在盘上读不到（它找的是 {}）—— 本条此刻**判不了**，\
             而判不了不许写成「那棵树里零命中」。两种可能：那棵树搬家了（改 `TREES` 的登记），\
             或者仓根算错了（本判据只跳一级，见头注那条 `K-R13`）。",
        dir.display()
    ))
}

/// 文件数地板。**空语料与干净的树在终端上一模一样**，所以这一道必须是错、不是 0。
fn floor_check(label: &str, got: usize, floor: usize) -> Result<(), String> {
    if got >= floor {
        return Ok(());
    }
    Err(format!(
        "`{label}` 这棵树只收到 {got} 份 `.rs`（地板 {floor}）—— 遍历坏了，\
             此刻这条判据在空转：它会零命中地绿，而那与「这棵树里真的没有取用口」不是一件事。"
    ))
}

/// 一棵树的语料：`(「标签:树内相对路径」, 原文)`。
fn corpus_of(label: &str, rel: &str, floor: usize) -> Result<Vec<(String, String)>, String> {
    let dir = tree_dir(rel)?;
    let mut out: Vec<(String, String)> = Vec::new();
    for (path, src) in guard_core::scan_tree!(&dir, &["rs"]) {
        let inside = path
            .strip_prefix(&dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((format!("{label}:{inside}"), src));
    }
    floor_check(label, out.len(), floor)?;
    out.sort();
    Ok(out)
}

/// 两根针，**运行时拼** —— 本文件的头注里逐字写着它们，写字面量会读到判据自己。
///
/// ⚠ 〔`P4` 2026-09-21〕先前这里写着「`scan_tree!` 已经按构造摘掉本文件，这是第二道」——
/// **那一道不生效**（自摘在这一处恒空转）。今天承重的是**运行时拼针** ＋ **住址**
/// （本文件住 `tests/bridge/`，而两棵语料树是 `src/bridge/src` 与 `src/backend`）。
/// 本仓这一族栽过五次，别把拼针改回字面量。
fn port_needles() -> Vec<(String, &'static str)> {
    vec![
        (format!("Engine{}open", "::"), "引擎取用口"),
        (format!("code_picture{}core", "_"), "引擎类型的导入口"),
    ]
}

/// 纯函数：一份语料里命中这根针的**住址表**（生产段、带词边界）。
///
/// 抽成纯函数是为了能**直接喂夹具** —— 否则「backend 那棵树 0 处」这个读数
/// 与「这把尺子根本数不出东西」在终端上没有区别。
fn ports_in(corpus: &[(String, String)], needle: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (addr, src) in corpus {
        if guard_core::contains_word(&guard_core::production_code(src), needle) {
            out.push(addr.clone());
        }
    }
    out.sort();
    out
}

/// ★ 正题：**每一棵我们编的树合起来，取用口的住址表只有那一处。**
///
/// 〔TL1 · 4C · 2026-09-25〕**取用面今天只有这一份真相**：后端那侧 `panorama_locus_guard` 原先的正题②（后端树零处）
/// 与正题③（小程序树恰一处 ＋ monitor 树零处）并进来了 —— 六刀现量，凡红它们的，本条或
/// `tests::the_engine_is_opened_in_exactly_one_place`（`main.rs` 里恰一次）都红，它们没有独占格（`TL1.md` 件 2）。
/// 那边只留链接面（lock 登记 · 共享 crate 零依赖声明）。⚠ 本条的「非空」（住址表 == 小程序 `main.rs`）
/// 就是后端那一格「零」不是空真的证明 —— 原先由那边正题③的「非空的 1」担着。
#[test]
fn the_engine_port_is_pinned_across_every_tree_we_compile() {
    let mut corpus: Vec<(String, String)> = Vec::new();
    let mut sizes: Vec<String> = Vec::new();
    for (label, rel, floor, why) in TREES {
        assert!(
            why.trim().chars().count() >= 20,
            "`{label}` 没写清它凭什么在这条性质的射程里（实得 {} 字）",
            why.trim().chars().count()
        );
        let files = corpus_of(label, rel, *floor).unwrap_or_else(|e| panic!("{e}"));
        sizes.push(format!("{label} {} 份", files.len()));
        corpus.extend(files);
    }
    for (needle, what) in port_needles() {
        let found = ports_in(&corpus, &needle);
        // 〔RM1f〕住址从 `monitor:panorama.rs`（已删）搬到全景小程序（上面 ② 那一种：换侧，同轮改期望）。
        assert_eq!(
            found,
            vec!["全景小程序:main.rs".to_string()],
            "{what}（针 `{needle}`）的住址表变了：{found:?}\n\
                 各树语料：{sizes:?}\n\
                 ★ **这不是要你把这里的期望改掉了事** —— 先回答是下面哪一种：\n\
                 ① 多出来的那一处是**第二个取用口** ⇒ 那就是第二条 SQLite 连接，\n\
                 「换侧要改的就是那一处」这句承重的话当场作废；\n\
                 ② 是**换侧**（取用口搬到 backend / 搬进某个共享 crate）⇒ 把期望改成新住址，\n\
                 并**同轮**回答 `K-W2D` 的 `KW2D4`：新那一侧谁在数、`panorama.rs` 那条\n\
                 `find_pinned` 还指得对不对（它只读那一份文件）；\n\
                 ③ 少了 ⇒ 多半是抽取坏了、或引擎整个搬走了，两种都要人看一眼。"
        );
    }
}

/// ★★ **「0」要先证明尺子接上了** —— 否则后端那棵树的零命中什么也说明不了。
///
/// 三刀：树指错 ⇒ 出声；语料塌了 ⇒ 出声；**合成一处喂进后端侧的语料 ⇒ 数得出来**。
///
/// # ⚠ 第三刀证明的比它听起来的少（**实测出来的边界，不是谦虚**）
///
/// 那份夹具的文本是**用同一根针拼出来的** ⇒ 针本身错了，这一条照样绿。
/// 09-04 现打（死值验 M9，把针改成一个盘上不存在的写法再跑一趟门禁）：
/// **本条绿，而上面那条正题当场红**，报文逐字「住址表变了：`[]`」。
/// ⇒ 分工写清：本条证的是「**剥生产段 + 词边界这条链，在后端侧的语料上真的会命中**」；
/// 「**针指的是不是那个事实**」由正题钉着（针错了住址表就空，正题红）。两条合起来才是一句完整的话。
///
/// ★ 为什么不把夹具改成独立字面量：那要在本文件里写下针的**完整字面量**，
/// 而本模块的针刻意运行时拼（头注第二道防线）。⇒ 这里选**登记边界 + 由正题补位**，
/// 而不是把防线换掉。〔同一轮里另一条控制（`readonly_guard` 那侧的 feature 探针）
/// 走的是相反的选择 —— 那条**没有**正题替它补位，所以它必须换独立见证。〕
#[test]
fn a_tree_it_cannot_read_makes_it_say_so_instead_of_counting_zero() {
    let backend = TREES
        .iter()
        .find(|(label, ..)| *label == "backend")
        .expect("`TREES` 里没有后端那一棵 —— 而本件的整个题目就是往那一侧搬");
    // ① 指错的树：`Err`，而且诊断要点出它找的那个住址（读的人才知道去哪儿看）。
    let bogus = format!("{}-此处刻意不存在", backend.1);
    let verdict = tree_dir(&bogus);
    let why = verdict.expect_err(
        "指到一个不存在的目录上，`tree_dir` 居然给了 `Ok` —— \
             那意味着「读不到那棵树」会静默变成「那棵树里零命中」",
    );
    assert!(
        guard_core::contains_word(&why, &bogus),
        "诊断里没印出它找的那个住址：{why}"
    );
    // ② 语料塌成空集：地板那一支同样是错，不是 0。
    assert!(
        floor_check("夹具", 0, 3).is_err(),
        "空语料过了地板 —— 那正是「零命中地绿」的入口"
    );
    assert!(
        floor_check("夹具", 3, 3).is_ok(),
        "地板把恰好够的语料也挡了 —— 那会变成假红，而假红最省事的消法是把判据删掉"
    );
    // ③ 尺子真的能在后端侧的语料上数出命中。
    for (needle, what) in port_needles() {
        let fixture = vec![
            (
                "backend:某个适配层.rs".to_string(),
                format!("fn 取一次() {{ let e = {needle}(&key)?; }}"),
            ),
            (
                "backend:干净的一份.rs".to_string(),
                "fn f() -> u8 { 7 }".to_string(),
            ),
        ];
        assert_eq!(
            ports_in(&fixture, &needle),
            vec!["backend:某个适配层.rs".to_string()],
            "{what} 这根针在后端侧的语料上数不出命中 —— \
                 那么正题里后端那棵树的「0 处」是**空真**，说明不了任何事"
        );
    }
}

/// ★ 人群的**分母自己**也要钉住：**再多一棵编进来的树，本条当场说话。**
///
/// 分母从两份清单的**依赖段**里现算（`path = "…"` 那一形），不写死一张 crate 名单
/// —— 名单会腐，而清单是那件事唯一的住址。
///
/// ⚠ 它看不见的那一类，如实写在这里：**谁的清单都没引的 crate**
/// （单独构建、只在运行期被起进程调用的那种）。那正是侧车那一形，归 `KW2D4`。
#[test]
fn the_set_of_trees_this_scope_covers_is_itself_pinned() {
    let root = repo_root();
    let mut dirs: Vec<String> = Vec::new();
    for (manifest_rel, home) in [
        ("src/bridge/Cargo.toml", "src/bridge"),
        ("src/backend/Cargo.toml", "src/backend"),
        // 〔RM1f〕第三份清单：全景小程序（今天唯一链 vendored 引擎的那一棵）。
        ("src/panorama-engine/Cargo.toml", "src/panorama-engine"),
    ] {
        let manifest = std::fs::read_to_string(root.join(manifest_rel))
            .unwrap_or_else(|e| panic!("读不到 {manifest_rel}: {e}"));
        dirs.extend(in_repo_path_deps(&manifest, home));
    }
    dirs.sort();
    dirs.dedup();
    // 抽取器自检：分母塌了下面那条对账就零命中地绿。
    // 地板而不是相等：**变大那个方向由下面那条逐条对账管**，这一道只挡「抽取坏了」。
    assert!(
        dirs.len() >= 8,
        "两份清单的依赖段里只抽出 {} 条仓内 `path` 依赖\
             （09-04 现打 8：7 个共享 crate + vendor 里的引擎）—— 抽取坏了：{dirs:?}",
        dirs.len()
    );
    let mut uncovered: Vec<String> = Vec::new();
    for dir in &dirs {
        let covered = TREES
            .iter()
            .any(|(_, rel, ..)| dir.as_str() == *rel || dir.starts_with(&format!("{rel}/")))
            || EXCLUDED_TREES.iter().any(|(rel, _)| dir.as_str() == *rel);
        if !covered {
            uncovered.push(dir.clone());
        }
    }
    assert!(
        uncovered.is_empty(),
        "有仓内 crate 被编进来了，而 `TREES` 的射程盖不住它：{uncovered:?}\n\
             ★ 这正是 `K-W2D` `KW2D4` 说的「第三棵树」那一刻。两条路，选一条：\n\
             ① 它可能承载引擎取用口 ⇒ 登记进 `TREES`（带文件数地板与理由）；\n\
             ② 它按构造不可能 ⇒ 登记进 `EXCLUDED_TREES` 并写清为什么。\n\
             **不许静默加一条过滤** —— 没人知道的过滤器与没人知道的洞长得一模一样。"
    );
    for (rel, why) in EXCLUDED_TREES {
        assert!(
            dirs.iter().any(|d| d.as_str() == *rel),
            "排除项 `{rel}` 今天已经不在两份清单的依赖面上了 —— 幽灵条目，同轮摘掉"
        );
        assert!(
            why.trim().chars().count() >= 20,
            "排除项 `{rel}` 的理由太短（实得 {} 字）—— 这一列的读者是下一个想再排除一棵树的人",
            why.trim().chars().count()
        );
    }
}

/// 一份清单的**依赖段**里所有 `path = "…"`，归一化成仓根相对目录。
///
/// 只认依赖段：`[[bin]]` 那条 `path` 指的是入口文件，不是一棵树。
/// 段的判据是「段名以 `dependencies]` 收尾」⇒ `[build-dependencies]` 与
/// `[target.'…'.dependencies]` 一并收得进来（那两种今天在 monitor 清单上真的有）。
///
/// ⚠ 剥 `#` 整行注释走**共享原语**（`strip_comment_lines` 的 YAML/TOML 兄弟），
/// 不在这里自己写第二份 —— 本函数第一版内联了一个 `#` 过滤，
/// 而 `structural_scan` 那张「剥注释实现只许一份」的登记表**当场逮住了它**
/// （09-04 现打，同一趟门禁里连本文件与后端那侧两处一起点名）。
/// 先例逐字在那张表里：另一处「已变成一句委托 ⇒ 登记删掉」。
fn in_repo_path_deps(manifest_text: &str, home: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_deps = false;
    let uncommented = guard_core::strip_hash_comment_lines(manifest_text);
    for line in uncommented.lines() {
        let entry = line.trim();
        if entry.starts_with('[') {
            in_deps = entry.ends_with("dependencies]");
            continue;
        }
        if !in_deps {
            continue;
        }
        let quoted: Vec<&str> = entry.split('"').collect();
        for (k, seg) in quoted.iter().enumerate() {
            // 奇数下标才是引号**里面**；它前面那一段必须出现 `path` 这个词。
            if k % 2 == 0 || !guard_core::contains_word(quoted[k - 1], "path") {
                continue;
            }
            let joined = format!("{home}/{seg}");
            let mut norm: Vec<&str> = Vec::new();
            for part in joined.split('/') {
                match part {
                    "." | "" => {}
                    ".." => {
                        norm.pop();
                    }
                    other => norm.push(other),
                }
            }
            out.push(norm.join("/"));
        }
    }
    out
}
