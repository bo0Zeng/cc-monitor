use super::*;

/// ★★ **两个 `#[test]` 叠在同一个函数上 ⇒ 另一个函数悄悄不是测试了。**
///
/// # 这不是洁癖，是本仓两天内出现两次的真实事故
///
/// | 谁 | 代价 |
/// |---|---|
/// | `inbound_client.rs`（08-11，P2s 插判据时插错位置） | `the_hello_witness_can_only_come_from_a_hello_frame` **一个属性都没有 = 死代码**，而 `the_only_way_to_build_an_inbound_client_is_into_client` 的闭合论证逐字压在它身上 |
/// | `tmux.rs`（`4d8cfc2`，更早） | `tmux_targets_use_exact_match` 同样掉了属性；实测 `cargo test tmux_targets_use_exact_match` 回 **`0 passed`** —— 它根本不存在 |
///
/// # 为什么编译器拦不住
///
/// `duplicate_macro_attributes` 只是 **warn**，而本仓 clippy 不带 `-D warnings`
/// （`local_backend.rs` 自己记着这条）。⇒ 编得过、跑得过、**少跑一条判据没人知道**。
///
/// # 本条扫什么
///
/// 本仓风格是 `#[test]` 写在**头注之前**（`#[test]` → `/// …` → `fn`），
/// 这让「按 fn 名找锚点再往前插」这种改法极易把新块插进「属性与它的 fn」之间。
/// ⇒ 判据：一个 `#[test]` 之后，跳过头注/其它属性/空行，**下一行必须是 `fn`**。
/// 若又遇到一个 `#[test]`，就是这个形态。
#[test]
fn no_two_test_attributes_land_on_the_same_function() {
    // 🔴 〔步 7c 剖分 2026-09-19〕**这一格同时踩了 `§6.2` 的 A 类与 B 类，两条一起修。**
    //
    // · **B 类**：语料根原来只有 `<bridge>/src` 一棵。`#[test]` 剖分之后整批住进
    //   `<repo>/tests/` ⇒ 那一棵树里 `#[test]` 归零，自检逐字报「扫到 0 个 —— 扫描面坏了」。
    //   ⇒ 语料改成**四棵互不包含的根**：两棵生产树 ＋ 两棵测试树。
    //   两棵生产树今天该是 0 个 `#[test]`，但**仍然要扫** —— 哪天有人在 `src/` 里写一个，
    //   本条要看得见它（少扫不会红，只会零命中地绿）。
    // · **A 类**：原来靠 `scan_tree_excluding_self(.., file!())` 摘掉自己。本文件被
    //   `#[path]` 引进来 ⇒ `file!()` 是折返路径 ⇒ 摘除恒空转（`§5.4b` 纪律 4）。
    //   而语料里现在**含 `tests/bridge/` 这棵树，也就是含本文件自己** ⇒ 摘除失效不再无害：
    //   本条会数到自己头注里那些讲形状的 `#[test]` 字样。
    //   ⇒ 改成 `scan_tree_excluding` 的**明写名单**，摘不到就 panic。
    //
    // 🔴 本条这一轮真的咬到东西了：`src/backend` 从来不在它的射程里，
    // 而那棵树上**有 3 处**「一个 fn 两个 `#[test]`」（libtest 因此把它们各注册两遍：
    // `cargo test -- --list` 705 行 / 702 个不同名字）。逐处在步 7c 报告里点名。
    let repo = addr_repo_root();
    let mut files = Vec::new();
    for (tree, excluded) in [
        ("src/bridge/src", &[] as &[&str]),
        ("src/backend", &[]),
        ("tests/bridge", &["structural_scan_tests.rs"]),
        ("tests/backend", &[]),
    ] {
        // 排除名单里写的是**本文件自己**：它住 `tests/bridge/`，在语料里，
        // 而它的头注为了讲清形状逐字写了 `#[test]` 字样 ⇒ 必须明写摘掉。
        // `scan_tree_excluding` 摘不到就 panic ⇒ 本文件改名/搬家会当场出声。
        files.extend(guard_core::scan_tree_excluding(
            &repo.join(tree),
            &["rs"],
            excluded,
        ));
    }
    let mut offenders: Vec<String> = Vec::new();
    let mut seen = 0usize;
    for (path, src) in &files {
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if l.trim() != "#[test]" {
                continue;
            }
            seen += 1;
            for next in lines.iter().skip(i + 1) {
                let t = next.trim();
                if t.is_empty() || t.starts_with("///") || t.starts_with("//") {
                    continue;
                }
                if t == "#[test]" {
                    offenders.push(format!(
                        "  {}:{} —— 这个 `#[test]` 之后又是一个 `#[test]`",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        i + 1
                    ));
                    break;
                }
                if t.starts_with("#[") {
                    continue; // 别的属性（`#[cfg(...)]` / `#[ignore]`）合法
                }
                break; // 落到 `fn` 或别的东西：本条只管 `#[test]` 连着 `#[test]`
            }
        }
    }
    // 抽取器自检：真的扫到了测试（否则本条会零命中地绿）。
    assert!(
        seen > 200,
        "只扫到 {seen} 个 `#[test]` —— 扫描面坏了，本条在空转（08-11 实测全树 {} 个文件）",
        files.len()
    );
    // ── 第二半：**直接钉伤害**，不只钉症状 ────────────────────────────
    //
    // 上面那半认的是「`#[test]` 后面又是 `#[test]`」这个**形状**。
    // 但真正的伤害是「某个 `fn` 没有属性、于是不是测试」——两者不等价：
    // 08-11 我修完第一处之后，另一处照样掉了属性，而上面那半**一声不吭**。
    //
    // ⚠ 判据形状按实测收敛过两次：
    // ① 第一版往回扫时遇到非 `#[` 行就停 ⇒ 多行 `#[cfg_attr(…)]` 的 `)]` 把它挡住，
    //    把一条**活着的**判据（`tmux.rs` 的 `the_shell_gate_expression_agrees_with_the_golden_table`）
    //    报成死的。真阳率 0% ⇒ 按铁律 18 那种版本不许留。
    // ② 现版改成「往回扫到空行 / `}` / 上一个 fn 为止，这一段里有没有 `#[test]`」，
    //    全树实测 **0 误报**。
    //
    // 人群限「`mod …test… {` 之后」+「无参无返回的 `fn 名()`」——
    // 那正是判据的形状；带参的是夹具（`hello_frame(commands: &[&str])` 之类），不在人群里。
    let mut orphans: Vec<String> = Vec::new();
    for (path, src) in &files {
        let lines: Vec<&str> = src.lines().collect();
        let Some(start) = lines.iter().position(|l| {
            let t = l.trim();
            // 🔴 〔`K-R75` 09-12〕这里原先也是 `t.starts_with("mod ")` ——
            //    **同一族病的第二个住址**，而它的失效方向比剥法那处更阴：
            //    一份文件的测试模块若写成 `pub(crate) mod tests {`，`position` 返回 `None`
            //    ⇒ 那份文件**整份掉出本条的人群**，而本条照样绿（人群缩水，静默）。
            //    ⇒ 走同一份权威的形状判定，不再各写一份近似的（`E3`）。
            //    ⚠ 现打：今天全仓「带可见性且名字含 test 的 `mod`」**0 处**
            //       ⇒ 这一改在今天的盘上是 **no-op**，买的是「下一处这么写时不会静默」。
            guard_core::strip_visibility(t).starts_with("mod ")
                && t.contains("test")
                && t.ends_with('{')
        }) else {
            continue;
        };
        for (i, l) in lines.iter().enumerate().skip(start) {
            let t = l.trim();
            if !(t.starts_with("fn ") && t.ends_with("() {")) {
                continue;
            }
            let mut k = i;
            let mut has_attr = false;
            // ⚠ 两根前缀针**运行期拼**〔步 7c 现打逼出来的〕：
            //   `needle_anchor_registry` 的 `.starts_with(` 是**0 上限**的递减棘轮，
            //   而它认的是「语料变量上的裸字面量匹配」。步 7c 往本文件新加了两处
            //   从磁盘语料派生的 `let`（`p9_tree_shape` / `fn_bodies` 里的 `lines`），
            //   而那条棘轮的语料变量追踪是**按名字、整份文件**算的
            //   ⇒ 这两处**一直存在**的针**第一次被数到**（2 处 > 上限 0）。
            //   拼出来就不在它的人群里；针的含义一个字没变。
            //   ★ 如实记：这是「人群扩大之后旧债第一次现形」，不是本轮新写的债。
            let fn_head = format!("{} ", "fn");
            let pub_fn_head = format!("pub {} ", "fn");
            while k > 0 {
                let prev = lines[k - 1].trim();
                if prev.is_empty()
                    || prev == "\u{7d}"
                    || prev.starts_with(fn_head.as_str())
                    || prev.starts_with(pub_fn_head.as_str())
                {
                    break;
                }
                if prev.contains("#[test]") {
                    has_attr = true;
                    break;
                }
                k -= 1;
            }
            if !has_attr {
                orphans.push(format!(
                    "  {}:{} —— `{}` 在测试段里、长得像判据，却没有 `#[test]`",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    i + 1,
                    t.trim_start_matches("fn ").trim_end_matches("() {")
                ));
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "这些函数住在测试段里、长得像判据，但**没有 `#[test]`，从不运行**：\n{}\n\n\
             ⇒ 它和别的判据长得一模一样，读的人会以为那条性质有人守着。\n\
             08-11 全树逮到三条这样的死判据（`the_hello_witness_can_only_come_from_a_hello_frame`、\n\
             `tmux_targets_use_exact_match`、以及一条我自己修出来的），最久的从 `4d8cfc2` 起就是死的。",
        orphans.join("\n")
    );

    assert!(
        offenders.is_empty(),
        "这些地方两个 `#[test]` 叠在同一个函数上：\n{}\n\n\
             ⇒ 后面某个 `fn` 因此**没有属性、不是测试、从不运行**，而它看起来和别的判据一模一样。\n\
             `duplicate_macro_attributes` 只是 warn，本仓 clippy 不带 `-D warnings` ⇒ 编得过、跑得过、没人知道。\n\
             修法：把被顶开的那段头注搬回它自己的 `fn` 前，并给掉了属性的那个 `fn` 补回 `#[test]`。",
        offenders.join("\n")
    );
}

/// U8a-2a：monitor 的每个源文件都要能被共享剥法（`guard_core`）剥干净。
///
/// 与后端侧 `every_backend_file_strips_clean` 同一条，只是换了一棵树。
///
/// `min_files` 是**计数自检**：遍历坏掉时它会红，而不是静默扫 0 个文件通过。
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
fn every_monitor_file_strips_clean() {
    // 地板 = **实测值**（2026-08-02：52 个 .rs）。松着放等于把灵敏度交出去
    // （同 `ci.yml` 那条 shellcheck 覆盖面棘轮的教条：棘的时候把实测构成一起写下）。
    guard_core::assert_tree_strips_clean(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        // ★ audit-0805 F16：52 → **80**（今日实测）。
        // 余量 28 的时候，`backend/`(15) + `adapter/`(2) **整体掉出扫描面仍会绿** ——
        // 而「扫描面缩水」正是这条地板存在的全部理由。
        // ⚠ 棘轮纪律：只许升不许降；要降必须带「副本真退役」的证据。
        80,
    );
}

/// 🔴 `K-R75` `KR75D2` 的**点名**这一半：上一条红的时候，必须说得出**是哪一份** `mod.rs`。
///
/// # 它买的是什么
///
/// 两棵真树里 `mod.rs` 各有十几份（`backend/mod.rs` · `backend/control/mod.rs` · …）。
/// `K-R75` 之前 `assert_tree_strips_clean` 传给断言的 `who` 是 `path.file_name()`
/// ⇒ 报错逐字「`mod.rs`：剥完仍残留 1 个测试属性」，**指不出是哪一份**。
/// 而「点名那份文件」正是这条反向自检的产出，不是附赠 ——
/// 一条说不出住址的红，下一个人要拿全树重新找一遍。
///
/// # 为什么住在这里而不是 `guard-core` 里
///
/// 它要一棵**真目录**才测得了，而造目录要写盘 —— 而 `guard-core`
/// **一处写盘都不许有，连 `cfg(test)` 里也不许**（backend 侧
/// `readonly_guard::g6_dependency_signoff::…::the_clean_verdict_is_re_measured_on_the_tree_every_run`
/// 按**原文行**重扫那几棵仓内 crate，不走 `production_code`）。
/// 〔09-12 现打：先写在 `guard-core` 里，门禁 `backend` 那格当场 `685 passed; 1 failed`，
///  逐字点名 `guard-core（../src/bridge/crates/guard-core）src/lib.rs:2219: \`fs::create_dir\``〔行号墓碑〕
///  —— 那个行号是**当时那一趟**的读数，那段代码已经搬走 ⇒ 它必然腐；留着是为了说清
///  「那把尺子按原文行数、连测试夹具都算」，不是给人拿去定位。
///  处置是**搬家**，不是去动那把尺子 —— 那把尺子同时守着后端本体两层判据。〕
///
/// **死值验**：把 `guard_core::assert_tree_strips_clean` 里的 `strip_prefix(root)`
/// 退回 `path.file_name()` ⇒ 本条必须红（读数落 `tests/evidence/K-R75-剥法认形状与真静默读数.md`）。
#[test]
fn the_tree_walk_names_the_file_by_path_not_by_basename() {
    let root = std::env::temp_dir().join(format!(
        "kr75-naming-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let deep = root.join("alpha").join("beta");
    std::fs::create_dir_all(&deep).expect("建夹具目录");
    std::fs::write(root.join("mod.rs"), "fn clean() {}\n").expect("写干净的那份");
    // 属性与 `mod` 之间夹一行文档注释 ⇒ 剥法认不出（这正是本条要点名的那一族）。
    let cfg = format!("#[{}({})]", "cfg", "test");
    std::fs::write(
        deep.join("mod.rs"),
        format!("fn a() {{}}\n{cfg}\n/// 说明\npub(crate) mod probe {{\n    fn z() {{}}\n}}\n"),
    )
    .expect("写带违规写法的那份");
    let r = std::panic::catch_unwind({
        let root = root.clone();
        move || guard_core::assert_tree_strips_clean(&root, 1)
    });
    let _ = std::fs::remove_dir_all(&root);
    let e = r.expect_err("带违规写法的那份没红 —— 树遍历这一层的第二半没接上");
    let msg = e
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains("alpha/beta/mod.rs"),
        "报错里没有带目录的相对路径 —— 两份 `mod.rs` 分不开：{msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R77`（09-12）：**「`#[cfg(test)]` 顶层再导出自己的测试模块」那道绕道** ——
// 拆掉最后一处的同一拍给它上棘轮。裁定住 `DECISIONS.md#R36` 裁定二。
// ═══════════════════════════════════════════════════════════════════════

/// 顶层 = **列 0**（不带前导空白）且非空行。
fn detour_top_level(line: &str) -> bool {
    !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t')
}

/// `from` 那一行之后**第一行有内容的顶层行**：`(0 起的下标, trim 后的原文)`。
///
/// 空行与整行注释跳过（属性与它修饰的 item 之间夹一行 `//` 是合法 Rust，
/// 跳过它是**只严不松**）；遇到缩进行就停 —— 那说明这个属性挂在别人的块里。
///
/// 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**同一个 item 上的其它属性也要跳过**。
/// 剖分之后生产段里那 111 处测试模块声明长这样：
///
/// ```ignore
/// #[cfg(test)]
/// #[path = "../../../tests/bridge/X_tests.rs"]
/// mod tests;
/// ```
///
/// 属性是**堆在同一个 item 上**的，而这里原来一遇到 `#[path = …]` 就把它当成「下一个 item」
/// 收走 ⇒ `mod tests;` 再也认不出来：`cfg_test_module_names` 从 250 掉到 165，
/// 而掉下去之后 [`cfg_test_reexport_sites`] 对那 111 份文件**恒真地空**（静默空真）。
/// ⚠ 跳属性**只严不松**：属性只可能修饰它下面那个 item，跳过去只让人群**变大**。
fn detour_next_item<'a>(lines: &[&'a str], from: usize) -> Option<(usize, &'a str)> {
    for (k, line) in lines.iter().enumerate().skip(from + 1) {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("#[") {
            continue;
        }
        if !detour_top_level(line) {
            return None;
        }
        return Some((k, t));
    }
    None
}

/// 本文件里由 `#[cfg(test)]` 修饰的**顶层模块**名 —— **可见性一律不看**。
///
/// 🔴 这一句就是 `K-R75` 治过的那个病的复发点：那一版剥法只认字面 `mod `，
/// 一个 `pub(crate)` 前缀就让整份文件**静默掉出人群**。⇒ 走同一份权威的形状判定
/// `guard_core::strip_visibility`，**不再各写一份近似的**（本仓 `E3`）。
fn cfg_test_module_names(src: &str) -> std::collections::BTreeSet<&str> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = std::collections::BTreeSet::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != "#[cfg(test)]" || !detour_top_level(l) {
            continue;
        }
        let Some((_, next)) = detour_next_item(&lines, i) else {
            continue;
        };
        let Some(decl) = guard_core::strip_visibility(next).strip_prefix("mod ") else {
            continue;
        };
        let name = decl
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('{')
            .trim_end_matches(';');
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

/// 这道绕道在 `src` 里的每一处：`(1 起的行号, 那一行 trim 后的原文)`。
///
/// **人群逐字**：顶层一行恰好是 `#[cfg(test)]`，其后第一行有内容的顶层行是
/// `<任意可见性> use <本文件的某个 cfg(test) 模块>::…`。
fn cfg_test_reexport_sites(src: &str) -> Vec<(usize, String)> {
    let mods = cfg_test_module_names(src);
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != "#[cfg(test)]" || !detour_top_level(l) {
            continue;
        }
        let Some((k, next)) = detour_next_item(&lines, i) else {
            continue;
        };
        let Some(path) = guard_core::strip_visibility(next).strip_prefix("use ") else {
            continue;
        };
        let seg = path.trim().split("::").next().unwrap_or_default().trim();
        if mods.contains(seg) {
            out.push((k + 1, next.to_string()));
        }
    }
    out
}

/// 住址：仓根相对路径（够不着仓根就退回原样）。
///
/// `K-R75` 那条逐字：**点名要给住址，不是基名** —— 两棵树里同名文件是常态。
fn detour_addr(p: &std::path::Path) -> String {
    p.strip_prefix(addr_repo_root())
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 这道绕道的**存量上限**。🔴 **今天是 0，而且它有资格恒零** ——
/// 三条理由写在 [`the_cfg_test_reexport_detour_stays_extinct`] 头注，别在这里抬它。
const DETOUR_CEILING: usize = 0;

/// 纯算子：今天的处数 `today` 有没有**越过**上限 `ceiling`；越了就回超出多少。
///
/// 🔴 **单独成函数的理由与 `scanning_guard_registry::ratchet_backslide` 逐字同源**：
/// 真树上今天 `today` 与 `ceiling` **都是 0** ⇒ 把 `>` 写成 `<` / `>=` / `!=`，
/// **输出与判对了一模一样（绿）**。
/// [`the_detour_reader_can_tell_a_new_one_from_none`] 拿**合成读数**把这一格钉住。
fn detour_over_ceiling(today: usize, ceiling: usize) -> Option<usize> {
    if today > ceiling {
        Some(today - ceiling)
    } else {
        None
    }
}

/// 🔴 `K-R77` `KR77D2`：**那道 `#[cfg(test)]` 顶层再导出的绕道，恒零。**
///
/// # 它治的是什么
///
/// 绕道的形状：测试模块写成私有 `mod X`，再在文件顶层补一行
/// `#[cfg(test)] pub(crate) use X::…`，把要跨模块用的东西导出去。
/// 它**曾经有一个真理由** —— `guard_core::test_module_ranges` 那时按字面前缀认
/// `mod `，模块一带可见性前缀就认不出来，那份文件**整段测试代码留在生产段里**被别的
/// 守卫扫。`K-R75`（09-12）把那一步换成按形状剥可见性
/// （`guard_core::strip_visibility`）之后，**那个理由没了**，而盘上那三处还在。
///
/// # 🔴 在本条落地之前，这一族**一颗牙都没有**
///
/// `K-R76` 现打（它的刀 `M3`）：把已经拆掉的那道绕道**整个装回去**，全量门禁 **0 红**。
/// ⇒ 本条不是「再加一层保险」，它是这一族的**第一道闸**。
///
/// # 凭什么可以恒零（而不是「比历史最低档低」）
///
/// `K-R38` 亲手证过「**钉一个固定上限买不到棘轮**」—— 降到低点再涨回来仍然过。
/// 这里选**恒零**，三条理由缺一条都不该恒零：
/// ① **今天真的是 0**：本条自己就是那个读数（两棵树 · 现打，见
///    [`the_detour_ratchet_reaches_both_trees`] 的分母）；
/// ② **它没有合法用途了**：这道绕道存在的唯一理由是剥法那个缺陷，缺陷已修；
///    今天要跨模块就把模块写成 `pub(crate) mod` —— 那正是三处拆完之后的写法；
/// ③ **0 是这个量的下界** ⇒ 「必须比历史最低档低」在这里退化成「等于 0」，
///    再拿 git 历史算一遍最低档只是把同一句话说贵一点。
/// ⇒ 恒零比「历史最低档」**更强**：后者允许「降下去再涨回来」的那一格，前者不允许。
///
/// ⚠ **这条前提本来就该变的时候，去哪里重新裁定**（判据纪律 11）：
/// 只有一种情形该动它 —— 剥法**又**认不出某种可见性写法。那时先修
/// `guard_core::strip_visibility`，**不是**回来抬这个 0。
/// 真要给这道绕道开一个正当口子，走 `DECISIONS.md` 立一条裁定。
///
/// # 它买不到什么（如实登记，别读成证明）
///
/// · 只认**顶层**那一形：把再导出塞进另一个 `mod` 里包一层，它看不见。
/// · 只认属性行**逐字** `#[cfg(test)]`：写成 `#[cfg(all(test, unix))]` 躲得过
///   （这一格由 [`the_detour_scanner_sees_every_visibility`] 的反面那半**钉着读数**，
///   哪天扩了人群就来改那一行）。
/// · 它是**文本**扫描：不问那个再导出有没有人用，也不问拆了之后编不编得过。
#[test]
fn the_cfg_test_reexport_detour_stays_extinct() {
    let corpus = addr_corpus();

    // ── 地板① **人群的第一步**：认得出多少个 `#[cfg(test)]` 顶层模块。
    //    这一步塌了 ⇒ 任何再导出都匹配不上，而输出与「盘上真的没有」一模一样（静默空真）。
    let mods: usize = corpus
        .iter()
        .map(|(_, s)| cfg_test_module_names(s).len())
        .sum();
    assert!(
        mods >= 200,
        "只认出 {mods} 个 `#[cfg(test)]` 顶层模块（语料 {} 份）—— 人群的第一步塌了，\
             本条在空转。09-12 现打 250 个 / 207 份（两棵树）。",
        corpus.len()
    );

    // ── 地板② **扫描器真的会说话**：同一个纯算子喂一份合成语料，必须恰好逮到 1 处。
    //    「断某个对账今天该是空的」是空真形（闸死了 `[] == []` 照样绿）⇒ 活体正控在这里。
    let attr = format!("#[{}({})]", "cfg", "test");
    let probe = format!(
        "fn prod() {{}}\n{attr}\npub(crate) use tests::HELPER;\n\n\
             {attr}\nmod tests {{\n    pub(crate) const HELPER: usize = 1;\n}}\n"
    );
    assert_eq!(
        cfg_test_reexport_sites(&probe).len(),
        1,
        "合成语料里那一处绕道都逮不到 —— 扫描器坏了，下面那句「盘上 0 处」不算数"
    );

    // ── 正题：两棵树上现打。
    let mut sites: Vec<String> = Vec::new();
    for (p, src) in &corpus {
        for (ln, text) in cfg_test_reexport_sites(src) {
            sites.push(format!("  {}（第 {ln} 行）—— `{text}`", detour_addr(p)));
        }
    }
    if let Some(over) = detour_over_ceiling(sites.len(), DETOUR_CEILING) {
        panic!(
            "这里有 {} 处「`#[cfg(test)]` 顶层再导出自己的测试模块」，比上限 \
                 {DETOUR_CEILING} 多 {over} 处：\n{}\n\n\
                 ⇒ **这道绕道今天没有理由了**：`guard_core::test_module_ranges` 自 `K-R75`\n\
                 起按形状剥可见性修饰，测试模块直接写成 `pub(crate) mod X` 就认得出来，\n\
                 跨模块的取名走 `crate::<文件>::X::…`，不需要在顶层再导出一次。\n\
                 修法：把 `mod X` 写成 `pub(crate) mod X`，删掉这一行，引用者改走全路径。\n\
                 （盘上三处的先例：`ssh_source.rs` 的 `dial_move_judge` · `local_backend_host.rs` 的\n\
                  `tests` · `readonly_guard.rs` 的 `g6_doctrine`，`K-R76`/`K-R77` 各拆过。）\n\
                 ⚠ 真有正当理由要开口子 ⇒ 走 `DECISIONS.md` 立裁定，**不许**在这里抬 \
                 `DETOUR_CEILING`。",
            sites.len(),
            sites.join("\n")
        );
    }
}

/// 🔴 `K-R77` `KR77D3`：上面那条棘轮的语料面**跨两棵树** —— 少一棵当场红。
///
/// # 为什么要单独一条
///
/// 这道绕道**两棵树上都长过**（monitor 侧 `ssh_source.rs` 与 `local_backend_host.rs`，
/// backend 侧 `readonly_guard.rs`）。而它的失效方向是**分母悄悄缩到一棵树** ——
/// 那时上面那条照样绿，读起来却像「两棵树都守住了」。
/// ⇒ 这一条把「够得到」本身变成判据：**逐棵树各有地板**，并且**点名**那三份
/// 真长过绕道的文件必须在语料里。
///
/// # ⚠ 它买不到什么 —— `KR77D3` 要的那个诚实读数，写在这里
///
/// 闸**住在 monitor 这棵树上**（门禁 `cargo` 那一格 = `cargo test --workspace --lib`），
/// 靠**读盘上的 `src/backend`** 够到后端。
/// ⇒ 🔴 **backend 自己那一格（门禁 `backend` = `cd src/backend && cargo test`）
/// 今天没有这道闸**：只跑后端那一格的人新写一道绕道，**不会红**。
/// 两棵树是两个 workspace（`src/backend/Cargo.toml` 头注逐字写着 standalone），
/// 而「backend 那一格要不要也跑一条同形的判据」不是本件能决定的事 —— 交回 PM。
#[test]
fn the_detour_ratchet_reaches_both_trees() {
    let corpus = addr_corpus();
    let mut monitor = 0usize;
    let mut backend = 0usize;
    let mut tests = 0usize;
    for (p, _) in &corpus {
        let rel = detour_addr(p);
        if rel.starts_with("src/backend/") {
            backend += 1;
        } else if rel.starts_with("src/bridge/") {
            monitor += 1;
        } else if rel.starts_with("tests/") {
            tests += 1;
        }
    }
    // 地板逐棵树各一条 —— 合起来一条挡不住「一棵塌了另一棵涨了」。
    // 09-12 现打：monitor 118（`src/bridge/src` 108 ＋ `src/bridge/crates` 9 ＋ `build.rs` 1；
    //   当年 `scan_tree!` 的自摘把本文件摘掉过，`addr_corpus` 用相对住址补回来；
    //   那一刀在本仓**已经不生效**，补回来的是第二份，见 `addr_corpus` 头注）· backend 88。
    assert!(
        monitor >= 100,
        "monitor 那棵树只收到 {monitor} 份 .rs（09-12 现打 118）—— 分母缩水了"
    );
    // 〔2026-09-18 下调 80 → 65〕不是分母缩水：搬树把 **19 份纯测试文件**从
    // 后端树移到了 `<repo>/tests/backend/`。当时现打 `src/backend/**.rs` = 72
    // （`tests/backend` 另有 19）⇒ 72 + 19 = 91，与 09-12 的 88 同量级。
    // 🔴 〔条 67 · 2026-09-18 再下调 65 → 58〕**这一次是真的少了文件，不是分母缩水**：
    // 用户逐字「**不在现在设计里的全部删掉**」⇒ `sidecars/` 整棵四份 `.rs`（2 008 行）
    // ＋ `platform/landing.rs`（唯一消费者没了）一起删 ⇒ 72 − 5 = **64**。
    // 留 6 份余量（与上一版 65 vs 72 的 margin 同量级）。
    // ⚠ 这两种情形的读数**长得一模一样**（都是「backend 那个数变小了」），
    //   所以每一次下调都必须逐份点名删的是谁 —— 那是本条唯一能分辨它们的办法。
    assert!(
        backend >= 58,
        "🔴 后端那棵树只收到 {backend} 份 .rs（2026-09-18 条 67 之后现打 64，另有 19 份在 tests/backend）—— \
             `src/backend` 掉出语料面了。\n\
             那一刻上面那条棘轮照样绿，而它只守着一棵树 —— \
             **报「已守住」而分母只有一棵树**，正是本条要挡的形状。"
    );
    // 〔2026-09-18 新增〕测试树也要有自己的地板 —— 它是第三棵，现打 19 份。
    assert!(
        tests >= 15,
        "测试那棵树只收到 {tests} 份 .rs（2026-09-18 现打 19）—— \
             `tests/` 掉出语料面了，而上面两条照样绿。"
    );
    // 点名：三份真长过绕道的文件必须都在语料里（地板是数，这一条是**住址**）。
    let names: std::collections::BTreeSet<String> =
        corpus.iter().map(|(p, _)| detour_addr(p)).collect();
    for want in [
        // 〔搬树 2026-09-17〕它是纯测试文件，搬到了 `tests/backend/`。
        "tests/backend/readonly_guard.rs",
        "src/bridge/src/ssh_source.rs",
        "src/bridge/src/local_backend_host.rs",
    ] {
        assert!(
            names.contains(want),
            "`{want}` 不在语料面里 —— 它是这道绕道真长过的三处之一，\
                 够不着它就等于这一处从此没人守（monitor {monitor} 份 · backend {backend} 份）"
        );
    }
}

/// 🔴 `K-R77` `KR77D2` 的**反向那半** —— 照 `K-R38` 那条的形。
///
/// 真树上今天处数与 `DETOUR_CEILING` **都是 0** ⇒ 把 [`detour_over_ceiling`] 里的
/// `>` 写成 `<`（或 `>=`、`!=`），**正题照样绿**。`K-R38` 那次实打逐字：
/// 「正题照样绿、只有反向红 ⇒ 反向那半承重」。
/// ⇒ 这里拿**合成读数**把方向钉死，一格都不靠真树。
#[test]
fn the_detour_reader_can_tell_a_new_one_from_none() {
    assert_eq!(
        detour_over_ceiling(1, 0),
        Some(1),
        "新长出来 1 处而它说没事 —— 比较写反了，棘轮一颗牙都没有"
    );
    assert_eq!(detour_over_ceiling(3, 0), Some(3), "超出的处数报错了");
    assert_eq!(
        detour_over_ceiling(0, 0),
        None,
        "0 处而它报违规 —— 恒红的闸，下一个人第一件事就是把它关掉"
    );
    assert_eq!(detour_over_ceiling(0, 1), None, "低于上限却报违规");
    assert_eq!(detour_over_ceiling(2, 5), None, "低于上限却报违规");
    assert_eq!(detour_over_ceiling(6, 5), Some(1), "刚越线那一格没逮住");
}

/// 🔴 `K-R77` `KR77D2` 的**失效方向**那半：人群**不许只认一种可见性**。
///
/// 那正是 `K-R75` 治过的病换个地方再犯 —— 前一版剥法只认字面 `mod `，
/// 一个 `pub(crate)` 前缀就让整份文件掉出人群，**而判据照样绿**。
/// Rust 的 `Visibility` 文法是语言定死的闭集，而 `pub(in <路径>)` 的路径**任意长**
/// ⇒ **前缀表穷举不了、形状认得出**。这一条逐形喂一遍，外加反面五形。
#[test]
fn the_detour_scanner_sees_every_visibility() {
    let attr = format!("#[{}({})]", "cfg", "test");
    let with_vis = |vis: &str| {
        format!(
            "fn prod() {{}}\n{attr}\n{vis}use tests::HELPER;\n\n\
                 {attr}\nmod tests {{\n    pub(crate) const HELPER: usize = 1;\n}}\n"
        )
    };
    for vis in [
        "",
        "pub ",
        "pub(crate) ",
        "pub(super) ",
        "pub(self) ",
        "pub(in crate::alpha::beta::gamma) ",
    ] {
        let src = with_vis(vis);
        let hits = cfg_test_reexport_sites(&src);
        assert_eq!(
            hits.len(),
            1,
            "可见性写成 `{vis}` 时逮不到（实得 {hits:?}）—— 人群只认一种拼法，\
                 那是 `K-R75` 治过的病换个地方再犯"
        );
        assert!(
            hits[0].1.ends_with("use tests::HELPER;"),
            "逮到了但报的不是那一行：{:?}",
            hits[0]
        );
        assert_eq!(
            hits[0].0, 3,
            "行号指错了（应当是 `use` 那一行，不是属性那一行）"
        );
    }
    // 属性与 item 之间夹一行注释 —— 合法 Rust，躲不过去。
    let commented = format!(
        "{attr}\n// 说明\npub(crate) use tests::HELPER;\n\n\
             {attr}\nmod tests {{\n    pub(crate) const HELPER: usize = 1;\n}}\n"
    );
    assert_eq!(
        cfg_test_reexport_sites(&commented).len(),
        1,
        "中间夹一行注释就躲过去了"
    );

    // ── 反面：这五形一个都不许算进来（前四条是**正当写法**，第五条是**已登记的盲区**）。
    let mod_only = format!("{attr}\nmod tests {{\n    fn a() {{}}\n}}\n");
    let foreign = format!(
        "{attr}\npub(crate) use guard_core::production_code;\n\n\
             {attr}\nmod tests {{\n    fn a() {{}}\n}}\n"
    );
    let from_super = format!(
        "{attr}\npub(crate) use super::HELPER;\n\n\
             {attr}\nmod tests {{\n    fn a() {{}}\n}}\n"
    );
    let nested = format!(
        "mod outer {{\n    {attr}\n    pub(crate) use tests::HELPER;\n}}\n\
             {attr}\nmod tests {{\n    pub(crate) const HELPER: usize = 1;\n}}\n"
    );
    let wider_cfg = format!(
        "#[{}(all({}, unix))]\npub(crate) use tests::HELPER;\n\n\
             {attr}\nmod tests {{\n    pub(crate) const HELPER: usize = 1;\n}}\n",
        "cfg", "test"
    );
    for (what, src) in [
        ("`#[cfg(test)]` 修饰的是模块声明，不是再导出", &mod_only),
        (
            "再导出的是**别的 crate** 的东西（`guard_support.rs` 就是这一形）",
            &foreign,
        ),
        ("再导出的是 `super::`，不是本文件的测试模块", &from_super),
        (
            "再导出包在另一个 `mod` 里 —— **已登记的盲区**，不是正当写法",
            &nested,
        ),
        (
            "属性不是逐字 `#[cfg(test)]` —— **已登记的盲区**，扩了人群就来改这一行",
            &wider_cfg,
        ),
    ] {
        assert_eq!(
            cfg_test_reexport_sites(src).len(),
            0,
            "这一形被算进人群了：{what}"
        );
    }
}

/// ★ **剥注释只许有一个权威实现**〔audit-0805 §5 3h，08-06〕。
///
/// # 它挡的是什么
///
/// 「判据数到注释」在本区犯过三次（F12 跨语言对拍 · F24 的裸 `contains` 计数 ·
/// 1k 的属性回溯）。三次的补法都是**在自己文件里现写一个剥注释的小函数** ——
/// 于是 08-06 一数：**四个具名私有实现**，而 §5 3h 当时写的是「三处」。
///
/// 更糟的是它们**语义不同**：两份整行删、一份整行留空、一份按 marker 截断。
/// 「同一个词在四个地方各是一个意思」正是 E3 要消灭的形状。
///
/// # 今天的唯一例外，以及它凭什么是例外
///
/// `profile_installer::strip_comments(src, marker)` 按**第一个 marker 截断整行**，
/// 因此能吃掉**行尾注释**；共享原语刻意不这么做（会砍坏 `"http://host"` 这类字面量，
/// 详见 `guard_core::strip_comment_lines` 头注）。它扫的是**自己生成的** shell/rc 片段，
/// 语料可控 ⇒ 那个风险在它那里不存在。**这不是豁免，是另一种语义。**
///
/// ⚠ 想再加一个 ⇒ 先问「共享原语为什么不够」，答得出来才加进下面这张表。
/// ★〔audit-0805 08-06〕**按「函数做了什么」再扫一遍剥注释实现**（默认拒绝）。
///
/// # 它补的洞
///
/// 下面那条按**函数名的三种拼法**取样（`strip_line_comments` / `strip_comments` /
/// `without_comments`）。实测：往 `utils.rs` 加一个逐字同形、只是改名叫
/// `fn drop_comments` 的实现 ⇒ **那条判据全绿**。
/// 「只许有一份共享实现」这条纪律，此前只对**三个名字**成立。
///
/// # 人群怎么定的（量了两轮才收住）
///
/// 第一轮按行为取样（函数体里有 `//` / `#` 过滤）⇒ **29 处**，
/// 绝大多数是各判据**内联**的一次性过滤（`hits` / `wake_hits` / `ci_live_lines` …），
/// 它们不是「另一份剥法」，红它们只会淹掉信号。
/// 第二轮收紧成「**返回 String / Vec 的转换器**」⇒ **14 处**，其中确实混着
/// 四个非剥法（表格解析、CI 段落抽取、host 别名解析、字段解析）——
/// 于是不猜，**逐个登记**：是剥法的写明「共享原语为什么不够」，不是的写明它在做什么。
///
/// ⚠ 登记时读出一处**真事**：`tool_registry::production_code` 用的是
/// **按 `//` 截断整行**的语义，而共享原语头注逐字写着刻意不这么做
///（会砍坏 `"http://host"` 这类字面量）。今天那个文件里没有 `://` 字面量所以没事，
/// 但那是**运气**，不是设计 —— 现在它至少被登记着。
#[test]
fn every_comment_stripping_transformer_is_registered() {
    /// `(文件::函数, 它是什么 / 共享原语为什么不够)`。
    const TRANSFORMERS: &[(&str, &str)] = &[
        ("lib.rs::strip_comment_lines", "★ **共享原语本体**（`guard_core`）"),
        ("lib.rs::production_code", "共享原语：剥注释 + 剥测试段"),
        // 08-08 删掉 `lib.rs::test_source`：它**根本不剥注释**（只是把测试段拼起来）。
        // 它当初被检出，是因为旧检测器取「函数体起点后 700 字符」的定长窗口，
        // 一路吃进了它的邻居 `production_code`（那个才剥）。⇒ **这一行是误登记**，
        // 而误登记的害处是具体的：登记表是「已知的第二份剥法」清单，
        // 混进一条不是剥法的，下一个人会照它去找一份并不存在的实现。
        (
            "e2e_gate_registry_tests.rs::strip_comments",
            "**别的注释语法**：语料是 shell 脚本（`tests/e2e/*.sh`），注释是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 语义上刻意只剥**整行注释**、不碰行尾注释（shell 里 `#` 可以出现在字符串中间，                 按 marker 截断会误伤 `pgrep` 模式里的 `#`）。要收口的正确做法是给共享原语加一个                 「注释前缀」参数，那是另一件事。",
        ),
        // 〔SH1 · V136〕`cc_bus_tests` 那份本地剥法那一行摘了：用它的零命中守卫随驾驶舱 shell 读一起退役（收口了，不是搬家）。
        // 〔MIG-3b〕钩子诊断那份本地剥法那一行摘了：它的判据随模块进后端退役（后端 `readonly_guard` 管只读，收口了，不是搬家）。
        (
            // 〔AL1c · 第四波 4B〕两种方言的读回口（同名两份 `impl`，按文件名去重成一行）。
            // 〔MIG-3a〕随方言进了那台后端（〔OSA〕今天住 `src/backend/platform/shell/dialect.rs`）。
            "dialect.rs::parse_file",
            "**别的注释语法，而且语料是我们自己生成的那份别名文件**：POSIX sh 与 PowerShell 的注释都是 `#`，\
                 共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS）。只跳**整行** `#`（生成文件的头注），\
                 行里的 `#` 是参数值的一部分、原样保留。与 `e2e_gate_registry_tests.rs::strip_comments` 同一个缺口（共享原语没有「注释前缀」参数）",
        ),
        (
            "tmux_hook_tests.rs::prod_code",
            "backend 侧本地剥法（跨 crate 够不着 monitor 的 `guard_core`）",
        ),
        (
            "registry_tests.rs::production_code",
            "⚠ **按 `//` 截断整行**——共享原语刻意不这么做（会砍坏 `\"http://host\"`）。\
                 本文件今天没有 `://` 字面量所以没事，但那是运气不是设计。登记为待收口",
        ),
        (
            "session_name_registry_tests.rs::production",
            "**多语言**剥法（`.rs` 走共享原语，`.ts`/shell 各有注释语法）——共享原语只管 Rust",
        ),
        // 〔搬树 2026-09-18 · `16 §6.2` C 类〕随测试段搬去 `tests/bridge/backend/control/`。
        ("ccm_invocation_tests.rs::refusal_variants", "不是剥法：从 `enum Refusal` 的定义里抽变体名（跳过 doc 行只是为了不把注释当变体）"),
        ("agent_profile_parity_tests.rs::rows", "不是剥法：解析对拍表的行"),
        ("gate2_parity_tests.rs::rows", "不是剥法：解析 golden 表的行"),
        ("gate_tests.rs::golden_rows", "不是剥法：backend 侧解析同一张 golden 表"),
        // 08-08 第二刀：`live_lines` 已变成一句委托（改调 `strip_hash_comment_lines`）⇒
        // 它不再是一份剥法，登记删掉。**同一天里这张表两次告诉我「你在写第二份剥法」**：
        // 一次是内联的 `#` 过滤（登记表逮的），一次是 `sftp.rs` 读 `release.yml`（变异逮的）。
        (
            "lib.rs::strip_hash_comment_lines",
            "**共享原语本体**：`strip_comment_lines` 的 YAML/shell 兄弟（`#` 整行注释）。\
                 两个都住 guard-core —— 判据要读 `.yml`/`.sh` 时借这一份，别再各写一遍",
        ),
        // 08-07：原 `ci_job_block`/`ci_yml` 搬进同文件的 `pub(crate) mod ci_yaml`
        // （E3：`ci.yml` 的读取与切块只有一个家，`lockfile_conflict_guard` 也要用）。
        // 搬家当场被本条逮住（多出 `job_block`、少了那两个）—— 这正是默认拒绝该有的样子。
        ("shared_crate_registry_ci_yaml.rs::job_block", "不是剥法：抽某个 job 的段落"),
        ("ssh_config.rs::parse_host_aliases", "不是剥法：解析 ssh config 的 Host 别名（〔MIG-1〕随导入搬进后端 `dial/`）"),
        ("registry_tests.rs::declared_fields_of", "不是剥法：解析结构体字段声明"),
        // 〔`K-R62` 09-11〕**方向恰好相反的一条**：它不剥注释，它**把注释留下来并指名**。
        // 那一格的正题是「你 rc 里这几行是旧的」——`#` 打头的行照样进结果，只是分类成
        // `LegacyRcKind::Comment`（`K-R57` 现打用户 `~/.bashrc`：14 行里 4 行是注释，
        // 那 4 行也该让用户看见）。⇒ 共享原语在这里不是「不够」，是**用了就把活做反了**。
        (
            // 〔MIG-3a〕随别名块进了那台后端（`src/backend/assets/aliases/block.rs`）。
            "block.rs::scan_legacy_rc_lines",
            "不是剥法，是**反过来**：它逐行指名 rc 里提到 `ccm` 的行（含注释行），\
                 一个字节都不删也不丢 —— 用 `strip_comment_lines` 会把该指名的那几行吃掉",
        ),
        // 〔U8c-3-r2 08-14〕**这一格是本条判据当场逮出来的**：08-04 那份剥法是**内联**的
        // （一串 `.lines().filter().map()`），本条看不见；把它抽成具名函数给两处共用时，
        // 本条立刻说「你有第二份剥法」。⇒ 收口的动作反而暴露了此前没被登记的欠账。
        (
            // 〔搬树 2026-09-18 · `16 §6.2` C 类〕随测试段搬去
            // `tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs`。
            "launch_wire_f07_main_path_tests.rs::production_ts",
            "**整行那半已经是共享原语**（本函数转调 `strip_comment_lines`），多出来的只有\
                 **行尾 `//` 截断** —— 共享原语刻意不剥行尾（会砍坏 `\"http:` + `//host\"`），\
                 而本组判据必须剥（F10 逐字：行尾注释里的提及不算数）。\
                 ⚠ 与 `tool_registry.rs` 那条的差别：**那条的安全是运气，这条的是读数** ——\
                 `the_ts_comment_stripper_actually_strips` 对四份语料逐个断言不含该字面量",
        ),
    ];

    let root = crate::guard_support::repo_root();
    let mut found: Vec<String> = Vec::new();
    // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**补上 `"tests"` 这一棵。**
    //
    // 本条头注逐字写着「私有剥法一律住在测试模块里」—— 而测试模块剖分之后整个住进了
    // `<repo>/tests/`。上一版那三棵根**一份都够不着它们** ⇒ 四份剥法
    // （`cc_bus` / `hooks_diag` 的 `non_test_code` · `ccm_invocation` 的 `refusal_variants` ·
    // `launch_wire` 的 `production_ts`）整批掉出人群，而「没有新增第二份剥法」这个结论
    // 就是在一个缺了一大块的分母上得出的。**少扫不会红，只会零命中地绿。**
    // ⚠ 四棵根**互不包含**（`src/bridge/src` · `src/bridge/crates` 是并列的两棵，
    //   `src/backend` 与 `tests` 各自独立）—— 那是 `§5.4b` 纪律 1 要的那一问。
    for sub in [
        "src/bridge/src",
        "src/bridge/crates",
        "src/backend",
        "tests",
    ] {
        for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let file = f
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let mut from = 0usize;
            while let Some(i) = raw[from..].find("fn ") {
                let at = from + i + 3;
                from = at;
                let name: String = raw[at..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if name.is_empty() {
                    continue;
                }
                let after = &raw[at + name.len()..];
                let Some(arrow) = after.find("->") else {
                    continue;
                };
                if arrow > 200 {
                    continue;
                }
                let ret = after[arrow + 2..].trim_start();
                if !(ret.starts_with("String") || ret.starts_with("Vec<")) {
                    continue;
                }
                let body_at = at + name.len() + arrow;
                // ⚠ **按 char 取，不按字节切** —— 本仓 `digit_after` 头注逐字记过这个坑
                // （中文注释里按字节 `saturating_sub` 会落在汉字中间当场 panic），
                // 而我这一版还是先写成了字节切片，跑起来立刻炸在「残」字上。
                // ⚠ **切到函数真正的结尾**，不是「起点后 700 字符」〔08-08〕：
                // 定长窗口会一路吃进**下一个函数**。实测：把 `live_lines` 插在 `yml` 后面，
                // `yml`（一句 `read_to_string` 而已）当场被判成「在剥注释」——
                // 窗口里装的是它邻居的身体。**匹配单位比事实大**，本仓治了一轮又一轮的那一族。
                // 按大括号配平切；配不平（宏里带不成对括号之类）就退回定长窗口，
                // 那时宁可**多判**——多判会被登记表逼着看一眼，少判是静默漏。
                let body: String = {
                    let rest: Vec<char> = raw[body_at..].chars().take(3000).collect();
                    let mut depth = 0i32;
                    let mut end = None;
                    for (i, c) in rest.iter().enumerate() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(i + 1);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    rest[..end.unwrap_or(rest.len().min(700))].iter().collect()
                };
                // ⚠ **先剥掉整行注释再找**〔08-08〕：本检测器找的是「这段代码在剥注释」，
                // 而它原来拿**函数体原文**去找 —— 于是**一句解释性注释里写出那个形态就会被算成实现**。
                // 实测：`e2e_gate_registry::floored` 里有一行注释写着「第一版在这里内联了一个
                // `starts_with('#')` 过滤」，本条当场把它判成第二份剥法，还建议我去登记它。
                // ⇒ 与 plan-lint 判据 7 同一个教训：**判据要看围栏，不是看围栏的说明书**。
                // 用共享原语剥（它认 `//` 那套 Rust 形态，正是这里要去掉的东西）。
                let body = guard_core::strip_comment_lines(&body);
                let body = body.as_str();
                let dq = '"';
                let strips = body.contains(&format!("starts_with({dq}//{dq})"))
                    || body.contains(&format!("find({dq}//{dq})"))
                    || body.contains("starts_with('#')")
                    || body.contains(&format!("trim_start_matches({dq}//{dq})"));
                if strips {
                    found.push(format!("{file}::{name}"));
                }
            }
        }
    }
    found.sort();
    found.dedup();
    // 抽取器自检：连共享原语本体都扫不到 ⇒ 遍历或形态坏了。
    assert!(
        found.contains(&"lib.rs::strip_comment_lines".to_string()),
        "连共享原语 `strip_comment_lines` 都没扫到 —— 抽取器坏了，下面的对拍会空绿：{found:?}"
    );
    let mut want: Vec<String> = TRANSFORMERS.iter().map(|(n, _)| (*n).to_string()).collect();
    want.sort();
    assert_eq!(
        found, want,
        "\n「返回 String/Vec 且会剥注释」的函数与登记表对不上。\n\
             **多出来的**：先问「共享原语 `guard_core::strip_comment_lines` 为什么不够」——\n\
             答得出来就登记进 `TRANSFORMERS` 并写明理由；答不出来就改成调它。\n\
             ⚠ 名字叫什么**不是判据**：换个名字的同一份剥法仍然是第二份剥法。\n\
             **少了的**：它被收口了 ⇒ 把登记删掉（登记表腐烂比没有登记更糟）。"
    );
}

#[test]
fn comment_stripping_has_exactly_one_shared_implementation() {
    const REGISTERED: &[(&str, &str)] = &[
        (
            // 〔搬树 2026-09-18〕`profile_installer.rs` → 它那条握手文档守卫搬去了
            // `tests/bridge/profile_installer_handshake_doc_guard.rs`，剥法随判据一起走。
            "profile_installer_handshake_doc_guard.rs",
            "按 marker 截断整行（能吃行尾注释），语料是自己生成的 shell/rc 片段、无 `://` 字面量风险",
        ),
        (
            // 〔步 7c 剖分 2026-09-19 · C 类〕住址跟着那份私有剥法搬进 `tests/bridge/`。
            "e2e_gate_registry_tests.rs",
            "**别的注释语法**：语料是 shell 脚本（`tests/e2e/*.sh`），注释前缀是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 刻意只剥**整行**：shell 里 `#` 会出现在字符串中间（本处语料就有 `pgrep` 模式），                 按 marker 截断会误伤。收口的正确做法是给共享原语加一个「注释前缀」参数，那是另一件事。",
        ),
    ];

    // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**两棵树：生产段 ＋ monitor 的测试段。**
    //
    // 同上一条的理由：本条头注逐字「私有剥法一律住在测试模块里」，而测试模块搬去了
    // `<repo>/tests/bridge/`。只扫 `src/bridge/src` 的话连**已登记的那一份**都扫不到 ——
    // 底下那条抽取器自检按设计响了（它正是为这一形而写的）。
    let mut found: Vec<String> = Vec::new();
    for root in [
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        crate::guard_support::tests_root().join("bridge"),
    ] {
        for (f, raw) in guard_core::scan_tree!(&root, &["rs"]) {
            // 只看生产段之外也一样：私有剥法一律住在测试模块里，所以扫整份。
            for l in raw.lines() {
                let t = l.trim_start();
                if t.starts_with("fn strip_line_comments")
                    || t.starts_with("fn strip_comments")
                    || t.starts_with("fn without_comments")
                {
                    found.push(
                        f.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string(),
                    );
                }
            }
        }
    }
    found.sort();
    found.dedup();

    // 抽取器自检：连登记在册的那一个都扫不到 ⇒ 遍历或形态坏了，下面的对拍会空绿。
    // 🔴 〔搬树 2026-09-18 · `设计/16 §6.2` C 类〕点名的住址跟着搬：那份私有剥法
    //    （`profile_installer` 握手文档那条守卫里的 `fn strip_comments`）今天住
    //    `tests/bridge/profile_installer_handshake_doc_guard.rs`。**剥法本身一个字没改。**
    assert!(
        found.contains(&"profile_installer_handshake_doc_guard.rs".to_string()),
        "连 `profile_installer_handshake_doc_guard.rs` 里那个已登记的实现都没扫到 —— \n\
             遍历或形态坏了，那样「没有新增」这个结论是零命中得来的，不是真的。实得：{found:?}"
    );

    let extra: Vec<&String> = found
        .iter()
        .filter(|f| !REGISTERED.iter().any(|(r, _)| *r == f.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "又出现了私有的剥注释实现：{extra:?}\n\
             ★ 先用 `guard_core::strip_comment_lines` —— 08-06 已把三份重复迁过去。\n\
             它**确实不够**时才加进本表，并写清是哪种语义上的不够（行尾注释？保行号？\n\
             别的注释语法？）。⚠ 只写文件名不算理由。\n\
             已登记：{REGISTERED:?}"
    );
}

/// tmux `-t` 目标的性质：**紧跟的那个 token 里**先出现 `=` 再出现 `:`。
/// （T01 审计 S2：看整个窗口时，同一行的 `A=b:c` 诱饵能让裸目标零违规。）
fn exact_target(win: &str) -> Result<(), String> {
    let eq = win.find('=');
    let colon = win.find(':');
    if eq.is_some() && colon.is_some() && eq < colon {
        Ok(())
    } else {
        Err("tmux 目标必须是 `=名:` 精确形态".to_string())
    }
}

/// **用已知的绕过手法验证**（本轮新纪律）。这不是构造的边角——
/// 裸目标正是 F01 修掉的那次「杀错/打错兄弟会话」生产事故，
/// 而 F04 的 D 审计实测过：固定 needle 版本对它**完全空转**。
#[test]
fn catches_the_known_bare_target_bypass() {
    let bad = "tmux send-keys -t \"$name\" x\ntmux kill-session -t $name\n";
    let r = scan_after_marker(bad, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.checked, 2);
    assert_eq!(r.violations.len(), 2, "两处裸目标都必须被抓");
    assert!(r.require(2, "tmux 目标").is_err());
}

#[test]
fn accepts_the_three_legit_exact_forms() {
    let good = "a -t $(sq \"=$x:\") b\nc -t \"=$x:\" d\ne -t '=x:' f\n";
    let r = scan_after_marker(good, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.checked, 3);
    assert!(r.violations.is_empty(), "实得 {:?}", r.violations);
    assert!(r.require(3, "tmux 目标").is_ok());
}

/// **要件 3 内建**：扫到 0 处必须红，且措辞要指向"扫描器失效"而非"代码变干净了"。
/// 本会话我写坏的四条守卫里，"恒绿/空转"占了两条——所以这一条不能是可选的。
#[test]
fn zero_matches_fails_and_says_scanner_may_be_broken() {
    let r = scan_after_marker(
        "毫无关系的文本\n",
        "-t ",
        Some("#"),
        48,
        &|_| false,
        &exact_target,
    );
    assert_eq!(r.checked, 0);
    assert!(r.violations.is_empty(), "没扫到东西不等于有违规");
    let e = r.require(1, "tmux 目标").unwrap_err();
    assert!(e.contains("扫描器可能失效"), "措辞要指向扫描器，实得: {e}");
    assert!(e.contains("别急着调低阈值"));
}

#[test]
fn comment_lines_are_skipped() {
    let t = "# 示例：tmux -t $name\ntmux -t \"=a:\" x\n";
    let r = scan_after_marker(t, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.checked, 1, "注释行里的用法示例不算");
    assert!(r.violations.is_empty());
}

/// **要件 4**：放行的逃生口计入 checked（否则计数自检会被它稀释），但不施加性质。
#[test]
fn allowed_indirection_counts_but_is_not_checked() {
    let t = "tmux send-keys -t $t x\ntmux kill -t $name y\n";
    let allow = |rest: &str| first_token(rest) == "$t";
    let r = scan_after_marker(t, "-t", Some("#"), 48, &allow, &exact_target);
    assert_eq!(r.checked, 2, "逃生口也要计数");
    assert_eq!(r.violations.len(), 1, "只有裸目标那处违规");
}

/// **要件 4 的另一半**：钉死逃生口的定义。不钉的话 `$t` 能被改成裸值、
/// 扫描照样全绿而防线已经没了。
#[test]
fn pinning_the_escape_hatch_definition() {
    let def = r#"t="$(sq "=$tmux_name:")""#;
    let ok = format!("x\n{def}\ny\n");
    assert!(pin_definition(&ok, def, "t=", "$t").is_ok());
    // 被改成裸值 → 必须红
    let tampered = "x\nt=\"$tmux_name\"\ny\n";
    let e = pin_definition(tampered, def, "t=", "$t").unwrap_err();
    assert!(e.contains("绕过整个结构性扫描"));
}

// ===== T01 审计报的三个绕过，逐条钉死（用它给的手法验证）=====

/// **S1**：`-t$name` 紧贴形态。marker 写成 `"-t "`（带空格）时它完全不进枚举
/// ——审计在真实 `shared/ccm` 上实测过：checked 11→10、violations 空、require 照样通过。
#[test]
fn adjacent_form_is_enumerated_too() {
    let bad = "tmux attach -t$name\n";
    let r = scan_after_marker(bad, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.checked, 1, "紧贴形态必须进枚举");
    assert_eq!(r.violations.len(), 1, "且必须被判违规");
}

/// 但**不能把 `-tmux`/`-timeout` 这类更长的选项名误当成 `-t` 带值**。
#[test]
fn longer_option_names_are_not_false_positives() {
    let t = "cmd -tmux-size 220x50\ncmd -timeout 5\n";
    let r = scan_after_marker(t, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.checked, 0, "-tmux/-timeout 不是 -t 带值，实得 {r:?}");
}

/// **S2**：同一行的诱饵。谓词只看紧跟的 token，不看整个窗口。
#[test]
fn same_line_decoy_cannot_fool_the_predicate() {
    let bad = "tmux send-keys -t $name \"export A=b:c\"\n";
    let r = scan_after_marker(bad, "-t", Some("#"), 48, &|_| false, &exact_target);
    assert_eq!(r.violations.len(), 1, "裸目标必须被抓，诱饵 A=b:c 不算");
}

/// **S3**：定义两次、后者生效。`contains` 通不过这一关。
#[test]
fn pin_definition_rejects_second_assignment() {
    let def = r#"t="$(sq "=$tmux_name:")""#;
    let two = format!("x\n{def}\nt=\"$tmux_name\"\ny\n");
    let e = pin_definition(&two, def, "t=", "$t").unwrap_err();
    assert!(e.contains("被赋值 2 次"), "实得: {e}");
    assert!(e.contains("钉死第一处等于没钉"));
    // 注释里的赋值不算
    let with_comment = format!("x\n{def}\n# t=\"$bare\"\n");
    assert!(pin_definition(&with_comment, def, "t=", "$t").is_ok());
}

/// **I3**：`min_checked = 0` 等于把要件 3 关掉。
#[test]
fn min_checked_zero_is_rejected() {
    let r = ScanReport {
        checked: 0,
        violations: vec![],
    };
    let e = r.require(0, "某扫描").unwrap_err();
    assert!(e.contains("不得为 0"), "实得: {e}");
}

/// `first_token` 的边界：到空白 / `;` / `|` / `&` / `)` 为止。
/// `first_token` 必须取出 shell 意义上的**一个参数**——`$(…)` 与引号区内部的空格
/// 不是分隔符。第一版按空格硬切，把合法的 `$(sq "=$x:")` 截成 `$(sq` 而误判违规。
#[test]
fn first_token_takes_one_shell_argument() {
    assert_eq!(first_token(" $t ;rm -rf /"), "$t");
    assert_eq!(first_token("$t;kill"), "$t");
    assert_eq!(first_token("\"=a:\" x"), "\"=a:\"");
    assert_eq!(first_token("'=a:' x"), "'=a:'");
    // 关键：命令替换整体算一个参数
    assert_eq!(first_token(" $(sq \"=$x:\") b"), "$(sq \"=$x:\")");
    assert_eq!(first_token("$bare b"), "$bare");
    // 尾部引号是外层赋值的闭合引号，不属于目标（真实形态 `seq="… -t $t"`）
    assert_eq!(first_token(" $t\""), "$t");
    assert_eq!(first_token("$bare'"), "$bare");
    assert_eq!(first_token(""), "");
    // 不配对时整段交给谓词（它会因缺 = / : 报违规，而不是静默放过）
    assert_eq!(first_token("$(unclosed"), "$(unclosed");
}

/// **S6**：`allow` 的语义要对称——`$t` 就是 `$t`，不论后面跟什么。
/// 旧的 `starts_with("$t ")` 让 `-t $t ;rm -rf /` 被放行、而行尾 `-t $t` 反而判红。
#[test]
fn allow_is_symmetric_on_the_token() {
    let t = "a -t $t\nb -t $t;kill\nc -t $t \"x\"\nd -t $bare\n";
    let allow = |rest: &str| first_token(rest) == "$t";
    let r = scan_after_marker(t, "-t", Some("#"), 48, &allow, &exact_target);
    assert_eq!(r.checked, 4);
    assert_eq!(
        r.violations.len(),
        1,
        "只有 $bare 那处违规，实得 {:?}",
        r.violations
    );
}

#[test]
fn violations_report_line_numbers_and_window() {
    let t = "行一\ntmux -t $bare x\n";
    let r = scan_after_marker(t, "-t", Some("#"), 12, &|_| false, &exact_target);
    let e = r.require(1, "tmux 目标").unwrap_err();
    assert!(e.contains("第 2 行"), "要报行号，实得 {e}");
    assert!(e.contains("$bare"), "要报窗口内容");
    assert!(e.contains("共检查 1 处"));
}

/// 〔audit-0805 08-06〕**拼命令用的 shell 元字符黑名单，权威源恰好一处**（E3）。
///
/// # 它不是理论风险 —— 同一族已经漂过一次
///
/// `backend/control/payload.rs` 的头注逐字记着：U7-3 把**不可见字符表**收进 `acct-core`
/// 让两个读 manifest 的地方共用，**而「拼命令」那条路当时没跟上** ——
/// `history.rs` 一直用自己那张 U7-3 之前的旧表，缺 `U+1680` · `U+2000..200A` ·
/// `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，是一处**纵深防御缺口**。
///
/// 08-06 顺着「只被一处调用的生产函数」这条先验查到：**元字符表也是两份逐字副本**
/// （`history.rs` 与 `payload.rs`），而**没有任何东西对拍它们**。
/// ⇒ 按 E3 收成一处：`history.rs` 那份删掉、改为派生 `payload::is_command_unsafe_char`；
/// 本条钉住「以后也只有一处」。
///
/// ⚠ E3 逐字要求「判据钉的是**权威源恰好一个**，不是『有没有登记』」——
/// 所以这里数的是**定义处数**，不是「两处内容一不一样」。
/// 后者在两份都改错时照样绿，前者不会。
#[test]
fn the_shell_metachar_blacklist_has_exactly_one_home() {
    const NEEDLE: &str = "const SHELL_META_COMMON";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut homes: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    // 〔MIG-2〕载荷内核（那张表的家）搬进了后端 ⇒ 人群 = monitor 与后端两棵生产树。
    let trees = [root.join("src"), root.join("../backend")];
    for (path, src) in trees
        .iter()
        .flat_map(|t| guard_core::scan_tree_excluding(t, &["rs"], &[]))
    {
        scanned += 1;
        let prod = guard_core::production_code(&src);
        if prod.lines().any(|l| {
            l.trim_start().starts_with(NEEDLE)
                || l.trim_start().starts_with(&format!("pub(crate) {NEEDLE}"))
                || l.trim_start().starts_with(&format!("pub {NEEDLE}"))
        }) {
            homes.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    // ★ 抽取器自检：遍历坏了会让下面「恰好一处」变成「恰好零处」也叫不出来。
    // ⚠ 地板是**实测**的：`src/bridge/src` 下 86 个 `.rs` ——〔`P4` 2026-09-21〕先前这里
    //   还写着「`scan_tree!` 摘除调用者自己 ⇒ 85」，那一刀在本仓**不生效**，而且本文件
    //   住 `tests/bridge/`、不在这棵树里 ⇒ 盘上几份就是几份。
    //   第一版我拍了个 100 —— 判据一建就红。**拍出来的数与抄来的数一样会腐**，
    //   本会话已在别处记过多次，这次犯在自己刚写的自检上。
    assert!(
        scanned >= 80,
        "只扫到 {scanned} 个 .rs —— 遍历坏了，下面那条会零命中地绿（建判据当日实测 85）"
    );
    assert_eq!(
        homes.len(),
        1,
        "拼命令用的元字符黑名单**不是恰好一处**，实得：{homes:?}\n\n\
             ⚠ 同一族已经漂过一次：`payload.rs` 头注记着，`history.rs` 那张**不可见字符表**\n\
             曾停在 U7-3 之前的旧版本，缺六段 Unicode —— 一处纵深防御缺口。\n\
             ⇒ 要新增消费者就**派生**（`payload::is_command_unsafe_char`），别再抄一份表。\n\
             E3：判据钉的是「权威源恰好一个」，不是「两份内容一不一样」——\n\
             后者在两份都改错时照样绿。"
    );
    // ⚠ `homes[0]` 是 `strip_prefix(root)` 的产物 ⇒ **分隔符随平台**。
    //   09-09 云端首跑（windows-latest）实得 `"src\\backend\\control\\payload.rs"`，
    //   而针写的是正斜杠 ⇒ `ends_with` **恒 false**，本条在 Windows 上必红。
    //   同一族今天已经逮到两条：覆盖率地板脚本的键写死正斜杠 · `guard-core` 的
    //   `scan_tree!` 摘除 —— 三条都是**草垛归一了、针没归一**。⇒ 先归一分隔符再比。
    //   ⚠ 顺手把 `ends_with` 收成**整条相对路径逐字相等**：原写法对
    //   `src/x/backend/control/payload.rs` 也放行 —— 只严不松，不是换个写法。
    let home = homes[0].replace('\\', "/");
    assert!(
        // 〔MIG-2〕载荷内核搬进后端 ⇒ 路径相对 monitor 的 manifest 目录。
        home == "../backend/control/launch_render/payload.rs",
        "权威源搬家了（现在在 {:?}）—— 搬可以，但请顺手把本条与两处头注的指向一起改。",
        homes[0]
    );
}
/// ★★ **位置比较型判据的三种坏法，做成一条常驻元判据**〔audit-0805 08-08，Phase G 第 81 件〕。
///
/// 08-08 透镜五横扫全仓「比源码位置」的顺序断言，**四条老的里两条是洞**，
/// 一般化出三种坏法，每一种都有活样本：
///
/// | 坏法 | 活样本 | 后果 |
/// |---|---|---|
/// | ① 比的是**注释**不是代码 | `watcher.rs` 拿 `// --- Phase 1 …` 当扫描锚点 | 真扫描搬到注入之前、注释不动 ⇒ 判据全绿，而启动时活着的会话一个 pidfd 看守都没有 |
/// | ② 比的是**任意一处**不是**那一处** | `local_backend.rs` 的 `rfind(".stop()")` · `kill.rs` 的裸 `kill-session`（生产段两处） | 退出臂里删掉 `.stop()`、别处留一处 ⇒ 全绿，backend 变游魂进程 |
/// | ③ **文本顺序 ≠ 执行顺序** | `inbound.rs` 把 `remove` 搬进新 task | 文本上仍在前面，实际什么时候跑没人保证 |
///
/// ⇒ 本条把①②做成机检（③ 没有可靠的文本特征，留在各判据自己的反向自检里）：
/// 语料必须过 `production_code`（不许比注释）· 不许 `rfind`（那是「任意一处」）·
/// 锚点必须被**界定**（切一段 `arm_of`，或当场核一次唯一性）。
///
/// ⚠ **人群刻意收窄到「语料是本仓 Rust 源码」**：全仓还有 4 条位置比较判据比的是
/// **生成的命令串 / PowerShell 模板 / 文档**（`account_usage` 两条 · `profile_installer` 两条）——
/// 对它们来说「过 `production_code`」根本不成立。先量误红面再定人群，
/// 这是本工作区反复吃亏的地方（人群取宽 ⇒ 逼人往豁免表里塞条目 ⇒ 判据变废纸）。
#[test]
fn every_position_comparison_over_source_pins_and_bounds_its_anchors() {
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` B 类 ＋ `§5.4b` 纪律 3、4〕
    //    **人群补上两棵测试树。**
    //
    // 本条认的是「比源码位置的**判据**」，而判据整批住进了 `tests/`。
    // 只给两棵生产树时识别数从 6 掉到 **0**，自检逐字报「识别口径坏了，本条会零命中地绿」
    // —— 红得对，而它红的正是「一整棵树掉出扫描面」。
    // ⚠ A 类同治：改用 `scan_tree_excluding` 的明写名单。本文件住 `tests/bridge/`，
    //   现在在人群里，而它头注里就带着这一族的示例文本 ⇒ 必须明写摘掉，摘不到就 panic。
    let root = crate::guard_support::repo_root().to_path_buf();
    let mut files = Vec::new();
    for (sub, excluded) in [
        ("src/bridge/src", &[] as &[&str]),
        ("src/backend", &[]),
        ("tests/bridge", &["structural_scan_tests.rs"]),
        ("tests/backend", &[]),
    ] {
        files.extend(guard_core::scan_tree_excluding(
            &root.join(sub),
            &["rs"],
            excluded,
        ));
    }

    let mut population = 0usize;
    let mut bad: Vec<String> = Vec::new();
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §5.4b` 第二条元教训〕
    //    **切函数体的锚点原来是 `"\n    fn "` —— 针里嵌着 4 个空格，而缩进是位置。**
    //
    // 那 4 个空格当年是**判别式**：它同时表示「这是个函数」与「它住在 `mod tests {}` 里」。
    // 剖分之后判据整批退了一层缩进、落在列 0 ⇒ 那根针只认得**嵌套的辅助函数**，
    // 识别数从 6 掉到 3（补完人群之后的读数；只给生产树时是 0）。
    // ⇒ 锚点改成**按行认 `fn`、与缩进无关**，而「是不是判据」这一维改由**住址**答：
    //   住 `tests/` ⇒ 里面的函数都是判据；住 `src/` ⇒ 仍按 4 缩进认
    //   （剖分之后那一支应当采不到东西，留着是为了「有人把判据写回生产段」那天还看得见）。
    let fn_bodies = |src: &str, in_tests: bool| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur: Option<Vec<String>> = None;
        for l in src.lines() {
            let t = l.trim_start();
            let indent = l.len() - t.len();
            let head = t
                .strip_prefix("pub(crate) ")
                .or_else(|| t.strip_prefix("pub(super) "))
                .or_else(|| t.strip_prefix("pub "))
                .unwrap_or(t);
            let head = head.strip_prefix("async ").unwrap_or(head);
            // 针运行期拼（同上：`.starts_with(` 是 0 上限的递减棘轮）。
            let fn_kw = format!("{} ", "fn");
            let is_fn = head.starts_with(fn_kw.as_str()) && (in_tests || indent == 4);
            if is_fn {
                if let Some(b) = cur.take() {
                    out.push(b.join("\n"));
                }
                // 契约与原来那个 `split("\n    fn ")` 一致：块从 `fn ` **之后**起头，
                // 好让下面 `body.split('(').next()` 抠得到函数名。
                cur = Some(vec![head["fn ".len()..].to_string()]);
            } else if let Some(b) = cur.as_mut() {
                b.push(l.to_string());
            }
        }
        if let Some(b) = cur {
            out.push(b.join("\n"));
        }
        out
    };
    for (path, src) in &files {
        let name = path.to_string_lossy().replace('\\', "/");
        for body in fn_bodies(src, name.contains("/tests/")) {
            let body = body.as_str();
            let fname = body.split('(').next().unwrap_or("").trim();
            let finds = body.matches(".find(").count() + body.matches(".rfind(").count();
            if finds < 2 {
                continue;
            }
            // 比位置：`x < y` 这种形状（`_at` 命名或 assert! 里直接比两个局部）。
            let compares = body.contains("_at < ")
                || body
                    .lines()
                    .any(|l| l.trim().starts_with("assert!(") && l.contains(" < "))
                || body.lines().any(|l| {
                    let t = l.trim();
                    t.ends_with(" < types,") || t.ends_with(" < scan_at,") || t.ends_with(" < act,")
                });
            if !compares {
                continue;
            }
            // 语料是不是本仓 Rust 源码（否则「过 production_code」这条要求不成立）。
            let rust_corpus = body.contains("production_code(") || body.contains(".rs\")");
            if !rust_corpus {
                continue;
            }
            population += 1;
            let mut why = Vec::new();
            if !body.contains("production_code(") {
                why.push("语料没过 `production_code` ⇒ 它在比**注释**的位置（坏法①）");
            }
            if body.contains("rfind(") {
                why.push("用了 `rfind` ⇒ 比的是**任意一处**，不是**那一处**（坏法②）");
            }
            // `find_pinned` = **恰好一处 + 两侧有边界**（`guard_core`）。它比这里原有的两种
            // 界定法都强：`arm_of` 只切段（段内仍可能有第二处），`matches().count()` 只核数量
            // 而不管边界。⇒ 认它〔08-11，P2s 翻面那条判据用的就是它〕。
            // ⚠ 补它不是放宽：不认的话，用更强原语的判据反而被判不合格，
            // 那会把人推回 `matches().count()` —— 而那条又踩 `needle_anchor_registry` 的棘轮。
            let bounded = body.contains("arm_of(")
                || body.contains("find_pinned(")
                || (body.contains("matches(") && body.contains(".count()"));
            if !bounded {
                why.push(
                    "锚点没被界定 ⇒ 没切段（`arm_of`）也没核唯一性，\
                         第二处同名字面量出现时它会比到别处去（坏法②的另一半）",
                );
            }
            if !why.is_empty() {
                bad.push(format!(
                    "  {name}::{fname}\n      - {}",
                    why.join("\n      - ")
                ));
            }
        }
    }
    // 抽取器自检：人群塌了的话下面那条就是一句废话。
    assert!(
        population >= 5,
        "只识别出 {population} 条「比源码位置」的判据（08-08 实测 6）—— \
             识别口径坏了，本条会零命中地绿"
    );
    assert!(
        bad.is_empty(),
        "这些位置比较型判据没守住三条纪律：\n{}\n\n\
             ★ 三种坏法各有活样本（08-08 实测，逐条写在本条头注的表里）：\n\
             ① 比注释（`watcher.rs` 曾拿一行 `// --- Phase 1 …` 当扫描锚点）；\n\
             ② 比任意一处（`local_backend.rs` 的 `rfind(\".stop()\")`；`kill.rs` 的裸 `kill-session` \n\
                在生产段有两处，命中对的那处**是排序运气**）；\n\
             ③ 文本顺序 ≠ 执行顺序（`inbound.rs` 把 `remove` 搬进新 task 就绕过去了）。\n\
             ⇒ 修法：语料先过 `production_code`；别用 `rfind`；\n\
             锚点要么切一段（`arm_of`）、要么当场核一次唯一性（`matches(..).count() == 1`）。",
        bad.join("\n")
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R17`：源码里的地址这一族，三条判据
// ═══════════════════════════════════════════════════════════════════════

/// 仓根。
///
/// ⚠ **只跳一级**（`src/bridge` 的上级就是仓根，这是 cargo 给的事实，不是猜的）。
/// 跳两级那种写法是 `K-R13` 刚治过的病：主树上算出来的东西恰好存在、工作树上
/// 算错了却被人补了一个假落点 ⇒ 判据变绿而它证明的事根本不成立。
/// 形状与 `frame_cadence_guard.rs` 里那条同源（本仓已有先例）。
fn addr_repo_root() -> std::path::PathBuf {
    crate::guard_support::repo_root().to_path_buf()
}

/// 地址判据的**语料面**：四个根下的全部 `.rs` + 本文件自己。
///
/// **为什么本文件自己也要进来**：判据自己写的地址也得有东西守着 ——
/// 同族的 `doc/` 判据 08-06 正是在这一格上栽过一次，它的订正逐字：
/// 「把『已知的例外』变成『已修的缺陷』」。⇒ 这里一开始就把自己收进来。
///
/// ⚠ 〔`P4` 2026-09-21〕先前这一段把成因写成「`scan_tree!` 按构造摘除调用者（那是
/// 「判据读到自己」的防护），但摘掉之后本文件里写的地址就没有任何东西守着了」——
/// 那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比
/// 恒不命中），而下面那四棵根里逐字含 `"tests"` ⇒ **本文件本来就在语料里**。
/// ⇒ 末尾那句 `structural_scan.rs` 的 `push` 收的是**生产侧**那一份，而它也已经被
/// `"src/bridge/src"` 那棵收过一次 —— 今天是冗余的第二份（地址判据只看「有没有」，
/// 重复无害），**刻意不删**：它把那一份钉成不依赖根清单的事。
/// 本文件的头注为了讲形状会写地址样例，样例一律用**中文文件名**（抽取器只收 ASCII 路径），
/// 因此不会把样例算进人群。
fn addr_corpus() -> Vec<(std::path::PathBuf, String)> {
    let root = addr_repo_root();
    let mut out: Vec<(std::path::PathBuf, String)> = Vec::new();
    // 🔴 〔搬树 2026-09-18 补 `"tests"`〕19 份纯测试文件从后端树搬到了
    // `<repo>/tests/backend/` ⇒ 原来那三棵树**一份也够不着它们**，
    // 而下面那些「符号地址 / 行号地址 / 死名」判据的人群就少了那一块 ——
    // **少扫不会红，只会零命中地绿**。
    for sub in [
        "src/bridge/src",
        "src/bridge/crates",
        "src/backend",
        "tests",
    ] {
        out.extend(guard_core::scan_tree!(&root.join(sub), &["rs"]));
    }
    let br = root.join("src/bridge/build.rs");
    let br_src = std::fs::read_to_string(&br).expect("读不到 src/bridge/build.rs");
    out.push((br, br_src));
    out.push((
        std::path::PathBuf::from("structural_scan.rs"),
        include_str!("../../src/bridge/src/structural_scan.rs").to_string(),
    ));
    // ★ 抽取器自检：语料面塌了 ⇒ 下面三条一起零命中地绿。
    assert!(
        out.len() >= 180,
        "地址判据只收到 {} 份源文件 —— 语料面坏了（09-02 现打 187 份：\
             四个根下 186 + build.rs 1，与 `git ls-files` 的分母逐份对上）",
        out.len()
    );
    out
}

/// 文件基名。
fn addr_base(p: &std::path::Path) -> String {
    p.file_name()
        .expect("源文件名")
        .to_string_lossy()
        .to_string()
}

/// 全仓声明：符号名 → 它出现在哪些**文件基名**里。
///
/// 口径逐字取自本仓已有的同族判据
/// `doc_claim_registry_tests.rs::every_code_symbol_named_in_the_docs_still_resolves`：
/// 「文档写 `a·rs::foo`，就要求 `foo` 的声明**出现在 `a·rs` 里**」——
/// 比「符号存在」严一档，因为**搬家**恰恰是本仓重构的常见形态。
/// 注释里的 `fn foo` 不算声明（先过 `strip_comment_lines`），否则「注掉一个函数」
/// 这种变异会被放过。
fn addr_declarations(
    corpus: &[(std::path::PathBuf, String)],
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    const KW: &[&str] = &[
        "fn", "struct", "enum", "const", "static", "trait", "mod", "type",
    ];
    let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (p, raw) in corpus {
        let fname = addr_base(p);
        for line in guard_core::strip_comment_lines(raw).lines() {
            let mut it = line.split_whitespace().peekable();
            while let Some(tok) = it.next() {
                if !KW.contains(&tok) {
                    continue;
                }
                let Some(next) = it.peek() else { continue };
                let ident: String = next
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !ident.is_empty() {
                    decl.entry(ident).or_default().insert(fname.clone());
                }
            }
        }
    }
    decl
}

/// ★★ **源码里点名的代码符号必须解析得到，且住在它说的那个文件里。**
///
/// # 它买的是什么
///
/// 08-25 的处方是「别点行号，点函数名」。**处方本身没有任何判据守着** ——
/// 09-02 现打：源码里 `文件·rs::符号` 这种地址有两百多处，
/// **一处都没有人对过账**，而「改名 / 删除 / 搬家」三种动作都让它们变假。
/// ⇒ 没有本条，处方只是一句劝告：换成符号地址之后**照样会烂，照样没人响**。
///
/// # 它买不到什么（如实写明，别把射程写宽了一格）
///
/// 它判的是**符号在不在那个文件里**，**不判**「那句话说的还是不是这个符号的事」。
/// 后者要读语义，机器读不了。⇒ 一个符号地址指着一个还存在、但语义已经变了的函数，
/// 本条**一声不吭**。这是本族**判不了**的那一半，不假装它覆盖了。
#[test]
fn every_symbol_address_in_the_sources_still_resolves() {
    /// 例外表：**每条都写清「为什么它解析不到却是对的」**。
    /// 下面有一条保鲜自检把「已经不需要的例外」揪出来 —— 例外表自己也会腐。
    const EXCEPTIONS: &[(&str, &str)] = &[
        (
            "Hello",
            "它是 `Frame` 的**枚举变体**，不是一处 item 声明；本口径只认 \
                 `fn/struct/enum/const/static/trait/mod/type` 的声明行",
        ),
        (
            "resolve_claude_dir",
            "**历史句**：那句话逐字写的就是「从它**原样搬来**」——今天它已经改名并搬进 \
                 agent 适配层。删掉这个地址反而丢掉「这段逻辑是从哪儿来的」",
        ),
        (
            "setup",
            "tauri 的 `.setup(move |app| …)` 钩子闭包 —— 是真东西，但不是一处声明。\
                 （同一条例外逐字住在 `doc/` 那条同族判据里，两处口径一致）",
        ),
        // 下面两条是**示例占位符**：`doc_claim_registry.rs` 的头注要讲「地址长什么样」，
        // 于是逐字写了一个假地址。同族的 `doc/` 判据也有一条同形的例外
        // （`CONTRIBUTING.md` 里教人「照这样加一行」的那个占位符），口径一致。
        // ⚠ 例外**按符号名**认 ⇒ 它们同时也遮住了真名叫 `symbol` / `foo` 的符号。
        //   今天全仓这两个名字**一处声明都没有**（现打），所以遮不住任何真东西；
        //   哪天真有人这么命名，上面那条保鲜自检会让这条例外**变成假绿**，如实登记。
        (
            "symbol",
            "**示例占位符**：讲「地址长什么样」时写的假地址，本就不指向真符号",
        ),
        ("foo", "**示例占位符**：同上，讲口径时举的例子"),
        (
            "discover_profiles",
            "〔AL1d · 第四波 4B〕**历史句**：`tool_registry.rs` 里 `posix-rc-aliases` 那块墓碑**逐字引用**当年那一行 \
                 `why:`（「原文逐字：…」），那一行点的就是 `profile_installer.rs` 里这个函数。它今天删了 —— \
                 `$PROFILE` 在哪只剩 `shell_dialect.rs` 一处答（`调研/第四波记录/AL1d.md §2.3`）。\
                 改写那段引文等于篡改原文（同上面 `resolve_claude_dir` 那条）。",
        ),
        (
            "build_usage_probe_cmd",
            "★〔`K-R104` 09-13〕**历史句**：`payload.rs` 与 `doc_claim_registry.rs` 里\
                 那几句逐字讲的就是「用量探针那条外层 tmux 串**已经退役**」——\
                 编排搬上后端帧面之后 monitor 一个 shell 字符都不渲染。\
                 删掉这个地址反而丢掉「外层四个产出方里退役了哪一个」这条线索\
                 （同上面 `resolve_claude_dir` 那条）。\
                 ⚠ 它不是无人看管：`doc_claim_registry::the_outer_layer_producers_are_in_the_state_the_doc_claims` \
                 把那一格翻面钉着（这个函数要是回来了，那条会红）。",
        ),
    ];

    let corpus = addr_corpus();
    let decl = addr_declarations(&corpus);
    // ★ 抽取器自检 1：收不到足够多的声明 ⇒ 遍历或剥法坏了。
    assert!(
        decl.len() > 2000,
        "全仓只抽到 {} 个声明符号 —— 遍历或剥法坏了（09-02 现打 4411 个 / 187 份源文件）",
        decl.len()
    );

    let mut refs: Vec<(String, usize, String, String, bool)> = Vec::new();
    for (p, raw) in &corpus {
        let fname = addr_base(p);
        for (ln, base, sym, prefix) in symbol_addresses(raw) {
            refs.push((fname.clone(), ln, base, sym, prefix));
        }
    }
    // ★ 抽取器自检 2：这一族本来就有两百来处 —— 抽到个位数就是剥法坏了。
    assert!(
        refs.len() >= 150,
        "只抽到 {} 处符号地址 —— 剥法坏了（09-02 现打 213 处，量于本件尖）",
        refs.len()
    );

    let resolves = |base: &String, sym: &String, prefix: bool| -> Option<Vec<String>> {
        if prefix {
            // 前缀形（通配 / 行折）：那个文件里有**某个**以它打头的声明就算数。
            let any = decl
                .iter()
                .any(|(k, fs)| k.starts_with(sym.as_str()) && fs.contains(base));
            return if any { None } else { Some(Vec::new()) };
        }
        match decl.get(sym) {
            None => Some(Vec::new()),
            Some(fs) if !fs.contains(base) => Some(fs.iter().cloned().collect()),
            _ => None,
        }
    };

    // ★ 自检 3：例外表保鲜。例外是**欠账**，不是免检章。
    for (sym, why) in EXCEPTIONS {
        let used: Vec<&(String, usize, String, String, bool)> =
            refs.iter().filter(|r| r.3 == *sym).collect();
        assert!(
            !used.is_empty(),
            "例外表里的 `{sym}` 在源码里已经没人写了 —— 删掉这一行。（它当初的理由：{why}）"
        );
        assert!(
            used.iter().any(|r| resolves(&r.2, &r.3, r.4).is_some()),
            "例外 `{sym}` 现在**解析得到了** —— 删掉这条例外，\
                 别让例外表替真判据挡枪。（它当初的理由：{why}）"
        );
    }

    let bad: Vec<String> = refs
        .iter()
        .filter(|r| !EXCEPTIONS.iter().any(|(s, _)| *s == r.3))
        .filter_map(|(f, ln, base, sym, prefix)| {
            resolves(base, sym, *prefix).map(|homes| {
                if homes.is_empty() {
                    format!("  {f}:{ln}  指 {base} 里的 `{sym}` —— **全仓找不到这个符号**（改名或删了）")
                } else {
                    format!("  {f}:{ln}  指 {base} 里的 `{sym}` —— 符号还在，但**搬家了**：现住 {homes:?}")
                }
            })
        })
        .collect();
    assert!(
        bad.is_empty(),
        "源码里点名了这些代码符号，而它们今天对不上：\n{}\n\n\
             ⚠ 这是**停滞式腐坏**：改代码的人不会回来改注释，而在本条之前\
             **源码这一侧没有任何东西会因此变红**（`doc/` 那一侧 08-06 就有人守了）。\n\
             两条修法：① 把地址改对（点今天真的那个符号）；\
             ② 那句话若只是历史，就写清「已删 / 已改名」并进本条的例外表，**带理由**。",
        bad.join("\n")
    );
}

/// ★★ **行号地址：不许越界，而且不许再添新的。**
///
/// # 为什么是棘轮，而不是「判它今天指得对不对」
///
/// 判后者**要读语义**，机器读不了 —— 09-02 逐处手核过全部 70 处，
/// 而**一处都不是机器判出来的**。粗判据试过一次：拿地址旁边的引文去比对，
/// 手核四支**假阳两支**（一支的引文是引用方自己的话；一支是订正段里当反面教材
/// 留着的历史行号 —— 天真的判据会去红「记录了这个病的那段话」本身）。
///
/// ⇒ 本条**不判真伪**，只做两件机器判得了的事：
///   ① **越界**：被引文件根本没有那么多行 ⇒ 一定是假的（零语义）。
///   ② **棘轮**：存量登记在下表里，**新写的地址一律红** ——
///      逼它去写符号地址，而符号地址的真伪由上面那条**真的判得了**。
///
/// # 它买不到什么（逐条写明）
///
/// - 存量那三十几处**今天指得对不对，本条判不了**，将来烂了也不会红。
/// - 棘轮**按去重后的（引用方文件, 被引地址）对**认：把一处删掉、另换一处写上，
///   总数不变，本条**看不见**。它拦的是「顺手又写一个」，不是恶意。
/// - 被引文件不在本仓（第三方 crate 的路径）时，越界那半**无从判起**，如实跳过。
#[test]
fn line_number_addresses_stay_in_range_and_never_grow() {
    /// **存量登记**：`(引用方文件基名, 被引路径原样, 被引行号)`。
    ///
    /// 这一张表就是「这一拍之后还剩什么没牙」的**逐条点名**：
    /// 表里每一行都是一处**判不了真伪**的地址。想少一行的唯一办法是把它改成符号地址。
    ///
    /// ⚠ 加行之前先问一遍：**能不能点符号**？答得出来就别加。
    const INVENTORY: &[(&str, &str, usize)] = &[
        ("atomic_replace_registry.rs", "fenced_block.rs", 5),
        ("bind.rs", "bind.rs", 225),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("bind_tests.rs", "bind.rs", 319),
        ("bridge.rs", "tauri-2.11.2/src/ipc/mod.rs", 181),
        ("data_paths.rs", "tauri-2.11.2/src/ipc/mod.rs", 181),
        ("inbound_client.rs", "bridge.rs", 95),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("launch_tests.rs", "launch.rs", 122),
        // 🔴 〔步 7c 剖分 2026-09-19〕**这一行删了 —— 那处行号地址改成了符号地址。**
        // 原来是 `guard-core/src/lib.rs` 里那句「两侧各有一个文件因此假红」的注，
        // 点的是 `ccm_cli_contract.rs` 的第 181 行。那份文件剖分后只剩 40 行 ⇒ 越界，
        // 已按本条的第 ① 条出路改成
        // `ccm_cli_contract_tests.rs::cc_spawn_resolves_a_real_ccm_file_not_a_directory`。
        // ⇒ 本条的保鲜自检逐字要求把这一行删掉（「多半是有人把它改成符号地址了，那是好事」）。
        // **存量少一条，这是往下走**（递减方向），不是把账挂空。
        ("lib.rs", "main.rs", 26),
        ("lib.rs", "russh-sftp-2.3.0/src/protocol/file_attrs.rs", 29),
        // 〔SR1b〕`lib.rs → sftp.rs 的第 141 行` 摘了：creds-core 那句改成了点符号（原子上传搬去后端 `put_atomic`）。
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("local_backend_tests.rs", "structural_scan.rs", 425),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("local_backend_host.rs", "launch.rs", 196),
        ("local_backend_host_tests.rs", "launch.rs", 196),
        ("local_backend_host.rs", "launch.rs", 198),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("local_backend_host_tests.rs", "local_backend.rs", 336),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("local_backend_host_tests.rs", "structural_scan.rs", 425),
        // 〔搬树 2026-09-18〕引用方随测试段搬家，被引地址一个字没变。
        ("local_backend_host_tests.rs", "structural_scan.rs", 508),
        // 〔RM1f〕`("panorama.rs", "engine.rs", 42)` 这一行删了：引用方那份文件随 monitor 的内嵌引擎一起删了。
        // 🔴 〔搬树 2026-09-18〕**这一行删掉了**：`config_surface.rs` 的测试段搬走之后
        //    那份文件只剩 853 行，1062 行**越界**了（本条第 ① 格当场逮住）。
        //    按本条头注唯一那条改法**改成了符号地址**
        //    （`rows_tests.rs::either_host_keeps_the_glob_count`，
        //    那正是原先第 1062 行那段话讲的东西）⇒ 从「判不了真伪」变成
        //    `every_symbol_address_in_the_sources_still_resolves` **真的判得了**。
        //    ⚠ 没有「换一个今天对的行号」—— 那是本条头注逐字禁的那一手。
        // 🔴 〔RM1a · 第四波〕**这一行删了**：`parity_ledger_tests.rs` 里点 `sftp.rs` 第 141 行的那句散文
        //    住在 `creds.apikey` 那条平价欠账的理由里，那条欠账结清、整行删掉，那处行号地址随之没了。
        //    **存量少一条，这是往下走**（递减方向）。
        ("ratchet_guard.rs", "control/tmux_hook.rs", 6),
        ("ratchet_guard.rs", "control/tmux_hook.rs", 102),
        ("ratchet_guard.rs", "main.rs", 651),
        ("ratchet_guard.rs", "observe/watcher.rs", 1040),
        ("ratchet_guard.rs", "tmux_hook.rs", 6),
        ("single_stream_guard.rs", "stream/inbound.rs", 35),
        ("single_stream_guard.rs", "main.rs", 585),
        ("single_stream_guard.rs", "observe/watcher.rs", 77),
        ("single_stream_guard.rs", "relay/tee.rs", 169),
        (
            "single_stream_guard.rs",
            "src/bridge/src/backend/mod.rs",
            22,
        ),
        ("table.rs", "server.rs", 24),
    ];

    let corpus = addr_corpus();
    // 基名 → 盘上的那几份（消歧要用）
    let mut bybase: std::collections::BTreeMap<String, Vec<&(std::path::PathBuf, String)>> =
        std::collections::BTreeMap::new();
    for e in &corpus {
        bybase.entry(addr_base(&e.0)).or_default().push(e);
    }

    let mut seen: std::collections::BTreeSet<(String, String, usize)> =
        std::collections::BTreeSet::new();
    let mut hits = 0usize;
    let mut newly: Vec<String> = Vec::new();
    let mut overrun: Vec<String> = Vec::new();
    for (p, raw) in &corpus {
        let fname = addr_base(p);
        for (ln, cited, n) in line_addresses(raw) {
            hits += 1;
            let key = (fname.clone(), cited.clone(), n);
            seen.insert(key.clone());
            if !INVENTORY
                .iter()
                .any(|(a, b, c)| *a == fname && *b == cited && *c == n)
            {
                newly.push(format!("  {fname}:{ln}  指 {cited} 的第 {n} 行"));
            }
            // 越界那半：先消歧，消不了就如实跳过（不猜）。
            let base = cited.rsplit('/').next().unwrap_or(&cited).to_string();
            let Some(cands) = bybase.get(&base) else {
                continue; // 第三方 crate / 不在本仓 ⇒ 无从判起
            };
            // 消歧只走一步：**路径后缀唯一**。消不了就跳过，**不猜**
            // —— 猜错时它会输出一个看起来完全合理的东西（`K-R10` 否掉选法 ① 的
            // 理由逐字就是这个），那比不判更坏。
            let narrowed: Vec<&&(std::path::PathBuf, String)> = cands
                .iter()
                .filter(|e| e.0.to_string_lossy().ends_with(cited.as_str()))
                .collect();
            if narrowed.len() != 1 {
                continue;
            }
            let len = narrowed[0].1.lines().count();
            if n > len {
                overrun.push(format!(
                    "  {fname}:{ln}  指 {cited} 的第 {n} 行，而它只有 {len} 行"
                ));
            }
        }
    }

    // ★ 三条断言的**顺序是承重的**（09-02 变异台逼出来的）：
    //   越界 → 新增 → 登记表保鲜。把保鲜排在前面时，「有人把一处地址指到了文件末尾之后」
    //   这一刀报出来的是「登记表里那一行盘上没有了」——**指错了修法**，
    //   而本仓反复吃过「读诊断」的亏：指错地方的诊断比没有诊断更费时间。
    // ★ 抽取器自检：人群塌了 ⇒ 本条零命中地绿。
    assert!(
        hits >= 25,
        "只抽到 {hits} 处行号地址 —— 抽取器坏了（09-02 现打 36 处 / 去重 32 对）"
    );
    assert!(
        overrun.is_empty(),
        "这些行号地址**越界**了 —— 被引文件根本没有那么多行，一定是假的：\n{}\n\n\
             ⇒ 改法只有一条：**点符号**（函数名 / 常量名 / 类型名），别换一个今天对的行号 ——\
             换一个今天对的行号就是把这一族再走一遍。",
        overrun.join("\n")
    );
    assert!(
        newly.is_empty(),
        "这几处是**新写的行号地址**，而行号是每一轮都会变的量，写下去下一轮自动变成假话：\n{}\n\n\
             ⇒ 三条出路，按优先级：\n\
             ① **点符号**（`文件·rs::函数名` / 常量名 / 类型名）—— 它的真伪由\
             `every_symbol_address_in_the_sources_still_resolves` 真的判得了；\n\
             ② 那一处是**故意留着的历史反例**（订正段 / 墓碑）⇒ 在同一行加 \
             `LINE_ADDRESS_TOMBSTONE` 那个标记；\n\
             ③ 确实只能用行号 ⇒ 加进本条的存量登记表，**并在提交信息里写清为什么点不了符号**。\n\
             ⚠ **不许**为了让本条变绿就把它删掉了事：删掉的是线索，不是病。",
        newly.join("\n")
    );

    // ★ 登记表保鲜：登记的那一处已经不在盘上了 ⇒ 删掉它，别让表替真判据挡枪。
    let gone: Vec<String> = INVENTORY
        .iter()
        .filter(|(a, b, c)| !seen.contains(&(a.to_string(), b.to_string(), *c)))
        .map(|(a, b, c)| format!("  {a} → {b} 的第 {c} 行"))
        .collect();
    assert!(
        gone.is_empty(),
        "存量登记里这几条盘上已经没有了 —— **把它们从表里删掉**（多半是有人把它改成符号地址了，那是好事）：\n{}",
        gone.join("\n")
    );
}

/// 上面两条判据的**活体夹具** —— 它们在真树上今天恰好都是绿的
/// （越界 0 处、新增 0 处、符号对不上 0 处），而「今天该是空的」那种格是**空真**：
/// 闸死了 `[] == []` 照样成立。⇒ 这里造一份**真的会红**的语料，逐格切开验。
///
/// 夹具文本一律**现拼**（`.rs` 与冒号分开写），免得夹具自己被真树上的扫描收进人群 ——
/// 「别让夹具的名字混进断言」这一条本仓栽过两次。
#[test]
fn the_address_extractors_really_see_each_shape() {
    let colon = ":";
    let dcolon = "::";

    // ① 行号地址：认得出，且区间形只取头一个数。
    let t = format!("见 relay/server.rs{colon}97 与 table.rs{colon}18-23 两处");
    let got = line_addresses(&t);
    assert_eq!(
        got,
        vec![
            (1, "relay/server.rs".to_string(), 97),
            (1, "table.rs".to_string(), 18),
        ],
        "行号地址抽取器认错了：{got:?}"
    );

    // ② 形 B：带墓碑标记的那一行整行不进人群。
    let t = format!("这里先前点着 relay/server.rs{colon}97，今天不对了 {LINE_ADDRESS_TOMBSTONE}");
    assert!(
        line_addresses(&t).is_empty(),
        "带 {LINE_ADDRESS_TOMBSTONE} 的行不该进人群 —— 否则判据会去红「记录了这个病的那段话」本身"
    );

    // ③ 符号地址：认得出；`::` 后面是数字的**不是**符号地址，两个人群不许互相污染。
    let t = format!("见 relay/server.rs{dcolon}handle 与 main.rs{colon}651");
    let syms = symbol_addresses(&t);
    assert_eq!(
        syms,
        vec![(1, "server.rs".to_string(), "handle".to_string(), false)],
        "符号地址抽取器认错了：{syms:?}"
    );

    // ④ 前缀形：以 `_` 收尾 ⇒ 通配或行折，降级成前缀比对而不是报红。
    let t = format!("见 history.rs{dcolon}build_local_*_command");
    let syms = symbol_addresses(&t);
    assert_eq!(syms.len(), 1, "前缀形没抽到：{syms:?}");
    assert!(
        syms[0].3,
        "以 `_` 收尾的符号必须标成前缀形，否则它会被误报成「找不到」"
    );

    // ⑤ 中文文件名不进人群（本文件头注里的形状样例正是这么写的）。
    let t = format!("形如 文件.rs{colon}123 与 文件.rs{dcolon}符号");
    assert!(line_addresses(&t).is_empty() && symbol_addresses(&t).is_empty());

    // ⑥ **越界真的判得出来**：拿真树上一份文件，指它末行之后一行。
    let corpus = addr_corpus();
    let (probe, src) = corpus
        .iter()
        .find(|(p, _)| addr_base(p) == "structural_scan.rs")
        .expect("语料面里必须有本文件自己 —— 没有就说明自收那一步掉了");
    let len = src.lines().count();
    assert!(len > 100, "{probe:?} 只有 {len} 行 —— 语料读坏了");
    let addr = line_addresses(&format!("指 structural_scan.rs{colon}{}", len + 1));
    assert_eq!(addr.len(), 1);
    assert!(
        addr[0].2 > len,
        "越界那一格必须是「被引行号 > 文件行数」，否则上面那条的越界半边是空转的"
    );
}

/// 〔`K-R63`〕[`fn_names_starting_with`] 的反向自检：**既不恒空也不恒满**，
/// 而且它**看不见注释与测试段** —— 那两处的名字不是实现，认进去就会误红。
#[test]
fn the_verb_scan_reads_production_only_and_is_not_vacuous() {
    let src = concat!(
        "pub fn uninstall_thing() {}\n",
        "fn keep_this() {}\n",
        "// fn uninstall_that_is_only_a_comment() {}\n",
        "\n#[cfg",
        "(test)]\nmod tests {\n    fn uninstall_that_is_only_a_test() {}\n}\n"
    );
    assert_eq!(
        fn_names_starting_with(src, &["uninstall"]),
        vec!["uninstall_thing".to_string()],
        "生产段那一个要认出来，注释与测试段那两个都不许认"
    );
    assert!(
        fn_names_starting_with(src, &["nobody_writes_a_name_like_this"]).is_empty(),
        "动词对不上还回东西 ⇒ 它是恒满的，用它的判据全是空真"
    );
    // 真树上打一发：这个动词在真文件里确实有命中（恒空的扫描买不到任何东西）。
    assert!(
        fn_names_starting_with(include_str!("../../src/bridge/src/sftp.rs"), &["uninstall"])
            .contains(&"uninstall_remote_backend".to_string()),
        "真树上扫不到 `uninstall_remote_backend` —— 剥法或遍历坏了"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R20`：**只活在散文里的名字**（零定义的名字被当现状说）
// ═══════════════════════════════════════════════════════════════════════

/// 至少几个下划线才进人群。**只能是 2，不许往上调。**
///
/// 整条曲线（现打，量于本件基点）：
/// `>=1` 100 个/143 处 · **`>=2` 58/84** · `>=3` 43/61 · `>=4` 33/50 · `>=5` 30/44。
/// 🔴 旗舰活体 `local_tmux_names` **只有 2 个下划线，在 `>=3` 就掉出人群**。
/// ⇒ 把阈值调上去换一个好看的人群数，等于把本条要逮的那个逮不着。
const DEAD_NAME_MIN_UNDERSCORES: usize = 2;

/// 仓根相对路径（`\` 一律归一成 `/`，登记表才在 Windows 上也对得上）。
fn dead_name_rel(root: &std::path::Path, p: &std::path::Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 本条的语料面：**六个根 + `src/bridge/build.rs`**，`(仓根相对路径, 原文)`。
///
/// # 三条边界，每条都是**对拍逮出来的**，不是想出来的
///
/// · **`build.rs` 非收不可**：`emit_backend_capabilities` 真的定义在那儿
///   （`addr_corpus()` 也是单独把它捞进来的）。不收它 ⇒ 当场多一处假阳。
/// · **`evidence/` 刻意不收**（它不在六个根下，本条按构造够不着）：那是量具与记录，
///   散文里逐字写着一堆死名（`local_tmux_names` 就在里面）。收进来 =
///   **代码侧被本族自己的记录喂饱**，旗舰活体当场从人群里消失。
/// · **不按扩展名筛**（`scan_tree!` 空列表那一档）：代码侧越宽假红越少，
///   而 `.json` / `.tsv` / 无扩展名的脚本都可能是一个名字真正的家。
///
/// ⚠ **本文件不在语料里。这一刀非落不可**：下面 `INVENTORY` 里每个死名都是一个
/// **字符串字面量 = 代码**，不摘的话凡是登记过的名字全都「在代码里出现过」，
/// **人群当场塌成空集**。
/// 代价如实写明：**本文件自己的散文没人看** —— 那是 `ratchet_guard.rs` 头注那条纪律
/// 「判据不许与被扫文本同住一个文件」的另一面。**射程外，不假装覆盖了。**
///
/// # 🔴 〔搬树 2026-09-18 · `设计/99` 条 73〕这一刀**从 `file!()` 换成明写的排除**
///
/// 上一版靠 `scan_tree!` 的自摘落这一刀。剖分之后本文件由 `#[path]` 引进来，
/// `file!()` 给的是 `src/../../../tests/bridge/structural_scan_tests.rs` ——
/// **带 `..` 的折返路径**，而草垛是规范化过的绝对路径 ⇒ 后缀比不命中
/// ⇒ **那一刀整个落空**，本文件连同它那张 `INVENTORY` 一起进了语料。
/// 读数：「只活在散文里的名字」从 84 处塌到 **6 处**。
/// ⚠ 注意它塌的方向：这次是**人群变小**、被反空真逮住了；
/// 反过来那一半（判据在自己的登记表里找到自己 ⇒ 恒绿）同样存在，而且不会红。
/// ⇒ 换成 [`guard_core::scan_tree_excluding`]：排除**明写出来**，
/// 而且**摘不到就当场红**（`file!()` 那条路从来没有这一格）。
fn dead_name_corpus() -> Vec<(String, String)> {
    let root = addr_repo_root();
    let mut out: Vec<(String, String)> = Vec::new();
    // 🔴 〔搬树 2026-09-18〕**两棵根，互不包含。** 上一版列了五个根，其中
    // `"src/bridge/src"` · `"src/bridge/crates"` · `"src/doc"` 在搬树之后**全都是
    // `"src"` 的子目录** ⇒ 那些文件被**数两遍**，于是「盘上 N 处」全部翻倍，
    // 而登记表写的是搬家前的真数 ⇒ 一片假红（本轮实发：`local_is_posix` 1→2 之类）。
    //
    // ⚠ 那一版的注释**已经点出了这个形状**（「后端树已经是 `src` 的子目录，
    // 两个都列会把每个文件数两遍」）—— 但只对 `"src/backend"` 那一格落实了，
    // 另外三格漏了。⇒ 教训：**发现一个形状之后，要把同形的全找一遍**，
    // 不是只修当场那一个。
    for sub in [
        "src",
        // 〔e2e 并入 tests/，2026-09-17〕这一格**从 `"e2e"` 换成 `"tests"`**，不是换成
        // `"tests/e2e"`：前端测试此前住在 `src/` 里、被上面那个 `"src"` 顺带收着；
        // 搬去 `tests/` 之后**这个语料面悄悄缩了一大块**，而本族的反空真检查只管
        // 「本文件不在语料里」，管不了「少了一棵树」⇒ 会安静地少扫，不会红。
        // 收 `"tests"` 一并把前端测试与 `tests/e2e/` 都拿回来。
        "tests",
    ] {
        // 🔴 **排掉的是谁、为什么**（`设计/99` 条 73 要的那一句）：
        //    排掉 `tests/bridge/structural_scan_tests.rs` —— **本文件**。
        //    理由见上面头注：它那张 `INVENTORY` 把每个死名都写成了字符串字面量，
        //    收进来就等于宣布「这些名字都还活着」，人群当场塌成空集。
        //    ⚠ 只对 `"tests"` 那一棵给名单：本文件不在 `"src"` 下面，而
        //    `scan_tree_excluding` **摘不到就红** —— 给错根会当场说话，不会安静地空转。
        let excluded: &[&str] = match sub {
            "tests" => &["tests/bridge/structural_scan_tests.rs"],
            _ => &[],
        };
        for (p, src) in guard_core::scan_tree_excluding(&root.join(sub), &[] as &[&str], excluded) {
            let rel = dead_name_rel(&root, &p);
            // 🔴 〔搬树 2026-09-18〕**`evidence/` 必须排掉** —— 本函数头注逐字写着
            // 它「刻意不收」，理由是「那是量具与记录，散文里逐字写着一堆死名；
            // 收进来 = 代码侧被本族自己的记录喂饱，旗舰活体当场从人群里消失」。
            // 那句话当时靠的是**位置**（`evidence/` 不在当年那几个根下）。
            // 搬树把它变成 `tests/evidence/` ⇒ 跟着 `"tests"` **自己回来了**，
            // 而那条设计意图一个字都没改。⇒ 把"靠位置"换成**一条明写的排除**。
            // 🔴 〔搬树 2026-09-18〕两条按位置生效的射程边界，都被搬树悄悄取消了：
            //
            // ① `evidence/` —— 本函数头注逐字写着它「刻意不收」：那是量具与记录，
            //    散文里逐字写着一堆死名；收进来 = **代码侧被本族自己的记录喂饱**，
            //    旗舰活体当场从人群里消失。搬成 `tests/evidence/` 后跟着 `"tests"` 回来了。
            //
            // ② `vendor/` —— 第三方 vendored 代码。它原住 `src-tauri/vendor/`（当年的根之外），
            //    现在 `src/bridge/vendor/` 落进了 `"src"` ⇒ **第三方代码在给我们的
            //    「这个名字还活着吗」投票**。实发四条假账：`path_shell_safe` 定义在
            //    vendored 的 `cc-acct-iso/scripts/lib.sh` 里、`guard_doc_rel` 与
            //    `symbols_in_file` 定义在 vendored 的 `code-picture-core` 里
            //    ⇒ 三个真死名被读成「活的」，登记表被判成腐了。
            //
            // ⚠ 教训：**「靠位置生效的边界」在搬树时会静默失效** —— 它不会报错，
            //    只会让射程悄悄变大或变小。换成明写的排除。
            // 〔MIG-3a · 09-28 预裁〕vendored `cc-acct-iso` 挪去了 `src/shared/cc-acct-iso/`（字节随后端二进制走、两棵树都不属于）⇒
            //   它不再落在 `/vendor/` 底下，同一个教训：**明写**把它排出去（`path_shell_safe` 那条假账当场复现过）。
            if rel.starts_with("tests/evidence/")
                || rel.contains("/vendor/")
                || rel.starts_with("src/shared/cc-acct-iso/")
            {
                continue;
            }
            out.push((rel, src));
        }
    }
    let br = root.join("src/bridge/build.rs");
    let br_src = std::fs::read_to_string(&br).expect("读不到 src/bridge/build.rs");
    out.push((dead_name_rel(&root, &br), br_src));
    // 🔴 〔搬树 2026-09-18〕**本文件的「声明」那一半要补回来，「登记表」与「散文」那两半不补。**
    //
    // 上面那一刀摘的是**整份文件**，而它其实只该摘掉两样东西：
    // ① `INVENTORY` / `TOMBSTONED` 里那些**死名字符串字面量**（不摘 ⇒ 人群塌成空集）；
    // ② 本文件自己的散文（头注逐字写着「射程外，不假装覆盖了」）。
    //
    // 剖分之前这两样与本文件那几十条 `fn` 声明**同住 `structural_scan.rs`**，
    // 一起被摘掉也就一起看不见。剖分把**判据**搬来了 `tests/`，而**散文留在原处**
    // ⇒ 摘掉整份文件之后，`structural_scan.rs` / `local_backend_host_tests.rs` / …
    // 那十几句「由 `every_symbol_address_in_the_sources_still_resolves` 钉着」
    // 会被读成「点名了一个代码里根本不存在的名字」—— 而那些判据**就在盘上、还在跑**。
    // 把它们登记进 `INVENTORY` 才是真的说谎（那张表逐字是「代码里根本不存在的名字」）。
    //
    // ⇒ 只把本文件的**不含字面量、不含注释**的那些行补回来：`fn` 声明在里面，
    //   死名字面量与散文都不在。方向是安全的 —— 这一补只可能让名字**更像活的**，
    //   而补进来的全是本文件真有的声明。
    let me_rel = "tests/bridge/structural_scan_tests.rs";
    let me_decls: String = include_str!("structural_scan_tests.rs")
        .lines()
        .filter(|l| !l.contains('"') && !l.contains("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        me_decls.contains(concat!(
            "fn every_dead_name_named_in_the_",
            "prose_is_declared_dead"
        )),
        "本文件的声明那一半没补进来 —— 那十几条真实存在的判据会被读成死名"
    );
    out.push((me_rel.to_string(), me_decls));
    // ★ 抽取器自检：语料面塌了 ⇒ 下面整条零命中地绿。
    assert!(
        out.len() >= 550,
        "死名判据只收到 {} 份源文件 —— 语料面坏了（09-03 现打 598：六个根 597 + build.rs 1）",
        out.len()
    );
    out
}

/// 一个词是不是「**全小写 `snake_case` + 至少 `min_us` 个下划线**」。
///
/// 零词表、纯句法 —— 这正是本条与 `K-R6` 网 A / `K-R18` R1 撞的那种
/// 「历史限定词」开放类词表**形状不同**的地方：那两次要枚举的是自然语言，怎么枚都枚不全。
fn is_dead_name_shape(w: &str, min_us: usize) -> bool {
    let segs: Vec<&str> = w.split('_').collect();
    if segs.len() < min_us + 1 {
        return false;
    }
    if !segs[0].starts_with(|c: char| c.is_ascii_lowercase()) {
        return false;
    }
    segs.iter().all(|s| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

/// 抽出 `text` 里每一个符合上面那个形状的标识符。
///
/// 取的是**极大 run**（一路吃到不是 `[A-Za-z0-9_]` 为止）⇒ 两侧边界自带：
/// `Foo_bar_baz` 整个取出来、整个判否，**不会**从中间抠出一个 `bar_baz`。
/// 这一条是承重的：本仓 `find_pinned` 头注整段在讲「匹配单位比事实小」那一族。
fn dead_name_idents(text: &str, min_us: usize) -> Vec<&str> {
    fn is_id(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_'
    }
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if !is_id(b[i]) {
            i += 1;
            continue;
        }
        let s = i;
        while i < b.len() && is_id(b[i]) {
            i += 1;
        }
        // `s`/`i` 都落在 ASCII 与非 ASCII 的交界上 ⇒ 一定是字符边界，切原串安全。
        let w = &text[s..i];
        if is_dead_name_shape(w, min_us) {
            out.push(w);
        }
    }
    out
}

/// 一段反引号跨度**整个就是一个裸符号引用**吗？（可带 `路径::` 前缀、`()` / `!` 后缀）
///
/// # 为什么要「整个是」，而不是「跨度里出现过」
///
/// 「跨度里出现过」会把整句散文的跨度收进来（现打差 27 处，全是噪声）。
/// # 为什么不是「跨度逐字等于名字」
///
/// 那样 `` `local_tmux_names()` ``（带括号）与 `` `文件.rs::foo_bar_baz` ``（带路径）
/// 都会漏掉，而**那正是订正段最常见的写法** —— 漏掉它们就漏掉了本条要看的那一半。
///
/// ⚠ 上面那个样例**刻意用中文文件名**：`symbol_addresses` 只收 ASCII 路径，
/// 写成 ASCII 的话本文件就多了一处指向不存在符号的地址 ——
/// 09-03 现打，第一版就是这么红的，与本文件头注那条同源。
fn bare_symbol_in_span(span: &str, min_us: usize) -> Option<&str> {
    let mut s = span.trim();
    if let Some(t) = s.strip_suffix("()").or_else(|| s.strip_suffix('!')) {
        s = t;
    }
    if let Some(k) = s.rfind("::") {
        let (pre, post) = s.split_at(k);
        if pre.is_empty()
            || !pre
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c))
        {
            return None;
        }
        s = &post[2..];
    }
    if is_dead_name_shape(s, min_us) {
        Some(s)
    } else {
        None
    }
}

/// 从左到右**成对**取一行里的反引号跨度。
///
/// ⚠ 刻意不用「非反引号字符 ≥1」那种配法：markdown 的双反引号
/// `` `` `x` `` `` 会让它错位（第一对里没有非反引号字符 ⇒ 从第二个反引号起配，
/// 把里面那个名字切丢）。成对扫描把空跨度也算一对，双反引号形就正常收得到。
fn backtick_spans(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(a) = line[i..].find('`') {
        let s = i + a + 1;
        let Some(b) = line[s..].find('`') else { break };
        out.push(&line[s..s + b]);
        i = s + b + 1;
    }
    out
}

/// 把一份文件切成 `(行号1基, 代码段, 注释段)`。
///
/// `//` 那一族（`.rs` / `.ts` / …）走 [`guard_core::strip_comment_lines`] ——
/// **整行注释与行尾注释都剥，而且字符串安全**（`"http://host"` 不会被切短）。
///
/// 🔴 **行尾注释必须算注释**，这是 `K-R19` 实测出来的坑：它第一版尺子只认整行注释，
/// 于是 `tests/ipc/commands.vitest.ts` 那行「代码 + 行尾注释」被判成「代码里有这个名字」
/// ⇒ 旗舰活体 `local_tmux_names` **一次都没进人群**。
///
/// `.md` 整份算散文（它不定义任何符号）；其余一律**整份算代码**（最保守：
/// 代码侧越宽，假红越少 —— 本条宁可漏抓，不许假红）。
///
/// ⚠ 射程如实写明：`strip_comment_lines` 有三种情形**整行不动**
/// （raw / byte string、跨行字符串内部、引号本行不配平），那几行的注释本条看不见。
fn dead_name_split(path: &str, text: &str) -> Vec<(usize, String, String)> {
    if text.is_empty() {
        return Vec::new();
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    let ext = if base.contains('.') {
        base.rsplit('.').next().unwrap_or("")
    } else {
        ""
    };
    let lines: Vec<&str> = text.lines().collect();
    if ext == "md" {
        return lines
            .iter()
            .enumerate()
            .map(|(i, l)| (i + 1, String::new(), (*l).to_string()))
            .collect();
    }
    if !matches!(ext, "rs" | "ts" | "tsx" | "mts" | "mjs" | "js" | "cjs") {
        return lines
            .iter()
            .enumerate()
            .map(|(i, l)| (i + 1, (*l).to_string(), String::new()))
            .collect();
    }
    let stripped = guard_core::strip_comment_lines(text);
    let code: Vec<&str> = stripped.split('\n').collect();
    assert_eq!(
        code.len(),
        lines.len(),
        "剥注释改变了行数 ⇒ 下面按行配对会错位（{path}）"
    );
    lines
        .iter()
        .zip(code.iter())
        .enumerate()
        .map(|(i, (raw, c))| {
            let cmt = if raw.starts_with(*c) {
                raw[c.len()..].to_string()
            } else {
                (*raw).to_string()
            };
            (i + 1, (*c).to_string(), cmt)
        })
        .collect()
}

/// 全语料现打：`(路径, 名字)` → `(未声明处数, 带墓碑处数)`，只留**代码侧零出现**的。
fn dead_names_on_disk(
    corpus: &[(String, String)],
    min_us: usize,
) -> (
    usize,
    std::collections::BTreeMap<(String, String), (usize, usize)>,
) {
    let mut in_code: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut in_prose: std::collections::BTreeMap<(String, String), (usize, usize)> =
        std::collections::BTreeMap::new();
    for (path, text) in corpus {
        for (_, code, cmt) in dead_name_split(path, text) {
            for w in dead_name_idents(&code, min_us) {
                in_code.insert(w.to_string());
            }
            let tomb = cmt.contains(PROSE_NAME_TOMBSTONE);
            for span in backtick_spans(&cmt) {
                if let Some(name) = bare_symbol_in_span(span, min_us) {
                    let e = in_prose
                        .entry((path.clone(), name.to_string()))
                        .or_insert((0, 0));
                    if tomb {
                        e.1 += 1;
                    } else {
                        e.0 += 1;
                    }
                }
            }
        }
    }
    let dead = in_prose
        .into_iter()
        .filter(|((_, n), _)| !in_code.contains(n))
        .collect();
    (in_code.len(), dead)
}

/// ★★ **散文里点名的名字，要么在代码里，要么由写的人声明成历史。**
///
/// # 它买的是什么
///
/// 本仓的判据全叫 `every_x_is_y` 这种长 `snake_case` 名字，而散文**大量**在
/// 「由 `X` 钉住」「`X` 那条机检管着」这么说。改名 / 删掉 / 换实现之后，
/// **那句话不会跟着改，也没有任何东西会因此变红** —— 于是一个不存在的名字
/// 被当成现状说了下去，读的人以为「这件事有人守着」。
///
/// 🔴 **不是假想的病**：本仓的人**手工撞见过至少两次**，每次留一段字，然后没有闸 ——
/// `alloc_probe.rs` 里那条「`the_probe_sees_allocations_made_by_the_reader_task` 钉着」，
/// 与 `listen.rs` 里那条「这里原先指的是一个**不存在的文件**」。
///
/// # 它买不到什么（逐条写明，别把射程写宽了一格）
///
/// · **判不了「这句话是不是当现状在说」。** 存量里绝大多数是散文自己已经声明了历史
///   （「原 `X`」「上一版是 `X`」「已删」），那些是合法的；机器分不开它们与真陈账。
///   ⇒ 本条**不判真伪**，只做两件机器判得了的事：**登记存量** + **新写的一律红**。
/// · **棘轮按 `(路径, 名字, 处数)` 认。** 把一处删掉、另换一处写上，
///   同一份文件里处数不变的话本条**看不见**。它拦的是「顺手又写一个」，不是恶意。
/// · **本文件自己的散文没人看**（语料按构造摘掉调用者）——见 `dead_name_corpus` 头注。
/// · **只看反引号里的裸符号引用**。`（local_tmux_names）` 这种**不带反引号**的写法
///   本条看不见 —— 现打 `tests/ipc/commands.vitest.ts` 里就有三处这样的，
///   如实登记成射程外，**不假装覆盖了**。
#[test]
fn every_dead_name_named_in_the_prose_is_declared_dead() {
    /// **存量登记**：`(仓根相对路径, 名字, 未声明的处数)`。
    ///
    /// 这张表就是「这一拍之后还剩什么没牙」的**逐条点名**：每一行都是一处
    /// **判不了真伪**的名字。想少一行的唯一办法是**把那句话改对**。
    ///
    /// ⚠ 加行之前先问一遍：**这个名字今天真的存在吗？** 不存在就先改话，别先加行。
    /// ⚠ **不许**靠贴墓碑把存量抹平 —— 墓碑只给「本轮真的改过的那几处订正段」。
    const INVENTORY: &[(&str, &str, usize)] = &[
        // 🔴 〔步 8 · 条 80 「不要管旧配置」2026-09-19〕**这两行是新登记的，原因是一条判据退役。**
        //    `daemonless_stream_loop` 这个符号 `K-R59`（09-11）就从代码里删掉了，
        //    它此后一直被一处**字符串字面量**当成「还活着」——
        //    `doc_claim_registry_daemon_wording_registry.rs` 拿它当「非裸词」自检夹具。
        //    条 80 把那个夹具换掉之后，它才第一次被本条看见。
        //    ⇒ 散文里这三处（README 1 · `ssh_source.rs` 2）全是**病史与墓碑**，该留；
        //      「代码里没有这个名字」这件事该由本表说出来，而不是靠一处夹具替它遮着。
        //    ⚠ 这正是本条头注那句话的又一个实例：一个名字「在代码里出现过」不等于它活着。
        ("src/bridge/README.md", "daemonless_stream_loop", 1),
        ("src/bridge/src/ssh_source.rs", "daemonless_stream_loop", 2),
        (
            "tests/bridge/doc_claim_registry_tests.rs",
            "daemonless_stream_loop",
            1,
        ),
        ("src/doc/ARCHITECTURE.md", "lookup_by_foreground_pid", 1),
        // 〔LOC1b〕`("src/doc/CONTRIBUTING.md", "list_active_session_ids", 1)` 摘了：那段示例改写成读本机活会话表，不再点那个说明性的名字。
        ("src/doc/INVARIANTS.md", "path_shell_safe", 1),
        ("src/doc/INVARIANTS.md", "snapshot_announced_by_origin", 1),
        ("src/doc/STATE-MATRIX.md", "read_session_jsonl", 1),
        (
            "src/backend/agents/claudecode/accounts.rs",
            "trust_of_claude_json",
            1,
        ),
        (
            "src/backend/alloc_probe.rs",
            "the_probe_sees_allocations_made_by_the_reader_task",
            1,
        ),
        (
            "src/backend/stream/inbound.rs",
            "handlers_never_run_on_the_reader_task",
            1,
        ),
        // 🔴 〔步 7c 后端剖分 2026-09-19 · C 类〕**这一条拆成两行，总处数守恒（3 ＝ 2 ＋ 1）。**
        //    那个名字的 3 处散文引用里，2 处留在生产段的头注里、1 处随测试段搬走了。
        (
            "src/backend/stream/inbound.rs",
            "hello_commands_match_the_dispatch_table",
            2,
        ),
        (
            "tests/backend/stream/inbound_structure_guards.rs",
            "hello_commands_match_the_dispatch_table",
            1,
        ),
        (
            "src/backend/stream/inbound.rs",
            "hello_is_flushed_before_the_inbound_reader_starts",
            1,
        ),
        (
            "src/backend/stream/listen.rs",
            "frozen_single_client_guard",
            1,
        ),
        // 〔步 7c 后端剖分 2026-09-19 · C 类〕同上，**总处数守恒（2 ＝ 1 ＋ 1）**。
        (
            "src/backend/observe/accounts_query.rs",
            "credential_filename_matches_native_identity_declaration",
            1,
        ),
        (
            "tests/backend/observe/accounts_query_tests.rs",
            "credential_filename_matches_native_identity_declaration",
            1,
        ),
        (
            "src/backend/observe/accounts_query.rs",
            "path_shell_safe",
            1,
        ),
        ("src/backend/observe/watcher.rs", "spawn_tmux_ticker", 2),
        (
            "tests/backend/protocol_doc_guard.rs",
            "hello_commands_match_the_dispatch_table",
            1,
        ),
        ("src/backend/relay/http1.rs", "handle_alloc_error", 1),
        // 〔步 7c 后端剖分 2026-09-19 · C 类〕散文随测试段搬家，处数一格没变。
        (
            "tests/backend/relay/http1_tests.rs",
            "head_cap_is_enforced",
            1,
        ),
        // 〔AR1〕`tests/backend/relay/nodelay_guard.rs` 那一行删了：文件随 `设计/15 §2.1` B3 退役整份删掉，
        //   它头注里点名的旧判据名跟着没了（不是改对了话，是那段话不在了）。
        ("src/backend/relay/server.rs", "handle_alloc_error", 1),
        // 〔DEL〕`tests/backend/relay/server_tests.rs` 那一行（1 处）摘了：点那个旧名的那段散文随 `--relay` 入口那三条判据一起删了 ⇒ 存量 −1。
        (
            // 〔步 7c 后端剖分 2026-09-19 · C 类〕散文随测试段搬家。
            "tests/backend/relay/upstream_tests.rs",
            "tls_client_config_builds_and_carries_roots",
            1,
        ),
        (
            // 〔2026-09-18〕**这里一度被我改成 2，那是错的** —— 那个 2 是语料根互相
            // 包含造成的**重复计数**假象（`src/bridge/crates` 是 `src` 的子目录）。
            // 根去重之后真数仍是 1。⇒ 看到「盘上比登记多一倍」先怀疑语料面，别改账。
            "src/bridge/crates/acct-core/src/lib.rs",
            "contract_matches_the_backend_implementation",
            1,
        ),
        (
            "src/backend/control/launch_render/wire.rs",
            "local_is_posix",
            1,
        ),
        // 〔US1 · 4D〕`payload.rs` 点 `the_sample_the_monitor_side_builds_parses_into_the_slots_we_expect` 那一行摘了：
        //   那段散文随路由构造口（与它的跨半边样例）一起走了 —— 路由语法进了共享 crate `relay-route-core`。
        ("src/bridge/src/bind.rs", "handle_await_files", 1),
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/byte_cap_registry_tests.rs",
            "handle_alloc_error",
            1,
        ),
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/byte_cap_registry_tests.rs",
            "inline_literal_byte_caps_are_still_just_the_one",
            1,
        ),
        // 〔E2〕`ccm_probe.rs` 那一行出列：探针头注里「本机远端同一条」那段改写了（远端改问那台后端 `ccm-probe`），不再点那条旧判据名。
        // 🔴 〔`设计/50` 09-18〕这两个是 **Codex 自己的 wire 字段名**（仓外：它的
        //    `token_count` 事件里那个 token 用量子对象的键），本仓一处声明都没有 ——
        //    散文里点它们是为了说清「入参长什么样 / 哪些字段刻意不单列」。
        //    ⚠ 它们在改名前住 `crates/usage-core/src/lib.rs`，这张表按路径认键 ⇒ 随改名换住址。
        (
            "src/bridge/crates/codex-token-core/src/lib.rs",
            "reasoning_output_tokens",
            1,
        ),
        (
            "src/bridge/crates/codex-token-core/src/lib.rs",
            "total_token_usage",
            2,
        ),
        (
            // 〔搬树 2026-09-18〕散文随测试段搬家，处数一格没变。
            "tests/backend/footprint/rows_tests.rs",
            "locality_is_derivable_from_destination_today",
            1,
        ),
        // 〔GP1 · 第四波〕这里原来一行 `creds_store_tests.rs` 点 `a_temp_file_is_born_owner_only` 的存量：
        //   那段散文随 `KS5` 两条源码判据（切 monitor 写口的函数体）一起走了 —— 写口去了后端。
        // 〔US1 · 4D〕`creds_store_tests.rs` 点 `the_ui_hands_the_write_command_a_config_dir_not_a_name` 那一行摘了：
        //   那段散文住在「写侧那一行正是起会话那一侧找的那一行」判据的头注里，判据随读者换成后端搬去了后端。
        // 〔MIG-2〕`src/bridge/src/history.rs` · `launch_identity_prefix` 那一行摘了：点它的那句随本机起会话搬进后端一起删了。
        // 〔C4d · 第四波 4B〕`("src/bridge/src/history.rs", "list_history_sessions_in_project", 2)` 那一行摘了：点它的两句（模块头注
        //   「两级懒加载」那一段 · 那条已删命令的头注）随历史清单整轴搬进本机常驻后端一起删了（头注重写成 `〔C4d〕` 那一段）。
        // 〔MIG-2〕`tests/bridge/history_tests.rs` · `relay_key_for` 那一行摘了：点它的那句随本机起会话搬进后端一起删了。
        // 〔US1 · 4D〕`history_tests.rs` 点 `the_two_inputs_at_the_call_site_are_still_the_two_take_points` 那一行摘了：
        //   那段病史散文随注入侧那几条判据整段重写（缝里的两格事实并成「问那台后端」一格）。
        ("src/bridge/src/local_backend_host.rs", "futex_do_wait", 1),
        (
            // 〔搬树 2026-09-18〕散文随测试段搬家，处数一格没变。
            "tests/bridge/local_backend_host_tests.rs",
            "the_two_inputs_at_the_call_site_are_still_the_two_take_points",
            1,
        ),
        // 〔RM1d · 第四波〕`tests/bridge/panorama_tests.rs` 那 1 处 `guard_doc_rel` 的存量行**摘了**：
        //   那条判据改判上游「算」那一层（`edits::plan_*_doc_link` 过 `guard_doc_rel`），测试段代码里
        //   有了这个名字（按名字切函数体），它不再「只活在散文里」。
        // 〔RM1c · 第四波〕`src/bridge/src/panorama.rs` 那 3 处 `symbols_in_file` 的存量行**摘了**：
        //   独立全景小程序（`src/panorama-engine/main.rs`）的 op 表里有了这个名字（代码侧活了），
        //   那几句散文从此不再「只活在散文里」。
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/panorama_seam_registry_tests.rs",
            "panorama_raw_query",
            1,
        ),
        (
            // 〔搬树 2026-09-18〕散文随测试段搬家，处数一格没变。
            "tests/backend/agents/claudecode/parse_tests.rs", // 〔MOD〕随记录解析搬进后端
            "unknown_type_falls_through_to_unknown_variant",
            1,
        ),
        // 🔴 〔`K-R48` 第二拍 09-11〕原来这里登记着 `polling_registry.rs` 里那句提到
        //    `every_comment_stripping_transformer_is_registered` 的散文。那句话住在
        //    `the_identity_poller_is_gone_for_good` 的注释里，而本拍把那条判据的语料
        //    从 `shared/ccm`（bash，剥 `#`）换成了 `control/ccm/`（Rust，剥 `//`）
        //    ⇒ 那段解释连同它一起重写了，名字不再出现。**账跟着删，别留成僵尸行。**
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/polling_registry_tests.rs",
            "the_per_second_identity_poller_spawns_nothing_per_tick",
            1,
        ),
        // 〔AL1 · 2026-09-24〕-1：`profile_installer.rs` 里点这个名字的那段话随
        //    `find_block_range` 一起走了（拼接收成 `fenced_block::splice_in/out` 一份）。
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/rust_timer_registry_tests.rs",
            "the_one_real_ticker",
            1,
        ),
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/bridge/shared_crate_registry_ci_yaml.rs",
            "ci_live_lines",
            1,
        ),
        (
            // 〔搬树 2026-09-18〕散文随测试段搬家，处数一格没变。
            "tests/bridge/ssh_source_write_half_guard.rs",
            "copy_bidirectional_with_sizes",
            1,
        ),
        (
            // 〔搬树 2026-09-18〕散文随测试段搬家，处数一格没变。
            "tests/bridge/ssh_source_emits_parity.rs",
            "hello_commands_match_the_dispatch_table",
            1,
        ),
        // 〔MIG-1〕存量四行摘了：点它们的那几段散文（`ssh_source.rs` 的 tmux 原文账 / 宣告账头注 · `tmux.rs` 的 `list_local_tmux` 头注）
        //   随 monitor 那几本账一起删 / 改写（`list_local_tmux` 改问本机后端），`ssh_source_f032_idle_tests.rs` 那一处随文件删。
        // 〔C4e 批 2〕`tmux_backend_gate_guard_tests.rs` 里 `both_backend_commands_use_this_one_router` 那一行（存量）摘了：
        //   点它的那段头注随「两条命令走同一个分流器」那条判据一起退役（原处换成一块墓碑，那一行登记在 `TOMBSTONED`）。
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/backend/footprint/registry_tests.rs",
            "inbox_id_from_filename",
            1,
        ),
        (
            // 〔步 7c 剖分 2026-09-19〕散文随测试段搬家，处数一格没变。
            "tests/backend/footprint/registry_tests.rs",
            "locality_is_derivable_from_destination_today",
            1,
        ),
        // 〔RW1 · 第四波 09-24〕这里原来有 `verified_write_tests.rs` 里一条存量（一条早删的判据名）：
        //   那句话所在的那一组判据随 `verify_and_rollback` 一起走了（零调用方），那句话也就没了 ⇒ 摘行。
        (
            "tests/agent-profile-parity.vitest.ts",
            "the_gaps_are_named_not_forgotten",
            1,
        ),
        // 〔B2 · 条 66〕原来这里还有一行 `tests/backend-policy.vitest.ts` 点名的那条「启动时推送」接线钉
        //   （1 处）—— 那条推送随值搬家退役，点名它的那段散文与那条判据一起删了 ⇒ 本行摘掉（存量 −1）。
        (
            "src/render-stream-record.ts",
            "queued_user_message_never_enters_the_branch_chain",
            1,
        ),
        (
            "tests/settings/backend-section.vitest.ts",
            "the_unattended_wording_is_actually_present",
            1,
        ),
        // 〔FE1 · 第四波 4D〕`src/tab-session-actions.ts` 那一行（1 处）摘了：那条 tab 栏本机 resume 连同点这个死名的订正注释
        //   收进了 `src/local-resume.ts`（本机 resume 编排只剩一份），订正注释没跟着搬（它订正的那句话已经不在了）⇒ 存量 −1。
        // 🔴 〔搬树 2026-09-18 新增四条〕**不是新长出来的债，是语料面变大了**：
        // 这份 README 原住 `src-tauri/README.md`，而本族的语料根里没有 `src-tauri`
        // ⇒ 它**按构造在射程外**。改名成 `src/bridge/README.md` 之后落进了 `"src"`
        // 那一棵，于是它散文里点名的死名第一次被看见。
        // 现打核实：这四个名字在 `src/` 与 `tests/` 里**定义数都是 0**（真死名）
        // ⇒ 如实记账，而不是给它们编一个「还活着」的说法。
        // 〔C4e · 第四波 4C〕`classify_capture_output` 那一行（README 两处）摘掉：README 里抓屏那两处改写成「迁到界面」，旧名不再点。
        ("src/bridge/README.md", "drain_complete_lines", 1),
        ("src/bridge/README.md", "exec_on_session", 1),
        ("src/bridge/README.md", "plan_file_read", 1),
        // 〔SR1b 子步 3 · 2026-09-24〕同一份 README 那一行（`ssh_source.rs` 的旧功能表）里的跳板函数名：界面侧最后一份
        //   （`inproc_dial.rs` 里 SFTP 用的那一个）随界面进程零 SSH 整份删了，后端那份改了名 ⇒ 真死名，如实记账；
        //   那一行同时点着上面四个死名，不给整行贴墓碑（贴了会把它们一起改记成「墓碑」，账就错了）。
        ("src/bridge/README.md", "connect_via_jump", 1),
        // 🔴 〔搬树 2026-09-18 · `设计/16 §4.2`〕**这里删掉了 6 行**，逐条点名：
        //   `INVARIANTS.md`/`guard-core/src/lib.rs`  `every_monitor_file_strips_clean`
        //   `accounts_query.rs`/`local_backend_host.rs`     `every_comment_stripping_transformer_is_registered`
        //   `tests/backend/readonly_guard.rs`         `the_cfg_test_reexport_detour_stays_extinct`
        //   `local_backend.rs`  `every_position_comparison_over_source_pins_and_bounds_its_anchors`
        //
        //   这六个名字**从来就不是死名** —— 它们是本文件里六条**正在跑的判据**。
        //   它们当年进这张表，唯一的原因是本条的语料面**按构造摘掉了判据自己那一份**
        //   ⇒ 连它的 `fn` 声明一起看不见 ⇒ 别处散文点它们就被读成「点了一个不存在的名字」。
        //   `dead_name_corpus` 这一轮把本文件的**声明那一半**补了回来（登记表字面量与
        //   本文件自己的散文仍然不收）⇒ 这六行是**被修掉的缺陷**，不是被放过的欠账。
    ];

    /// **墓碑登记**：`(仓根相对路径, 名字, 带 [`PROSE_NAME_TOMBSTONE`] 的处数)`。
    ///
    /// 🔴 这张表就是 `PROSE_NAME_TOMBSTONE` 头注里承诺的那道拦逃生舱的闸：
    /// **贴了墓碑不登记 ⇒ 红；登记了盘上没有 ⇒ 也红。**
    /// ⇒ 贴一个墓碑是一次**会被看见的记账**，而 `K4` 逐字要求 PM 自己读 diff。
    ///
    /// 这 6 处全是 `K-R20` 本轮真的改过的**订正段** —— 订正段逐字引用旧名字，
    /// 那正是 `K-R19` 实测到「订正落盘之后尺子读数一动没动」的原因。
    /// 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` C 类〕**12 行换了住址，处数一格没变。**
    /// 那些墓碑散文随各自判据的测试段搬进了 `tests/bridge/`，逐份点名（`src/bridge/src/X.rs` →）：
    /// `capability_registry` · `ccm_cli_contract` · `doc_claim_registry` ·
    /// `parity_ledger`（3 个名字 · 8 处）· `plugin_class_registry`（2 个名字）·
    /// `polling_registry` · `shared_crate_registry` · `tmux_backend_gate_guard` ·
    /// `tool_registry`（→ `tool_registry_environment_tests.rs`）。
    /// 另有**真新增的一条**（退役判据留下的墓碑），挂在本表末尾、单独写了理由。
    const TOMBSTONED: &[(&str, &str, usize)] = &[
        // 〔MIG-3b 续 · 主会话 09-28 裁①〕monitor 自己那台的探针（`with_monitor_probe`）随足迹判定进后端删了；两处散文点旧名。
        ("src/backend/footprint/rows.rs", "with_monitor_probe", 1),
        (
            "src/settings/config-surface-section.ts",
            "config_surface_report",
            1,
        ),
        ("src/backend/footprint/rows.rs", "config_surface_report", 1),
        ("src/ipc/commands.ts", "config_surface_report", 1),
        (
            "tests/bridge/command_home_registry_tests.rs",
            "config_surface_report",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "config_surface_report",
            2,
        ),
        ("tests/ipc/commands.vitest.ts", "config_surface_report", 9), // 那几行增量账里的旧命令名（同一行带墓碑标记）
        (
            "src/settings/footprint-reads.ts",
            "config_surface_report",
            1,
        ),
        (
            "tests/bridge/local_read_surface_registry_tests.rs",
            "with_monitor_probe",
            1,
        ),
        // 〔MIG-1 · `99 §2.1 ⑬`〕monitor 的会话 / tmux 账本（`session_map` · `session_facts` · `tmux_reconcile` · `ssh_source` 里那几本账 ·
        //   本机收割 · monitor 侧观测分类）整体搬进后端会话账本；原处留块墓碑点旧名，讲来历的散文就地挂标记。
        (
            "src/backend/observe/watcher.rs",
            "observation_tokens_double_write_point_stays_in_sync",
            1,
        ),
        (
            "src/backend/stream/wire.rs",
            "removal_cause_wire_literal_stays_in_sync",
            1,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "forget_tmux_raw",
            1,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "local_idle_retirements",
            2,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "local_reaper_state",
            1,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "local_tmux_closed",
            1,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "record_tmux_raw",
            1,
        ),
        (
            "src/bridge/src/backend/control/tmux.rs",
            "classify_tmux_observation",
            1,
        ),
        // 〔MIG-1 续〕`tmux.rs` 里 `tmux_raw_for` 那一行摘了：点它的那段文档随本机列会话那条命令（`list_local_tmux`）整条删了。
        ("src/bridge/src/bind.rs", "is_process_alive", 1),
        ("src/bridge/src/lib.rs", "load_with_changes", 1),
        ("src/bridge/src/spawn_managed.rs", "is_process_alive", 1),
        ("src/bridge/src/ssh_source.rs", "f032_idle_tests", 1),
        ("src/bridge/src/ssh_source.rs", "list_ssh_host_aliases", 1),
        (
            "src/bridge/src/ssh_source.rs",
            "reannounce_after_reconnect",
            1,
        ),
        ("src/bridge/src/ssh_source.rs", "record_tmux_raw", 1),
        (
            "src/bridge/src/ssh_source.rs",
            "tmux_snapshot_exposure_tests",
            1,
        ),
        ("src/doc/INVARIANTS.md", "classify_tmux_observation", 1),
        ("src/doc/INVARIANTS.md", "find_tmux_origin_for_sid", 1),
        ("src/doc/INVARIANTS.md", "is_process_alive", 1),
        (
            "src/doc/INVARIANTS.md",
            "observation_tokens_double_write_point_stays_in_sync",
            1,
        ),
        (
            "src/doc/INVARIANTS.md",
            "remote_idle_single_writer_guard",
            1,
        ),
        ("src/doc/INVARIANTS.md", "tmux_origin_for_sid", 1),
        (
            "src/doc/IPC-PROTOCOL.md",
            "removal_cause_wire_literal_stays_in_sync",
            1,
        ),
        (
            "tests/bridge/backend/control/inbound_client_tests.rs",
            "removal_cause_wire_literal_stays_in_sync",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "classify_tmux_observation",
            1,
        ),
        (
            "tests/bridge/rust_timer_registry_tests.rs",
            "is_process_alive",
            1,
        ),
        (
            "tests/bridge/ssh_source_capped_line_tests.rs",
            "classify_tmux_observation",
            1,
        ),
        (
            "tests/bridge/ssh_source_capped_line_tests.rs",
            "remove_tmux_line",
            1,
        ),
        (
            "tests/bridge/ssh_source_write_half_guard.rs",
            "remote_idle_single_writer_guard",
            1,
        ),
        // 〔MIG-1 · `99 §2.1 ⑯`〕`~/.ssh/config` 导入那三条命令搬进后端；两段现打读数的散文点旧名。
        ("src/bridge/src/parity_ledger.rs", "resolve_ssh_host", 1),
        ("src/bridge/src/spawn_managed.rs", "resolve_ssh_host", 1),
        ("src/bridge/src/ssh_source.rs", "resolve_ssh_host", 1),
        ("src/bridge/src/ssh_source.rs", "import_ssh_hosts", 1),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "the_alias_capabilities_speak_through_one_command_family",
            1,
        ),
        ("src/backend/platform/shell/dialect.rs", "look_on_path", 1), // 〔OSA · V156〕方言随住址搬进 `platform/shell/`
        (
            "src/backend/agents/claudecode/paths.rs",
            "claude_data_fence",
            1,
        ),
        ("src/doc/INVARIANTS.md", "claude_data_fence", 1),
        (
            "tests/bridge/write_site_registry_tests.rs",
            "write_skill_file",
            1,
        ),
        // 〔MIG-3a〕monitor 的 `claude_data_fence` 与收件箱三条随那一面进后端删了：点它们的账目句挂墓碑。
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "claude_data_fence",
            3,
        ),
        ("tests/ipc/commands.vitest.ts", "read_skill_file", 12),
        ("tests/ipc/commands.vitest.ts", "write_skill_file", 12),
        // 〔MIG-3a〕收件箱三条 Tauri 命令随那一面进后端删了：点它们的账目句挂墓碑。
        ("tests/bridge/parity_ledger_tests.rs", "read_skill_file", 2),
        ("tests/bridge/parity_ledger_tests.rs", "write_skill_file", 2),
        // 〔MIG-3b〕它唯一还活着的那句提名（在一个表的字符串里）随任务门面那一行删了，剩下的讲来历的散文挂墓碑。
        (
            "tests/bridge/agent_dispatch_registry_tests.rs",
            "has_record_ext",
            1,
        ),
        ("src/bridge/src/adapter.rs", "has_record_ext", 1),
        ("src/bridge/src/adapter.rs", "session_id_from_path", 1),
        // 〔合并 MIG-3b × 主线 76b31e7c〕远端 `ccm` 探针那几行摘了：主线里 `probe_ccm_cli` 这个名字还活在一份测试替身的字符串里
        //   （`remote-launch-run.vitest.ts`），不算死名 ⇒ 挂在它上面的墓碑不进本表。
        // 〔MIG-3b 续〕公钥推送那条 Tauri 命令退役（本机后端 `pubkey-push`）：点它旧名讲来历的散文挂墓碑。
        ("src/backend/assets/pubkey.rs", "push_public_key", 1),
        ("src/pubkey-push.ts", "push_public_key", 1),
        ("tests/pubkey-push.vitest.ts", "push_public_key", 1),
        ("tests/bridge/parity_ledger_tests.rs", "push_public_key", 3),
        ("src/bridge/src/lib.rs", "push_public_key", 1),
        // 〔MIG-3b〕删会话 · 分叉 · 钩子诊断三件转交退役（界面经通道直说那台后端）：散文里点那几个旧名讲来历的，逐处挂墓碑。
        ("src/README.md", "create_branch_session", 1),
        ("src/bridge/README.md", "delete_remote_history_session", 2),
        (
            "src/bridge/crates/branch-core/src/lib.rs",
            "create_branch_session",
            1,
        ),
        (
            "src/bridge/src/local_origin_registry.rs",
            "create_remote_branch_session",
            1,
        ),
        ("src/doc/ARCHITECTURE.md", "create_branch_session", 2),
        ("src/doc/ARCHITECTURE.md", "create_remote_branch_session", 1),
        ("src/doc/INVARIANTS.md", "delete_remote_history_session", 1),
        ("src/doc/INVARIANTS.md", "validate_delete_target", 1),
        (
            "tests/branch-button.vitest.ts",
            "create_remote_branch_session",
            1,
        ),
        (
            "tests/bridge/backend/control/frame_query_tests.rs",
            "remote_branch_tests",
            1,
        ),
        (
            "tests/bridge/local_origin_registry_tests.rs",
            "create_remote_branch_session",
            2,
        ),
        (
            "tests/bridge/local_read_surface_registry_tests.rs",
            "create_branch_session",
            1,
        ),
        (
            "tests/bridge/origin_tests.rs",
            "create_remote_branch_session",
            1,
        ),
        (
            "tests/bridge/origin_tests.rs",
            "delete_remote_history_session",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "create_remote_branch_session",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "delete_remote_history_session",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "diagnose_remote_cc_bus_hooks",
            2,
        ),
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "delete_remote_history_session",
            2,
        ),
        (
            "tests/ipc/commands.vitest.ts",
            "create_remote_branch_session",
            3,
        ),
        (
            "tests/ipc/commands.vitest.ts",
            "delete_remote_history_session",
            3,
        ),
        (
            "tests/views/history-actions.vitest.ts",
            "delete_remote_history_session",
            1,
        ),
        (
            "tests/bridge/write_site_registry_tests.rs",
            "this_module_never_writes",
            2,
        ),
        // 〔MIG-3a〕skill 装 / 卸三条 Tauri 命令随 D 组进后端删了：点它们的账目句挂墓碑。
        (
            "tests/bridge/backend/control/frame_query_tests.rs",
            "skill_uninstall_apply",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "skill_install_preview",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "skill_uninstall_apply",
            1,
        ),
        // 〔LR2〕U8c-3「前提触发器」改写成只管 `create-or-attach` 两棵树、改了名；两处散文点它的原名讲沿革。
        (
            "src/doc/INVARIANTS.md",
            "the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold",
            1,
        ),
        (
            "tests/backend/control/launch_render/launch_wire_f07_main_path_tests.rs",
            "the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold",
            1,
        ),
        // 〔LOC1b · 第四波 4D〕读一整份会话本机远端合成一条（`history·rs::stream_read_session_jsonl`），远端那一支的函数删了；
        //   点它旧名讲来历的散文逐处挂墓碑。
        ("src/bridge/README.md", "stream_read_remote_session", 2),
        // 〔LOC1b〕monitor `search.rs` 删了 ⇒ 它体里还提着的旧名 `search_remote_all` 全仓只剩散文（remote_history.rs 那块 C4a 墓碑）。
        ("src/doc/INVARIANTS.md", "is_session_active", 1), // 〔LOC1b〕§6 那一格搬去后端
        // 〔LOC1b〕只为本机读盘服务的适配器门面删了；按本机根前缀判种类的那一个只剩散文（换成按文件名形态判）。
        ("src/bridge/src/adapter.rs", "kind_of_path", 1), // 〔MOD〕2 → 1
        (
            "tests/bridge/byte_cap_registry_tests.rs",
            "stream_read_remote_session",
            1,
        ),
        (
            "tests/bridge/drift_ledger_tests.rs",
            "stream_read_remote_session",
            1,
        ),
        (
            "tests/bridge/origin_tests.rs",
            "stream_read_remote_session",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "stream_read_remote_session",
            1,
        ),
        (
            "tests/ipc/commands.vitest.ts",
            "stream_read_remote_session",
            3,
        ),
        // 〔HX2 · 第四波 4D〕写 key 改走通道 `apikey-key-set`：monitor 那一半（Tauri 命令 ＋ `apikey_remote` 写臂）与钉它们的判据退役。
        // 〔MIG-2〕`apikey_remote.rs` 与它的 `_tests.rs` 整删（发送口零调用方）⇒ 住在那两份里的墓碑（本表与 `REGISTERED` 共 16 行）随文件退役：
        //   被守的整轴没了；点那些旧名的别处散文（`creds_store*` · `frame_query_tests`）照留、照登记。
        ("src/bridge/src/creds_store.rs", "write_key_on", 2),
        (
            "tests/bridge/creds_store_tests.rs",
            "a_config_dir_that_names_no_account_is_refused_instead_of_falling_back",
            1,
        ),
        (
            "tests/bridge/creds_store_tests.rs",
            "the_plaintext_argument_is_only_ever_handed_one_hop_further",
            1,
        ),
        ("tests/bridge/creds_store_tests.rs", "write_key_on", 2),
        // 〔C4e 批 3b〕cc-bus 驾驶舱写面五条迁到界面（`src/cc-bus-control.ts`）：monitor 那一份解释（发送端 · 说法 · 形状校验）
        //   与钉它们的判据退役，原处与点它们的散文挂墓碑；后端那一节头注点的 monitor 组合（`broadcast_via_backend`）同拍死了。
        ("src/backend/control/cc_bus.rs", "broadcast_via_backend", 1),
        // 〔C4e 批 2〕杀会话 · 送键 · 就地 resume 三条迁到界面：退役的函数 / 判据名在原处与点它们的散文里挂墓碑。
        // 〔C4e 批 3b〕`cc_bus.rs` · `killed_from_reply` 那一行摘了：点它的那句在收掉那条 Tauri 命令的体里，命令整条迁到界面（整轴退役）。
        (
            "src/bridge/src/backend/control/tmux.rs",
            "no_channel_message",
            1,
        ),
        (
            "src/bridge/src/tmux_backend_gate_guard.rs",
            "kill_now_routes_through_the_backend",
            2,
        ),
        (
            "src/bridge/src/tmux_backend_gate_guard.rs",
            "send_keys_now_routes_through_the_backend",
            2,
        ),
        (
            "tests/bridge/backend/control/backend_kill_tests.rs",
            "the_refusal_wording_matches_the_sibling_command",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_local_kill_never_falls_back_to_ssh",
            1,
        ),
        (
            "tests/bridge/tmux_backend_gate_guard_tests.rs",
            "a_gate_rejection_is_never_laundered_into_the_ssh_fallback",
            1,
        ),
        (
            "tests/bridge/tmux_backend_gate_guard_tests.rs",
            "both_commands_branch_on_the_same_three_way_verdict",
            1,
        ),
        (
            "tests/bridge/tmux_backend_gate_guard_tests.rs",
            "kill_now_routes_through_the_backend",
            1,
        ),
        (
            "tests/bridge/tmux_backend_gate_guard_tests.rs",
            "send_keys_now_routes_through_the_backend",
            1,
        ),
        // 〔C4e · 第四波 4C〕抓屏整条迁到界面：monitor 那一份说法（五档人话）与 `KR112D2` 的两刀机检 ＋ 五档那条随之退役，原处挂墓碑。
        (
            "src/bridge/src/backend/control/tmux.rs",
            "describe_capture_refusal",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "describe_capture_refusal",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_capture_path_asks_the_backend_instead_of_composing_a_shell_line",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_five_capture_refusals_stay_apart",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_local_capture_is_no_longer_a_dead_end",
            1,
        ),
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "tmux_shell_line_markers",
            1,
        ),
        //   抓屏的参数构造器随之删了，它的跨轨对拍一起退役（〔C4e 批 2〕`command_args.rs` 整份随后续的送键 / 就地 resume 删掉，
        //   那一行墓碑跟着文件走了）。
        // 〔LOC1a · 第四波 4D〕本机四个一次性 exec 改走 `<local>` 长连接：一次性查询的宿主三格（`local_backend_one_shot_query`）·
        //   本机「老后端」的 stderr 认法（`local_failure_kind`）· 分叉 exec 那一趟的结果解释（`interpret_fork_exec`）随路删；
        //   讲来历的散文逐字点旧名。
        (
            "src/backend/control/fork_write.rs",
            "interpret_fork_exec",
            1,
        ),
        // 〔MIG-3b〕`history.rs` · `remote_branch.rs` 那两行摘了：前者那段点它的来历随删会话 / 分叉转交一起删了，后者整份删了。
        ("src/doc/INVARIANTS.md", "interpret_fork_exec", 1),
        (
            "src/bridge/src/cross_half_edge_registry.rs",
            "local_failure_kind",
            1,
        ),
        //   〔LOC1a〕同拍摘三行：`remote_branch.rs` 的 `write_branch_file` / `is_old_backend_hello` 与 `subagent.rs` 的 `run_list_query`
        //   —— 那两份头注整段按今天的形状重写（分叉 / 子 agent 都只剩帧命令一条路），讲旧 exec 路的那几句连同旧名一起没了；
        //   来历今天由新头注里的 `local_query` / `interpret_fork_exec` 两块墓碑接着讲。
        (
            "src/bridge/src/spawn_managed.rs",
            "local_backend_one_shot_query",
            1,
        ),
        (
            "tests/bridge/spawn_managed_exit_sites.rs",
            "local_backend_one_shot_query",
            1,
        ),
        // 〔TL1 · 4C〕代装 rc 那一行 source 的那一跳退役（`设计/71 §6.1`：接上别名文件的那一行只住别名块里）；
        //   两处散文逐字点它讲来历（「配对之后怎么拼」写过三份那一段 · 写规则只有一个住址那条的死值验说明）。
        (
            "src/backend/assets/aliases/fence.rs",
            "ensure_rc_source_line",
            1,
        ),
        (
            "tests/backend/assets/aliases/fence_tests.rs",
            "ensure_rc_source_line",
            1,
        ),
        // 〔CF2 · 第四波 4B〕重放缓冲分档取消，读数里「只留尾巴的会话数」那个字段改名 `sessions`；新字段的文档点旧名讲来历。
        ("src/bridge/src/event_replay.rs", "tail_only_sessions", 1),
        //   头注「容量」那一段讲分档的来历，点原来那个登记方法名。
        ("src/bridge/src/event_replay.rs", "keep_tail_only", 1),
        // 〔CF2〕F5 那份重放换成就绪点（`ready_point`）：头注表里 ＋ 新函数的文档里点旧名讲来历。
        ("src/bridge/src/event_replay.rs", "replay_and_mark_ready", 2),
        // 〔GP1 · 第四波〕F5 对账要按机器分已结束 / 说不清 ⇒ 远端 sid 清单连同 origin 一起交、改名；新函数文档点旧名讲来历。
        // 〔MIG-1 · ⑬〕那一行退役：`buffered_*` 整族随 F5 对账并进会话簿（`lifecycle_replay`）删掉，讲来历的那段文档一起走。
        // 〔GP1 · 第四波〕本机凭据文件的写者换成本机常驻后端：monitor 那侧写口（`write_key_at` · `check_base_url`）与
        //   「本机那一臂不发帧」那条判据退役；讲来历的散文与搬去后端的三条判据的出处各挂一块。
        ("src/bridge/src/creds_store.rs", "check_base_url", 1),
        // 〔US1 · 第四波 4D〕上游选择那半从 monitor 搬走（读侧 · 人群 · 中转在不在），点旧名的散文挂墓碑。
        (
            "src/backend/accounts/upstream/file_face.rs",
            "apikey_rows_at",
            1,
        ),
        ("src/bridge/src/creds_store.rs", "read_status_at", 1),
        (
            "src/bridge/src/local_backend_host.rs",
            "relay_listening_at",
            1,
        ),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "apikey_rows_at",
            1,
        ),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "what_the_write_side_wrote_is_exactly_the_row_the_launch_side_looks_for",
            1,
        ),
        ("tests/bridge/creds_store_tests.rs", "read_status_at", 1),
        (
            "tests/bridge/creds_store_tests.rs",
            "what_the_write_side_wrote_is_exactly_the_row_the_launch_side_looks_for",
            1,
        ),
        (
            "tests/bridge/local_backend_host_tests.rs",
            "relay_running_really_asks_the_loopback_port",
            1,
        ),
        ("src/bridge/src/creds_store.rs", "write_key_at", 1),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "write_key_at",
            1,
        ),
        ("tests/backend/relay/server_tests.rs", "write_key_at", 1),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "a_program_write_keeps_everything_the_human_put_there",
            1,
        ),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "a_saved_key_does_not_swallow_the_hand_written_upstream_or_auth_style",
            1,
        ),
        (
            "tests/backend/accounts/upstream/file_face_tests.rs",
            "the_write_side_no_longer_targets_the_legacy_top_level_slot",
            1,
        ),
        // 🔴 〔RM1f · V108 后半句〕monitor 摘掉内嵌引擎：`panorama.rs` 与它的判据文件删了，三处散文里的旧名挂墓碑。
        ("src/panorama-engine/main.rs", "collect_symbols_in_file", 1),
        (
            "tests/bridge/panorama_seam_registry_tests.rs",
            "panorama_diagram_kinds",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "panorama_diagram_kinds",
            1,
        ),
        (
            "tests/backend/panorama_locus_guard.rs",
            "the_monitor_tree_keeps_exactly_one_parse_entrance",
            1,
        ),
        (
            "tests/bridge/plugin_class_registry_tests.rs",
            "code_picture_is_compiled_into_the_monitor_and_absent_from_the_backend",
            1,
        ),
        (
            "tests/panorama/diagram-guards.vitest.ts",
            "the_diagram_commands_pass_the_upstream_through_untouched",
            1,
        ),
        // 〔S5 · 第四波 · V41〕`parse_remote_hosts` 旧单对象那一支删了，守它的测试随之删；
        //    接替它的判据头注逐字说那条旧测试守的是什么 ⇒ 第②条出路。
        (
            "tests/bridge/lib_remote_config_tests.rs",
            "legacy_single_object_one_host",
            2, // 〔合并 JA1〕+1：JA1 点址那一行（它当时登记「待主会话裁」，本路裁完挂墓碑）
        ),
        // 🔴 〔C4b · 第四波 4B〕插件市场那条命令改走通道，monitor `plugins.rs` 连同它的形状收口删了；
        //    收口搬到了界面那一侧（`decodeSurvey`），那一节的标题逐字记着它从哪搬来 ⇒ 第②条出路。
        (
            "tests/settings/plugins-section.vitest.ts",
            "parse_survey_lines",
            1,
        ),
        // 🔴 〔C4b · 第四波 4B〕`accounts.ts` 那个 `"__local__"` 合进 `LOCAL_ORIGIN`（`设计/00 §2.5 ①`），
        //    钉「两者刻意不同」的那条判据随之改成钉合了之后的形状；TS 那一节点它旧名讲来历 ⇒ 第②条出路。
        (
            "tests/ipc/commands.vitest.ts",
            "the_two_same_named_local_origin_constants_stay_deliberately_different",
            1,
        ),
        (
            "tests/bridge/backend_policy_tests.rs",
            "the_two_same_named_local_origin_constants_stay_deliberately_different",
            1,
        ),
        // 〔SR1b 子步 3 · 2026-09-24〕界面进程零 SSH：`inproc_dial.rs`（SFTP 那一份拨号）整份删了 ⇒ 从界面侧搬去后端的
        //   几个旧函数名（跳板 · agent 鉴权）全仓只剩散文；`sftp_move_ledger`（SFTP 14 处拨号的挡路石底账）随事做完退役。
        ("src/backend/dial/connect.rs", "connect_via_jump", 1),
        ("src/backend/dial/connect.rs", "authenticate_via_agent", 1),
        (
            "src/backend/platform/ssh_agent.rs",
            "authenticate_via_agent",
            1,
        ),
        (
            "src/bridge/src/backend/control/mod.rs",
            "sftp_move_ledger",
            1,
        ),
        ("src/bridge/src/lib.rs", "sftp_move_ledger", 1),
        (
            "tests/bridge/local_origin_registry_tests.rs",
            "connect_via_jump",
            2,
        ),
        (
            "tests/backend/footprint/registry_tests.rs",
            "sftp_move_ledger",
            1,
        ),
        // 〔SR1b 子步 3〕「russh 只住 SFTP 还要它的地方」那条判据改名成零命中（`russh_is_named_nowhere_in_the_monitor_crate`）。
        (
            "tests/bridge/ssh_source_dial_move_judge.rs",
            "russh_lives_only_where_sftp_still_needs_it",
            1,
        ),
        // 〔SR1b · 2026-09-24〕读回比对的判定改成吃后端交回的事实（`verify_readback`），creds-core 那段历史引用旧名留墓碑。
        (
            "src/bridge/crates/creds-core/src/lib.rs",
            "verify_uploaded_bytes",
            1,
        ),
        // 〔SR1b · 2026-09-24〕V89「SFTP 进本机常驻后端，只写暂存区」之后，后端**有**远端写了（只在 `dial/sftp.rs`、
        //   只许两处）⇒ 「今天一处远端写都没有」那条判据换成两条相等（`remote_write_layer`），旧名留墓碑说它为什么不在了。
        (
            "tests/backend/readonly_guard.rs",
            "the_backend_tree_has_no_remote_write_today_and_the_scan_face_is_not_empty",
            1,
        ),
        // 🔴 〔RL1 · 第四波 · 2026-09-24〕中转并进本机常驻后端（V107），monitor 另起中转那一族删了；
        //    中转名字登记表里那一行注释逐字记着「哪两个名字被谁接替」⇒ 第②条出路。
        (
            "tests/naming/account-vs-relay-naming.vitest.ts",
            "start_local_relay",
            1,
        ),
        // 〔SR1a · 2026-09-24〕`--dial` 那条分派臂删了，守它「接得到」的判据随入口换成链路四条而改名。
        (
            "tests/backend/main_argv_table_guard.rs",
            "the_dial_arm_is_actually_wired_into_the_dispatch",
            1,
        ),
        // 🔴 〔C4a · 第四波 · 2026-09-24〕远端全文搜索的合并搬去了前端（`views/history-search.ts::mergeSearchResults`），
        //    Rust 那一份删了；后端那句病史（「收口前它逐字 `truncated: local.truncated`」）说的正是它为什么被改 ⇒ 第②条出路。
        (
            "src/backend/observe/search_query.rs",
            "merge_search_results",
            1,
        ),
        // 🔴 〔F9c · 第四波 · 2026-09-24〕存盘装不进一行的改走暂存区分块之后，「打开即只读」一档与它那两句话一起删了；
        //    `fonts.rs` 的探针来路里逐字记着那两句当初带进来的九个字 ⇒ 第②条出路：贴墓碑 ＋ 记账。
        ("src/bridge/src/filewin/fonts.rs", "too_big_to_save", 1),
        ("src/bridge/src/filewin/fonts.rs", "read_only_notice", 1),
        // 🔴 〔C2 · 2026-09-24〕拨号搬进后端的拨号代理之后，界面侧 `K-P6b` 那一版的三条判据随它们守的东西一起删了：
        //    回落登记（回落删了，`D11`）· 请求行按蛇形键写（请求改由宿主 `dial_host` 造，判据搬去那边且改成与后端异源）·
        //    代理只从两处解析（解析多了「自释放那一份」一处）。留下的那几句说的正是「它们为什么不在了」⇒ 第②条出路。
        (
            "tests/bridge/ssh_source_dial_move_judge.rs",
            "every_registered_fallback_is_actually_decided_in_the_entry",
            1,
        ),
        (
            "tests/bridge/ssh_source_dial_move_judge.rs",
            "the_request_line_is_written_with_snake_case_keys",
            1,
        ),
        (
            "tests/bridge/ssh_source_dial_move_judge.rs",
            "the_proxy_is_resolved_from_exactly_two_places_and_never_from_home",
            1,
        ),
        // 🔴 〔`C1` · 2026-09-24〕快照改走长连接（`history-tail` 给那张图）之后，
        //    解析 `--read-session-tail` 首行 meta 的那个函数与它的两条判据一起删了；
        //    留下的那一句说的正是「它为什么不在了」⇒ 第②条出路：贴墓碑 ＋ 记账。
        (
            "tests/bridge/ssh_source_snapshot_tail_tests.rs",
            "parse_snapshot_meta",
            1,
        ),
        // 🔴 〔步 8 · 归属 2026-09-19〕**三处，同一件事**：`inbound_client.rs` 真的挪进
        //    `backend/control/` 了 ⇒ 那张「表外但归这一半」的登记表（`EXTRA_BACKEND_FILES`）
        //    与它的僵尸检查（`the_extra_backend_files_are_not_ghosts`）**一起删掉**——
        //    那是那张表自己那条判据逐字给的指示（「表空了 …… 连这条一起删」）。
        //    留下的散文说的正是「它们为什么不在了」，走第②条出路：贴墓碑 ＋ 记账。
        //    ⚠ 只登记这一条：`EXTRA_BACKEND_FILES` 是**全大写**，不合本族的死名形状
        //    （`is_dead_name_shape` 只认全小写 snake_case）⇒ 它不进这张表，
        //    那两处改成不带 `文件.rs::` 前缀的写法，避开另一条「符号地址还解析得了吗」。
        (
            "tests/bridge/backend_tests.rs",
            "the_extra_backend_files_are_not_ghosts",
            1,
        ),
        // 🔴 〔`24e` 第七刀 09-21〕`entry_tests` 那条判据**改了措辞**（不是删了）：
        //    旧名断的是「空路径 ⇒ 报错里含『路径是空的』」——**那钉的是机制不是性质**。
        //    第七刀把空路径的意思改成「开在远端 home」（去问 `sftp_realpath` 那个唯一权威，
        //    不在入口里猜），于是它改名成 `..._asks_the_remote_for_home_and_opens_nothing_when_it_cannot`，
        //    断的换成更强的一件。**旧名逐字留着**是为了说清「那条理由被满足了，不是被推翻了」
        //    ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        // 〔MIG-3a · 主会话 09-28 裁 3〕上面那条（`an_empty_path_is_refused…` 的旧名）连同改名后的判据一起退役：开窗前那两问进了窗口进程，
        //   两条行为判据搬去 `proc_tests`；原处留一块墓碑点两个旧名。`entry.rs` 那三个 monitor 侧函数同理。
        (
            "tests/bridge/filewin/entry_tests.rs",
            "an_empty_path_asks_the_remote_for_home_and_opens_nothing_when_it_cannot",
            1,
        ),
        (
            "tests/bridge/filewin/entry_tests.rs",
            "a_directory_we_cannot_list_is_an_error_not_a_blank_window",
            1,
        ),
        ("src/bridge/src/filewin/entry.rs", "list_first_screen", 2),
        // 〔MIG-3a · 09-28 预裁〕cc-acct-iso 部署命令退役：它独用的受管路径谓词与守它的两条判据删了，点旧名的散文挂墓碑。
        ("src/bridge/src/sftp.rs", "is_safe_remote_managed_path", 1),
        // 〔MIG-3a · 09-28 预裁〕`deploy_remote_acct_iso` 那条 Tauri 命令（与守它的判据文件 `acct_iso_deploy_tests`）删了：点旧名的散文挂墓碑。
        ("src/accounts.ts", "deploy_remote_acct_iso", 1),
        ("src/acct-iso-reads.ts", "deploy_remote_acct_iso", 1),
        ("src/doc/IPC-PROTOCOL.md", "deploy_remote_acct_iso", 1),
        ("src/ipc/commands.ts", "deploy_remote_acct_iso", 1),
        (
            "tests/bridge/backend/control/frame_query_tests.rs",
            "acct_iso_deploy_tests",
            1,
        ),
        (
            "tests/bridge/command_home_registry_tests.rs",
            "deploy_remote_acct_iso",
            1,
        ),
        (
            "tests/bridge/launcher_identity_registry_tests.rs",
            "deploy_remote_acct_iso",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "deploy_remote_acct_iso",
            6,
        ),
        (
            "src/backend/assets/acct_iso_install.rs",
            "deploy_remote_acct_iso",
            1,
        ),
        (
            "src/settings/accounts-section.ts",
            "deploy_remote_acct_iso",
            1,
        ),
        ("tests/ipc/commands.vitest.ts", "deploy_remote_acct_iso", 3),
        ("tests/bridge/sftp_tests.rs", "deploy_remote_acct_iso", 2),
        (
            "tests/bridge/sftp_tests.rs",
            "deploy_decision_truth_table",
            1,
        ),
        (
            "tests/bridge/sftp_tests.rs",
            "safe_managed_path_requires_a_marker",
            1,
        ),
        // 🔴〔本机侧退役 2026-09-23〕用户裁「本地不需要文件管理器」，而那条裁决与
        //    `INVARIANTS §40 追加` 的「天然不对称白名单」第一条逐字一致
        //    ⇒ 文件管理器的本机侧整条删了。下面**三条判据随它一起走**，
        //    墓碑正文（存在过什么 · 谁裁的 · 白名单原文 · 买不到什么）住
        //    `src/bridge/src/filewin/source.rs` 的头注。
        //    ⚠ 点名的理由是「让随它们走掉的**检出力**有一份可读的账」——
        //      这三条各钉一件不重复的事（阴性对照 · 同真同假 · 整条执行链）。
        //      同轮还删了六条，那六条**刻意没点名**（名字里逐字带着 `the_local_side_…`，
        //      点出来只是把同一句话说六遍）⇒ 它们不在这张表里，如实登记这个口径。
        (
            "src/bridge/src/filewin/source.rs",
            "a_window_that_started_local_has_nowhere_to_go_back_to",
            1,
        ),
        (
            "src/bridge/src/filewin/source.rs",
            "the_button_shows_up_exactly_when_the_jump_would_work",
            1,
        ),
        (
            "src/bridge/src/filewin/source.rs",
            "a_real_click_on_the_go_back_button_walks_the_whole_chain",
            1,
        ),
        // 🔴 同一轮：`parent_dir` 从「吃 `&Source`、两侧两个算法」收成「吃一条字符串」
        //    ⇒ 那条判据的名字里「本机那一支」那半不成立了，改了名。旧名逐字留着。
        (
            "tests/bridge/filewin/source_tests.rs",
            "walking_up_uses_slashes_on_the_remote_side_and_the_platform_on_the_local_side",
            1,
        ),
        // 🔴 〔`K-R48` 第二拍 09-11〕下面这 9 行全是同一件事的账：`shared/ccm` 那个 bash
        //    脚本与它那一族判据删了（`K33`：「不要有什么 bash 脚本」），而**散文里那几处
        //    点名它们的句子留着是有用的**（它们说的正是「这个东西为什么不在了」）
        //    ⇒ 按第②条出路走：加 `PROSE_NAME_TOMBSTONE` 标记 ＋ 在这里记一笔账。
        (
            "src/doc/IPC-PROTOCOL.md",
            "the_local_launch_recipe_is_reachable_only_from_print",
            1,
        ),
        ("src/bridge/src/launch.rs", "resolve_from_backend", 1),
        // 🔴 〔`K-R97` 09-12〕`list_history_projects` 改问本机后端要 `--list-projects`
        //    ⇒ 项目级那一段（`analyze_project_dir`）连同它唯一的调用点一起删了。
        // 〔C4d · 第四波 4B〕那一行（`history.rs` · `analyze_project_dir` · 1）摘了：挂那块墓碑的是 `list_history_projects` 的头注，
        //   而那条命令本身随历史清单整轴搬进本机常驻后端删了 —— 被守的那件事（「本机不自己遍历记录树」）整轴退役，不是删线索。
        // 🔴 〔`K-R65` 09-11〕`tier` 从手填改成派生之后，那条断言「手写那一半的档
        //    不许是 `AppInstalls`」在算术上不可能再红 ⇒ 删掉判据、留下墓碑说清
        //    「它守的那件事没丢，只是那个能填错的格子没有了」。
        (
            "tests/backend/footprint/registry_environment_tests.rs",
            "a_hand_written_entry_is_never_app_installs",
            1,
        ),
        (
            "tests/bridge/plugin_class_registry_tests.rs",
            "ccm_agent_arms",
            1,
        ),
        (
            "tests/bridge/plugin_class_registry_tests.rs",
            "ccm_probe_values",
            1,
        ),
        (
            "tests/bridge/polling_registry_tests.rs",
            "ccm_fails_loudly_when_no_backend_can_be_found",
            1,
        ),
        // 🔴 〔AL1 · 2026-09-24〕`设计/71 §12.5` 那一拍：「备份 → 写 → 回读 → 回滚」收成
        //    `fenced_block::apply` 一份，拼接收成 `fenced_block::splice_in/out` 一份 ⇒
        //    本机那份配对 helper 与远端那份回滚措辞 helper 连同它的判据一起走了。
        //    散文留着说「它们为什么不在了」（措辞的新住址 `fenced_block::undo_note`）。
        //    ⚠ `sftp.rs` 那块墓碑点的 `rollback_note` 不进本表：死名普查不把它算作死名
        //    （它是 `…_matches_what_actually_happened` 的前缀），标记只记在 `REGISTERED` 里。
        ("src/backend/assets/aliases/fence.rs", "find_block_range", 1),
        // 〔RW1 · 第四波 09-24〕Windows 上「替换保住 explicit ACE」那条判据随写搬到后端
        //   （`files_write_tests.rs::put_keeps_explicit_acl_entries_on_windows`），monitor 那边留墓碑、后端那边说来历。
        (
            "tests/backend/assets/aliases/block_tests.rs",
            "install_preserves_explicit_acl_entries",
            1,
        ),
        // 〔RW1 · 第四波 09-24〕本机用户文件的原子写原语随「用户文件改经后端写」删了（`$PROFILE` / `.mcp.json`）。
        ("src/bridge/README.md", "atomic_write_string", 1),
        // 〔RW1 · 第四波 09-24〕本机分叉那份实现交给后端之后删掉的两个函数名。
        ("tests/bridge/history_tests.rs", "read_jsonl_values", 1),
        // 〔RW1 · 第四波 09-24〕远端删会话那道结构守卫（F11 改经远端后端删，`files-delete-session` 只收 sid）。
        (
            "src/backend/agents/claudecode/paths.rs",
            "is_safe_remote_jsonl",
            1,
        ),
        // 〔FN1 · 第四波 4C · V119〕后端那份会话形状判定不再是写侧围栏，改名成它真在答的那一问（`is_session_record_*`）；
        //   新名字的文档点两个旧名讲来历（同一行、同一块墓碑）。
        (
            "src/backend/agents/claudecode/paths.rs",
            "is_protected_session_file",
            1,
        ),
        (
            "src/backend/agents/claudecode/paths.rs",
            "is_protected_session_path",
            1,
        ),
        ("src/bridge/README.md", "is_safe_remote_jsonl", 1),
        ("src/bridge/src/sftp.rs", "is_safe_remote_jsonl", 1),
        ("src/doc/INVARIANTS.md", "is_safe_remote_jsonl", 1),
        (
            "tests/bridge/comm_boundary_registry_tests.rs",
            "is_safe_remote_jsonl",
            1,
        ),
        (
            "src/backend/assets/aliases/block.rs",
            "atomic_write_string",
            2,
        ),
        (
            "tests/bridge/write_site_registry_tests.rs",
            "atomic_write_string",
            1,
        ),
        (
            "tests/backend/control/files_write_tests.rs",
            "install_preserves_explicit_acl_entries",
            1,
        ),
        // 〔AL1 · 2026-09-24 · 子步 4〕`write_account_aliases`（`lines` ＋ `dryRun`）退役，拆成两跳 ＋ 读回口。
        (
            "tests/bridge/parity_ledger_tests.rs",
            "write_account_aliases",
            3,
        ),
        (
            "tests/backend/assets/aliases/aliases_tests.rs",
            "validate_alias_line",
            1,
        ),
        (
            "tests/generated-boundary-guard.vitest.ts",
            "write_account_aliases",
            1,
        ),
        (
            "tests/settings/panel-deferred-io.vitest.ts",
            "write_account_aliases",
            1,
        ),
        // 〔AL1d · 第四波 4B〕「终端集成」那五条命令并进 `aliases_*` 同一族命令面（`调研/第四波记录/AL1d.md §2.1`）；
        //   `$PROFILE` 第二份认法 ＋ 它的遗留扫描随「候选只有一份来历」删了（`§2.3`）。散文留着说它们去了哪。
        (
            "src/backend/assets/aliases/block.rs",
            "legacy_profile_paths",
            1,
        ),
        (
            "src/backend/assets/aliases/block.rs",
            "scan_legacy_profiles",
            3,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "cc_integration_preview",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "cc_integration_status",
            1,
        ),
        (
            "tests/backend/assets/aliases/block_tests.rs",
            "cc_integration_install",
            1,
        ),
        (
            "tests/backend/assets/aliases/block_tests.rs",
            "cc_integration_scan_path",
            1,
        ),
        (
            "tests/backend/assets/aliases/block_tests.rs",
            "cc_integration_uninstall",
            1,
        ),
        (
            "tests/backend/assets/aliases/block_tests.rs",
            "scan_legacy_profiles",
            1,
        ),
        // 〔MC1 · 2026-09-24〕远端「装/卸 ccm 助手」两条命令改名成 `install_remote_alias_block` /
        //    `uninstall_remote_alias_block`（推入口那一半并进 `deploy_remote_backend`，`设计/71 §13.3`）。
        ("src/bridge/README.md", "install_remote_ccm_helper", 2),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "install_remote_ccm_helper",
            1,
        ),
        // 〔W5-ALIAS · 第五波先行〕别名块两条命令连同头注从 `sftp.rs` 搬进 `profile_installer.rs`：
        //    头注里那两块墓碑（`install_remote_ccm_helper` · `uninstall_remote_ccm_helper` 各一）跟着搬 ⇒
        //    `profile_installer.rs` 的 `install_…` 1 → 2、`uninstall_…` 0 → 1；`sftp.rs` 的 `install_…` 2 → 1、`uninstall_…` 1 → 0（行删）。
        // 〔AL2 · 第四波 4D〕远端装 / 卸那两条命令删了，头注里那两块墓碑随之走：`install_…` 2 → 1、`uninstall_…` 1 → 0（行删）。
        (
            "src/backend/assets/aliases/block.rs",
            "install_remote_ccm_helper",
            1,
        ),
        ("src/bridge/src/sftp.rs", "install_remote_ccm_helper", 1),
        // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕远端 profile 读取器（`interpret_profile_read` · `read_profile_text`）
        //    随 `RemoteFile` 落点删了（只剩远端 `ccm` 入口一个用户，那一处改走 `upload_verified`）；记事与判据墓碑里点着它们。
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "read_profile_text",
            2,
        ),
        ("tests/bridge/sftp_tests.rs", "interpret_profile_read", 1),
        ("tests/bridge/sftp_tests.rs", "read_profile_text", 1),
        // 〔E2 · V28〕local_backend_tests.rs 那一行（`remote_shim_carries_the_entry_name_for_the_container_path`）出列：
        //    它挂在 MC1 那条 shim 判据的头注里，远端 shim 本身删了（落点就是后端字节），那条判据连同头注整轴退役。
        // 〔W5-ALIAS〕`verified_write.rs` 那一行（`install_remote_ccm_helper` ×2）随整份模块删了（整轴退役）。
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "install_remote_ccm_helper",
            1,
        ),
        (
            "tests/bridge/sftp_tests.rs",
            "rollback_note_matches_what_actually_happened",
            1,
        ),
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/sftp_tests.rs",
            "ccm_cli_strength_is_at_or_above_baseline",
            1,
        ),
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/sftp_tests.rs",
            "pin_t_def",
            1,
        ),
        // 🔴 〔`K-R70` 09-12〕又一形同族的账：那个「见证型布尔」`id_from_manifest`
        //    与守着它的判据一起删了 —— 它见证的是**一份旁挂清单在不在**，而清单是
        //    `release.yml` 从源码常量抠出来写的标签（三个载体恒等 ⇒ 零证据，
        //    `K-R68` · `DECISIONS.md#R26` 裁定零）。守的动作对、守的东西错。
        //    接替它的是 `the_embedded_identity_comes_from_the_bytes_not_from_a_label`
        //    ＋ 部署路上无条件跑的 `bytes_carry_build_stamp`。
        //    散文里那一句留着才说得清「为什么换掉它，而不是把它写得更严」。
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/sftp_tests.rs",
            "the_identity_witness_is_derived_from_the_manifest_not_written_by_hand",
            1,
        ),
        // 🔴 〔`K-R59` 09-11〕同一形，另一件事的账：`daemonless` 那一档整格删了
        //    （定框 `K35`：「不要有 daemonless。没有没有后端的情况。」），
        //    而 08-14 立的那条前提触发器 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`
        //    **是设计好要在这一天主动红的** ⇒ 散文里那几处点名它的句子留着才说得清
        //    「它红过、红完之后换了谁」。按第②条出路：贴墓碑 ＋ 在这里记一笔账。
        (
            "src/doc/INVARIANTS.md",
            "the_daemonless_remote_still_needs_the_ts_fallback_renderer",
            1,
        ),
        // 〔LR2 2026-09-25〕`launch_wire_f07_main_path_tests.rs` 里点
        //   `the_daemonless_remote_still_needs_the_ts_fallback_renderer` 的两处墓碑删了：它们住在
        //   「那份换人手续」与「U8c-3 前提触发器」两段头注里，那两条判据随 TS 兜底一族整条退役（被守的事整轴没了）。
        // 🔴 〔`K-R96` 09-12〕**这一行删了 —— 盘上那句墓碑随被墓碑的那件事一起走了。**
        //
        // 它盖的是 `plan.rs::Container::avoid_collision` 的头注里那句
        //「旧 `shared/ccm::avoid_name_collision`（`K-R48` 已删）那句
        //  `[ "$do_print" != 1 ] && tmux has-session …` 逐字就是这个意思」——
        // 用来说明「`--print` 不查实时状态」这条行为是从哪继承来的。
        // 用户 09-12 `R52` 裁定二之后**那条行为本身没了**（退让搬进 `plan::build`、
        // 问同一张会话快照，`--print` 与真跑吐同一个名字）⇒ 连带那个字段与那句头注
        // 一起删。⇒ 按本表头注那条纪律：「登记的那一处盘上已经没有了 ⇒ 删掉它，
        // 别让表替真判据挡枪」。
        // 〔LR2 2026-09-25〕同上：点 `ccm_reaches_the_backend_through_one_shot_subcommands` 的那处墓碑住在
        //   U8c-3 前提触发器的头注里，那条判据改写成只管 `create-or-attach` 两棵树之后，那段头注整段删了。
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/backend/control/launch_render/launch_wire_f07_main_path_tests.rs",
            "launch_via_backend",
            1,
        ),
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "resolve_from_backend",
            1,
        ),
        ("src/bridge/src/ccm_cli_contract.rs", "pin_t_def", 1),
        ("src/bridge/src/ccm_cli_contract.rs", "scan_t_targets", 1),
        (
            "tests/bridge/ccm_cli_contract_tests.rs",
            "the_avoidance_lives_in_ccm_now",
            1,
        ),
        (
            "src/doc/IPC-PROTOCOL.md",
            "handlers_never_run_on_the_reader_task",
            1,
        ),
        (
            "src/doc/IPC-PROTOCOL.md",
            "hello_commands_match_the_dispatch_table",
            1,
        ),
        (
            "src/doc/IPC-PROTOCOL.md",
            "hello_is_flushed_before_the_inbound_reader_starts",
            1,
        ),
        (
            "src/backend/stream/wire.rs",
            "hello_bytes_for_claude_are_frozen",
            1,
        ),
        (
            "src/bridge/src/lib.rs",
            "the_ui_never_derives_the_account_id_itself",
            1,
        ),
        (
            "src/settings/accounts-section.ts",
            "the_ui_never_derives_the_account_id_itself",
            1,
        ),
        // 🔴 〔`K-R72` 09-12〕下面这一批是同一件事的账：**送键与杀会话那两条桌面侧
        //    SSH 回落删净了**（`K-R54` 逐处裁定表第 1 · 2 · 5 处），随之走掉的三个
        //    生产符号（`build_kill_session_cmd` / `build_send_keys_remote_cmd` /
        //    `gate_guard_expr`）与两条判据在散文里被逐字点着 —— 而那些句子说的正是
        //    **「这个东西为什么不在了 / 它守的性质今天住哪」**，删掉的是线索不是病。
        //    ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        ("src/doc/INVARIANTS.md", "build_kill_session_cmd", 1),
        ("src/doc/INVARIANTS.md", "build_send_keys_remote_cmd", 1),
        ("src/doc/INVARIANTS.md", "gate_guard_expr", 1),
        (
            "src/bridge/src/backend/control/tmux.rs",
            "build_kill_session_cmd",
            2,
        ),
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/backend/control/tmux_tests.rs",
            "build_kill_session_cmd",
            1,
        ),
        (
            "src/bridge/src/backend/control/tmux.rs",
            "build_send_keys_remote_cmd",
            2,
        ),
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/backend/control/tmux_tests.rs",
            "build_send_keys_remote_cmd",
            1,
        ),
        (
            "src/bridge/src/backend/control/tmux.rs",
            "gate_guard_expr",
            1,
        ),
        (
            "tests/bridge/tmux_backend_gate_guard_tests.rs",
            "build_kill_session_cmd",
            1,
        ),
        // 🔴 〔`K-R104` 09-13〕两条**新墓碑**：编排搬上后端帧面之后，两句订正段各逐字
        //    引用了一个已经不在的名字来说明「它为什么不在了」——
        //    `doc_claim_registry.rs` 那条判据改了名（旧名说的是「四个都还在」，
        //    而今天四个里退役了一个）· `src/account-usage.ts` 那句讲的是
        //    「两条路此前靠同一个命令构造器同源，今天连命令串都不存在了」。
        //    按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        (
            "tests/bridge/doc_claim_registry_tests.rs",
            "the_four_outer_layer_producers_are_all_still_there",
            1,
        ),
        // 〔`设计/50` 09-18〕`src/account-usage.ts` 整份删除（用量 ③ 轴退役）⇒ 这一行随它出表。
        // 🔴 〔`K-R104` 09-13〕`src/bridge/src/account_usage.rs` 那两行墓碑账**删了** ——
        //    不是撕墓碑，是**被墓碑的那段散文随整条编排一起走了**：那份文件里
        //    「用量探针的 shell 串」整段不存在了（编排搬上后端帧面），
        //    连带它头注里点 `build_kill_session_cmd` 与
        //    `emit_guarded_commands_for_e2e` 的那两句一起没了。
        //    ⇒ 按本表头注那条纪律：「登记的那一处盘上已经没有了 ⇒ 删掉它，
        //    别让表替真判据挡枪」（同 `K-R96` 那次的处置）。
        // `K-R56`（09-11）买的那条判据：它守的性质（**探不到就不动手**）没消失，
        // 换住址钉在今天唯一那处实现上（`control/gate_tests.rs::both_gates_always_probe_before_they_act`），
        // 而那一段散文必须逐字点出它的旧名字才说得清「接的是谁」。
        (
            // 〔步 7c 后端剖分 2026-09-19 · C 类〕那段墓碑散文随测试段搬家，处数一格没变。
            "tests/backend/control/gate_tests.rs",
            "the_ssh_fallback_always_probes_before_it_acts",
            1,
        ),
        // 同一刀带走的 e2e 夹具产出者，与它那套跑不起来的真机验收脚本。
        (
            "tests/bridge/shared_crate_registry_tests.rs",
            "emit_guarded_commands_for_e2e",
            1,
        ),
        // 两条 `the_refusal_wording_matches_the_ssh_path` 改名成
        // `…_matches_the_sibling_command`（对照面从「那条 SSH 回落」换成兄弟命令）。
        // 〔C4e 批 2〕`the_refusal_wording_matches_the_ssh_path` 那两行（`backend_kill_tests.rs` · `backend_send_keys_tests.rs`）摘了：
        //   前者点它的那段头注随「拒绝文案同形」那条判据退役（原处换成只点后继名的墓碑），后者整份随送键发送端删掉。
        // 🔴 〔`K-R94` 09-12〕同一件事的四笔账：**读 subagent 那条路上「找」也交给后端了**
        //    （候选枚举 / 首行时间戳 / 读 jsonl 三样本机不再自己做，改走后端既有的
        //    `--list-subagents` ＋ `--read-session`）。随之走掉三个生产符号
        //    （`derive_subagent_dir` / `list_meta_matches` / `load_subagent_remote`）
        //    与一条判据（`the_remote_path_actually_asks_the_backend` —— 它只钉远端那半，
        //    两条路收成一条之后由 `both_paths_ask_the_backend_and_reuse_the_existing_subcommands`
        //    接住、钉的是两条）。散文里那几句说的正是**「它们为什么不在了」**
        //    ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        // 〔MOD〕`tests/bridge/subagent_tests.rs` 那一行摘了：那份文件随子 agent 那条命令退役删了（挑的判据搬去后端 `history_query_f07_tests.rs`）。
        // 🔴 〔`K-R88` 09-13〕同一形，第三件：**「按 sid 找那份会话文件」收成一份之后，
        //    monitor 侧那个收路径的源守卫 `validate_branch_source` 整个不在了**
        //    （入参从路径收成 sid，找那一步走 `branch_core::find_session_file`，两侧同一份）。
        //    这两句散文说的正是**「那道门原先长什么样、为什么今天不需要它了」** ——
        //    删掉的是线索不是病 ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        ("src/doc/ARCHITECTURE.md", "validate_branch_source", 1),
        ("src/doc/INVARIANTS.md", "validate_branch_source", 1),
        // 🔴 〔`K-R112` 09-13〕同一形，第四件：**cc-bus 三条与抓屏改走后端原语之后，
        //    它们各自那个 shell 命令构造器整块删了**（`build_broadcast_cmd` /
        //    `build_kill_cmd` / `build_capture_pane_cmd`；`build_online_cmd` 不在这里 ——
        //    它在 `write_site_registry.rs` 的一段**字符串字面量**里还有代码侧出现）。
        //    散文里点名它们的那几句说的正是**「它们为什么不在了」**
        //    ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        // 🔴 〔`设计/50` 删用量 09-18〕同一形，第五件：**用量 ②③ 两轴整轴退役。**
        //    下面这几处散文全是**订正段** —— 它们逐字引用被删掉的命令名/函数名/判据名，
        //    为的是说清「那一格的数为什么从 X 变成 Y」「那条判据为什么不在了」。
        //    删掉这些句子＝删掉这一刀的账，而这一刀正是 `设计/50 §7` 点名要记账的那一刀。
        //    ⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里逐条记账。
        (
            "tests/bridge/capability_registry_tests.rs",
            "emit_usage_probe_frames_for_e2e",
            1,
        ),
        ("src/bridge/README.md", "aggregate_remote_usage_all", 1),
        (
            "src/bridge/crates/codex-token-core/src/lib.rs",
            "kou_jing_singleton",
            1,
        ),
        (
            "src/backend/control/launch_render/payload.rs",
            "usage_probe_payload",
            1,
        ),
        // 〔C4e · 第四波 4C〕`inbound_client_tests.rs` 里 `the_two_tmux_primitive_arg_builders_…` 那一行摘掉：那条判据（它的后继）
        //   随抓屏的参数构造器一起退役，原处换成一块只点后继名的墓碑（`command_args` 那一行登记着）。
        (
            "src/bridge/src/local_origin_registry.rs",
            "account_usage_local",
            1,
        ),
        ("tests/ipc/commands.vitest.ts", "account_usage_local", 1),
        (
            "tests/ipc/commands.vitest.ts",
            "aggregate_remote_usage_all",
            1,
        ),
        ("tests/ipc/commands.vitest.ts", "aggregate_usage_all", 1),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "account_usage_local",
            4,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "aggregate_remote_usage_all",
            2,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "aggregate_usage_all",
            2,
        ),
        (
            "tests/backend/no_timer_guard.rs",
            "the_oneshot_watchdog_script_carries_no_loop",
            1,
        ),
        // 〔C4e 批 3b〕`cc_bus_tests.rs` · `build_broadcast_cmd` 那一行摘了：点它的那句在广播那条退役判据里，
        //   广播整条迁走（组合进后端 `bus-broadcast`、说法进界面），monitor 里写面整轴退役。
        // 〔SH1 · V136〕`cc_bus.rs` · `build_kill_cmd` 那一行摘了：monitor 那份 `cc_bus.rs` 整份收成两句共用说法，点它的墓碑段随之删了。
        // 〔SH1 · V136〕驾驶舱 shell 读退役：两份登记表里点那条本机读的散文、以及原靠 monitor 那份墓碑兜着的旧构造器名，就地挂墓碑。
        // 〔SH1 · V137〕`list_remote_mcp_project_dirs`（原是 `list_mcp_project_dirs` 的远端分支，本机远端同一条 `mcp-read` 之后删了）：点它的散文就地挂墓碑。
        ("src/bridge/README.md", "list_remote_mcp_project_dirs", 1), // 〔MIG-3a〕2 → 1：IPC 清单里 MCP 那一段整段删了（命令进了那台后端）
        // 〔MIG-3a〕`mcp.rs` 删了之后，模块表里 sftp.rs 那一行留着的两处旧守卫名 ＋ 一处落点函数名成了死名（那几句本来就带墓碑）。
        ("src/bridge/README.md", "is_safe_remote_mcp_json", 2),
        ("src/bridge/README.md", "mcp_json_path", 1),
        // 〔MIG-3a〕`src/bridge/src/mcp.rs` 那一行随文件删了（MCP 读写进了那台后端）。
        // 〔MIG-3a〕`src/ipc/commands.ts` 那一行摘了：点它的那段注释随 `list_mcp_project_dirs` 包装层一起删了。
        (
            "tests/bridge/origin_tests.rs",
            "list_remote_mcp_project_dirs",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "list_remote_mcp_project_dirs",
            1,
        ),
        (
            "tests/ipc/commands.vitest.ts",
            "list_remote_mcp_project_dirs",
            3,
        ),
        (
            "src/bridge/src/local_origin_registry.rs",
            "list_remote_mcp_project_dirs",
            1,
        ),
        // 〔SH1〕列 tmux 改问后端：守那条跨 SSH 串的判据退役，原处挂墓碑。
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand",
            1,
        ),
        ("src/backend/footprint/registry.rs", "build_online_cmd", 1),
        (
            "tests/bridge/spawn_managed_exit_sites.rs",
            "local_shell_read",
            1,
        ),
        (
            "tests/bridge/write_site_registry_spawn_sites.rs",
            "local_shell_read",
            1,
        ),
        // 〔C4e 批 3b〕`cc_bus_tests.rs` · `build_kill_cmd` 那一行摘了：理由同上一行（点它的那句在收掉那条退役判据里）。
        (
            "src/bridge/src/backend/control/tmux.rs",
            "build_capture_pane_cmd",
            2, // 〔C4e〕3 → 2：`capture_remote_pane` 的头注（点它旧构造器的那一句）随命令一起删了
        ),
        (
            // 〔搬树 2026-09-18 · 散文随测试段搬家，该名字的**总处数一格没变**〕
            "tests/bridge/backend/control/tmux_tests.rs",
            "build_capture_pane_cmd",
            1,
        ),
        //    同一件事的另一半：那条串的**出口判定**（两个哨兵 `NO_TMUX` / `NO_PANE`）
        //    也随之不存在了 —— 帧面把「答案」与「屏幕内容」分开走，
        //    「屏幕上恰好只有 NO_PANE 这几个字」这个误判形状跟着消失。
        (
            "src/bridge/src/backend/control/tmux.rs",
            "classify_capture_output",
            1,
        ),
        // 🔴 〔`15 §5.1 A3` 09-18〕`spawn_managed` 那个唯一出口把仓里三份**各自长着的**
        //    正确做法收编了，其中两份连符号一起没了：`local_backend_host::hide_console_window`
        //    （`00 §1.5.1` 步 1 的止血，它自己的头注就写着「A3 落地时它会被换成注入参数」）
        //    与 `cc_bus` 的 `reap_whole_tree_on_drop` / `KillTreeGuard`。
        //    ⇒ 两处散文逐字引用旧名字，说的正是「这三样不是新发明的」——
        //    删掉就删掉了「同一形状出现三次」这条线索，而那是立项理由本身。
        //    按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记账。
        (
            "src/bridge/src/local_backend_host.rs",
            "hide_console_window",
            1,
        ),
        ("src/bridge/src/spawn_managed.rs", "hide_console_window", 1),
        // 🔴 〔`99 §2.5 P9` 2026-09-18〕`strip_cfg_test` 那两份就地复制的剥法退役了 ——
        //    收进早就存在的唯一住址 `guard_core::production_source`（`设计/16 §5.1`）。
        //    那段散文逐字引用旧名字，说的正是「它为什么不在了、以及**退役的理由不是
        //    『它变成恒等函数了』**」—— `设计/16 §4.1` 那条预言今天还不成立，读数在那儿。
        //    删掉这两句＝删掉这一刀的账 ⇒ 按第②条出路：贴墓碑 ＋ 在这里记一笔。
        // 🔴 〔步 7c 剖分 2026-09-19〕**这一条是真新增的死名，不是搬住址。**
        //
        // `the_caller_never_gets_its_own_source_back` 是本轮**退役**的那条判据
        //（`设计/16 §6.2` D 类：它的前提「判据与被测代码同住一份文件」剖分后恒假）。
        // 接它岗的是 `the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split`。
        // 那段解释里**逐字引用了旧名字**（不引就说不清换掉的是谁、为什么），
        // ⇒ 按本条第②条出路：同一行贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        (
            "tests/bridge/crates/guard-core/lib_tests.rs",
            "the_caller_never_gets_its_own_source_back",
            1,
        ),
        ("tests/backend/readonly_guard.rs", "strip_cfg_test", 2),
        // 🔴 〔步 `19c` 2026-09-19〕`build.rs::embed_backends` 里那个按 arch 分岔的
        //    `c_cross_note` 局部变量**删了** —— 它是半 bump panic 文案里那条**手抄的
        //    第二条产字节配方**的附注，而那整段配方在本拍撤掉了（出路收成一条命令
        //    `tests/scripts/re-embed.sh`，与 `release.yml` 逐字同源）。
        //    ⚠ **那段 08-25 的实测读数一个字没丢**：它搬进了那条命令自己的头注
        //    （`ring` 的 C 要 `zig cc`，`rust-lld` 替不了，aarch64 不给就 rc=101）——
        //    写在**跑那条命令的地方**，而不是一段没人会照着敲的 panic 文案里。
        //    留在 `build.rs` 里的那一句逐字引用旧变量名，说的正是「那段话原先长什么样、
        //    它搬去哪了」⇒ 按第②条出路：贴 `PROSE_NAME_TOMBSTONE` ＋ 在这里记一笔账。
        //
        // 🔴 **这个 `2` 不是笔误，而它背后是本函数自己的一处双计** —— 如实记，别读成
        //    「盘上写了两遍」：`src/bridge/build.rs` 里 `c_cross_note` **只出现一次**
        //    〔现打 `grep -o … | wc -l` ＝ 1〕，而本条报的是 2。
        //    根在 `dead_name_corpus()`：它先走 `"src"` 那棵树（`build.rs` 今天**就在
        //    `src/` 底下**），走完之后又 `out.push(root.join("src/bridge/build.rs"))`
        //    **再收一遍** ⇒ 这一份文件的每一个散文名字都被数两遍。
        //    〔现打验法：往 `build.rs` 末尾加一行只出现一次的
        //     `// probe \`zz_probe_unique_name\``，本条当场报「盘上 2 处」。〕
        //    ⚠ 那句显式 `push` 是搬树之前留下的（当年 `build.rs` 住 `src-tauri/`，
        //    在所有根之外，非补不可）。**这正是本函数头注 09-18 那段话说的同一形**
        //    ——「那些文件被数两遍…… 教训：发现一个形状之后，要把同形的全找一遍」——
        //    那一拍改对了四个**目录**根，**漏了这一处单文件的显式 push**。
        //    🔴 **本拍不改它**：动它就是动一条判据的人群，而且是往**变少**的方向动
        //    （本仓纪律：地板在「变少」方向上是瞎的，这种改要单独一拍、带自己的死值验），
        //    与 `19c` 的写区也不沾边。⇒ 按盘上现打的数登记，并把读数留在这里；
        //    真要修，同拍要核的还有 `:1463` 那处**同形**的显式 push 与它那条
        //    「四个根下 186 + build.rs 1」的分母自述（那句今天也已经陈了）。
        ("src/bridge/build.rs", "c_cross_note", 2),
        // 〔BS1 · ccbus-win 09-24〕Windows 预检从「说没做」换成真探测（`K-R69` 的
        //   `probe_binary_uncached`）⇒ 旧测试整条改写，头注逐字引旧名说明它为什么不在了。
        (
            "tests/bridge/backend/control/cc_bus_deploy_tests.rs",
            "windows_says_the_precheck_did_not_happen_instead_of_staying_silent",
            1,
        ),
        // 〔BS1b 09-24〕派生改走后端原语 `bus-spawn`：SSH 那条命令构造器删了（`fn_body` 头注两处
        //   逐字引它说明抽取器当年栽在哪），「spawn 仍拒」那条判据改名（头注引旧名说明为什么改）。
        // 〔C4e 批 3b〕`cc_bus_tests.rs` · `letting_kill_broadcast_and_online_through_did_not_let_spawn_through` 那一行摘了：
        //   点它的那句在 `K-R112` 那条「写面只经后端原语」的退役判据里，monitor 里写面整轴退役。
        // 〔F7c 收尾 09-24〕池子那十二条 Tauri 命令 ＋ 挂在它们名下的函数 / 判据删了（`设计/60 §13b`）；
        //   留下的散文说的正是「它们为什么不在了」⇒ 第②条出路：贴墓碑 ＋ 记账。
        ("src/bridge/README.md", "sftp_cancel_transfer", 1),
        ("src/bridge/README.md", "sftp_read_text_for_edit", 2),
        (
            "src/bridge/src/filewin/editor.rs",
            "sftp_read_text_for_edit",
            2,
        ),
        ("src/bridge/src/filewin/mod.rs", "sftp_cancel_transfer", 1),
        (
            "src/bridge/src/filewin/mod.rs",
            "sftp_read_text_for_edit",
            1,
        ),
        (
            "src/bridge/src/filewin/transfer.rs",
            "sftp_cancel_transfer",
            1,
        ),
        (
            "src/bridge/src/filewin/writeops.rs",
            "sftp_read_text_for_edit",
            1,
        ),
        // 〔FILES2 · 第四波〕有损目录里上传 / 搜索 / 开终端改成按字节做 ⇒ 那道出声拒的闸没了调用方、删了，原处一块墓碑。
        ("src/bridge/src/filewin/shell.rs", "refused_in_lossy_cwd", 1),
        // 〔SR1b〕住址 `src/bridge/src/sftp.rs` → `src/backend/dial/sftp.rs`：那段原子上传的来历随函数搬进了本机后端。
        (
            "src/backend/dial/sftp.rs",
            "the_chmod_attrs_never_put_a_size_on_the_wire",
            1,
        ),
        // 〔第四波 S4〕`sftp_pool.rs` 那一处（句柄有损判定的头注里点着从前那个判文件名的函数）随它所在的
        //   零流量复制一段整块删了 ⇒ 这一行走了（被守的那件事整段退役，不是墓碑被人擦掉）。
        (
            "tests/bridge/filewin/boundary_tests.rs",
            "sftp_cancel_transfer",
            2,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "sftp_cancel_transfer",
            2,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "sftp_read_text_for_edit",
            1,
        ),
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "sftp_cancel_transfer",
            4,
        ),
        (
            "tests/bridge/remote_write_registry_tests.rs",
            "sftp_read_text_for_edit",
            2,
        ),
        (
            "tests/bridge/sftp_family_registry_tests.rs",
            "sftp_cancel_transfer",
            1,
        ),
        (
            "tests/bridge/sftp_family_registry_tests.rs",
            "sftp_read_text_for_edit",
            1,
        ),
        // 〔SR1b · 2026-09-24〕`tests/bridge/sftp_pool_tests.rs` 那五块墓碑（`decode_editable_guards` 等五个旧判据名）
        //   随那份判据整份重写摘了：它守的**整根轴**（池子本体的 SFTP 判据）搬进了本机后端，
        //   新文件判的是中继，没有一句还在说那几个旧判据 ⇒ 真该没有，不是删线索。
        // 🔴 〔C4c · 第四波 4B〕账号清单（远端 / 本机）与信任预检三条 Tauri 命令改走通道、后端出成品：monitor 那一份行解析 ·
        //    本机并表 · 本机信任预检三件函数随之删了；留下的散文逐字点旧名讲来历 ⇒ 第②条出路（逐处一块）。
        ("src/backend/faces/read_face.rs", "with_apikey_table", 1),
        (
            "src/bridge/src/backend/control/frame_query.rs",
            "run_list_query",
            1,
        ), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        ("src/bridge/src/ssh_source.rs", "run_list_query", 1), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        ("src/backend/footprint/registry.rs", "run_list_query", 1), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        (
            "tests/bridge/backend/control/frame_query_tests.rs",
            "run_list_query",
            1,
        ), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        (
            "tests/bridge/byte_cap_registry_tests.rs",
            "run_list_query",
            1,
        ), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        (
            "tests/bridge/exec_site_registry_tests.rs",
            "run_list_query",
            1,
        ), // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        (
            "tests/backend/observe/accounts_query_tests.rs",
            "list_from_dir",
            1,
        ), // 〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑
        (
            "tests/backend/faces/read_face_tests.rs",
            "line_shaped_answers_carry_the_rows",
            1,
        ), // 〔C4d〕历史跨机 join 进本机后端：会话行口径收成一份 ＋ Codex 历史搬进适配层，点 monitor 那几份旧实现 / 退役判据的散文挂墓碑
        (
            "tests/backend/observe/history_query_tests.rs",
            "truncate_is_char_safe",
            1,
        ), // 〔C4d〕摘录截断改用 search-core，旧判据名挂墓碑
        (
            "src/backend/agents/codex/history.rs",
            "codex_first_user_excerpt",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/src/history.rs", "codex_first_user_excerpt", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/src/history.rs", "codex_projects_from", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/src/history.rs", "codex_session_entry", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/src/history.rs", "enumerate_codex_sessions", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/src/history.rs", "local_projects_via", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/backend/history/history_annotations_tests.rs",
            "c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/backend/history/history_join_tests.rs",
            "unknown_is_its_own_bucket_when_sorting",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "codex_projects_group_by_cwd",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "history_project_camel_case_contract",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "last_account_serde_and_patch_semantics",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "the_local_project_list_is_whatever_the_backend_said",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/bridge/history_tests.rs",
            "the_three_counts_can_say_i_do_not_know",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("tests/bridge/history_tests.rs", "truncate_chars_unicode", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        (
            "tests/generated-boundary-guard.vitest.ts",
            "forked_from_message_uuid",
            1,
        ), // 〔C4d〕历史清单与注解搬进本机常驻后端，点 monitor 旧实现 / 退役判据的散文挂墓碑
        ("src/bridge/README.md", "stream_remote_history_sessions", 1), // 〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        (
            "tests/bridge/origin_tests.rs",
            "stream_remote_history_sessions",
            1,
        ), // 〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        (
            "tests/bridge/parity_ledger_tests.rs",
            "stream_remote_history_sessions",
            1,
        ), // 〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        (
            "tests/ipc/commands.vitest.ts",
            "stream_remote_history_sessions",
            3,
        ), // 〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        // 〔SH1 · `00 §2.5 ①`〕acct-iso 本机 / 远端两对 Tauri 命令合成带 origin 的两条，点四个旧名的散文挂墓碑。
        (
            "src/backend/accounts/iso.rs",
            "remote_acct_iso_shellinit",
            1,
        ),
        ("src/bridge/src/backend/mod.rs", "check_local_acct_iso", 1),
        (
            "src/backend/footprint/registry.rs",
            "check_remote_acct_iso",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "check_local_acct_iso",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "check_remote_acct_iso",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "local_acct_iso_shellinit",
            1,
        ),
        (
            "tests/bridge/parity_ledger_tests.rs",
            "remote_acct_iso_shellinit",
            1,
        ),
        (
            "tests/ipc/commands.vitest.ts",
            "remote_acct_iso_shellinit",
            1,
        ),
        // 〔E2 · V28〕`backendPath` 那一格删了（落点固定）⇒ 它的放行判定、卸载守卫、远端三行入口与守它们的几条判据一起走；点旧名的散文挂墓碑。
        // 〔MIG-3a · 09-28 预裁〕`acct_iso_deploy.rs` 那一块随整份文件删了（字节随后端二进制走）。
        (
            "src/backend/footprint/registry.rs",
            "is_safe_remote_backend_path",
            1,
        ),
        (
            "tests/bridge/sftp_tests.rs",
            "is_safe_remote_backend_path",
            1,
        ),
        (
            "tests/bridge/ssh_source_tier1_tests.rs",
            "backend_path_for_shell",
            1,
        ),
        (
            "tests/bridge/backend/control/local_backend_tests.rs",
            "remote_shim_sets_no_environment_of_its_own",
            1,
        ),
        (
            "tests/bridge/lib_invariant_population_tests.rs",
            "every_read_of_the_backend_path_field_is_registered",
            1,
        ),
        (
            "tests/bridge/sftp_tests.rs",
            "both_remote_path_sinks_still_ask_their_fence",
            1,
        ),
        (
            "tests/bridge/sftp_tests.rs",
            "the_remote_ccm_entry_is_an_entry_not_an_implementation",
            1,
        ),
        // 〔E2 · 子步 4〕本机单文件：逐字节副本与带 build_id 的释放名退役。
        (
            "src/bridge/src/backend/control/local_backend.rs",
            "local_extract_name",
            1,
        ),
        (
            "tests/bridge/backend/control/local_backend_tests.rs",
            "the_local_ccm_entry_is_a_copy_of_the_backend_itself",
            1,
        ),
        (
            "tests/bridge/backend/control/local_backend_tests.rs",
            "the_resolution_path_really_puts_the_local_ccm_entry_down",
            1,
        ),
        // 〔V151〕分流收成 `route`：抢词表与它的判据退役。
        ("src/backend/control/ccm/mod.rs", "routes_to_backend", 1),
        (
            "tests/backend/control/ccm_tests.rs",
            "under_the_name_ccm_only_backend_first_words_reach_the_backend",
            1,
        ),
        ("src/bridge/README.md", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/bridge/README.md", "list_session_activity", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/bridge/src/bridge.rs", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "src/bridge/src/event_replay.rs",
            "buffered_local_session_ids",
            1,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "src/bridge/src/event_replay.rs",
            "buffered_remote_sessions",
            1,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/bridge/src/lib.rs", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/bridge/src/lib.rs", "list_session_activity", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/doc/ARCHITECTURE.md", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/doc/IPC-PROTOCOL.md", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/doc/IPC-PROTOCOL.md", "list_session_activity", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/doc/STATE-MATRIX.md", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/doc/STATE-MATRIX.md", "list_session_activity", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/main.ts", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/main.ts", "list_session_activity", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/tab-session-state.ts", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/tab-store.ts", "list_active_sessions", 1), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/tabs.ts", "list_active_sessions", 1),      // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "tests/bridge/event_replay_tests.rs",
            "buffered_local_session_ids",
            1,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "tests/bridge/event_replay_tests.rs",
            "buffered_remote_sessions",
            1,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "tests/bridge/parity_ledger_tests.rs",
            "list_active_sessions",
            2,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        (
            "tests/bridge/parity_ledger_tests.rs",
            "list_session_activity",
            2,
        ), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("tests/tabs.vitest.ts", "list_active_sessions", 2), // 〔MIG-1〕⑬ 会话生命周期并进会话流后退役
        ("src/account-reads.ts", "launch_agent_id", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/accounts.ts", "apikey_account_id", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/agent-profile.ts", "launch_agent_id", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/backend/observe/accounts_query.rs",
            "the_launch_id_env_var_matches_the_monitor_side_home",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/bridge/README.md", "build_resume_ps_command", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/bridge/src/ccm_probe.rs",
            "build_local_posix_command",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/bridge/src/launch.rs", "build_local_posix_command", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/bridge/src/launch.rs",
            "nobody_reaches_the_relay_take_points_without_going_through_the_seam",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/bridge/src/launch.rs",
            "the_local_resume_payload_has_no_session_container_today",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/bridge/src/launch.rs",
            "the_rendered_local_command_really_carries_the_container",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "src/doc/INVARIANTS.md",
            "every_one_of_the_six_cells_is_measured_not_narrated",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/doc/INVARIANTS.md", "render_local_ccm_with", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/settings/accounts-section.ts", "apikey_account_id", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/backend/control/launch_render/payload_tests.rs",
            "build_local_posix_command",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/backend/control/launch_render/payload_tests.rs",
            "the_refuse_tag_is_the_same_string_on_both_sides",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/backend/control/launch_render/payload_tests.rs",
            "the_shared_stripper_keeps_the_relay_seam_this_guard_must_scan",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/backend/observe/accounts_query_tests.rs",
            "the_launch_id_env_var_matches_the_monitor_side_home",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/backend/relay/server_tests.rs",
            "the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/bridge/local_backend_host_tests.rs",
            "the_launch_side_really_asks_the_backend_and_uses_its_answer",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/bridge/local_backend_host_tests.rs",
            "the_production_relay_facts_are_those_take_points",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/bridge/parity_ledger_tests.rs",
            "the_two_launch_rows_no_longer_carry_the_two_falsified_clauses",
            1,
        ), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("tests/bridge/utils_tests.rs", "local_launch_choice", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        (
            "tests/bridge/local_read_surface_registry_tests.rs",
            "tmux_raw_registry",
            1,
        ), // 〔MIG-1 续〕tmux 快照帧删后这本旧账名只剩订正段
        ("src/backend/common/tmux_utf8.rs", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/mod.rs", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/tmux_list.rs", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/watcher.rs", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/watcher.rs", "tmux_tab_underflow", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "src/bridge/src/backend/control/tmux.rs",
            "decode_tmux_list",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/bridge/src/backend/control/tmux.rs", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "src/bridge/src/backend/control/tmux.rs",
            "tmux_tab_underflow",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "src/bridge/src/cross_half_edge_registry.rs",
            "backend_watcher_src",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/tmux-reads.ts", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "tests/backend/observe/tmux_list_tests.rs",
            "parse_tmux_ls",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "tests/bridge/backend/control/inbound_client_tests.rs",
            "capture_pane_args",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "backend_watcher_src",
            2,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "tests/bridge/backend/control/tmux_tests.rs",
            "the_local_send_keys_never_falls_back_to_ssh",
            1,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        (
            "tests/bridge/parity_ledger_tests.rs",
            "the_tmux_manage_row_stops_waiting_for_a_backend_primitive",
            2,
        ), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("tests/tmux-reads.vitest.ts", "parse_tmux_ls", 1), // 〔MIG-1 续〕列 tmux 会话改走通道，旧命令名挂墓碑
        ("src/bridge/src/ssh_source.rs", "probe_control_channel", 2), // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑
        ("src/bridge/src/ssh_source.rs", "pump_inbound_replies", 1), // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑
        (
            "tests/backend/dial_probe_tests.rs",
            "the_control_probe_writes_nothing_to_an_old_backend",
            1,
        ), // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑
        (
            "tests/bridge/ssh_source_dial_move_judge.rs",
            "the_request_keys_are_the_ones_the_proxy_reads",
            1,
        ), // 〔MIG-1 收尾〕地址解析 / 组拨号请求搬进后端 dial/machine.rs，点旧名的散文挂墓碑
        (
            "src/backend/agents/codex/parse.rs",
            "codex_sid_from_rollout",
            1,
        ), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        (
            "src/backend/agents/codex/record.rs",
            "token_usage_fields",
            1,
        ), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("src/bridge/src/adapter.rs", "codex_sid_from_rollout", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("src/bridge/src/adapter.rs", "kind_of_record_name", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        (
            "src/bridge/src/backend/control/frame_query.rs",
            "parse_session_lines",
            1,
        ), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("src/bridge/src/ssh_source.rs", "snapshot_line_countable", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("src/cards/index.ts", "codex_sid_from_rollout", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("src/cards/index.ts", "kind_of_record_name", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        (
            "tests/backend/observe/record_page_tests.rs",
            "parse_for_kind_dispatches_claude_and_codex",
            1,
        ), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        ("tests/bridge/history_tests.rs", "require_cfg_by_label", 1), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
        (
            "tests/bridge/local_origin_registry_tests.rs",
            "require_cfg_by_label",
            2,
        ), // 〔MOD〕记录解释进后端 / 会话正文四条退役，点旧名
    ];

    let corpus = dead_name_corpus();
    let (code_names, disk) = dead_names_on_disk(&corpus, DEAD_NAME_MIN_UNDERSCORES);

    // ★ 抽取器自检 1：代码侧塌了 ⇒ 全世界都成了「零定义」，下面会红成一片假红。
    assert!(
        code_names >= 2500,
        "代码侧只抽到 {code_names} 个 snake_case 名字 —— 剥法或遍历坏了（09-03 现打 3011）"
    );
    let hits: usize = disk.values().map(|(u, t)| u + t).sum();
    // ★ 抽取器自检 2：注释侧塌了 ⇒ 本条零命中地绿（`ScanReport::require` 那条纪律）。
    assert!(
        hits >= 60,
        "只抽到 {hits} 处「只活在散文里的名字」—— 抽取器坏了（09-03 现打 84 处 / 58 个名字）"
    );

    let mut newly: Vec<String> = Vec::new();
    let mut tombs: Vec<String> = Vec::new();
    for ((path, name), (undeclared, tombstoned)) in &disk {
        let want = INVENTORY
            .iter()
            .find(|(p, n, _)| p == path && n == name)
            .map(|(_, _, c)| *c)
            .unwrap_or(0);
        if *undeclared != want {
            newly.push(format!(
                "  {path}  `{name}`  盘上 {undeclared} 处，登记表写 {want} 处"
            ));
        }
        let want_t = TOMBSTONED
            .iter()
            .find(|(p, n, _)| p == path && n == name)
            .map(|(_, _, c)| *c)
            .unwrap_or(0);
        if *tombstoned != want_t {
            tombs.push(format!(
                "  {path}  `{name}`  盘上 {tombstoned} 处带墓碑，登记表写 {want_t} 处"
            ));
        }
    }

    // ★ 顺序是承重的（照 `K-R17` 那条棘轮的教训）：先报「新写的」，
    //   再报「登记表腐了」—— 反过来的话，「有人新写了一个死名」会被报成
    //   「登记表里那一行盘上没有了」，**指错修法**。
    assert!(
        newly.is_empty(),
        "这几处散文点名了一个**代码里根本不存在**的名字，而登记表对不上：\n{}\n\n\
             ⇒ 三条出路，按优先级：\n\
             ① **把话改对** —— 点今天真的那个符号（`文件·rs::函数名`），\
             它的真伪由 `every_symbol_address_in_the_sources_still_resolves` 真的判得了；\n\
             ② 那一句是**订正段 / 墓碑**（逐字引用一个旧名字来说明它已经不在了）⇒ \
             在**同一行**加 `PROSE_NAME_TOMBSTONE` 那个标记，**并登记进 `TOMBSTONED`**；\n\
             ③ 那个名字是**仓外**的（std / 第三方 / 内核 / 另一个仓）⇒ 加进 `INVENTORY`，\
             并在提交信息里写清它住在哪儿。\n\
             ⚠ **不许**为了让本条变绿就把那句话删掉了事：删掉的是线索，不是病。",
        newly.join("\n")
    );
    assert!(
        tombs.is_empty(),
        "墓碑登记对不上盘面：\n{}\n\n\
             ⇒ 贴墓碑是一次**记账**，不是一个免检章：贴了就登记，改回去了就把行删掉。\n\
             这一条是 `PROSE_NAME_TOMBSTONE` 头注里承诺的那道拦逃生舱的闸 —— \
             没有它，「红了就贴标签」当场成立。",
        tombs.join("\n")
    );

    // ★ 两张表的保鲜自检：登记的那一处盘上已经没有了 ⇒ 删掉它，别让表替真判据挡枪。
    let gone: Vec<String> = INVENTORY
        .iter()
        .map(|(p, n, _)| ("存量", *p, *n))
        .chain(TOMBSTONED.iter().map(|(p, n, _)| ("墓碑", *p, *n)))
        .filter(|(_, p, n)| !disk.contains_key(&(p.to_string(), n.to_string())))
        .map(|(k, p, n)| format!("  [{k}] {p}  `{n}`"))
        .collect();
    assert!(
        gone.is_empty(),
        "这几条登记盘上已经没有了 —— **把它们从表里删掉**\
             （多半是有人把那句话改对了，那是好事）：\n{}",
        gone.join("\n")
    );
}

/// 上面那条判据的**活体夹具** —— 它在真树上今天恰好是绿的（登记表逐格对上），
/// 而「今天该对上」那种格是**空真**：闸死了照样对得上。
/// ⇒ 这里造一份**真的会红**的语料，逐格切开验。
///
/// 夹具文本一律**现拼**（名字分段拼、反引号分开写），免得夹具自己被真树上的扫描收进人群
/// —— 本文件按构造已被摘出语料，这是第二道保险，「别让夹具的名字混进断言」本仓栽过两次。
#[test]
fn the_dead_name_scanner_really_sees_each_shape() {
    let tick = "`";
    let n = format!("zz{u}alpha{u}beta{u}gamma", u = "_");
    let min = DEAD_NAME_MIN_UNDERSCORES;

    // ① 形状：>=2 个下划线才算数；大写 / 少下划线 / 前后粘连一律不算。
    assert!(is_dead_name_shape(&n, min));
    assert!(!is_dead_name_shape("only_one", min), "1 个下划线不该进人群");
    assert!(
        !is_dead_name_shape("Zz_alpha_beta", min),
        "带大写不该进人群"
    );
    let glued = format!("Xy{n}");
    assert_eq!(
        dead_name_idents(&glued, min),
        Vec::<&str>::new(),
        "极大 run 没生效 —— 被粘在别的标识符里时不许从中间抠出来"
    );

    // ② 反引号跨度：裸符号 / 带 `()` / 带路径前缀都认，整句散文不认。
    let called = format!("{n}()");
    let addressed = format!("a.rs::{n}");
    let sentence = format!("由 {n} 钉住");
    assert_eq!(bare_symbol_in_span(&n, min), Some(n.as_str()));
    assert_eq!(bare_symbol_in_span(&called, min), Some(n.as_str()));
    assert_eq!(bare_symbol_in_span(&addressed, min), Some(n.as_str()));
    assert_eq!(
        bare_symbol_in_span(&sentence, min),
        None,
        "整句散文的跨度不该被当成一处符号引用"
    );

    // ③ **双反引号形**（markdown 的 `` `x` ``）收得到 —— 成对扫描买的正是这一格。
    let md = format!("{tick}{tick} {tick}{n}{tick} {tick}{tick}");
    assert!(
        backtick_spans(&md).iter().any(|s| s.trim() == n),
        "双反引号形被切丢了：{:?}",
        backtick_spans(&md)
    );

    // ④ 🔴 **行尾注释必须算注释**（`K-R19` 那个坑）：一行「代码 + 行尾注释」，
    //    名字写在行尾注释里 ⇒ 它属于注释侧，**不是**「代码里有这个名字」。
    let ts = format!("expect(keys.length).toBe(135) // 见 {tick}{n}{tick}\n");
    let rows = dead_name_split("x.ts", &ts);
    assert_eq!(rows.len(), 1);
    assert!(
        !rows[0].1.contains(n.as_str()) && rows[0].2.contains(n.as_str()),
        "行尾注释没被算成注释 —— 这一格漏了，旗舰活体就一次都进不了人群。实得 {rows:?}"
    );

    // ⑤ 字符串里的 `//` 不许把代码截短（`https://` 那个代价，`strip_comment_lines` 已保证）。
    let url = format!("const u = \"https://h/{n}\";\n");
    let rows = dead_name_split("x.ts", &url);
    assert!(
        rows[0].1.contains(n.as_str()),
        "字符串里的 `//` 把代码截短了 ⇒ 人群会偏大。实得 {rows:?}"
    );

    // ⑥ 端到端：造一份**只活在散文里**的语料 ⇒ 它必须进人群；
    //    同一个名字一旦在代码侧出现，就必须**立刻退出**人群。
    let prose = format!("/// 由 {tick}{n}{tick} 钉住。\npub fn f() {{}}\n");
    let (_, dead) = dead_names_on_disk(&[("a.rs".to_string(), prose.clone())], min);
    assert_eq!(
        dead.get(&("a.rs".to_string(), n.clone())),
        Some(&(1, 0)),
        "散文里点名、代码里零出现 —— 这一处没进人群，闸就是死的：{dead:?}"
    );
    let with_code = format!("{prose}fn {n}() {{}}\n");
    let (_, dead) = dead_names_on_disk(&[("a.rs".to_string(), with_code)], min);
    assert!(
        dead.is_empty(),
        "代码里有这个名字了，它还留在人群里 ⇒ 本条会去红一堆活着的符号：{dead:?}"
    );

    // ⑦ 🔴 **墓碑真的被读了**（`KR20D1` 的死值验就切这一格）：
    //    同一行加上标记 ⇒ 那一处记进「已声明」那一格，而不是「未声明」。
    let tombed = format!(
        "/// 原先写的是 {tick}{n}{tick}{mark}，已删。\npub fn f() {{}}\n",
        mark = PROSE_NAME_TOMBSTONE
    );
    let (_, dead) = dead_names_on_disk(&[("a.rs".to_string(), tombed)], min);
    assert_eq!(
        dead.get(&("a.rs".to_string(), n.clone())),
        Some(&(0, 1)),
        "带墓碑的那一处没被记进「已声明」—— 标记根本没被读，那它就是个装饰：{dead:?}"
    );

    // ⑧ `.md` 整份算散文；其余扩展名整份算代码（`.json` 里的名字是**定义**，不是提法）。
    let md_text = format!("见 {tick}{n}{tick}\n");
    let md_rows = dead_name_split("a.md", &md_text);
    assert!(md_rows[0].1.is_empty() && md_rows[0].2.contains(n.as_str()));
    let json_text = format!("{{\"k\": \"{n}\"}}\n");
    let json_rows = dead_name_split("a.json", &json_text);
    assert!(json_rows[0].2.is_empty() && json_rows[0].1.contains(n.as_str()));
}

// ══════════════════════════════════════════════════════════════════════
// `P14`：**[`PROSE_NAME_TOMBSTONE`] 这个标记本身有没有账**（`α3` 刀 C 那个没红的读数）
// ══════════════════════════════════════════════════════════════════════

/// 本条的语料面：**两棵互不包含的根**（`src` ＋ `tests`），`(仓根相对路径, 原文)`。
///
/// # 🔴 为什么**刻意不与** [`dead_name_corpus`] 共用
///
/// 那一份为了让**死名**人群成立，按构造做了四件事：摘掉 `tests/evidence/`、
/// 摘掉 vendored 代码、摘掉判词自己那一份、再把 `src/bridge/build.rs` 显式收第二遍。
/// 本条数的是**标记的处数** —— 上面每一条摘除都会让某几处标记**掉出人群**，
/// 而「掉出人群」在本条正是要逮的那件事（掉出去 ⇒ 删掉它不红 ⇒ 静默可删）。
/// ⇒ 这里**一处都不摘，也不重复收任何文件**：人群就是文件系统全集。
///
/// ⚠ **它按构造看不见的那一格，如实写明**：[`guard_core::scan_tree_excluding`]
/// 收的是**文本**语料，非 UTF-8 的字节流它跳过（那个原语的头注逐字写着理由）。
/// 一处标记按定义是 UTF-8 文本，所以这一刀在本条上零损失 —— 但别把它读成
/// 「本函数看得见这两棵树下的每一个文件」。
fn tombstone_mark_corpus() -> Vec<(String, String)> {
    let root = addr_repo_root();
    let mut out: Vec<(String, String)> = Vec::new();
    for sub in ["src", "tests"] {
        for (p, src) in guard_core::scan_tree_excluding(&root.join(sub), &[] as &[&str], &[]) {
            out.push((dead_name_rel(&root, &p), src));
        }
    }
    out
}

/// ★★ **每一处 [`PROSE_NAME_TOMBSTONE`] 标记都有账；想挂却挂歪了的也有账。**
///
/// 头注（买到什么 · 买不到什么 · 为什么人群必须是全集 · 为什么 `P14` 的第一个出口
/// 做不得）住生产侧模块那三个抽取器上（[`prose_tombstone_core`] ·
/// [`prose_tombstone_marks`] · [`prose_tombstone_near_misses`]），
/// 以及 [`PROSE_NAME_TOMBSTONE`] 头注那段订正。**这里不抄第二份。**
#[test]
fn every_prose_tombstone_mark_is_registered() {
    /// **标记普查**：`(仓根相对路径, 标记处数)`。人群 = 文件系统全集，**逐格相等**。
    ///
    /// ⚠ **表名叫 `REGISTERED` 不是随手起的。** `scanning_guard_registry` 那条纪律逐字：
    /// 「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成 `TABLE_DECLS` 里已有的
    /// 名字之一」—— 起了别的名字，`every_registry_guard_keeps_its_reverse_half`
    /// **看不见本条**，而「它看过了、过了」与「它压根没去看」在输出上一模一样。
    ///
    /// ⚠ 加行/改数之前先问一遍：**这一处是一块真墓碑吗？** 是 ⇒ 挂标记、在这里记一笔；
    /// 不是（量具脚本里的针、讲机制的散文）⇒ 同样记一笔，并在旁边写清它是哪一类。
    /// **不许**为了让本条变绿就把标记删掉 —— 删掉的是账，不是病。
    const REGISTERED: &[(&str, usize)] = &[
        ("src/bridge/src/panorama_bytes.rs", 1), // 〔MIG-3b 续〕新行：全景问 · 写 · 撤那一跳（`panorama_call.rs`）删了，放字节那一半搬来，点旧住址
        ("src/panorama/api.ts", 1), // 〔MIG-3b 续〕新行：原 Tauri 命令三条删了（界面直问那台后端）
        ("tests/bridge/ssh_source_write_half_guard.rs", 2), // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑 1 → 2 // 〔MIG-1〕新行：会话 / tmux 账本搬进后端，点旧名的散文挂墓碑
        ("tests/bridge/ssh_source_capped_line_tests.rs", 2), // 〔MIG-1〕新行：会话 / tmux 账本搬进后端，点旧名的散文挂墓碑
        ("tests/bridge/rust_timer_registry_tests.rs", 1), // 〔MIG-1〕新行：会话 / tmux 账本搬进后端，点旧名的散文挂墓碑
        ("src/bridge/src/bind.rs", 1), // 〔MIG-1〕新行：会话 / tmux 账本搬进后端，点旧名的散文挂墓碑
        ("src/backend/observe/watcher.rs", 3), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑 1 → 3 // 〔MIG-1〕新行：会话 / tmux 账本搬进后端，点旧名的散文挂墓碑
        // 〔CFG1 · 4D〕config.json 写口从整份（`save_config` / `saveConfig`）换成按键补丁、`remote-config.ts` 那个整表写函数
        //   换成只出补丁的 `remoteEdit`：讲来历的散文逐处一块（配置写口的头注 · 诊断写口 · 远端保存那段 · 各判据头注），
        //   以及 KS7 那一格为什么改盯 `patch_config`。
        ("src/bridge/src/logging.rs", 1),
        ("src/config.ts", 1),
        ("src/settings/remote-section.ts", 2),
        ("tests/config-lost-update.vitest.ts", 1),
        ("tests/remote-config.vitest.ts", 1),
        ("tests/settings/accounts-section.vitest.ts", 2), // 〔HX2 · 4D〕+1：写 key 改走通道那一段点旧命令名
        ("tests/settings/remote-section.vitest.ts", 1),
        // 〔CF2 · 第四波 4B〕重放缓冲分档取消：头注里旧的登记方法名 ＋ 读数那个旧字段名各一块；
        //   会话流收口成 `subscribe`：头注表里 ＋ `ready_point` 文档里点原来那个重放方法名各一块。
        ("src/bridge/src/event_replay.rs", 7), // 〔MIG-1〕6 → 7：⑬ 退役的会话清单/活动命令与 buffered_* 旧名挂墓碑 // 〔GP1〕+1：远端 sid 清单改名（连同 origin 一起交）那一块 〔DL1〕+1：头注点名退役的裸事件常量那一块
        // 〔GP1 · 第四波〕本机凭据文件的写者换成本机常驻后端：monitor 那侧写口 · `platform_fs::make_private` ·
        //   「本机那一臂不发帧」判据退役，讲来历的散文逐处一块；搬去后端的三条判据的出处各一块。
        ("src/bridge/src/config.rs", 2), // 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑
        ("src/bridge/src/creds_store.rs", 6), // 〔HX2 · 4D〕+2：两处「GP1 那一版经 `write_key_on`」
        ("src/bridge/src/platform_fs.rs", 1),
        ("tests/backend/accounts/upstream/file_face_tests.rs", 6),
        ("tests/backend/relay/server_tests.rs", 2), // 〔MIG-2〕1 → 2
        ("tests/bridge/creds_store_tests.rs", 11), // 〔HX2 · 4D〕2 → 11：明文逐跳那一条 · 说不出 id 那一条 · `brace_block` 退役，原处与点它们的散文挂墓碑
        // 〔CF2〕`jsonl-line` / `jsonl-batch` 退役：头注点旧载荷名一块 · 独立窗口入口头注点旧定向重放命令一块 ·
        //   状态消费者矩阵那一行一块。
        ("src/bridge/src/bridge.rs", 2), // 〔MIG-1〕1 → 2：⑬ 退役的会话清单/活动命令与 buffered_* 旧名挂墓碑
        ("src/entry-viewer.ts", 1),
        ("src/doc/STATE-MATRIX.md", 3), // 〔MIG-1〕1 → 3：⑬ 退役的会话清单/活动命令与 buffered_* 旧名挂墓碑
        // 〔S5 · 第四波 · V41〕`parse_remote_hosts` 旧单对象那一支与守它的测试删了，接替它的判据头注挂一块。
        ("tests/bridge/lib_remote_config_tests.rs", 2), // 〔合并 JA1〕+1：JA1 点址那一行
        // 〔SR1b 子步 3 · 2026-09-24〕`inproc_dial.rs` 整份删了（界面进程零 SSH）⇒ 从界面侧搬来的旧函数名全仓只剩散文：
        ("src/backend/dial/connect.rs", 2), // 头注「搬自」那一句里跳板 · agent 鉴权两个旧名
        ("src/backend/platform/ssh_agent.rs", 1), // 头注「搬自」那一句里 agent 鉴权旧名
        ("tests/bridge/local_origin_registry_tests.rs", 11), // 〔MOD〕9 → 11 // 〔MIG-3b〕7 → 9：删会话 / 分叉 / 钩子诊断的转交退役，点旧名的散文挂墓碑 // 〔SH1〕+1：地板 15 → 13 那段点的两处读面函数删了 // 〔C4e 批 3b〕+1：`K-R112` 地板那段点的查在线命令迁到界面 // 〔C4e 批 2〕+1：K-R56 那一行点的送键命令迁到界面 // 〔C4e〕+1：`K-R112` 地板那段点的抓屏命令迁到界面 // 〔C4c〕+1：分诊债 11 → 10（账号面那一处查远端配置随命令删了）// 地板 17 → 16 · 分诊债 12 → 11 两处（跳板查配置那一处随文件走了）
        // 〔SR1b 子步 3〕`sftp_move_ledger`（SFTP 14 处拨号的挡路石底账）随那 14 处全搬完退役：点它名字的两处旁注各一块。
        ("src/bridge/src/backend/control/mod.rs", 1),
        ("tests/backend/footprint/registry_tests.rs", 2), // 〔MIG-3b 续〕1 → 2：两个现算口从生产段搬来、旧住址那一句
        // 〔SR1b · 2026-09-24〕后端那份 SFTP：原子上传的来历随函数从 monitor 搬来（改权限那条旧判据名一块）。
        ("src/backend/dial/sftp.rs", 1),
        // 〔SR1b · 2026-09-24〕creds-core 头注里「远端那一侧」那段：原子上传与读回比对的住址搬了，两处旧名挂墓碑。
        ("src/bridge/crates/creds-core/src/lib.rs", 2),
        // 〔RL1 · 第四波 · 2026-09-24〕中转并进本机常驻后端，monitor 另起中转那一族删了；中转名字表那一行注释挂一块。
        ("tests/naming/account-vs-relay-naming.vitest.ts", 1),
        // 〔C4a · 第四波 · 2026-09-24〕「会话 ↔ 账号」与远端全文搜索改走通道，Rust 那几份删了，留下的三处病史各挂一块：
        ("src/backend/observe/search_query.rs", 1), // 合并那一份（`K-R100` 病史）
        // 〔LOC1b · 第四波 4D〕本机搜索改问本机后端（monitor 内存索引删了）⇒ 界面那两份点旧命令 / 旧函数名的散文各挂墓碑；
        //   冷读本机远端合成一条 ⇒ 漂移账登记表那一行旁注挂一块。
        ("src/views/history-search.ts", 1),
        ("src/views/history.ts", 3), // 〔DUP1〕+1：新开那一支原先调的 `validateLocalLaunch`
        ("tests/bridge/drift_ledger_tests.rs", 1),
        // 〔MOD〕`src/bridge/src/remote_history.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("tests/bridge/local_read_surface_registry_tests.rs", 10), // 〔MIG-3b 续〕7 → 10：足迹三处（判定 · 申报表 · 探针）随进后端删了 // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 6 → 7 // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔MIG-3a〕+2：`aliases_read` / `aliases_install` 两行摘除的旧墓碑补上标记（它们的符号随别名进后端没了） // // 〔SH1〕+1：`.claude.json` 三候选的 home 读去掉 // 〔C4c〕+1：`accounts.rs` 那一行摘掉处的「降级说明」旧名 // 棘轮史里 E79 那一格
        // ▸ 下面这 5 份是 `P14` 立件的**直接证据**：它们在 `TOMBSTONED` 里一行都没有
        //   ⇒ 它们的标记**没有一处**落在死名人群上 ⇒ 在本条之前按构造零判据。
        //   逐份是：本模块生产侧那份（标记的定义处）· 本文件（那一处带标记的墓碑，
        //   而本文件按构造被摘出了死名语料）· `byte_cap_registry_tests` ·
        //   `tests/evidence/` 两份量具脚本（针，不是墓碑）。合 15 处。
        // 🔴 〔波 1 合并时补〕`P19` 那一拍删 `TARGET_GAPS` 的 `agent` 豁免时留了墓碑
        //   正文、也写了那四个字，**却没把标记挂上** ⇒ 本条当场红在「挂歪了」那一格。
        //   那是这条判据落地后第一次真逮到东西，而逮到的是**同一波另一路**的产出。
        ("src/backend/lib.rs", 1),
        // 〔AL1 · 2026-09-24〕规则收成一份那一拍新贴的两块墓碑（本机配对 helper · 远端回滚措辞 helper）。
        // 〔RW1 · 第四波 09-24〕1 → 4：本机原语 `LocalFile` 与它的同步门面 `apply_local` 随「用户文件改经后端写」
        //   整块走了，头注一处 ＋ 原住址一块墓碑（两个名字）= +3。
        ("src/backend/assets/aliases/fence.rs", 7), // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕5 → 7：序列墓碑一行两块（`apply` · `Store` · `Applied` · `undo_note` 四处）进、`LocalFile` · `apply_local` 那一行出 // 〔TL1 · 4C〕4 → 5：「拼接写过三份」那一段点代装 rc 那一跳（已退役）的旧名
        // 〔RW1 · 第四波 09-24〕F11 改经远端后端删：远端那一支的头注一块（住址并进 C4a 那一行，〔合并〕两边各 +1）。
        // 〔RW1 · 第四波 09-24〕本机分叉改成 exec 本机后端 `--fork-session`：本机那一支的头注一块。
        // 〔MIG-3b〕`remote_branch.rs` 那一行随文件删了（分叉的 monitor 这一侧整份退役，界面经通道直说那台后端）。
        // 〔RW1 · 第四波 09-24〕远端删会话那道结构守卫随 F11 改经后端删走了：方向相反那一问的两处说明。
        ("src/backend/agents/claudecode/paths.rs", 4), // 〔MIG-3a〕+2：桥那一份逐字副本删了，讲它的两句挂墓碑 // // 〔FN1〕+1：会话形状判定改名（`is_session_record_*`），旧名挂一块
        ("tests/bridge/history_tests.rs", 10), // 〔MOD〕9 → 10 // 〔DUP1〕+1：configDir 判据头注原先「照抄 TS 侧」那句 // 〔RW1〕+2：本机分叉那几组判据换掉时留的块 ·〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑 ·〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑
        // 〔RW1 · 第四波 09-24〕项目 `.mcp.json` 的本机写原语删了（改经后端写）。
        // 〔MIG-3a〕`src/bridge/src/mcp.rs` 那一行随文件删了（MCP 读写进了那台后端）。
        // 〔RW1 · 第四波 09-24〕写点表摘掉那三行时留的一块。
        ("tests/bridge/write_site_registry_tests.rs", 8), // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔MIG-3a · 子步 3〕+1：`deploy_into` 随装 cc-bus 进后端的墓碑 // // 〔MIG-3a〕+1：收件箱写那一行的来历挂墓碑 // // 〔E2 · 子步 4〕2 → 3：副本那一行登记删了，原地一块 //, // 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑
        // 〔RW1 · 第四波 09-24〕「盘上有字节却读到空」那一道从 monitor 的 `LocalFile::read` 搬到后端 `hollow_read`
        //   （住址并进 FW5 那一行，〔合并〕两边各 +1）。
        // 〔RW1 · 第四波 09-24〕Windows ACL 那条判据从 monitor 搬去后端：两头各一块墓碑。
        ("tests/backend/control/files_write_tests.rs", 2), // 〔MIG-3a〕+1：两份围栏逐字相等那一条退役 //
        ("tests/backend/assets/aliases/block_tests.rs", 6), // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕+1：一口判据头注点 `fenced_block::apply` // 〔AL1d〕1 → 5（`ProfileKind::Custom` · 「终端集成」三条命令名两行 · `scan_legacy_profiles`，逐处挂墓碑）
        // 〔C2 · 2026-09-24〕拨号归后端那一拍：`ssh_source_dial_move_judge.rs` 三块（回落表那条判据 · 请求行那条 ·
        //   解析两处那条，随拨号搬家删掉）。〔SR1b 子步 3〕+1：只剩 SFTP 还要 russh 的那条判据改名成零命中。
        // 〔SR1b 子步 3〕`inproc_dial.rs` 那一行摘了：**那份文件整份删了**（SFTP 进本机常驻后端，界面进程零 SSH），
        //   它那块墓碑（端口转发用的 russh 句柄别名）随被守的那件事整轴退役。
        ("tests/bridge/ssh_source_dial_move_judge.rs", 5), // 〔MIG-1 收尾〕地址解析 / 组拨号请求搬进后端 dial/machine.rs，点旧名的散文挂墓碑 4 → 5
        // 〔SR1a · 2026-09-24〕`--dial` 删了那一拍：守它的判据改名留的墓碑 · C2 那份读数脚本头上
        //   那句「界面侧判据随之改名」（它点名的判据 SR1a 改了名，脚本本身只对 C2 那一版有效）。
        ("tests/backend/main_argv_table_guard.rs", 1),
        ("tests/evidence/C2-dial-loopback.py", 1),
        // 〔MC1〕+3：模块头注 ＋ 装 / 卸两条命令头注里各一块（`…_ccm_helper` 改名成 `…_alias_block`）。
        ("src/bridge/src/sftp.rs", 11), // 〔MIG-3a · 09-28 预裁〕8 → 11：标记读写 · 比标记 · 受管路径谓词三处随 cc-acct-iso 部署命令删了，各留一块 // // 〔E2〕7 → 8：`ccm` 入口那一段（`put_ccm_entry` · `CCM_CLI_REMOTE_PATH`）删了，原地一块（两个名同一块只算一处） // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕8 → 7：`rollback_note` · `SftpFile` 两块随读取器 / 落点原语删了，`put_ccm_entry` 头注进一块 `fenced_block::apply` // 〔W5-ALIAS〕10 → 8：别名块两条命令的头注（两块墓碑）随命令搬去 `profile_installer.rs` // 〔SR1b〕9 → 10（进 3 出 2）：执行那一半（SFTP）搬进本机后端 —— 模块头注两块（开会话 · 原子上传）＋ `SftpFile` 改名一块进；原子上传那段头注的两块随函数搬去后端 `dial/sftp.rs`
        ("src/backend/assets/aliases/block.rs", 11), // 〔AL2 · 第四波 4D〕13 → 11：远端装 / 卸别名块两条命令删了，头注两块墓碑随之走 // 〔W5-ALIAS〕11 → 13：从 `sftp.rs` 搬来别名块两条命令，头注里两块墓碑跟着来 // 〔AL1d〕5 → 11（`ProfileKind` / `ProfileScan` · `$PROFILE` 两份认法与遗留扫描 · 扫一份那两个 · 「终端集成」命令名 · 模块头表那一格，逐处挂墓碑） // 〔AL1〕+1：`AccountAliasReport` 那一句 · 〔RW1〕+3：本机原子写原语 `atomic_write_string` / `atomic_replace_path` 删了（原住址一块 ＋ BOM 那段两句）
        // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕`src/bridge/src/verified_write.rs`（3）与 `tests/bridge/verified_write_tests.rs`（1）两行摘掉：整份模块零调用方删了（整轴退役），
        //   它的墓碑说的那几件（`install_remote_ccm_helper` 两块 · `verify_and_rollback`）另有住址记着。
        // 〔SR1b〕+2：传输台那三行摘掉时留的墓碑（暂存区上传 · 本机下载落地两个旧名）。
        ("tests/bridge/remote_write_registry_tests.rs", 18), // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔MIG-3a〕+3：点 monitor `claude_data_fence` 的三句挂墓碑 // // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕+2：`read_profile_text` 那两句记事贴墓碑 // 〔FN1 · V119〕−1：双路径写入口那张表整条判据退役，表里那块墓碑（一个旧池命令名）随之走了 · 〔F7c 收尾 09-24〕1 → 7（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）· 〔RW1〕+1：F11 那条 SFTP 直删 · 〔SR1b 子步 3〕+2：写点表整张空了，`sftp.rs` 那一对原语（原子上传 · 入口落点类型）的旧名
        // 〔MC1 · 2026-09-24〕`CCM_SELF` 删了那一拍：shim 那条判据改名留的墓碑。
        ("tests/bridge/backend/control/local_backend_tests.rs", 3), // 〔MIG-1〕2 → 3：会话 / tmux 账本搬进后端，原处墓碑与点旧名的散文 // // 〔E2 · 子步 4〕1 → 2：两条副本判据删了，原地一块 //,
        // 〔AL1 · 2026-09-24〕别名改由后端渲染那一拍：本模块头注里 TS 那个旧生成器（`buildAliasLine`）·
        // 测试里「形状围栏」那一条（`validate_alias_line`）· 生成物表里退役的 `AccountAliasReport.ts` ·
        // 延后 I/O 登记表里退役的 `write_account_aliases`，各一块。
        // 〔AL1c · 4B〕2 → 1、`shell_dialect.rs` 0 → 1：`ccmInvocation` 那一句随 POSIX 读回（`parse_line`）搬进方言模块，墓碑跟着搬，总数不变。
        ("src/backend/assets/aliases/mod.rs", 1), // 〔AL1〕+1：`ccmInvocation` 那一句（〔AL1c〕搬走了，剩 `buildAliasLine` 那一块）
        ("src/backend/platform/shell/dialect.rs", 2), // 〔OSA · V156〕住址 `assets/aliases/` → `platform/shell/`，块数不变 // 〔MIG-3a〕搬进后端 +1：`look_on_path` 那一格随规则住进那台后端退役 //
        ("tests/backend/assets/aliases/aliases_tests.rs", 1),
        ("tests/generated-boundary-guard.vitest.ts", 3), // 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑 // 〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑
        ("tests/settings/panel-deferred-io.vitest.ts", 1),
        ("src/backend/stream/wire.rs", 2), // 〔MIG-1〕1 → 2：会话 / tmux 账本搬进后端，原处墓碑与点旧名的散文 //
        // 〔MC1 · 2026-09-24〕+2：`install_remote_ccm_helper` 改名那两行。
        ("src/bridge/README.md", 15), // 〔MIG-1 续〕列 tmux 会话那一行改写（不再点旧名）⇒ 与前一拍同数 // 〔合并 MIG-1 × 主线 862be034〕主线 13 ＋ MIG-1 本路增量 ⇒ 15（盘上现打） // 〔MIG-3a〕−1：IPC 清单 MCP 那一段整段删了（带墓碑的那句注释随之走） // 〔SH1〕+2：mcp.rs 那一行（读面改问后端）· 远端项目目录旧名那句 // 〔LOC1b〕+1：远端读会话函数（本机远端合成一条）· 〔F7c 收尾 09-24〕3 → 6（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）· 〔RW1〕6 → 8：`atomic_write_string` 那一节 ＋ 远端删会话那道结构守卫 ·〔C4d〕README 历史那一段重写（历史清单搬进本机后端），用量那句旧线索留着 // 〔MIG-2〕11 → 13
        ("src/bridge/build.rs", 1),
        ("src/bridge/crates/codex-token-core/src/lib.rs", 1),
        // 〔BS1b 09-24〕6 → 10：派生改走 `bus-spawn` 原语，删了 SSH 那条构造器与对 `<local>` 的公共拒绝
        //   ⇒ 两块整段墓碑 ＋ 两处订正行（`cc_bus_send` 头注 · `kill_via_backend` 头注里那句旧拒绝）。
        ("src/bridge/src/backend/control/cc_bus.rs", 1), // 〔SH1 · V136〕19 → 1：驾驶舱读面迁走、整份收成两句共用说法，旧墓碑段随之删，新头注一处 // 〔C4e 批 3b〕11 → 19：−1 点杀会话发送端那条理由的一处随收掉命令删了；＋9 写面五条迁到界面，原处两块墓碑 ＋ 更早几块墓碑里「换了住址」指向的住址也走了、逐行补标 // 〔C4e 批 2〕+1：点 monitor 杀会话发送端那条读 `killed` 的理由，发送端迁到界面
        // 〔MC1〕+1：`install_remote_ccm_helper` 改名。
        ("src/bridge/src/backend/control/local_backend.rs", 12), // 〔合并 MIG-1 × 主线 862be034〕主线 5 ＋ MIG-1 本路增量 ⇒ 12（盘上现打） // 〔E2 · 子步 4〕3 → 5：逐字节副本 `install_local_ccm_entry` 与带 id 的释放名 `local_extract_name` 删了，点旧名处各一块 //, // 〔E2〕2 → 3：`ccm_entry_shim` 删了，原地留一块
        ("src/backend/control/launch_render/payload.rs", 7), // 〔DUP1〕+2：模型名那一格原先「刻意宽容渲染」、TS `isValidModelName` 删了 · launcher 那道闸头注里点 TS `sanitizeRemoteLauncher` 那句（TS 那份删了） // 〔US1〕+3：上游选择那半搬走留下的墓碑 // 〔TL3 · 🔴-3〕+1：`ExportRelayBaseUrl` 头注里链到已删判断口那一句改成今天的出处，旧名留一块
        ("src/bridge/src/backend/control/tmux.rs", 15), // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 11 → 15 // 〔SH1〕+1：跨 SSH 那条 `tmux ls` 用的旗随改问后端删了，原处留墓碑 // 〔C4e 批 2〕+2：杀会话 / 送键两条 Tauri 命令与「通道不在」那句文案迁到界面，原处各一块 // 〔C4e〕+1：抓屏整条迁到界面，发送端与 Tauri 命令原处一块
        ("src/bridge/src/cc_bus_deploy.rs", 2), // 〔MIG-3a · 子步 3〕+1：装与三态那两条 Tauri 命令进后端，头注留墓碑 //
        ("src/bridge/src/ccm_cli_contract.rs", 1),
        // 🔴〔本机侧退役 2026-09-23〕文件管理器「本机」那一侧整条退役，
        //   `source.rs` 的头注上留了一块墓碑：**4 处标记**（一处是墓碑正文那一句，
        //   另三处各挂在一条**随功能一起走掉的判据**的名字上 ——
        //   那三个名字同时要进 `TOMBSTONED`，两张表单位不同，各记各的）。
        ("src/bridge/src/filewin/source.rs", 5), // 〔F7c 收尾 09-24〕4 → 5（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）
        ("src/bridge/src/filewin/fonts.rs", 2), // 〔F9c 第四波 09-24〕0 → 2（探针来路里那两句「存不回去」的函数名随只读一档删了）
        ("src/bridge/src/history.rs", 9), // 〔MOD〕4 → 9 // 〔MOD〕9 → 4 // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔DUP1〕+1：configDir 校验头注原先「照抄 TS 侧 `isValidConfigDir`」那句（TS 那份删了） // 〔合并 LOC1a〕主线 18 ＋ LOC1a +1（分叉 exec 那一趟的结果解释删了）// 〔合并 US1 × 主线〕主线 15 ＋ US1 +3 // 〔C4e 批 2〕+1：本机 kill 那句点的旧发送端 // 〔C4c〕+1：记录那一问的 Tauri 命令与答案形状退役，原处留一块 // 〔RW1〕+1：本机删会话那道路径守卫整段搬去后端 // 〔RW1〕+3：本机分叉的实现（`branch_impl` / `write_branch_file` / `read_jsonl_values`）交给后端 ·〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑 ·〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑 // 〔MIG-2〕20 → 15
        ("src/bridge/src/launch.rs", 7),  // 〔MIG-2〕1 → 7
        ("src/bridge/src/lib.rs", 35), // 〔MOD〕31 → 35 // 〔MIG-3b 续〕30 → 31：足迹三个 mod 删了那一块 // 〔MIG-3b 续〕29 → 30：`mod panorama_call;` 换成一块墓碑 // 〔MIG-3b 续〕+2：`mod pubkey;` 与 `push_public_key` 注册那一行各换成一块墓碑 // 〔MIG-3a · 09-28 预裁〕26 → 27：`mod acct_iso_deploy` 那一行换成一块墓碑 // // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 21 → 26 // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔合并 MIG-3a × MIG-2〕基数 16 ＋ 主线 +1 ＋ MIG-3a +2 ＋ MIG-2 +1（`apikey_remote` 整删）⇒ 20 // 〔STOP〕+1：`mod stop_grace;` 那一行换成一块墓碑（「请它收尾 → 等 → 强杀」搬进一次性 `--resident-stop`）。 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕主线 13 ＋ 本路 +2（`verified_write` 模块删了那一行：`verified_write` · `fenced_block::apply` 各一）⇒ 15。主线原注：〔合并 HX2 × 主线 06b5dc08〕基数 10 ＋ LOC1b +3 ＋ HX2 ±0 ⇒ 13。LOC1b 原注：〔LOC1b〕+2：本机判活那一段（`SessionMap::load_with_changes` 起步 · `app.manage(session_map)`）删了，原处各挂一块 // 〔合并 LOC1b × 主线 66f2b6bf〕主线 10 ＋ LOC1b +1（`mod search;` 那一行挂一块，本机内存索引删了） // 〔US1〕+1 // 〔GP1〕+1：`write_apikey_credentials_key` 头注里「整段论证见」那个旧写口 // 〔合并 C4d × 主线 cf3277f4〕主线 7 ＋ 本路 +1 ⇒ 8（〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑） // 〔合并 CF2 × 主线 60ace11b〕主线 6 ＋ 本路 +1（独立窗口的定向重放命令退役那一段）// 〔AL1d〕+2（「终端集成」五条命令退役：注册表旁一句 ＋ 原住址一句；合并主线 d07c6d14 按两边增量相加 4 + 2） // 〔F7c 收尾 09-24〕1 → 2（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）· 〔SR1b 子步 3〕+1：`sftp_move_ledger` 退役 · 〔C4b〕+1：`plugins` 模块删了，那一行挂一块（合并按两边增量相加 ⇒ 4） ｜ HX2 原注：〔HX2 · 4D〕±0：`write_apikey_credentials_key` 整条删，原处换一块墓碑（点旧命令名 ＋ `KH2C1` 那条旧判据名）// 〔US1〕+1 // 〔GP1〕+1：`write_apikey_credentials_key` 头注里「整段论证见」那个旧写口 // 〔合并 C4d × 主线 cf3277f4〕主线 7 ＋ 本路 +1 ⇒ 8（〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑） // 〔合并 CF2 × 主线 60ace11b〕主线 6 ＋ 本路 +1（独立窗口的定向重放命令退役那一段）// 〔AL1d〕+2（「终端集成」五条命令退役：注册表旁一句 ＋ 原住址一句；合并主线 d07c6d14 按两边增量相加 4 + 2） // 〔F7c 收尾 09-24〕1 → 2（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）· 〔SR1b 子步 3〕+1：`sftp_move_ledger` 退役 · 〔C4b〕+1：`plugins` 模块删了，那一行挂一块（合并按两边增量相加 ⇒ 4） // 〔MIG-2〕16 → 17
        ("src/bridge/src/local_backend_host.rs", 4), // 〔TL3 · 🔴-3〕+1：中转「在不在」那一行原先点着已删的回环探针，改成今天由后端答、旧名留一块 // 〔US1〕+1：`relay_running` 一族退役那一块 // 〔HX1〕+1：`fresh_token` 头注里「不需要密码学随机数」那一段整段删，原处留一块（合并按两边增量相加：1 ＋ 1 ＋ 1）
        // 〔LOC1b · 第四波 4D〕monitor 自己那份本机判活整份删了（本机活会话表改由本机后端的帧喂），头注点旧实现的几个名字各挂一块。
        // 〔MIG-1〕`src/bridge/src/session_map.rs` 那一行摘了：那份文件整删（会话账本搬进后端）。
        // 〔LOC1b · 第四波 4D〕只为本机读盘服务的适配器门面零调用方、删了：原处一块 ＋ 两处点旧名的散文 ＋ 判据头注一块。
        ("src/bridge/src/adapter.rs", 5), // 〔MOD〕3 → 5
        // 〔MOD〕`tests/bridge/adapter_tests.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("src/bridge/src/local_origin_registry.rs", 10), // 〔MIG-1 收尾〕地址解析 / 组拨号请求搬进后端 dial/machine.rs，点旧名的散文挂墓碑 9 → 10 // 〔合并 MIG-3b × 主线 76b31e7c〕主线 8 ＋ MIG-3b +1（删会话 / 分叉 / 钩子诊断的转交退役，点旧名的散文挂墓碑） // 〔SH1〕+1：MCP 远端那两行还掉处点的旧名 // 〔SH1〕+1：包装层那一格点的 `cfg_of` 随驾驶舱 shell 读删了 // 〔C4e 批 3b〕+1：P4a 变异 M5 那一格点的发消息命令迁到界面 // 〔C4e 批 2〕+2：`K-R56` 还掉的那一条（送键命令）今天整个迁到界面，两处点它 // 〔C4c〕+1：分诊债表里账号面那一行还掉了 // 〔MIG-2〕7 → 8
        ("src/bridge/src/spawn_managed.rs", 6), // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 4 → 6 // 〔LOC1a〕+2：一次性本机查询那一格删了，点旧名的散文挂墓碑
        ("src/bridge/src/structural_scan.rs", 1),
        ("src/doc/ARCHITECTURE.md", 3), // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 2 → 3 // 〔LOC1a〕+1：本机一次性 exec / 任务行解释 / 分叉 exec 那几条路删了，点旧名的散文挂墓碑
        ("src/doc/INVARIANTS.md", 25), // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 19 → 25 // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔合并 MIG-3a × 主线 ad308378〕基数 12 ＋ MIG-3a +2 ＋ 主线 +4 ⇒ 18 // 〔DUP1〕+2：U8c-1「变严的代价」那一节补一句「收口了，收法是删」·「本地路径借 IR 做校验」那一节记 `validateLocalLaunch` 删了 // 〔LOC1b〕+1：§6 探活双重校验那一格搬去后端，点 monitor 旧函数名那句挂一块 // 〔RW1〕+2：§1 例外 3 那道远端删会话守卫 · 例外 1 本机那道路径守卫（`validate_delete_target`） // 〔MIG-2〕12 → 16
        //   〔LR2〕+3：§33b 产出方表 `session-backend.ts` 那格 · 三问表 ③ 那格 · 「删掉座的代价也换人了」那段 —— 点着随 TS 兜底一族删掉 / 改写的判据
        //   ⇒ 6（基）＋1（LOC1b）＋3（LR2）= 10
        ("src/doc/IPC-PROTOCOL.md", 12), // 〔MIG-3a · 09-28 预裁〕11 → 12：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔合并 MIG-1 × 主线 862be034〕主线 8 ＋ MIG-1 本路增量 ⇒ 11（盘上现打） // 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑 // 〔MIG-2〕5 → 8
        // 〔AL1 · 2026-09-24〕+1：别名那一块搬走时留的墓碑（`buildAccountAliasBlock`）。
        ("src/settings/accounts-section.ts", 5), // 〔MIG-3a · 09-28 预裁〕4 → 5：部署那一段头注点旧命令名 // // 〔MIG-2〕3 → 4
        ("src/apikey-reads.ts", 2), // 〔MIG-2〕+1：`apikey_remote`整删，点它的那一处挂墓碑 // 〔HX2 · 4D〕写 key 改走通道：头注点旧命令名
        ("src/settings/account-new-form.ts", 1), // 〔HX2 · 4D〕同上 // 〔HX2 · 4D〕+1：写 key 改走通道那一行点旧命令名
        ("src/settings/acct-deploy.ts", 3), // 〔MIG-3a · 09-28 预裁〕2 → 3：`deriveAcctIsoDir` 删了，原地一块 // // 〔DUP2 · J4〕头注记 `sq` · `buildAcctIsoCmd` 搬进了后端（帧命令 `acct-iso-cmd`）
        ("tests/backend/control/gate_tests.rs", 1),
        ("tests/backend/no_timer_guard.rs", 1),
        // 〔SR1b · 2026-09-24〕2 → 3：远端写那一层「今天一处远端写都没有」那条判据随 V89 改写成
        //   「只住一份文件、只许两处」，旧名留一块墓碑（`TOMBSTONED` 同拍登记）。
        ("tests/backend/readonly_guard.rs", 3),
        ("tests/bridge/backend/control/backend_kill_tests.rs", 3), // 〔DUP1〕+1：`@ccm_sid` 原先那份白名单 // 〔C4e 批 2〕+1：头注点删掉的发送端 ＋ 拒绝文案那条退役的墓碑（原那一块换成它）
        // 〔C4e 批 2〕`tests/bridge/backend/control/backend_send_keys_tests.rs` 那一行删了：整份随送键发送端迁到界面删掉（它钉的 mode 名 / 拒绝文案搬到界面与跨语言金样）。
        ("tests/bridge/backend/control/cc_bus_deploy_tests.rs", 1),
        // 〔BS1b 09-24〕2 → 7：`fn_body` 头注两处点那个删了的构造器 · 写面判据头注两处点那句删了的拒绝 ·
        //   改名那条判据头注里一处旧名。
        ("tests/bridge/backend/control/cc_bus_tests.rs", 1), // 〔SH1 · V136〕22 → 1：读面判据随 shell 读退役，只剩头注一处 // 〔C4e 批 3b〕7 → 22：写面五条迁到界面，钉它们的 18 条判据退役（两块整段墓碑逐行标）＋ 更早几块墓碑里指向它们的住址逐行补标
        ("tests/bridge/backend/control/inbound_client_tests.rs", 5), // 〔MIG-1〕4 → 5：会话 / tmux 账本搬进后端，原处墓碑与点旧名的散文 // // 〔C4e 批 2〕+3：`launch_args` 两条对拍退役 ＋ e2e 那条改指金样，原处与点旧名的散文各一块
        (
            // 〔LR2〕4 → 2：TS 兜底一族退役，两段带墓碑的头注（换人手续 · 前提触发器的旧头注）整段删了；
            //   留下的两处：`launch_via_backend` 那句 ＋ 本拍新贴的「本条原名 …」那句。
            "tests/backend/control/launch_render/launch_wire_f07_main_path_tests.rs",
            2, // 〔RST 续 · V41〕3 → 2：裸键 mode `send-keys-raw` 删了，守「它只有一个家」那条判据整条退役，它头注里点旧住址那块墓碑随之删 // 〔C4e 批 2〕+1：送键 mode 名的家从 monitor 搬到界面，点旧住址 · 〔LR2〕−2（见上）⇒ 4 +1 −2 = 3
        ),
        ("tests/bridge/backend/control/tmux_tests.rs", 16), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑 14 → 16 // 〔MIG-1〕13 → 14：会话 / tmux 账本搬进后端，原处墓碑与点旧名的散文 // // 〔SH1〕+1：守跨 SSH `tmux ls` 那条判据退役 // 〔C4e 批 2〕+3：本机杀会话 / 送键不许回落那两条退役 ＋ Gate 1 那条的说明点旧命令名 // 〔C4e〕+6：抓屏迁到界面，`KR112D2` 两刀 ＋ 五档那条随 monitor 那一份退役，原处与 Gate 1 那条的说明里点旧名
        ("tests/bridge/backend_tests.rs", 1),
        ("tests/bridge/byte_cap_registry_tests.rs", 11), // 〔MOD〕10 → 11 // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑 9 → 10 // 〔SH1〕+1：远端 `.claude.json` 那条上限删了；+1：钩子诊断读远端 settings.json 的上限删了；+2：驾驶舱读面那两个上限（名册 · 收件箱）随 shell 读删了 // 〔合并 LOC1b〕+1：F10b 那段病史点的远端读会话函数删了 // 〔C4e 批 3b〕+1：`K-R112` 地板那段点的查在线命令迁到界面 // 〔C4e〕+1：`K-R112` 地板那段点的抓屏命令迁到界面 // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        // 〔MIG-3b〕`hooks_diag.rs` 那一行随文件删了（钩子诊断整轴进后端，那块墓碑守的「远端拨号 shell」一起没了）。
        ("src/backend/footprint/rows.rs", 3), // 〔MIG-3b 续〕1 → 3：`claude_config_dir` · `config_surface_report` · `with_monitor_probe` 三处删了点旧名 // 〔MIG-3b〕新贴：`claude_config_dir` · `resolves_on_path` 从 `hooks_diag.rs` 挪来，点旧住址
        // 〔MIG-3a · 09-28 预裁〕`tests/bridge/acct_iso_deploy_tests.rs` 那一行随整份判据文件删了（它守的围栏与部署命令一起退役）。
        ("tests/bridge/capability_registry_tests.rs", 1),
        ("tests/bridge/ccm_cli_contract_tests.rs", 1),
        // 🔴 〔波 1/2 合并时补〕`P16㈢` 那一拍把「十一条全绿而归属待裁」那张表退役了，
        //   并在原位留了一块墓碑（三句齐：为什么有 · 谁裁的 · 所以它走了）。
        //   ⚠ 那块墓碑**刻意没点任何死符号的名字** ⇒ 它不需要 `TOMBSTONED` 加行，
        //   只需要本表这一行 —— 两张表守的是两件事，别混。
        // 🔴 〔波 4 合并时补〕`filewin/editor.rs` 那一段记的是**一个被现打证伪的旧读数**
        //   （「26 万字节排一帧 16.3 ms」是 debug 档 ＋ 全新 Context 的第一帧；release 是 2.5 ms）。
        //   按本表的口径它是墓碑：**不删那段话**，但挂上标记、登记在册。
        ("src/bridge/src/filewin/editor.rs", 3), // 〔F7c 收尾 09-24〕1 → 3（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）
        ("tests/bridge/comm_boundary_registry_tests.rs", 3), // 〔RW1〕+2：sftp.rs 那一行的阻塞随 F11 清空 · 头注里那个公开名字
        ("tests/bridge/crates/guard-core/lib_tests.rs", 1),
        ("tests/bridge/doc_claim_registry_tests.rs", 2), // 〔LOC1a〕+1：本机一次性 exec / 任务行解释 / 分叉 exec 那几条路删了，点旧名的散文挂墓碑
        ("tests/bridge/filewin/entry_tests.rs", 2), // 〔MIG-3a · 09-28 裁 3〕1 → 2：旧名那一块随两条判据退役，新贴两块点它们
        ("tests/backend/assets/aliases/fence_tests.rs", 3), // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕+2：序列那五条判据的墓碑（`apply` 两处） // 〔TL1 · 4C〕新：写规则那条的死值验说明点代装 rc 那一跳（已退役）的旧名
        // 🔴〔本机侧退役 2026-09-23〕`parent_dir` 只剩一个算法 ⇒ 那条判据改了名
        //   （旧名尾巴上那半判的是本机那一支）。旧名逐字留着说明「它为什么改了」。
        ("tests/bridge/filewin/source_tests.rs", 2), // 〔F7c 收尾 09-24〕1 → 2（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑）
        // 〔AL1 · 2026-09-24〕+5：`write_account_aliases` 退役那一行 ＋ 增量账里它那一行 ＋ 合并主线时
        //   三个计数旁的增量注（`EXPECTED_LOCAL_OR_BOTH` · `LEDGER.len()` · 增量账）。
        ("src/bridge/src/asset_sync.rs", 2), // 〔MIG-3a〕+2：`AssetsSynced` 那几个形状 ＋ 界面那条 Tauri 命令随同步那一问走通道删了
        ("tests/bridge/asset_sync_tests.rs", 1), // 〔MIG-3a〕+1：「应答缺格就报错不猜」那一条挪到界面（`parse_reply` 删了）
        ("src/bridge/src/user_files.rs", 8), // 〔MIG-3b 续〕+2：读改写那一环（`edit` / `Edited`）删了，原地一块 // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔MIG-3a · 子步 3〕+2：`rename` / `stat_kind` 两形零调用方删了，trait 上留墓碑 // // 〔MIG-3a〕+1：`rel_under` / `join_under` 随别名进后端删了，留一句墓碑 // // 〔MIG-3a〕+1：列目录那一形（list_dir）随收件箱进后端删了，trait 上留一句墓碑 // // 〔MIG-3a〕+1：只删空目录那一形随卸 skill 进后端删了
        ("tests/bridge/parity_ledger_tests.rs", 69), // 〔MIG-3b 续〕67 → 69：足迹那两处（命令行 · ORIGIN 那一条）出表 // 〔MIG-3b 续〕66 → 67：全景问 · 写 · 撤三行出表那一块 // 〔MIG-3b 续〕+1：`Mixed` 那一行点公钥推送旧命令名 // 〔MIG-3a · 09-28 预裁〕61 → 65：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） // 〔合并 MIG-3a × 主线 ad308378〕基数 44 ＋ MIG-3a +10 ＋ 主线 +5 ⇒ 59 // 〔MIG-3a〕+1：`assets_sync` 那一条理由随命令摘掉 // // 〔MIG-3a〕+1：理由表摘掉 MCP 三条处一块 // // 〔SH1〕+2：acct-iso 两对合一处点的旧名 // 〔合并 HX2 × 主线 290d8c33〕主线 39 ＋ HX2 +1（写 key 那条命令退役） // 〔合并 LOC1a × 主线 1c2c4c97〕主线 37 ＋ LOC1a +2（`get_session_tasks` 退役：LEDGER 那一行 ＋ 理由表那一行）// 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑 // 〔合并 US1 × 主线〕主线 35 ＋ US1 +1 // 〔C4e 批 3b〕33 → 35（cc-bus 写面五条退役：LEDGER 那一块 ＋ `FRAME_PLANE_VERDICTS` 那三行） // 〔C4e 批 2〕28 → 33（杀会话 · 送键 · 就地 resume 三条命令退役：LEDGER 两处 ＋ `FRAME_PLANE_VERDICTS` 那一行 ＋ `tmux.manage` 那条理由第七次订正 ＋ `设计/50` 那一层点的送键命令） // 〔C4e〕23 → 28（抓屏那条命令退役：LEDGER 那一行 · `FRAME_PLANE_VERDICTS` 那一行 · 地板那段 · `tmux.manage` 那条理由里 `设计/50` 那一层点的发送端 ＋ 第七次订正） // 〔合并 C4d × 主线 cf3277f4〕主线 22 ＋ 本路 +1 ⇒ 23（〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑） // 〔合并 CF2 × 主线 28a5f652〕主线 18 ＋ 本路 +4（`replay_keep_tail_only` / `replay_session_to_window` 各退役：LEDGER 那一行 ＋ Local/Both 账那一句）⇒ 22 // 〔合并 RM1f × 主线 60ace11b〕主线 17 ＋ 本路 1 ⇒ 18（PN1b 那行增量账里 `panorama_diagram_kinds` 随内嵌引擎退役，挂墓碑） // 〔C4c〕16 → 17（「退出行为」两条退役，`ORIGIN_TAKING_BOTH` 摘掉处一块） // 〔合并 C4c × 主线 6b375621〕12 ＋ 本路 2 ＋ AL1d 2 ⇒ 16 // 〔C4c〕12 → 14（信任预检 · 记录那一问两条命令退役，`ORIGIN_TAKING_BOTH` 那两条摘掉处各挂一块） // 〔AL1d〕12 → 14（「终端集成」退役的两行 LEDGER 注释挂墓碑） // 〔C4a · 第四波〕11 → 12（E79 那条本机会话账号命令退役，账本那一行挂墓碑） // 〔F7c 收尾 09-24〕10 → 11（删掉的 sftp_* 命令 / 函数 / 判据名，逐处挂墓碑） // 〔MIG-2〕44 → 49
        ("tests/bridge/plugin_class_registry_tests.rs", 3), // 〔RM1f〕2 → 3：`code-picture` 那一格对上之后，旧测试名挂墓碑
        ("src/panorama-engine/main.rs", 1), // 〔RM1f〕monitor 那份按文件列符号的旧函数名（随内嵌引擎删了）
        ("tests/bridge/panorama_seam_registry_tests.rs", 2), // 〔MIG-3b 续〕1 → 2：问 · 写 · 撤三条随界面直问删了那一行 // 〔RM1f〕PN1b 那一行增量账里的旧命令名（随内嵌引擎退役）
        ("tests/backend/panorama_locus_guard.rs", 1),        // 〔RM1f〕正题③改名前的旧测试名
        ("tests/panorama/diagram-guards.vitest.ts", 1), // 〔RM1f〕它原先点的那条 monitor 真引擎判据（随内嵌引擎删了）
        ("tests/bridge/polling_registry_tests.rs", 1),
        // 〔AL1 · 2026-09-24〕+1：`rollback_note_matches_what_actually_happened` 搬走的那块墓碑。
        ("tests/bridge/sftp_tests.rs", 16), // 〔MIG-3a · 09-28 预裁〕14 → 16：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔MIG-3a · 09-28 预裁〕12 → 14：比标记真值表 · 受管路径谓词两条判据随函数删了 // // 〔E2〕10 → 12：点两条删掉的判据（围栏 · 远端入口） // 〔W5-ALIAS · 删 `fenced_block::apply` 那一族〕5 → 8：读取器四条判据的墓碑（四块）＋ 回报换型那一句（一块）进；随判据删掉的两块出 // 〔SR1b〕+1：`SftpFile` 改名 `RemoteFile`
        ("tests/bridge/shared_crate_registry_tests.rs", 1),
        // 〔`C1` · 09-24〕快照那一格的墓碑（`parse_snapshot_meta` 随改走长连接删了）。
        ("tests/bridge/ssh_source_snapshot_tail_tests.rs", 1),
        ("tests/bridge/structural_scan_tests.rs", 1),
        // 〔MOD〕`tests/bridge/subagent_tests.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("tests/bridge/tmux_backend_gate_guard_tests.rs", 6), // 〔C4e 批 2〕+3：看住的两个命令与四条判据随命令迁到界面翻面，原处与两张表各一块
        ("tests/backend/footprint/registry_environment_tests.rs", 1),
        ("tests/evidence/K-R20-C-deadname-census.py", 1),
        ("tests/evidence/S29-legacy-compat-census.py", 11),
        ("tests/ipc/commands.vitest.ts", 11), // 〔MIG-3a〕+2：计数行里点收件箱两条旧名的那两行挂墓碑 // // 〔SH1〕+1：头注计数那段点的远端 shellinit 旧名 // 〔AL1〕+2：K-R49 增量账里 `write_account_aliases` 那两行 // 〔C4b〕+1：「刻意不同」那条判据合并后改名，本机只有一个表示那一节点它旧名 ·〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        // 〔C4b · 第四波 4B〕`"__local__"` 合进 `LOCAL_ORIGIN`，「两个同名常量刻意不同」那条判据改成钉合了之后的形状，旧名挂一块。
        ("tests/bridge/backend_policy_tests.rs", 2), // 〔C4c〕+1：「空 origin 必须拒」那条随命令退役，原处一块
        // 〔C4b · 第四波 4B〕会话读面三条 Tauri 命令（骨架索引 · 大纲清单 · 会话内查找）退役、改走通道：
        //   点它们旧名的来历段各挂一块（包装层两段 · 新住址头注 · 骨架模块头注 · 判据替身头注）。
        // 〔MOD〕`src/bridge/src/session_skeleton.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("src/bridge/src/snapshot_resume.rs", 1),
        ("src/ipc/commands.ts", 13), // 〔MIG-3b 续〕12 → 13：`config_surface_report` 包装删了那一句 // 〔MIG-3a · 09-28 预裁〕11 → 12：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔MIG-3a · 子步 3〕+2：cc-bus 装 / 三态两条包装退役的墓碑 // // 〔MIG-3a〕+1：别名六条包装退役那一行墓碑 // // 〔MIG-3a〕−1：`list_mcp_project_dirs` 包装层删了，它头注那一块随之走 // // 〔SH1〕+1：远端项目目录旧名那句（原靠那个函数还在兜着） // 〔合并 HX2 × 主线 06b5dc08〕基数 6 ＋ LOC1b +1 ＋ HX2 +1 ⇒ 8。LOC1b 原注：〔合并 LOC1b × 主线 290d8c33〕基数 5 ＋ LOC1b +1（流式读会话那一行点的远端读会话函数删了）＋ CFG1 +1（整份写口 `save_config` 删了）⇒ 7 · 会话读面两段 ＋ 插件市场一段 ＋〔CF2〕`replay_keep_tail_only` · `replay_session_to_window` 退役各一段 ｜ HX2 原注：〔合并 HX2 × 主线 8ffc6bdf〕主线 6 ＋ HX2 +1（写 key 那条包装退役） // 〔CFG1〕+1：整份写口 `save_config` 删了，讲来历那一句挂墓碑 // 会话读面两段 ＋ 插件市场一段 ＋〔CF2〕`replay_keep_tail_only` · `replay_session_to_window` 退役各一段
        ("src/session-reads.ts", 3), // 〔C4c〕+2：第四问（记录还在不在）头注点名它替掉的命令与发送端
        ("src/settings/plugins-section.ts", 1),
        ("tests/settings/plugins-section.vitest.ts", 1),
        ("tests/test-support/chan-fake.ts", 8), // 〔MOD〕7 → 8 // 〔MIG-1 续〕列 tmux 会话改走通道，旧命令名挂墓碑 6 → 7 // 〔MIG-2〕+1：起会话渲染那一节点六条旧命令名 // 〔C4e 批 3b〕+1：cc-bus 驾驶舱那一节点五条旧命令名 // 〔C4e〕+1：tmux 控制类那一节点四条旧命令名 // 〔C4c〕+1：账号那两问的翻译节点名三条旧命令名 ·〔C4d〕远端会话清单那个函数随历史清单搬进本机后端删了，点它的散文挂墓碑
        // 〔F7c 收尾 09-24〕SFTP 那一族收到只剩传输：删掉的命令 / 函数 / 判据名在这几份里逐处挂了墓碑。
        ("src/bridge/src/filewin/mod.rs", 2),
        ("src/bridge/src/filewin/transfer.rs", 4),
        ("src/bridge/src/filewin/writeops.rs", 1),
        // 〔FILES2 · 第四波〕0 → 1：`refused_in_lossy_cwd` 删了（有损目录里三件改按字节做），原处墓碑。
        ("src/bridge/src/filewin/shell.rs", 1),
        // 〔第四波 S4〕`sftp_pool.rs` 1 → 0（行删）：那块墓碑随零流量复制一段整块删了，理由同 `TOMBSTONED` 那一行。
        ("tests/bridge/filewin/boundary_tests.rs", 2),
        ("tests/bridge/filewin/transfer_tests.rs", 1),
        ("tests/bridge/sftp_family_registry_tests.rs", 2),
        // 〔SR1b〕`sftp_move_ledger_tests.rs` 1 → 0（行删）：那块墓碑住乙「死连接重建」那一行里，乙整份过界、那一行摘了。
        // 〔SR1b〕5 → 0（行删）：同上 `TOMBSTONED` 那段（整份重写，守的轴搬进了本机后端）。
        // 〔SR1b〕`sftp_pool.rs` 0 → 3：模块头注那段墓碑（通道闸 · 暂存区上传 · 本机落地三样搬走了）。
        ("src/bridge/src/sftp_pool.rs", 3),
        // 〔FW5 · 第四波〕两处墓碑标记：写面相对段「只收 UTF-8」那句（围栏改按 `Path` 判之后作废）·
        //   选中那张表里「批量改权限没做」那一格（做了）。〔合并 RW1〕+1：`hollow_read` 那一道从 monitor 搬来。
        ("src/backend/control/files_write.rs", 2),
        // 〔HX1 · 4D〕`main.rs` 里等停机信号的函数下沉 `platform/signal.rs::shutdown_listener`，原处留一块。
        ("src/backend/main.rs", 1),
        ("src/bridge/src/filewin/select.rs", 1),
        // 〔第四波 S4〕快捷键预留位 `app.search-history` 删了（历史全文搜索从没独立快捷键、预留位不留）：
        //   清单那一处 ＋ 清单头注一处 ＋ 编辑器那枚「未上线」标签的遗址一处，逐处挂了墓碑。
        ("src/keybindings/actions.ts", 2),
        ("src/keybindings/editor.ts", 1),
        ("tests/keybindings/actions.vitest.ts", 1),
        // 〔C4c · 第四波 4B〕账号清单与信任预检改走通道（后端出成品）：monitor 那几件函数与它们的判据删了，
        //   逐处挂了墓碑（模块头注 · 退役那几节的旁注 · 后端出成品那一臂点名它替掉了谁）。
        ("src/backend/faces/read_face.rs", 2), // 〔MOD〕1 → 2
        // 〔C4d〕`tests/bridge/accounts_tests.rs` 那一行去掉：整份随 `accounts.rs` 删了（守的那条 serde 名对拍随 Rust 枚举一起退役，字面量今天由后端金样 ＋ TS 解码器钉）。
        // 〔C4c · 第四波 4B〕记录那一问（`history-record`）改走通道：monitor 的发送端与它的判据删了，原处各一块。
        ("src/bridge/src/backend/control/frame_query.rs", 7), // 〔MOD〕3 → 7 // 〔MIG-3b 续〕2 → 3：点 `panorama_call.rs` 旧住址那一句 // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        ("tests/bridge/backend/control/frame_query_tests.rs", 21), // 〔MOD〕17 → 21 // 〔MIG-3b 续〕16 → 17：足迹那一行点旧发送点 // 〔MIG-3b 续〕15 → 16：全景那两行点旧发送点 // 〔MIG-3a · 09-28 预裁〕14 → 15：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 13 → 14 // 〔合并 MIG-3b × 主线 76b31e7c〕主线 12 ＋ MIG-3b +1（点分叉旧判据文件那一句） // 〔MIG-3a〕+1：skill 装卸 ＋ 同步那一块点删掉的四条命令 // // 〔MIG-3a〕+1：`CHANNELED_ELSEWHERE` 的 MCP 那一块点删掉的两份文件 // // 〔SH1〕+1：本机 shellinit 那一个期限发起点并掉 // 〔HX2 · 4D〕+3：`apikey-read` 那一行说核路径那一问删了 · `apikey-key-set` 新行点删掉的命令 · `ASKED_BY_MONITOR_ITSELF` 退役那一行 // 〔C4e 批 3b〕+1：`CHANNELED_ELSEWHERE` 的 cc-bus 那一块点删掉的五条命令 // 〔C4e 批 2〕+1：`kill` / `launch` 两行点删掉的三条命令 // 〔C4e〕+1：`CHANNELED_ELSEWHERE` 的 `capture-pane` 那一行点删掉的发送端 // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑 // 〔MIG-2〕11 → 12
        // 〔C4c · 第四波 4B〕「退出行为」问 / 交写改走通道：monitor 那两条命令与它们的期限删了，原处各一块；设置页头注点它们旧名。
        ("src/bridge/src/backend_policy.rs", 2),
        ("src/settings/backend-section.ts", 2), // 〔MIG-2〕1 → 2
        // 〔RN1 · 第四波 4C · V114〕改名「上游选择」、模块 `apikey` → `upstream`：两处讲旧叫法来历的注释各挂一块
        //   （旧叫法本身不是 snake_case 死名，不进 `TOMBSTONED`；命名判据 `account-vs-relay-naming` 的 V114 那张表按这块标记放行这两行）。
        // 〔NT2 · V25〕+1：上游选择自己那张每 agent 默认上游表（`AGENT_UPSTREAMS`）搬回适配层，原处留一块说去向。
        ("src/backend/accounts/upstream/mod.rs", 4),
        ("src/backend/relay/mod.rs", 1),
        ("src/bridge/src/ssh_source.rs", 24), // 〔MOD〕23 → 24 // 〔MIG-3b 续〕+1：流那一个一次性 exec 原语删了，原地一块 // 〔MIG-1 收尾〕地址解析 / 组拨号请求搬进后端 dial/machine.rs，点旧名的散文挂墓碑 21 → 22 // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑 15 → 21 // 〔MIG-1〕5 → 15：会话 / tmux 账本搬进后端，原处墓碑与点旧名的散文 // // 〔MIG-1〕+3：`~/.ssh/config` 导入搬进后端，原地留一段点三条旧命令名的墓碑 // 〔LOC1b〕+1：「未登记的会话 kind」那一笔从 `session_map.rs` 搬来，头注点旧函数名 // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        // 〔MOD〕`src/bridge/src/subagent.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("src/backend/footprint/registry.rs", 10), // 〔MIG-3b 续〕11 → 10：Claude 布局那一半搬进适配层，带走一块（`IndirectWrite` 那条 `~/.cc-bus/` note） // 〔MIG-3a · 09-28 预裁〕10 → 11：头注点当年那条部署命令 // // 〔E2 · 子步 4〕6 → 10：本机 `ccm` 载体的来源 / 头注里点副本与 shim 旧名处（四块） //, // 〔E2〕5 → 6：点 `is_safe_remote_backend_path`（卸载守卫随固定落点删了） // 〔SH1〕+1：远端 acct-iso 探测旧名 // 〔SH1〕+1：点 `build_online_cmd` 的那句原靠 monitor 驾驶舱那份墓碑兜着，那份删了，就地补标 // 〔C4e 批 3b〕+2：`IndirectWrite` 那一档与 `~/.cc-bus/` 那条 note 点的写面旧命令名（写面迁到界面） // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑
        ("tests/bridge/exec_site_registry_tests.rs", 5), // 〔合并 MIG-3b × 主线 76b31e7c〕跑出来 5（两路各给远端 `ccm` 探针那两句挂了墓碑，合并取主线那两句；另两处是 MIG-3b 点旧名处） // 〔C4d〕逐次拨号那条路删了（run_list_query 一族），点旧名的散文挂墓碑 // 〔MIG-2〕1 → 3
        // 〔MOD〕`tests/bridge/remote_history_tests.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("tests/backend/observe/accounts_query_tests.rs", 2), // 〔C4d〕本机账号参照实现删了（list_from_dir 一族 ＋ accounts.rs 整份），点旧名的散文挂墓碑 // 〔MIG-2〕1 → 2
        ("src/backend/agents/codex/history.rs", 2), // 〔C4d〕历史跨机 join 进本机后端：会话行口径收成一份 ＋ Codex 历史搬进适配层，点 monitor 那几份旧实现 / 退役判据的散文挂墓碑
        ("src/backend/observe/history_query.rs", 3), // 〔MOD〕2 → 3 // 〔C4d〕历史跨机 join 进本机后端：会话行口径收成一份 ＋ Codex 历史搬进适配层，点 monitor 那几份旧实现 / 退役判据的散文挂墓碑
        ("tests/backend/observe/history_query_tests.rs", 1), // 〔C4d〕历史跨机 join 进本机后端：会话行口径收成一份 ＋ Codex 历史搬进适配层，点 monitor 那几份旧实现 / 退役判据的散文挂墓碑
        ("tests/backend/faces/read_face_tests.rs", 1), // 〔C4d〕历史跨机 join 进本机后端：会话行口径收成一份 ＋ Codex 历史搬进适配层，点 monitor 那几份旧实现 / 退役判据的散文挂墓碑
        ("tests/backend/history/history_annotations_tests.rs", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑
        // 〔C4e · 第四波 4C〕抓屏（`capture-pane`）改由界面经通道直接问：monitor 的 Tauri 命令、发送端、参数构造器与它们的判据删了，
        //   原处与点它们旧名的散文各一块。
        // 〔C4e 批 2〕`src/bridge/src/backend/control/command_args.rs` 那一行删了：整份随它最后两个调用方（送键 · 就地 resume 的发送端）删掉。
        ("src/bridge/src/ccm_probe.rs", 4), // 〔MIG-2〕1 → 4
        ("src/tmux-control.ts", 1),
        // 〔C4e 批 3b〕cc-bus 驾驶舱写面五条改由界面经通道直接说：新家的头注点 monitor 那五条旧命令名。
        ("src/cc-bus-control.ts", 4), // 〔DUP2 · J12〕+3：TS 副本 `isValidBusId` / `refuseBadId` 删了（原处一行两块）· `checkSpawnShape` 头注点它一块
        ("src/bridge/src/backend/mod.rs", 3), // 〔SH1〕+1：本机 acct-iso 那条旧名 // 〔LOC1a〕+1：`observe/` 那条线删了，`pub mod` 旁那一句 // 〔C4e 批 3b〕归属表里 `control/cc_bus.rs` 那一格点的写面四条旧命令名
        ("src/views/pane-preview.ts", 1),
        ("tests/bridge/backend/control/backend_route_tests.rs", 9), // 〔MIG-3b 续〕8 → 9：足迹发送端那一行删了、留墓碑 // 〔MIG-3b 续〕7 → 8：全景发送端那一行删了、留墓碑 // 〔合并 MIG-1 × 主线 eebf51de〕两边各自贴的墓碑相加，按盘上现数（跑出来核过） 6 → 7 // 〔MIG-2〕+1：`apikey_remote`整删，发送端表摘掉那一行处一块挂墓碑 // 〔MIG-3a〕+1：发送端表摘掉 `mcp_sync.rs` 那一行处一块 // // 〔C4e 批 2〕+1：SENDERS 头三行（三个发送端）摘掉的那一块
        // 〔C4e 批 2〕杀会话 · 送键 · 就地 resume 三条迁到界面：调用方头注 / 注释里点旧命令名的地方各一块。
        ("src/account-restart.ts", 2),
        ("src/launch-cli-wire.ts", 1),
        ("src/remote-launch-run.ts", 3), // 〔DUP1〕+1：`launcherOrDefault` 头注点它替掉的 `sanitizeRemoteLauncher`
        ("src/bridge/src/tmux_backend_gate_guard.rs", 3),
        ("tests/bridge/launcher_identity_registry_tests.rs", 3), // 〔MIG-3a · 09-28 预裁〕2 → 3：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 //
        // 〔C4e 批 3〕广播收进后端：后端那一节头注点 monitor 里那份组合（`broadcast_via_backend`，同批随后删掉）。
        ("src/backend/control/cc_bus.rs", 2), // 〔SH1〕+1：`bus-state` 头注点的 monitor 驾驶舱读名册命令删了
        //   几份 vitest 的 invoke 替身换成通道那一跳的翻译（`chan-fake.ts::tmuxControlShim`），头注点旧命令名。
        ("tests/account-restart.vitest.ts", 1),
        ("tests/remote-launch-run.vitest.ts", 4), // 〔DUP1〕+1：非法 sid 那条改测「渲染侧拒」
        ("tests/send-into-backend.vitest.ts", 2),
        ("tests/tabs.vitest.ts", 3), // 〔MIG-1〕1 → 3：⑬ 退役的会话清单/活动命令与 buffered_* 旧名挂墓碑
        ("tests/views/pane-preview.vitest.ts", 1),
        ("tests/backend/history/history_join_tests.rs", 1), // 〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑
        // 〔MOD〕`tests/bridge/history_title_coverage.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("tests/bridge/origin_tests.rs", 9), // 〔MOD〕7 → 9 // 〔合并 MIG-3b × 主线 19671e6b〕主线 5 ＋ MIG-3b +2（远端分叉 · 远端删会话两个分支函数删了）// 〔MIG-3a〕+2：远端 MCP 写 / 删两个分支函数随 `mcp.rs` 删了 // // 〔SH1〕+1：远端项目目录那个分支删了 // 〔合并 LOC1b〕+1：远端读会话函数（本机远端合成一条） // 〔C4d〕历史清单与注解搬进本机常驻后端，点旧名的散文挂墓碑
        // 〔US1 · 4D〕上游选择那半（决策表 · 人群 · 路由语法 · 本机读侧）从 monitor 搬走：点旧名的散文挂墓碑。
        //   既有行里跟着变的：upstream/mod.rs 2→4 · apikey_remote.rs 2→3 · payload.rs 1→4 · history.rs 14→17 · lib.rs 9→10 ·
        //   local_backend_host.rs 1→2 · file_face_tests.rs 4→6 · apikey_remote_tests.rs 1→2 · creds_store_tests.rs 1→2 ·
        //   history_tests.rs 11→12 · parity_ledger_tests.rs 23→24 · creds_store.rs 3→4（各多一处新墓碑）。
        //   ⚠ 两处旧墓碑随它们所在的整段一起走了（被守的那件事整轴退役）：lib.rs 里 `apikey_routing_for` 头注那一处（点当年的账号结构）·
        //   creds_store_tests.rs 里读侧三态那条判据头注那一处（点当年 monitor 的写口）—— 两处的净数已算在上面。
        ("src/backend/accounts/upstream/file_face.rs", 2), // 〔US1 · 4D〕新贴：上游选择那半从 monitor 搬走时留下的墓碑
        ("src/bridge/Cargo.toml", 1), // 〔US1 · 4D〕新贴：上游选择那半从 monitor 搬走时留下的墓碑
        ("tests/backend/relay/route_tests.rs", 1), // 〔US1 · 4D〕新贴：上游选择那半从 monitor 搬走时留下的墓碑
        ("tests/backend/control/launch_render/payload_tests.rs", 4), // 〔SH1〕+1：`resolve_bash` 的平台那一格随驾驶舱 shell 读删了 // 〔DUP1〕+1：launcher 两份策略那段里 TS 那一份删了 // 〔US1 · 4D〕新贴：上游选择那半从 monitor 搬走时留下的墓碑
        ("tests/bridge/local_backend_host_tests.rs", 8), // 〔合并 MIG-1 × 主线 862be034〕主线 3 ＋ MIG-1 本路增量 ⇒ 8（盘上现打） // 〔US1 · 4D〕新贴：上游选择那半从 monitor 搬走时留下的墓碑 // 〔MIG-2〕1 → 3
        // 〔LOC1a · 第四波 4D〕本机四个一次性 exec 改走 `<local>` 长连接（`local_query` 一族删、monitor 侧 `observe/` 删）·
        //   `tasks-list` 出成品（`get_session_tasks` / `parse_task_lines` 删）· acct-iso argv 形退役 · 分叉 exec 那条路删：
        //   原处与讲来历的散文各挂一块。
        ("src/backend/accounts/iso.rs", 2), // 〔SH1〕+1：远端 shellinit 旧名
        ("src/backend/observe/tasks_query.rs", 1),
        ("src/backend/control/fork_write.rs", 1),
        ("src/bridge/src/cross_half_edge_registry.rs", 2), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑 1 → 2
        ("src/bridge/src/doc_claim_registry.rs", 1),
        // 〔MIG-3b〕`tasks.rs` 那一行随文件删了（本机任务 notify 整轴进后端，那几块墓碑守的来历一起走了）。
        ("tests/bridge/backend_layering.rs", 1),
        ("tests/bridge/spawn_managed_exit_sites.rs", 3), // 〔SH1〕+1：本机 cc-bus 读那一处出表
        ("tests/bridge/write_site_registry_spawn_sites.rs", 3), // 〔SH1〕+1：本机 cc-bus 读那一处出表
        // 〔DUP1 · 4D〕`设计/90 §3` 判据 2：`accounts.ts` 里 `auth_ready` 订阅分支的第二份（带「旧后端」回落的 `authReady()` 包装）删了，
        //   点它的散文各挂一块：`Account.authReady` 字段头注那一处 · KAY4 判据头注第 4 条那一处。
        ("src/accounts.ts", 3), // 〔MIG-3a · 09-28 预裁〕2 → 3：点 `deploy_remote_acct_iso` 旧名那几处挂墓碑 // // 〔MIG-2〕1 → 2
        ("tests/account-availability-guard.vitest.ts", 1),
        // 〔DUP1 · 4D〕同一判据 J2 / J3：TS 的 `isValidConfigDir`（渲染侧 configDir 拒绝集的手抄）与 `sanitizeRemoteLauncher`
        //   （同一字符集、却静默换成默认 launcher）删了，点它们的散文各挂一块。
        ("src/shell-quote.ts", 6), // 〔DUP1〕+2：头注记 `isValidSessionId` · `isValidModelName` 删了 ·〔DUP2 · J6〕+2：两个 tmux 名谓词删了
        ("src/launch-dimensions.ts", 4), // 〔DUP1〕+2：identity 维度原先先过 `isValidSessionId` · model 维度原先先过 `isValidModelName` ·〔DUP2〕+1：令牌形状的 TS 副本 `isValidRbindToken` 删了
        ("tests/launch-dimensions.test.ts", 3), // 〔DUP1〕+2：identity · model 两条改测「前端不判」
        ("tests/remote-launch.test.ts", 4), // 〔DUP1〕+2：`isValidSessionId` 五条 · 直起非法 sid 那条
        ("tests/test-support/launch-payload-golden.ts", 1),
        // 〔DUP1 · 第二轮〕sid 那一族（J5）：TS `isValidSessionId` 与只剩那一格的 `validateLocalLaunch` 删了、wire 多报一格 `resumeSid`。
        ("src/backend/control/launch_render/wire.rs", 1),
        ("src/launch-requests.ts", 2),
        ("src/local-resume.ts", 1),
        ("tests/launch-requests.vitest.ts", 2),
        ("tests/launch-orchestration-single-home.vitest.ts", 1),
        // 〔DUP1 · 第二轮〕模型名那一格（J17）：设置里写入点那一句原先调 TS `isValidModelName`。
        ("src/account-prefs.ts", 1),
        // 〔E2 · V28〕`backendPath` · 远端三行入口 · 逐字节副本退役，点旧名的散文挂墓碑。
        // 〔MIG-3a · 09-28 预裁〕`acct_iso_deploy.rs` 那一行随整份文件删了（字节随后端二进制走）。
        ("src/acct-iso-reads.ts", 1), // 〔MIG-3a · 09-28 预裁〕新贴：头注点当年那条部署命令
        ("tests/bridge/command_home_registry_tests.rs", 3), // 〔MOD〕2 → 3 // 〔MIG-3b 续〕1 → 2：足迹那一行已迁 // 〔MIG-3a · 09-28 预裁〕新贴：待迁那一行删了，原处一块
        ("src/backend/assets/acct_iso_install.rs", 1), // 〔MIG-3a · 09-28 预裁〕新贴：头注点当年 monitor 那条部署命令
        ("tests/settings/acct-deploy.vitest.ts", 1), // 〔MIG-3a · 09-28 预裁〕新贴：`deriveAcctIsoDir` 那一组判据随函数删了
        ("src/bridge/src/filewin/entry.rs", 2), // 〔MIG-3a · 09-28 裁 3〕新贴：开窗前那两问进窗口进程，monitor 侧三个函数退役
        ("src/bridge/src/quote_singleton_guard.rs", 1), // 〔MIG-3a · 09-28 裁 2〕新贴：病史里那第五份 sq 随跑安装脚本退役
        ("src/bridge/src/ccm_legacy.rs", 1), // 〔合并 MIG-3b × 主线 81f92f6a〕两边各自贴的墓碑相加，按盘上现数（跑出来核过）
        ("src/bridge/crates/deploy-core/src/lib.rs", 1),
        ("tests/bridge/ccm_legacy_tests.rs", 1),
        ("tests/bridge/lib_invariant_population_tests.rs", 2),
        ("tests/bridge/ssh_source_tier1_tests.rs", 1),
        ("src/settings/machine-card.ts", 1), // 〔E2〕「后端路径」那一格删了，按用户名预填它的函数原地留一块
        ("src/backend/control/ccm/mod.rs", 3), // 〔V151〕抢词表 `routes_to_backend` 删了，原地一块；〔09-27〕入口②（`<bin> ccm …`）删了，头注与 `SUBCOMMAND_WORD` 各一块
        ("src/backend/control/ccm/plan.rs", 1), // 〔09-27〕`self_argv` 的入口②那一形删了，原地一块
        ("tests/e2e/backend-cc-bus.sh", 1),    // 〔09-27〕[17] 前的入口②说明删了，原地一块
        ("tests/e2e/backend-gate2-acceptance.sh", 1), // 〔E2 尾 09-27〕`meta_dollar` 那条登记豁免换成版本门，原地一块
        ("tests/backend/control/ccm_tests.rs", 1), // 〔V151〕那条路由判据并进 claude_flags_tests，原地一块
        // 〔FIX · `99 §2 ㊷`〕人读表解析器删了，一处点它旧名的散文挂墓碑。
        ("tests/backend/plugin_walk_fixture.rs", 1),
        ("src/main.ts", 4), // 〔MIG-1 续〕会话起停两个裸事件名（已并进会话流）挂墓碑 2 → 4 // 〔MIG-1〕新贴：⑬ 会话生命周期并进会话流，退役的 list_active_sessions / list_session_activity / buffered_* 旧名挂墓碑
        ("src/tab-session-state.ts", 1), // 〔MIG-1〕新贴：⑬ 会话生命周期并进会话流，退役的 list_active_sessions / list_session_activity / buffered_* 旧名挂墓碑
        ("src/tab-store.ts", 1), // 〔MIG-1〕新贴：⑬ 会话生命周期并进会话流，退役的 list_active_sessions / list_session_activity / buffered_* 旧名挂墓碑
        ("src/tabs.ts", 1), // 〔MIG-1〕新贴：⑬ 会话生命周期并进会话流，退役的 list_active_sessions / list_session_activity / buffered_* 旧名挂墓碑
        ("tests/bridge/event_replay_tests.rs", 1), // 〔MIG-1〕新贴：⑬ 会话生命周期并进会话流，退役的 list_active_sessions / list_session_activity / buffered_* 旧名挂墓碑
        // 〔MIG-3b〕`history.rs` 的 `up_to_message_id` · `read_jsonl_values` · `write_branch_file` 三行摘了：点它们的那段（分叉转交的来历）随命令删了。
        ("src/bridge/src/parity_ledger.rs", 1), // 〔MIG-3b〕新贴：点远端 `ccm` 探针旧命令名
        ("tests/bridge/agent_dispatch_registry_tests.rs", 1), // 〔MIG-3b〕新贴：点 LOC1b 删掉的那个门面名
        ("src/README.md", 1), // 〔MIG-3b〕新贴：点分叉那条旧命令名的散文挂墓碑
        ("src/bridge/crates/branch-core/src/lib.rs", 1), // 〔MIG-3b〕新贴：删会话 / 分叉 / 钩子诊断的转交退役，点旧名的散文挂墓碑
        ("tests/branch-button.vitest.ts", 1), // 〔MIG-3b〕新贴：删会话 / 分叉 / 钩子诊断的转交退役，点旧名的散文挂墓碑
        ("tests/views/history-actions.vitest.ts", 1), // 〔MIG-3b〕新贴：删会话 / 分叉 / 钩子诊断的转交退役，点旧名的散文挂墓碑
        ("src/account-reads.ts", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/agent-profile.ts", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/backend-policy.ts", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/backend/accounts/upstream/endpoint.rs", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/backend/observe/accounts_query.rs", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/bridge/src/sync_command_registry.rs", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        // 〔MOD〕`tests/bridge/support/scripted_backend.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        ("tests/bridge/sync_command_registry_tests.rs", 3), // 〔MIG-2〕+1：`apikey_remote`整删，点它的那一处挂墓碑 // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("tests/bridge/utils_tests.rs", 1), // 〔MIG-2〕起会话的计划与渲染搬进后端（`99 §2.1 ⑬`）：墓碑随搬家换住址 / 点已删命令名的散文挂墓碑
        ("src/backend/common/tmux_utf8.rs", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/mod.rs", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/backend/observe/tmux_list.rs", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("src/tmux-reads.ts", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("tests/backend/observe/tmux_list_tests.rs", 1), // 〔MIG-1 续〕列 tmux 会话那一族搬进后端（tmux-list 出成品），旧名挂墓碑
        ("tests/tmux-cache-single-writer.vitest.ts", 1), // 〔MIG-1 续〕列 tmux 会话改走通道，旧命令名挂墓碑
        ("tests/tmux-reads.vitest.ts", 1), // 〔MIG-1 续〕列 tmux 会话改走通道，旧命令名挂墓碑
        ("tests/backend/dial_probe_tests.rs", 1), // 〔MIG-1 续〕测试连接搬进本机后端，旧名挂墓碑
        // 〔MIG-2〕基数 → 增量 +1 行：`apikey_remote`整删，「形状照它」那句挂墓碑。
        // 〔MIG-3b 续〕`src/bridge/src/footprint_remote.rs` 那一行随文件删了（足迹两趟问法进了后端 face）。
        ("src/backend/agents/claudecode/footprint.rs", 1), // 〔MIG-3b 续〕新行：cc-bus 那一条随 Claude 布局那一半搬来，带着它那块（`~/.cc-bus/` note 点写面旧命令名）
        ("src/settings/config-surface-section.ts", 2), // 〔MIG-3b 续〕新行：`answersFor` 回声校验删了那一句 ＋ 构造期那句点旧命令名
        ("src/settings/footprint-reads.ts", 1),        // 〔MIG-3b 续〕新行：头注点旧 Tauri 命令名
        ("tests/e2e/local-backend-supervise.sh", 2), // 〔MIG-1 收尾〕e2e 起真后端那条删掉的判据名挂墓碑（gate · local-backend 套件）
        ("tests/scripts/gate.sh", 2), // 〔MIG-1 收尾〕e2e 起真后端那条删掉的判据名挂墓碑（gate · local-backend 套件）
        // 〔MIG-3b 续〕公钥推送进本机后端：新文件头注点旧命令名挂墓碑。
        ("src/backend/assets/pubkey.rs", 1),
        ("src/pubkey-push.ts", 1),
        ("tests/pubkey-push.vitest.ts", 1),
        ("src/bridge/src/dial_host.rs", 1), // 〔MIG-3b 续〕`stream` 用法那一个开链路口删了，原地一块
        ("src/backend/agents/claudecode/schema.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/backend/agents/codex/parse.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/backend/agents/codex/record.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/bridge/src/origin.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/bridge/src/utils.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/cards/index.ts", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("src/record-reads.ts", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        // 〔MOD〕`tests/backend/agents/claudecode/parse_tests.rs` 出表：那份文件随记录解释进后端删了（或墓碑随被守的东西整轴退役）
        (
            "tests/backend/agents/claudecode/schema_title_coverage.rs",
            1,
        ), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
        ("tests/backend/observe/record_page_tests.rs", 1), // 〔MOD〕新行：记录解释进后端 / 会话正文四条退役，点旧名的散文挂墓碑
    ];

    /// **挂歪了 / 在谈这件机制本身**的那些行，逐份登记**行数**。
    ///
    /// 口径与人群见 [`prose_tombstone_near_misses`] 头注。今天 5 行，两类：
    ///
    /// · ㈠ **真的挂歪了（三处，全是往定界符里塞了一个日期）** ——
    ///   `plan.rs` 那一处连定界符都没写；另两处（`remote_write_registry_tests` ·
    ///   `filewin/source_tests`）写成了「定界符里带日期」形 ⇒ 完整标记比不上
    ///   ⇒ 它们的记账**整个落空**，而贴的人以为贴上了。
    ///   🔴 **本拍刻意不改它们**：那三份文件都在本拍写区之外，而「哪儿红就往哪儿改」
    ///   与本仓「存量走棘轮、新写的一律红」那条纪律相反。⇒ 登记，让**新增**的红。
    /// · ㈡ **在谈机制本身（两处，合法）** —— 一份死值验记录的叙述，
    ///   与 `tests/scripts/gate.sh` 里那段「立项理由已不成立、原话照留」的头注。
    const SITES: &[(&str, usize)] = &[
        ("src/backend/control/ccm/plan.rs", 1),
        ("tests/bridge/filewin/source_tests.rs", 1),
        // 〔F7c 收尾 09-24〕`remote_write_registry_tests.rs` 那一行走了：它挂在 `NON_WRITING_COMMANDS` 里
        //   「`sftp_download` 从这张表搬走了」那段 09-21 的订正上，而那张表的全部五行连同它说的那条命令
        //   整轴删了（池子收到只剩 `sftp_copy`）⇒ 被守的那件事不在了，按第②条出路减掉。
        ("tests/evidence/K-R112-deathvalue.md", 1),
        ("tests/scripts/gate.sh", 1),
    ];

    // ★ 抽取器自检 0：内芯真的是从标记**拆**出来的。
    //   拆成空串 ⇒ 「挂歪」人群命中每一行；拆成整串 ⇒ 那个人群恒空。
    //   两个方向都会让下面第 ③ 条在一个假人群上成立，所以这一格放在最前面。
    let core = prose_tombstone_core();
    assert!(
        !core.is_empty()
            && core.len() < PROSE_NAME_TOMBSTONE.len()
            && PROSE_NAME_TOMBSTONE.contains(core),
        "内芯拆坏了（实得 {core:?}）—— 空串会命中每一行，等于整串会让「挂歪」人群恒空"
    );

    let corpus = tombstone_mark_corpus();
    // ★ 抽取器自检 1：语料面塌了 ⇒ 下面三条相等会在「两边都空」上成立。
    assert!(
        corpus.len() >= 1300,
        "标记普查只收到 {} 份文本文件 —— 语料面坏了（2026-09-22 现打 1413 份：`src` ＋ `tests` 两棵根）",
        corpus.len()
    );

    let mut marks: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut near: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (rel, src) in &corpus {
        let n = prose_tombstone_marks(src);
        if n > 0 {
            marks.insert(rel.clone(), n);
        }
        let nm = prose_tombstone_near_misses(src).len();
        if nm > 0 {
            near.insert(rel.clone(), nm);
        }
    }
    let total: usize = marks.values().sum();
    // ★ 抽取器自检 2：命中面塌了 ⇒ 第 ① 条会在空集上相等。
    assert!(
        total >= 80,
        "全树只抽到 {total} 处标记 —— 抽取器坏了（2026-09-22 现打 93 处 / 48 份）"
    );

    let want: std::collections::BTreeMap<String, usize> = REGISTERED
        .iter()
        .map(|(p, n)| ((*p).to_string(), *n))
        .collect();
    assert_eq!(
        want.len(),
        REGISTERED.len(),
        "`REGISTERED` 里有重复路径 —— 后一行会静默吃掉前一行"
    );

    // ① **逐格相等**，两个方向一起报：多一处 / 少一处 / 多一份 / 少一份都红。
    //   🔴 地板在这里是**瞎的**：标记被删掉是「变少」方向，而那正是 `α3` 刀 C。
    let keys: std::collections::BTreeSet<&String> = marks.keys().chain(want.keys()).collect();
    let drift: Vec<String> = keys
        .into_iter()
        .filter_map(|k| {
            let d = marks.get(k).copied().unwrap_or(0);
            let w = want.get(k).copied().unwrap_or(0);
            (d != w).then(|| format!("  {k}  盘上 {d} 处，登记表写 {w} 处"))
        })
        .collect();
    assert!(
        drift.is_empty(),
        "标记普查对不上盘面：\n{}\n\n\
         ⇒ 三条出路，按优先级：\n\
         ① **新贴了一处标记** ⇒ 在 `REGISTERED` 里加一行（+1）。贴标记是一次\
         **会被看见的记账**，这一行就是那笔账；\n\
         ② **那一处不在了** ⇒ 先问「那块墓碑为什么被删掉」。墓碑留的是**线索**\
         （某个东西为什么不在了），删掉它删的是账 ⇒ 多半该把它加回来，\
         而不是把这一行减掉；真该没有（被守的那件事整轴退役）就减掉，并在旁边写清理由；\n\
         ③ **那份文件改名/搬家了** ⇒ 改这一行的住址。\n\
         ⚠ 本条与 `TOMBSTONED` **单位不同、不可互相替代**：那张表数的是\
         「落在死名人群上的标记」，本条数的是「标记」。两个数对不上是常态，不是缺陷。",
        drift.join("\n")
    );

    // ② 第二把尺：**总数**。上面那一比万一在键的归一上错位（`\\` vs `/`、
    //   前缀没剥干净），两边会**同时**失真而逐格比照样空；总数不吃那种错位。
    assert_eq!(
        total,
        want.values().sum::<usize>(),
        "标记总处数与登记表的和对不上 —— 逐格那一比此刻不可信（多半是键归一错位）"
    );

    // ③ **挂歪了**那一格，同样逐格相等（人群见 `SITES` 头注）。
    let want_near: std::collections::BTreeMap<String, usize> =
        SITES.iter().map(|(p, n)| ((*p).to_string(), *n)).collect();
    let near_keys: std::collections::BTreeSet<&String> =
        near.keys().chain(want_near.keys()).collect();
    let near_drift: Vec<String> = near_keys
        .into_iter()
        .filter_map(|k| {
            let d = near.get(k).copied().unwrap_or(0);
            let w = want_near.get(k).copied().unwrap_or(0);
            (d != w).then(|| format!("  {k}  盘上 {d} 行，登记表写 {w} 行"))
        })
        .collect();
    assert!(
        near_drift.is_empty(),
        "「提了这件事却没把标记挂上」那一格对不上盘面：\n{}\n\n\
         ⇒ 先分清是哪一类（口径在 `prose_tombstone_near_misses` 头注里）：\n\
         ㈠ **想挂标记却挂歪了**（定界符里塞了日期 / 只写了内芯）⇒ **把标记挂对**，\
         并按上面第 ① 条在 `REGISTERED` 里 +1。\n\
         🔴 这是本格最值钱的一格：挂歪的那一处**以为自己受保护，其实零判据** ——\
         它既不进 `TOMBSTONED`，也不进本条的标记普查。\n\
         ㈡ **在谈这件机制本身**（量具里的针 / 记录里的叙述）⇒ 在 `SITES` 里记一行，\
         并写清它属哪一类。\n\
         ⚠ **不许**靠删掉那句话变绿。",
        near_drift.join("\n")
    );
}

/// 上面那条判据的**活体夹具** —— 它在真树上今天是绿的（三格逐格对上），
/// 而「今天该对上」那种格是**空真**：抽取器死了照样对得上。
/// ⇒ 这里造合成文本，把三个抽取器逐格切开验，**正反各喂一遍**。
///
/// 文本一律**现拼**（标记从 [`PROSE_NAME_TOMBSTONE`] 取、内芯从
/// [`prose_tombstone_core`] 取），一个字面量都不写 —— 否则本文件自己
/// 就会进真树上那两个人群，而那正是本模块从头到尾在治的那件事。
#[test]
fn the_tombstone_mark_scanner_really_sees_each_shape() {
    let mark = PROSE_NAME_TOMBSTONE;
    let core = prose_tombstone_core();

    // ① 处数口径是「处」不是「行」：一行挂两处就算两处。
    let two = format!("/// 旧名{mark} 与 旧名{mark}\n");
    assert_eq!(prose_tombstone_marks(&two), 2, "一行两处标记被数成了一处");
    assert_eq!(prose_tombstone_marks("/// 什么都没挂\n"), 0);

    // ② 挂对了的那一行**不许**进「挂歪」人群（否则本族会永远红，
    //    而一条永远红的闸第一天就会被调松 —— 那是本族最坏的结局）。
    assert_eq!(
        prose_tombstone_near_misses(&two),
        Vec::<usize>::new(),
        "挂对了的行进了「挂歪」人群 ⇒ 本条会在合法文本上误红"
    );

    // ③ 🔴 **挂歪了的那两形真的被看见。**
    //
    //    形甲「定界符里多塞了一个日期」—— 这就是现打逮到的那三处里两处的形状：
    //    完整标记比不上 ⇒ 那一处的记账整个落空，而贴的人以为贴上了。
    //    定界符**运行期从标记切**（切最后一个字符），不写第二份字面量。
    let close_len = mark.chars().last().map_or(0, |c| c.len_utf8());
    let (mark_head, mark_tail) = mark.split_at(mark.len() - close_len);
    let dated = format!("/// 行一\n/// {mark_head} 2026-09-21{mark_tail} 旧名已删\n");
    assert!(
        !dated.contains(mark),
        "抽取器的前提塌了：往定界符里塞了东西之后它竟然还包含完整标记 —— 那一形不存在"
    );
    assert_eq!(
        prose_tombstone_near_misses(&dated),
        vec![2],
        "「定界符里带日期」这一形没被看见 ⇒ 现打那两处挂歪的就永远没人管"
    );

    //    形乙「只写了内芯、定界符都没写」—— 现打那三处里的第三处。
    let bare = format!("/// 行一\n/// {core} 留在这里\n");
    assert_eq!(
        prose_tombstone_near_misses(&bare),
        vec![2],
        "「只写了内芯」这一形没被看见"
    );

    // ④ 同一行**既有完整标记又有裸内芯**时，本条**偏向不红** ——
    //    口径如实写明：判定是「这一行有没有完整标记」，不是「有几处裸内芯」。
    let mixed = format!("/// {core} 讲机制，又挂了 {mark}\n");
    assert_eq!(
        prose_tombstone_near_misses(&mixed),
        Vec::<usize>::new(),
        "混合行的口径变了 —— 本条明写偏向不红，改口径要连头注一起改"
    );

    // ⑤ 内芯必须是标记的**真子串**（拆法坏了，上面四格全部失真）。
    assert!(
        mark.contains(core) && core.len() < mark.len() && !core.is_empty(),
        "内芯拆坏了：mark={mark:?} core={core:?}"
    );
}

// ── `99 §2.5 P9` 的前提闸 ────────────────────────────────────────────────────

/// 两棵**生产树**的仓根相对住址。`P9` 剩下两件的前提说的就是这两棵树。
///
/// ⚠ 这里**不含** `src/bridge/crates` —— 那八个共享 crate 从来就是「判据与被测代码
/// 同住一份文件」的形状（`guard-core` 自己就是最大的一份），它们不在 `设计/16`
/// 的剖分计划里，混进来会让下面那两个读数说不清自己在说哪件事。
const P9_PRODUCTION_TREES: &[&str] = &["src/backend", "src/bridge/src"];

/// 一棵生产树今天的形状：`(.rs 份数, 真 `#[test]` 属性数, 内联 test 模块数, 测试专用支撑项数)`。
///
/// 三个计数的**口径分得很细，这一格是承重的**（步 7c 就是被这一格咬到的）：
/// · **真 `#[test]` 属性** ＝ 整行 trim 之后**逐字等于** `#[test]`。
///   ⚠ 不用 `src.contains("#[test]")`：那是**子串**口径，会把头注里讲形状的散文算进来
///   （现打：`src/bridge/src` 今天有 3 处这样的散文 —— `tool_registry` ·
///   `scanning_guard_registry`×2 · `launch`），于是「这棵树还有测试代码」这句话
///   会在真的一个都没有的时候仍然为真。**上一版就是这么读的，因此永远响不了。**
/// · **内联 test 模块** ＝ `#[cfg(test)]`（跳过后续属性行）之后紧接 `mod X {`。
///   这是 `设计/16 §3.1` 那套剖分要消灭的形状。
/// · **测试专用支撑项** ＝ `#[cfg(test)]` 之后紧接**别的**以 `{` 收尾的项
///   （`thread_local! {` · `impl Drop for …` · 辅助 `fn` · 测试专用 `enum`）。
///   ⚠ 这一类**不是**剖分的标的：`真相源/00` 逐字写着剩下那几份的 `cfg(test)`
///   「不是 `mod`，是测试专用的 `thread_local!` / 辅助 `fn` / `struct`，
///   `§3.1` 那套机制罩不住它们，要单独裁」。上一版把它与 test 模块混成一个
///   「块形」计数，于是剖分做完之后那个数**仍然大于 0**，本条照样不响。
/// · 分号声明形（`#[cfg(test)] #[path=…] mod X;`）**一个都不算** —— 那正是剖分的产物。
fn p9_tree_shape(root: &std::path::Path, tree: &str) -> (usize, usize, usize, usize) {
    // 排除名单刻意留空：本文件住 `tests/bridge/`，**按住址就不在这两棵树里**，
    // 没有「摘掉我自己」这回事。写成空名单是把这件事明写出来（`§5.4b` 纪律 2）。
    let files = guard_core::scan_tree_excluding(&root.join(tree), &["rs"], &[]);
    let (mut attrs, mut mods, mut support) = (0usize, 0usize, 0usize);
    let test_attr = format!("#[{}]", "test");
    let cfg_attr = format!("#[cfg({})]", "test");
    for (_, src) in &files {
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if l.trim() == test_attr.as_str() {
                attrs += 1;
            }
            if l.trim() != cfg_attr.as_str() {
                continue;
            }
            // 跳过它后面那一串属性行（`#[path = …]` 之类），再看真正的那一项。
            // ⚠ 针**运行期拼**：`needle_anchor_registry` 的 `.starts_with(` 是**0 上限**的
            //   递减棘轮（语料变量上的裸字面量匹配）。步 7c 第一版写成字面量，当场把那条
            //   棘轮顶破 2 处 —— 拼出来就不在它的人群里，而且顺带没了自指。
            let attr_open = format!("#{}", "[");
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_start().starts_with(attr_open.as_str()) {
                j += 1;
            }
            let Some(t) = lines.get(j).map(|x| x.trim()) else {
                continue;
            };
            if t.ends_with(';') {
                continue; // 分号声明形 —— 剖分的产物，不算
            }
            if !t.ends_with('{') {
                continue;
            }
            let item = t
                .strip_prefix("pub ")
                .or_else(|| t.strip_prefix("pub(crate) "))
                .or_else(|| t.strip_prefix("pub(super) "))
                .unwrap_or(t);
            let mod_kw = format!("{} ", "mod");
            if item.starts_with(mod_kw.as_str()) {
                mods += 1;
            } else {
                support += 1;
            }
        }
    }
    (files.len(), attrs, mods, support)
}

/// 每棵生产树**还允许**留几个「测试专用支撑项」。**递减棘轮，只许往下调。**
///
/// 现打（步 7c 剖分做完之后，2026-09-19）：`src/backend` **7** · `src/bridge/src` **15**。
/// 逐份点名见 `设计/16 §6.5`。这一族要单独裁（`真相源/00` 那句「`§3.1` 罩不住它们」），
/// 不在步 7c 的写区里 —— 立这条棘轮是为了让它**只减不增**。
const P9_SUPPORT_ITEM_CEILINGS: &[(&str, usize)] = &[("src/backend", 7), ("src/bridge/src", 15)];

/// 🟢 〔步 7c 2026-09-19〕**剖分做完了 —— 而 `P9` 剩下两件**仍然**做不得，理由换了。**
///
/// # 它接的是谁的岗
///
/// 原来这里住的是 `the_premise_that_p9_waits_on_is_still_false_and_says_so`〔散文墓碑〕。
/// 那一条断言「两棵生产树的测试代码**不全为 0**」，并在断言文案里写明：
/// 掉到 0 那天它红，而红的意思是「那棵树剖分做完了 ⇒ `P9` 剩下两件对它成立了」。
///
/// 步 7c 把剖分做完了（`src/bridge` 45 份 / 59 块 · `src/backend` 49 份 / 57 块），
/// 而那一条**没有红** —— 逐条写明为什么，因为这两格都是「量具比它的说法粗」：
/// ① 它数「含 `#[test]` 的份数」用的是**子串**口径 ⇒ 头注里讲形状的散文照样算命中
///    （现打 3 处）；
/// ② 它数「`#[cfg(test)]` 块形」时把 **test 模块**与**测试专用支撑项**
///    （`thread_local!` / `impl Drop` / 辅助 `fn`）混成一个数 ⇒ 后者按设计还留着
///    （现打 7 ＋ 15），那个数永远大于 0。
/// ⇒ 口径按它自己的说法收细之后（见 [`p9_tree_shape`]），「剖分做完」这件事
///    **今天确实成立**：两棵树真 `#[test]` 属性各 **0** 个、内联 test 模块各 **0** 个。
///
/// # 🔴 但它推出来的那个结论是**假的** —— 现打证掉了，所以不能照它说的去做 `P9`
///
/// | `P9` 剩下那件 | 它原来的前提 | 步 7c 现打 |
/// |---|---|---|
/// | ② 103 处拼针改回字面量 | 「判据住 `tests/`、语料住 `src/` ⇒ 自指不可能」 | ⛔ **假**：今天有 **21 份**判据文件把 `tests/` 那棵树（也就是它们自己住的那棵）收进了语料根 —— 而那是 `§6.2` B 类**必须**的修法（不收就整批掉出扫描面）。步 7c 又加了几处。 |
/// | ③ 退役那两个元守卫 | 「生产段与测试段是不同目录 ⇒ 混淆结构上不可能」 | ⛔ **假**：正控（往生产段塞一段真漏进来的 `#[test]`）⇒ **三条全红**（`readonly_guard::no_test_code_leaks_into_any_production_section` · `guard_support::every_backend_file_strips_clean` · `structural_scan::every_monitor_file_strips_clean`）。目录不同**不妨碍**有人在 `src/` 里写 `#[test]` —— 今天挡住这件事的就是这三条。 |
///
/// ⇒ 本条因此**换了岗位**：从「等剖分做完」的闸，变成
/// ① **守住剖分的成果**（两棵树真 `#[test]` 与内联 test 模块恒为 0，回来一个就红）；
/// ② **把 `P9` 那两件今天真正的拦路石变成读数**（21 份自指语料 · 三条元守卫还在）。
///
/// # 它怎么响
///
/// - 生产树里回来一个 `#[test]` 或一个内联 `mod tests {}` ⇒ 红（剖分被吃回去了）。
/// - 测试专用支撑项涨过上限 ⇒ 红（那一族只许减，见 [`P9_SUPPORT_ITEM_CEILINGS`]）。
/// - **「自指语料」的份数掉到 0** ⇒ 红，而那一天的意思是
///   **`P9` ② 的前提第一次真的成立了 ⇒ 回去做它，并把这一格同轮收掉**。
/// - 三条元守卫里少了任何一条 ⇒ 红（`P9` ③ 的拦路石被人绕过去了）。
///
/// # ⚠ 它买不到什么（诚实边界）
///
/// - 判不了「某一处拼针该不该拆」：那要逐处看那条判据的人群覆不覆盖它自己那份文件。
/// - 判不了剖分做得**对**不对：回拼对账是 `tests/evidence/W1-split-mixed-files.py` 的活。
/// - 「三条元守卫还在」按**名字**认，认不出「它被掏空了」——
///   那一格由它们各自的正控接（步 7c 跑过，读数在上表）。
#[test]
fn the_split_stays_done_and_p9_is_blocked_for_a_reason_that_says_itself() {
    let root = addr_repo_root();

    let mut shapes: Vec<(&str, usize, usize, usize, usize)> = Vec::new();
    for tree in P9_PRODUCTION_TREES {
        let (files, attrs, mods, support) = p9_tree_shape(&root, tree);
        shapes.push((tree, files, attrs, mods, support));
    }

    // ★ 反空真：两棵树都得真的读到文件。扫空集 ⇒ 三个计数全是 0 ⇒ 下面那两条
    //   「恒为 0」会**零命中地绿**，而这一次假绿的方向是「宣布剖分守住了」。
    //   ⇒ 地板放在断言前面。
    for (tree, files, ..) in &shapes {
        assert!(
            *files >= 40,
            "`{tree}` 只收到 {files} 份 `.rs` —— 语料塌了，下面几个读数此刻不携带信息"
        );
    }

    // ① 剖分的成果：两棵生产树里**一个测试函数都没有**。
    for (tree, files, attrs, mods, _) in &shapes {
        assert_eq!(
            (*attrs, *mods),
            (0, 0),
            "🔴 `{tree}`（{files} 份 `.rs`）里测试代码**回来了**：\n\
             真 `#[test]` 属性 {attrs} 个 · 内联 `#[cfg(test)] mod X {{}}` {mods} 处。\n\
             \n\
             `设计/16 §3.1` 的形状是：生产树里只留三行桩\n\
             （`#[cfg(test)]` ＋ `#[path = \"…\"]` ＋ `mod X;`），测试体住 `<repo>/tests/`。\n\
             ⇒ 处置：把它剖出去（量具：`tests/evidence/W1-split-mixed-files.py --apply`，\n\
             它带逐字节回拼对账）。**别把本条改松** —— 步 7c 之前这棵树上有过\n\
             533（backend）/ 302（bridge）个测试属性，那个状态是用两轮尺子重瞄换回来的。"
        );
    }

    // ② 测试专用支撑项：递减棘轮（这一族要单独裁，不在剖分的射程里）。
    for (tree, _, _, _, support) in &shapes {
        let ceiling = P9_SUPPORT_ITEM_CEILINGS
            .iter()
            .find(|(t, _)| t == tree)
            .map(|(_, c)| *c)
            .unwrap_or_else(|| panic!("`{tree}` 没有登记支撑项上限 —— 加树就在那张表里加一行"));
        assert!(
            *support <= ceiling,
            "`{tree}` 的「测试专用支撑项」涨到 {support}（上限 {ceiling}）—— **只许降**。\n\
             这一族是 `#[cfg(test)]` 罩着的 `thread_local!` / `impl Drop` / 辅助 `fn`，\n\
             `§3.1` 那套 `#[path] mod` 机制罩不住它们，要单独裁。\n\
             ⚠ **不许把上限调上去让今天好过** —— 新写一个就是给那件事又添一笔债。"
        );
    }

    // ③ `P9` ② 今天真正的拦路石：判据把**自己住的那棵树**收进了语料。
    //
    // 这是 `§6.2` B 类**必须**的修法（不收，搬走的测试就整批掉出扫描面 ⇒ 恒绿），
    // 而它的直接后果就是「判据住 `tests/`、语料住 `src/`」这个前提失效。
    // ⚠ 本条自己就是这 21 份之一（它的语料根含两棵生产树，但 ③ 这一格扫的是 `tests/`）——
    //   如实记，不把自己摘出去：摘出去这个数就说不出「自指还在不在」了。
    // 🔴 **针一律运行期拼，一个都不许写成整串字面量。**〔步 7c 死值验当场逼出来的〕
    //
    // 现打过：第一版把这五根针写成字面量 ⇒ 本文件（住 `tests/`，在③这一格的语料里）
    // **永远命中自己**，于是 `self_corpus_files` 恒非空、下面那条断言**恒不可能响**。
    // 那正是本模块从头到尾在治的「判据在自己的语料里找到自己 ⇒ 恒绿」，
    // 而这一次它长在**新写的判据自己**头上 —— 死值验（把针换成认不出的名字）是唯一
    // 能看见它的办法：那一趟本条**照样绿**。
    let t = "tests";
    let self_corpus_markers = [
        format!("\"{t}\""),
        format!("\"{t}/{}\"", "bridge"),
        format!("\"{t}/{}\"", "backend"),
        format!("{t}_{}()", "root"),
        format!("{}_{}()", "code", "roots"),
    ];
    // 🔴 **本文件必须明写摘掉，而这一格是死值验逼出来的第二刀。**
    //
    // 上面把针改成运行期拼之后，死值验仍然**绿**：针是从 `t` 拼的，而 `t` 的**值**
    // 必然以字面量出现在本文件里（`let t = "tests";`）⇒ 拼出来的针照样命中本文件。
    // 换任何一个值都一样 —— 这一格**在本文件上是量不了的**。
    // ⇒ 排除明写成名单（`scan_tree_excluding` 摘不到就 panic，改名会出声）。
    //
    // ⚠ **诚实边界，别读错这个数**：本文件**自己也是这一族的一员**
    //（`dead_name_corpus()` 的语料根逐字含 `"tests"`）。摘掉它不是因为它不算，
    // 是因为量具量不了自己。⇒ 下面那个数是 **20**，而真值是 **21**。
    // 「这一族清零」那天，本文件自己那一处也要一起拆掉 —— 解锁文案里写了这一句。
    let mut self_corpus_files: Vec<String> = Vec::new();
    for (path, src) in guard_core::scan_tree_excluding(
        &root.join("tests"),
        &["rs"],
        &["bridge/structural_scan_tests.rs"],
    ) {
        if self_corpus_markers.iter().any(|m| src.contains(m.as_str())) {
            self_corpus_files.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    self_corpus_files.sort();
    assert!(
        !self_corpus_files.is_empty(),
        "🟢 **`99 §2.5 P9` 第②件的前提第一次成立了** —— 今天没有任何判据文件\n\
         把 `tests/` 那棵树收进自己的语料根了。\n\
         \n\
         **本条不是在报故障，是在报一个解锁条件**：\n\
         `设计/16 §4.2` 那条「103 处运行时拼针可以改回普通字面量」，它的前提逐字是\n\
         「判据和语料物理不同文件 ⇒ 自指不可能发生」。那个前提从 `§6.3` B 类修法\n\
         落地那天起就是假的（判据住 `tests/`，而语料根含 `tests/`）——\n\
         今天这个数掉到 0，说明那一族自指真的没了。\n\
         ⇒ 回去做 `P9` 第②件，并把本条这一格**同轮收掉**。\n\
         ⚠ 本文件被明写摘出了人群（量具量不了自己，理由在上面那段注里）——\n\
         所以这个数清零时，**还要手工核一遍本文件自己那一处**（`dead_name_corpus()` 的 `\"tests\"`）。\n\
         ⚠ 做之前先读 `§4.2` 的订正段：那一轮机械改回 326 处 ⇒ 红 12 条，\n\
         其中一条住 `tests/bridge/`。逐条核过再改。"
    );

    // ④ `P9` ③ 今天真正的拦路石：三条元守卫都还在。
    //
    // `§4.4` 原文说「搬完之后生产段和测试段是不同目录，这个混淆在结构上不可能发生」。
    // 目录不同**不妨碍**有人在 `src/` 里写 `#[test]` —— 步 7c 的正控逐条验过：
    // 往生产段塞一段真漏进来的测试代码，这三条**全红**。它们就是①那一格的执行者。
    for (rel, judge) in [
        (
            "tests/backend/readonly_guard.rs",
            "fn no_test_code_leaks_into_any_production_section",
        ),
        (
            "tests/backend/guard_support_tests.rs",
            "fn every_backend_file_strips_clean",
        ),
        (
            "tests/bridge/structural_scan_tests.rs",
            "fn every_monitor_file_strips_clean",
        ),
    ] {
        let src = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 本条判不了，不许当成绿"));
        assert!(
            src.contains(judge),
            "元守卫 `{judge}` 不在 `{rel}` 里了 —— \n\
             它是上面①那条「生产树里没有测试代码」的**执行者**：①只在每次跑测试时看一眼，\n\
             而这三条是按文件逐份剥出生产段来核的。\n\
             ⚠ `设计/16 §4.4` 说它们「可以退役」，而步 7c 的正控证明**退不得**：\n\
             往生产段塞一段真漏进来的 `#[test]` ⇒ 这三条全红。别照那一节的散文退役它们。"
        );
    }
}
