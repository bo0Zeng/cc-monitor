use super::*;

/// ★ 反空真：`src_root()` 必须指到**真的**源码树。
///
/// 它是 45 处扫描的共同入口 —— 一旦它指错（搬树时漏改这一行就会），
/// 下游那些守卫不会红，会**扫空集然后全绿**。这条把「指错」变成一条会红的机检。
#[test]
fn the_src_root_address_points_at_a_real_tree() {
    // ⚠ 判据**点名具体文件**，不数 `.rs` 个数。两次改法的账都记在这里：
    // ① 最早数的是**顶层条目数**（`>= 20`）—— 19 个纯测试文件搬去 `tests/backend/`
    //    之后顶层当场掉到 20 以下。那个数本来就不该是顶层的。
    // ② 接着改成**递归数 `.rs`**，能挡树塌，但它靠 `std::fs::read_dir` 裸遍历 ——
    //    而 `scanning_guard_registry` 那条元判据禁止测试段里裸遍历目录
    //    （理由：判据在自己那份语料里找到自己 ⇒ 恒绿），且那张存量清单**只许变短**。
    // ⇒ 换成点名式：**不遍历，而且比数个数更硬**（一个装了别的东西的目录数也能过）。
    for (name, dir, probes) in [
        (
            "src_root",
            src_root(),
            &["main.rs", "inbound.rs", "listen.rs", "Cargo.toml"][..],
        ),
        (
            "tests_root",
            tests_root(),
            &[
                "readonly_guard.rs",
                "build_id_guard.rs",
                "no_timer_guard.rs",
            ][..],
        ),
    ] {
        assert!(dir.is_dir(), "{name}() 不是目录：{}", dir.display());
        for probe in probes {
            assert!(
                dir.join(probe).exists(),
                "{name}() 下没有 {probe} —— 它指到了别的地方：{}",
                dir.display()
            );
        }
    }
    // 子目录也各点一个，免得整棵子树消失而顶层还在。
    for rel in [
        "control/mod.rs",
        "observe/mod.rs",
        "relay/server.rs",
        "platform/mod.rs",
    ] {
        assert!(
            src_root().join(rel).is_file(),
            "src_root() 下缺 {rel} —— 生产树疑似塌了一块"
        );
    }
    assert_ne!(src_root(), tests_root(), "两棵树的住址撞了");
}

/// ★ 语义钉：`main.rs` 的生产段必须含这几样东西。
///
/// 字节数地板挡不住「单个文件被剥空/剥过头」（`no_timer_guard` 的 80_000 是**全体**总量，
/// 实测最大的 `watcher.rs` 整个被吞它都照样绿）。这条用**语义锚点**直接钉住那一类失效：
/// `guard-core` 头注里记的那个真实病灶如果没修，这里会立刻红。
///
/// # 锚点会随重构变，**换锚点之前先问它为什么不见了**
///
/// U3 拆 `observe/`/`control/` 时这条**红了一次**：锚点里有 `mod watcher;`，
/// 而 `watcher` 正当地挪进了 `observe/mod.rs`。**这是钉子干对了活**（它就是要在
/// 「main.rs 生产段少了东西」时叫），只是这次的原因是重构而不是剥过头。
///
/// 处置纪律与「守卫钉死的计数」同一条：**不是把红的那条删掉了事**，
/// 而是问「main.rs 今天还剩哪些东西是承重的」，换成那些。
/// ⇒ `mod watcher;` 换成 `mod observe;` + `mod control;` —— 后两条恰恰是 U3 建出来的
/// 两条解耦线在 `main.rs` 里的落点，比原来那条更承重。
#[test]
fn main_production_section_keeps_its_load_bearing_items() {
    // 〔步 9 · 09-19〕承重项如今分居两份（`const BUILD_ID` 在 `lib.rs`，分派在 `main.rs`）
    // ⇒ 扫描面取全集。本条买的是「剥法没剥过头」，不是「它们住在同一份文件里」。
    let prod = production_code(&backend_root_source());
    for anchor in [
        format!("const BUILD{}", "_ID"),
        format!("const CAPA{}", "BILITIES"),
        format!("fn split_stream{}", "_flags"),
        format!("mod obser{}", "ve;"),
        format!("mod contr{}", "ol;"),
    ] {
        assert!(
            prod.contains(&anchor),
            "main.rs 的生产段里找不到 `{anchor}` —— 剥过头了，扫描面正在静默缩水"
        );
    }
}

/// ★ 全 crate 实测：backend 的每个源文件用列 0 收尾判据都能剥干净。
///
// ⟦KR115D3 共用段·起⟧
/// 这条同时是「列 0 收尾判据够不够用」的持续验证 —— **而它自己已经守不住那一形了**。
///
/// ⚠ 〔`K-R115` 09-14〕上一版这里逐字写着「哪天有人在测试模块里写了一段列 0 含右
/// 大括号的原始字符串，**这里会红**」。那句话**两天里假了两次**：
/// ① `K-R110`（09-13）现打它**当时就没红** —— 那 31 行漏进生产段，而守门人看不见；
/// ② `K-R110` 之后它**永远不会再因此红** —— 那一形已经被
///    `guard_core::assert_test_module_ranges_are_brace_balanced` 正确处理掉了
///    （匹配单位从「行」换成「计数」，区间在原始字符串里收尾会被它当场逮住）。
/// ⇒ **今天真正守这一形的是那条判据**；本条经 `assert_tree_strips_clean` 调到它，
///    本条自己守的只剩「这棵树剥得干净 ＋ 文件数没缩水」。
///
/// 🔴 **这一段在两棵树里各住一份，逐字必须相同**
/// （`src/backend/guard_support.rs` ＋ `tests/bridge/structural_scan_tests.rs`）——
/// `K-R110` 交回时点名过这个形状：**两个住址、同一句话**，改一处漏一处，
/// 下一次还是一处真一处假。钉着它的是
/// `guard_support_tests.rs::the_two_strip_clean_notes_stay_one_sentence`，**只改一处当场红**。
// ⟦KR115D3 共用段·止⟧
#[test]
fn every_backend_file_strips_clean() {
    // 地板 = **实测值**（2026-08-02：34 个 .rs）。原先是 10，松了 24 个文件。
    guard_core::assert_tree_strips_clean(
        &crate::guard_support::src_root(),
        // ★ audit-0805 F16：34 → **37**（今日实测）。余量 3 恰好等于
        // `platform/pidwatch/` 的文件数 —— 那一整个目录掉出去也不会红。
        37,
    );
}

/// 再导出没有把语义换掉：反向自检仍然会咬人。
#[test]
fn the_reexported_leak_check_still_bites() {
    let attr = format!("#[{}]", "test");
    let leaked = format!("{attr}\nfn t() {{}}\n");
    let r = std::panic::catch_unwind(|| assert_no_test_code("自检", &leaked));
    assert!(r.is_err(), "再导出之后判据形同虚设");
    assert!(production_source("fn a() {}\n").contains("fn a()"));
}

/// 🔴 `K-R115` `KR115D3`：**「两个住址、同一句话」不许再各说各话。**
///
/// # 题面
///
/// 本文件的 `every_backend_file_strips_clean` 与 monitor 那一侧的同名判据
/// （住 `src/bridge/src/structural_scan.rs`，扫的是另一棵树）头上挂着**同一段散文**。
///
/// ⚠ 这里**刻意不写出对侧那个判据的函数名**：`structural_scan.rs` 里的
/// `scan_tree!` 按构造摘掉调用者自己那一份 ⇒ 只住在那份文件里的符号**进不了**
/// 死名判据的代码侧语料，散文一点名它就被当成「代码里根本不存在的名字」。
/// 那是那条判据的一处盲区（`K-R115` 09-14 现打撞到），不是这一句写错了。
/// `K-R110` 交回时点名过这个形状：那句话**昨天是假的**（现打它没红）、
/// **今天变成另一种假**（那一形已被正确处理，它永远不会再因此红）——
/// 而**订正只落在其中一处**的话，下一次仍是一处真一处假。
///
/// ⇒ 本条把那一段钉成**逐字相同**：只改一处，当场红。
///
/// # ⚠ 它买不到什么（诚实边界，别把绿读宽）
///
/// - **不判那句话对不对** —— 两处一起改成同一句假话，本条照样绿（那要读语义）。
///   它买的是「**不会再一处真一处假**」，不是「说的是真话」。
/// - **人群写死是两处**。第三处地方再抄一遍同一句话，本条看不见。
/// - 抽取靠一对标记；标记被删掉 ⇒ 上面那条 `count() == 1` 先红，
///   **不会退化成「抽了个空串、两边相等、静默绿」**。
#[test]
fn the_two_strip_clean_notes_stay_one_sentence() {
    let repo = crate::guard_support::repo_root();
    // 🔴 **运行时拼**：写成字面量的话，本文件里这两行自己就会命中标记，
    //    下面那条 `count() == 1` 当场变成恒假 —— 而它正是防空真的那一条。
    let beg = format!("// ⟦KR115D3 共{}", "用段·起⟧");
    let end = format!("// ⟦KR115D3 共{}", "用段·止⟧");
    // 🔴 〔搬树 2026-09-18 · `设计/16 §6.2` C 类〕monitor 那一份住址跟着共用段搬了：
    //    那一段是一条判据的头注，剖分把它从 `src/bridge/src/structural_scan.rs`
    //    带去了 `tests/bridge/structural_scan_tests.rs`。**那段话一个字没改。**
    // 🔴 〔步 7c 剖分 2026-09-19 · C 类〕**后端那一份住址也跟着共用段搬了。**
    //    那一段同样是一条判据的头注，剖分把它从 `src/backend/guard_support.rs`
    //    带去了 `tests/backend/guard_support_tests.rs`（也就是**本文件**）。
    //    **那段话一个字没改**；两个住址今天都在 `tests/` 下，这正是 `§4.1` 说的
    //    「剖分之后判据与生产代码物理不同文件」在这一格上的样子。
    let sites = [
        "tests/bridge/structural_scan_tests.rs",
        "tests/backend/guard_support_tests.rs",
    ];
    let mut blocks: Vec<(&str, String)> = Vec::new();
    for rel in sites {
        let src = std::fs::read_to_string(repo.join(rel))
            .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 本条判不了，不许当成绿"));
        for (mark, which) in [(&beg, "起"), (&end, "止")] {
            assert_eq!(
                src.matches(mark.as_str()).count(),
                1,
                "{rel} 里共用段的「{which}」标记出现 {} 次，应当恰好 1 次 —— \
                     抽取器指不到唯一那一段时，下面那条「两处相等」就是空真",
                src.matches(mark.as_str()).count()
            );
        }
        let i = src.find(beg.as_str()).unwrap() + beg.len();
        let j = src.find(end.as_str()).unwrap();
        assert!(i < j, "{rel} 里共用段的两个标记次序反了");
        // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b`〕**比之前先把每行的行首空白削掉。**
        //
        // 剖分把 monitor 那一份从 `mod tests {}` 里搬了出来 ⇒ 它整段**左移了一级**，
        // 而后端那一份仍在模块内、缩进 4。两份逐字节比当场不等，而**那句话
        // 一个字都没改** —— 不等的是缩进，缩进是「位置」，正是搬树会改的那一样东西。
        // ⚠ 削掉的**只有行首空白**：句子内容、标点、行序、行数一律照比。
        //   这不是放宽 —— 本条买的逐字写在头注里：「不会再一处真一处假」，
        //   而「一处比另一处多四个空格」不是「说的不是同一句话」。
        // ⚠ 连**块尾那截空白**也要削：`j` 落在「止」标记的 `//` 上，而后端那一份
        //   那一行带 4 个缩进 ⇒ 块尾是 `\n    `，monitor 那一份是 `\n`。
        //   `str::lines()` 会把 `"a\n"` 切成 1 行、把 `"a\n    "` 切成 2 行
        //   —— 两边行数当场差一，而差的那一行是**空白**。
        let block: String = src[i..j]
            .trim_end()
            .lines()
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push((rel, block));
    }
    // 🔴 运行时拼：写成字面量会被 `needle_anchor_registry` 那条棘轮
    //    （语料变量上的裸子串匹配，只许降）数进去 —— 那条棘轮不许为了今天好过而调高。
    let pointer = format!("assert_test_module_ranges_are_brace{}", "_balanced");
    // 反向自检：抽出来的不许是空的、也不许短到什么都不是（`"" == ""` 照样成立）。
    for (rel, b) in &blocks {
        let lines = b.trim().lines().count();
        assert!(
            lines >= 10,
            "{rel} 抽到的共用段只有 {lines} 行 —— 抽空了的话「两处相等」照样绿"
        );
        assert!(
            b.contains(pointer.as_str()),
            "{rel} 的共用段里没有点名今天真正守这一形的那条判据 —— \
                 `KR115D3` 要的就是这两处从「这里会红」改成指向它"
        );
    }
    assert_eq!(
        blocks[0].1, blocks[1].1,
        "**两个住址、同一句话，而今天它们说的不是同一句。**\n\
             {} 与 {} 的共用段逐字对不上 ⇒ 有人只改了一处。\n\
             两处一起改，或者把这一段挪到一处去派生。",
        blocks[0].0, blocks[1].0
    );
}
