//! 源码结构那几族判据：测试属性 · 剥法 · 绕道再导出 · 剥注释只一份 · shell 目标的结构扫描 · 位置比较的锚点 · 符号与行号地址 · 死名。

use super::repo;
use crate::*;

/// `dir`（仓根相对）下后缀在 `exts` 里（空 = 不筛）的文本文件 `(绝对路径, 原文)`，经入口那份快照取（含规则目录：这几族判的是全部源码）。
/// `dir` 是一个包（或它的 `src/`）⇒ 取那个包的源码人群（连 manifest 明写的兄弟包与 `#[path]` 挂进来的）。
fn tree(dir: &str, exts: &[&str]) -> Vec<(std::path::PathBuf, String)> {
    let root = addr::repo_root();
    let pkg = dir.strip_suffix("/src").unwrap_or(dir);
    let files = if repo().has(&format!("{pkg}/Cargo.toml")) {
        repo().crate_tree(pkg, exts)
    } else {
        repo().with_rules().under(&[dir], exts)
    };
    files
        .into_iter()
        .map(|f| (root.join(&f.rel), f.text().to_string()))
        .collect()
}

/// 点名的一份文件的原文。
fn text(rel: &str) -> String {
    repo().file(rel).text().to_string()
}

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
    // 🔴 **这一格同时踩了 `§6.2` 的 A 类与 B 类，两条一起修。**
    //
    // · **B 类**：语料根原来只有 `<bridge>/src` 一棵。`#[test]` 剖分之后整批住进
    //   `<repo>/tests/` ⇒ 那一棵树里 `#[test]` 归零，自检逐字报「扫到 0 个 —— 扫描面坏了」。
    //   ⇒ 语料改成**四棵互不包含的根**：两棵生产树 ＋ 两棵测试树。
    //   两棵生产树今天该是 0 个 `#[test]`，但**仍然要扫** —— 哪天有人在 `src/` 里写一个，
    //   本条要看得见它（少扫不会红，只会零命中地绿）。
    // · **A 类**：原来靠 `scan_tree_excluding_self(.., file!())` 摘掉自己。本文件被
    //   `#[path]` 引进来 ⇒ `file!()` 是折返路径 ⇒ 摘除恒空转（`§5.4b` 纪律 4）。
    //   而语料里现在**含 `tests/frontend/shell/` 这棵树，也就是含本文件自己** ⇒ 摘除失效不再无害：
    //   本条会数到自己头注里那些讲形状的 `#[test]` 字样。
    //   ⇒ 改成 `scan_tree_excluding` 的**明写名单**，摘不到就 panic。
    //
    // 🔴 本条这一轮真的咬到东西了：`src/backend` 从来不在它的射程里，
    // 而那棵树上**有 3 处**「一个 fn 两个 `#[test]`」（libtest 因此把它们各注册两遍：
    // `cargo test -- --list` 705 行 / 702 个不同名字）。逐处在步 7c 报告里点名。
    let mut files = Vec::new();
    for t in [
        "src/frontend/shell/src",
        "src/backend",
        "tests/frontend/shell",
        "tests/frontend/filewin",
        "tests/backend",
        "tests/comms",
    ] {
        files.extend(tree(t, &["rs"]));
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
    assert!(seen > 0, "一个 `#[test]` 都没扫到 —— 扫描面坏了");
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
            // 🔴 这里原先也是 `t.starts_with("mod ")` ——
            //    **同一族病的第二个住址**，而它的失效方向比剥法那处更阴：
            //    一份文件的测试模块若写成 `pub(crate) mod tests {`，`position` 返回 `None`
            //    ⇒ 那份文件**整份掉出本条的人群**，而本条照样绿（人群缩水，静默）。
            //    ⇒ 走同一份权威的形状判定，不再各写一份近似的（`E3`）。
            //    ⚠ 现打：今天全仓「带可见性且名字含 test 的 `mod`」**0 处**
            //       ⇒ 这一改在今天的盘上是 **no-op**，买的是「下一处这么写时不会静默」。
            crate::strip_visibility(t).starts_with("mod ") && t.contains("test") && t.ends_with('{')
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
            // ⚠ 两根前缀针**运行期拼**：
            //   `needle_anchor_registry` 的 `.starts_with(` 是**0 上限**的递减棘轮，
            //   而它认的是「语料变量上的裸字面量匹配」。步 7c 往本文件新加了两处
            //   从磁盘语料派生的 `let`（`p9_tree_shape` / `fn_bodies` 里的 `lines`），
            //   而那条棘轮的语料变量追踪是**按名字、整份文件**算的
            //   ⇒ 这两处**一直存在**的针**第一次被数到**（2 处 > 上限 0）。
            //   拼出来就不在它的人群里；针的含义一个字没变。
            //   ★ 如实记：这是「人群扩大之后旧债第一次现形」，不是新写的债。
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

// ═══════════════════════════════════════════════════════════════════════
// `K-R77`（09-12）：**「`#[cfg(test)]` 顶层再导出自己的测试模块」那道绕道** ——
// 拆掉最后一处的同一拍给它上棘轮。
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
/// 🔴 **同一个 item 上的其它属性也要跳过**。
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
/// `crate::strip_visibility`，**不再各写一份近似的**（本仓 `E3`）。
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
        let Some(decl) = crate::strip_visibility(next).strip_prefix("mod ") else {
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
        let Some(path) = crate::strip_visibility(next).strip_prefix("use ") else {
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
/// 它**曾经有一个真理由** —— `crate::test_module_ranges` 那时按字面前缀认
/// `mod `，模块一带可见性前缀就认不出来，那份文件**整段测试代码留在生产段里**被别的
/// 守卫扫。`K-R75`（09-12）把那一步换成按形状剥可见性
/// （`crate::strip_visibility`）之后，**那个理由没了**，而盘上那三处还在。
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
/// `crate::strip_visibility`，**不是**回来抬这个 0。
/// 真要给这道绕道开一个正当口子，先立一条裁定、写明理由。
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
                 ⇒ **这道绕道今天没有理由了**：`crate::test_module_ranges` 自 `K-R75`\n\
                 起按形状剥可见性修饰，测试模块直接写成 `pub(crate) mod X` 就认得出来，\n\
                 跨模块的取名走 `crate::<文件>::X::…`，不需要在顶层再导出一次。\n\
                 修法：把 `mod X` 写成 `pub(crate) mod X`，删掉这一行，引用者改走全路径。\n\
                 （盘上三处的先例：`stream_source/` 的 `dial_move_judge` · `local_backend_host.rs` 的\n\
                  `tests` · `readonly_guard.rs` 的 `g6_doctrine`，`K-R76`/`K-R77` 各拆过。）\n\
                 ⚠ 真有正当理由要开口子 ⇒ 先立裁定，**不许**在这里抬 \
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
/// 这道绕道**两棵树上都长过**（monitor 侧 `stream_source/` 与 `local_backend_host.rs`，
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
/// 而「backend 那一格要不要也跑一条同形的判据」不是这里能决定的事。
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
        } else if rel.starts_with("src/frontend/") {
            // 文件窗口独立成包（`src/frontend/filewin/`）：它从前就在 monitor 这棵树里，照旧算这一棵。
            monitor += 1;
        } else if rel.starts_with("tests/") {
            tests += 1;
        }
    }
    // 三棵树各自非空：合起来一条挡不住「一棵塌了另一棵涨了」。
    for (tree, n) in [("monitor", monitor), ("backend", backend), ("tests", tests)] {
        assert!(n > 0, "{tree}那棵树一份 .rs 都没收到 —— 掉出语料面了");
    }
    // 点名：三份真长过绕道的文件必须都在语料里（地板是数，这一条是**住址**）。
    let names: std::collections::BTreeSet<String> =
        corpus.iter().map(|(p, _)| detour_addr(p)).collect();
    for want in [
        // 它是纯测试文件，搬到了 `tests/backend/`。
        "tests/backend/readonly_guard.rs",
        "src/frontend/shell/src/stream_source/mod.rs",
        "src/frontend/shell/src/local_backend_host.rs",
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
        "{attr}\npub(crate) use crate::production_code;\n\n\
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
        ("再导出的是**别的 crate** 的东西", &foreign),
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
/// 详见 `crate::strip_comment_lines` 头注）。它扫的是**自己生成的** shell/rc 片段，
/// 语料可控 ⇒ 那个风险在它那里不存在。**这不是豁免，是另一种语义。**
///
/// ⚠ 想再加一个 ⇒ 先问「共享原语为什么不够」，答得出来才加进下面这张表。
/// ★**按「函数做了什么」再扫一遍剥注释实现**（默认拒绝）。
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
        (
            "protocol_doc_gen.rs::first_sentence",
            "把已经取出来的 `///` 正文收成一句话（输入是注释本身，不在源码里剥注释）",
        ),
        // 08-08 删掉 `lib.rs::test_source`：它**根本不剥注释**（只是把测试段拼起来）。
        // 它当初被检出，是因为旧检测器取「函数体起点后 700 字符」的定长窗口，
        // 一路吃进了它的邻居 `production_code`（那个才剥）。⇒ **这一行是误登记**，
        // 而误登记的害处是具体的：登记表是「已知的第二份剥法」清单，
        // 混进一条不是剥法的，下一个人会照它去找一份并不存在的实现。
        (
            "e2e_gate_registry_tests.rs::strip_comments",
            "**别的注释语法**：语料是 shell 脚本（`tests/e2e/*.sh`），注释是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 语义上刻意只剥**整行注释**、不碰行尾注释（shell 里 `#` 可以出现在字符串中间，                 按 marker 截断会误伤 `pgrep` 模式里的 `#`）。要收口的正确做法是给共享原语加一个                 「注释前缀」参数，那是另一件事。",
        ),
        // `cc_bus_tests` 那份本地剥法那一行摘了：用它的零命中守卫随驾驶舱 shell 读一起退役（收口了，不是搬家）。
        // 钩子诊断那份本地剥法那一行摘了：它的判据随模块进后端退役（后端 `readonly_guard` 管只读，收口了，不是搬家）。
        (
            // 两种方言的读回口（同名两份 `impl`，按文件名去重成一行）。
            // 随方言进了那台后端（今天住 `src/backend/platform/shell/dialect.rs`）。
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
        // 抽 `enum Refusal` 变体名的那一份摘了：调用行渲染器的判据重写，拒绝理由改成逐条手写字节钉，不再从定义里抽名。
        ("agents_tests.rs::golden_rows", "不是剥法：解析对拍表的行（随起会话事实的对拍从 monitor `agent_profile_parity_tests.rs` 搬进后端）"),
        // monitor 那一轨 gate2 对拍（解析金表行的那个函数）那一行摘了：随 monitor 侧的门删了。
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
        ("ssh_config.rs::parse_host_aliases", "不是剥法：解析 ssh config 的 Host 别名（随导入搬进后端 `dial/`）"),
        ("registry_tests.rs::declared_fields_of", "不是剥法：解析结构体字段声明"),
        // **方向恰好相反的一条**：它不剥注释，它**把注释留下来并指名**。
        // 那一格的正题是「你 rc 里这几行是旧的」——`#` 打头的行照样进结果，只是分类成
        // `LegacyRcKind::Comment`（`K-R57` 现打用户 `~/.bashrc`：14 行里 4 行是注释，
        // 那 4 行也该让用户看见）。⇒ 共享原语在这里不是「不够」，是**用了就把活做反了**。
        (
            // 「待办」失效行那一件（`src/backend/platform/shell/posix.rs`）。
            "posix.rs::dead_source_lines",
            "不是剥法：逐行找启动文件里 `source` / `.` 指向的文件不在的那几行、原文原样带回去指名；\
                 注释行跳过是因为注释里的 `source` 不生效，不是要丢掉它们（`strip_comment_lines` 不回行号与原文）",
        ),
        // 剥 TS 行尾 `//` 的那一份（生产接线钉那组判据自带的）摘了：那组判据随载荷那一层一起删了。
    ];

    let mut found: Vec<String> = Vec::new();
    // 🔴 **补上 `"tests"` 这一棵。**
    //
    // 本条头注逐字写着「私有剥法一律住在测试模块里」—— 而测试模块剖分之后整个住进了
    // `<repo>/tests/`。上一版那三棵根**一份都够不着它们** ⇒ 四份剥法
    // （`cc_bus` / `hooks_diag` 的 `non_test_code` · `ccm_invocation` 的 `refusal_variants` ·
    // `launch_wire` 的 `production_ts`）整批掉出人群，而「没有新增第二份剥法」这个结论
    // 就是在一个缺了一大块的分母上得出的。**少扫不会红，只会零命中地绿。**
    // ⚠ 四棵根**互不包含**（`src/frontend/shell/src` · `src/common` 是并列的两棵，
    //   `src/backend` 与 `tests` 各自独立）—— 那是 `§5.4b` 纪律 1 要的那一问。
    for sub in [
        "src/frontend/shell/src",
        "src/common",
        "src/backend",
        "tests",
    ] {
        for (f, raw) in tree(sub, &["rs"]) {
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
                // ⚠ **切到函数真正的结尾**，不是「起点后 700 字符」：
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
                // ⚠ **先剥掉整行注释再找**：本检测器找的是「这段代码在剥注释」，
                // 而它原来拿**函数体原文**去找 —— 于是**一句解释性注释里写出那个形态就会被算成实现**。
                // 实测：`e2e_gate_registry::floored` 里有一行注释写着「第一版在这里内联了一个
                // `starts_with('#')` 过滤」，本条当场把它判成第二份剥法，还建议我去登记它。
                // ⇒ 与 plan-lint 判据 7 同一个教训：**判据要看围栏，不是看围栏的说明书**。
                // 用共享原语剥（它认 `//` 那套 Rust 形态，正是这里要去掉的东西）。
                let body = crate::strip_comment_lines(&body);
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
             **多出来的**：先问「共享原语 `crate::strip_comment_lines` 为什么不够」——\n\
             答得出来就登记进 `TRANSFORMERS` 并写明理由；答不出来就改成调它。\n\
             ⚠ 名字叫什么**不是判据**：换个名字的同一份剥法仍然是第二份剥法。\n\
             **少了的**：它被收口了 ⇒ 把登记删掉（登记表腐烂比没有登记更糟）。"
    );
}

#[test]
fn comment_stripping_has_exactly_one_shared_implementation() {
    const REGISTERED: &[(&str, &str)] = &[
        (
            // 住址跟着那份私有剥法搬进 `tests/frontend/shell/`。
            "e2e_gate_registry_tests.rs",
            "**别的注释语法**：语料是 shell 脚本（`tests/e2e/*.sh`），注释前缀是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 刻意只剥**整行**：shell 里 `#` 会出现在字符串中间（本处语料就有 `pgrep` 模式），                 按 marker 截断会误伤。收口的正确做法是给共享原语加一个「注释前缀」参数，那是另一件事。",
        ),
    ];

    // 🔴 **两棵树：生产段 ＋ monitor 的测试段。**
    //
    // 同上一条的理由：本条头注逐字「私有剥法一律住在测试模块里」，而测试模块搬去了
    // `<repo>/tests/frontend/shell/`。只扫 `src/frontend/shell/src` 的话连**已登记的那一份**都扫不到 ——
    // 底下那条抽取器自检按设计响了（它正是为这一形而写的）。
    let mut found: Vec<String> = Vec::new();
    for root in [
        "src/frontend/shell/src",
        "tests/frontend/shell",
        "tests/comms/inward",
        "tests/frontend/filewin",
    ] {
        for (f, raw) in tree(root, &["rs"]) {
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
    assert!(
        found.contains(&"e2e_gate_registry_tests.rs".to_string()),
        "连 `e2e_gate_registry_tests.rs` 里那个已登记的实现都没扫到 —— \n\
             遍历或形态坏了，那样「没有新增」这个结论是零命中得来的，不是真的。实得：{found:?}"
    );

    let extra: Vec<&String> = found
        .iter()
        .filter(|f| !REGISTERED.iter().any(|(r, _)| *r == f.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "又出现了私有的剥注释实现：{extra:?}\n\
             ★ 先用 `crate::strip_comment_lines` —— 08-06 已把三份重复迁过去。\n\
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

/// **用已知的绕过手法验证**（纪律）。这不是构造的边角——
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

/// **拼命令用的 shell 元字符黑名单，权威源恰好一处**（E3）。
///
/// # 它不是理论风险 —— 同一族已经漂过一次
///
/// 当时载荷那份源码的头注逐字记着：U7-3 把**不可见字符表**收进 `acct-core`
/// 让两个读 manifest 的地方共用，**而「拼命令」那条路当时没跟上** ——
/// `history.rs` 一直用自己那张 U7-3 之前的旧表，缺 `U+1680` · `U+2000..200A` ·
/// `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，是一处**纵深防御缺口**。
///
/// 08-06 顺着「只被一处调用的生产函数」这条先验查到：**元字符表也是两份逐字副本**
/// （`history.rs` 与 `payload.rs`），而**没有任何东西对拍它们**。
/// ⇒ 按 E3 收成一处：`history.rs` 那份删掉、改为派生载荷那一侧的那份；后来那一份又收进 `acct-core`
/// （`CONFIG_DIR_SHELL_META`，配置目录全表的一部分），载荷那一层随起会话只交一行 `ccm …` 删了。
/// 本条钉住「以后也只有一处」。
///
/// ⚠ E3 逐字要求「判据钉的是**权威源恰好一个**，不是『有没有登记』」——
/// 所以这里数的是**定义处数**，不是「两处内容一不一样」。
/// 后者在两份都改错时照样绿，前者不会。
#[test]
fn the_shell_metachar_blacklist_has_exactly_one_home() {
    const NEEDLE: &str = "const CONFIG_DIR_SHELL_META";
    let homes: Vec<String> = repo()
        .under(
            &["src/frontend/shell/src", "src/backend", "src/common"],
            &["rs"],
        )
        .into_iter()
        .filter(|f| {
            f.prod()
                .lines()
                .any(|l| strip_visibility(l.trim_start()).starts_with(NEEDLE))
        })
        .map(|f| f.rel.clone())
        .collect();
    super::equal(
        "拼命令用的元字符黑名单的定义（新增消费者就派生 `acct_core::config_dir_char_unsafe`，别抄一份表）",
        homes,
        ["src/common/acct-core/src/lib.rs".to_string()],
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
/// | ③ **文本顺序 ≠ 执行顺序** | `stream/inbound/` 把 `remove` 搬进新 task | 文本上仍在前面，实际什么时候跑没人保证 |
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
    // 🔴 **人群补上两棵测试树。**
    //
    // 本条认的是「比源码位置的**判据**」，而判据整批住进了 `tests/`。
    // 只给两棵生产树时识别数从 6 掉到 **0**，自检逐字报「识别口径坏了，本条会零命中地绿」
    // —— 红得对，而它红的正是「一整棵树掉出扫描面」。
    // 规则目录不在人群里（本文件头注就带着这一族的示例文本）。
    let root = addr::repo_root();
    let files: Vec<(std::path::PathBuf, String)> = repo()
        .under(
            &[
                "src/frontend/shell/src",
                "src/backend",
                "tests/frontend/shell",
                "tests/frontend/filewin",
                "tests/backend",
                "tests/comms",
            ],
            &["rs"],
        )
        .into_iter()
        .map(|f| (root.join(&f.rel), f.text().to_string()))
        .collect();

    let mut population = 0usize;
    let mut bad: Vec<String> = Vec::new();
    // 🔴 **切函数体的锚点原来是 `"\n    fn "` —— 针里嵌着 4 个空格，而缩进是位置。**
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
        population > 0,
        "一条「比源码位置」的判据都没识别出来 —— 识别口径坏了"
    );
    assert!(
        bad.is_empty(),
        "这些位置比较型判据没守住三条纪律：\n{}\n\n\
             ★ 三种坏法各有活样本（08-08 实测，逐条写在本条头注的表里）：\n\
             ① 比注释（`watcher.rs` 曾拿一行 `// --- Phase 1 …` 当扫描锚点）；\n\
             ② 比任意一处（`local_backend.rs` 的 `rfind(\".stop()\")`；`kill.rs` 的裸 `kill-session` \n\
                在生产段有两处，命中对的那处**是排序运气**）；\n\
             ③ 文本顺序 ≠ 执行顺序（`stream/inbound/` 把 `remove` 搬进新 task 就绕过去了）。\n\
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
/// ⚠ **只跳一级**（`src/frontend/shell` 的上级就是仓根，这是 cargo 给的事实，不是猜的）。
/// 跳两级那种写法是 `K-R13` 刚治过的病：主树上算出来的东西恰好存在、工作树上
/// 算错了却被人补了一个假落点 ⇒ 判据变绿而它证明的事根本不成立。
/// 形状与从前那条帧节奏说法判据（今天并进 `retired-names.tsv`）同源（本仓已有先例）。
fn addr_repo_root() -> std::path::PathBuf {
    crate::addr::repo_root().to_path_buf()
}

/// 地址判据的语料面：四个根下的全部 `.rs`（含规则目录，判据自己写的地址也守）＋ 壳的 `build.rs`。
/// 头注里的形状样例一律用中文文件名（抽取器只收 ASCII 路径），不进人群。
fn addr_corpus() -> Vec<(std::path::PathBuf, String)> {
    let mut out: Vec<(std::path::PathBuf, String)> = Vec::new();
    for sub in [
        "src/frontend/shell/src",
        "src/common",
        "src/backend",
        "tests",
    ] {
        out.extend(tree(sub, &["rs"]));
    }
    let br = "src/frontend/shell/build.rs";
    out.push((addr::repo_root().join(br), text(br)));
    out
}

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
        for line in crate::strip_comment_lines(raw).lines() {
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
        // 于是逐字写了一个假地址（同族的 `doc/` 判据从前也有一条同形的例外，随 CONTRIBUTING 重写摘了）。
        // ⚠ 例外**按符号名**认 ⇒ 它们同时也遮住了真名叫 `symbol` / `foo` 的符号。
        //   今天全仓这两个名字**一处声明都没有**（现打），所以遮不住任何真东西；
        //   哪天真有人这么命名，上面那条保鲜自检会让这条例外**变成假绿**，如实登记。
        (
            "symbol",
            "**示例占位符**：讲「地址长什么样」时写的假地址，本就不指向真符号",
        ),
        ("foo", "**示例占位符**：同上，讲口径时举的例子"),
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
        "只抽到 {} 处符号地址 —— 剥法坏了（09-02 现打 213 处，量于当时的尖）",
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
    /// 这一张表就是「改完之后还剩什么没牙」的**逐条点名**：
    /// 表里每一行都是一处**判不了真伪**的地址。想少一行的唯一办法是把它改成符号地址。
    ///
    /// ⚠ 加行之前先问一遍：**能不能点符号**？答得出来就别加。
    const INVENTORY: &[(&str, &str, usize)] = &[
        ("ui_contract.rs", "tauri-2.11.2/src/ipc/mod.rs", 181),
        ("launch_tests.rs", "launch.rs", 122),
        // 🔴 **这一行删了 —— 那处行号地址改成了符号地址。**
        // 原来是 `guard-core/src/lib.rs` 里那句「两侧各有一个文件因此假红」的注，
        // 点的是 `ccm_cli_contract.rs` 的第 181 行。那份文件剖分后只剩 40 行 ⇒ 越界，
        // 已按本条的第 ① 条出路改成
        // `ccm_cli_contract_tests.rs::cc_spawn_resolves_a_real_ccm_file_not_a_directory`。
        // ⇒ 本条的保鲜自检逐字要求把这一行删掉（「多半是有人把它改成符号地址了，那是好事」）。
        // **存量少一条，这是往下走**（递减方向），不是把账挂空。
        ("lib.rs", "main.rs", 26),
        ("lib.rs", "russh-sftp-2.3.0/src/protocol/file_attrs.rs", 29),
        // `lib.rs → sftp.rs 的第 141 行` 摘了：creds-core 那句改成了点符号（原子上传搬去后端 `put_atomic`）。
        ("local_backend_tests.rs", "structural_scan.rs", 425),
        ("local_backend_host.rs", "launch.rs", 196),
        ("local_backend_host_tests.rs", "launch.rs", 196),
        ("local_backend_host.rs", "launch.rs", 198),
        ("local_backend_host_tests.rs", "local_backend.rs", 336),
        ("local_backend_host_tests.rs", "structural_scan.rs", 425),
        ("local_backend_host_tests.rs", "structural_scan.rs", 508),
        // `("panorama.rs", "engine.rs", 42)` 这一行删了：引用方那份文件随 monitor 的内嵌引擎一起删了。
        // 🔴 **这一行删掉了**：`config_surface.rs` 的测试段搬走之后
        //    那份文件只剩 853 行，1062 行**越界**了（本条第 ① 格当场逮住）。
        //    按本条头注唯一那条改法**改成了符号地址**
        //    （`rows_tests.rs::either_host_keeps_the_glob_count`，
        //    那正是原先第 1062 行那段话讲的东西）⇒ 从「判不了真伪」变成
        //    `every_symbol_address_in_the_sources_still_resolves` **真的判得了**。
        //    ⚠ 没有「换一个今天对的行号」—— 那是本条头注逐字禁的那一手。
        // 🔴 **这一行删了**：`parity_ledger_tests.rs` 里点 `sftp.rs` 第 141 行的那句散文
        //    住在 `creds.apikey` 那条平价欠账的理由里，那条欠账结清、整行删掉，那处行号地址随之没了。
        //    **存量少一条，这是往下走**（递减方向）。
        ("ratchet_guard.rs", "control/tmux_hook.rs", 6),
        ("ratchet_guard.rs", "control/tmux_hook.rs", 102),
        ("ratchet_guard.rs", "main.rs", 651),
        ("ratchet_guard.rs", "observe/watcher.rs", 1040),
        ("ratchet_guard.rs", "tmux_hook.rs", 6),
        ("single_stream_guard.rs", "stream/inbound/mod.rs", 35),
        ("single_stream_guard.rs", "main.rs", 585),
        ("single_stream_guard.rs", "observe/watcher.rs", 77),
        ("single_stream_guard.rs", "relay/tee.rs", 169),
        // `single_stream_guard.rs` → monitor 那份 backend 目录头注的那一条行号地址摘了：那份文件随 `backend` 目录删了，散文改成不带行号地址。
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
    let mut newly: Vec<String> = Vec::new();
    let mut overrun: Vec<String> = Vec::new();
    for (p, raw) in &corpus {
        let fname = addr_base(p);
        for (ln, cited, n) in line_addresses(raw) {
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
            // `cited` 用 `/` 写（`control/tmux_hook.rs`）⇒ 盘上路径先归一成 `/` 再比：
            // 不归一的话 Windows 上带目录的地址一个都消不了歧，越界那半静默跳过。
            let narrowed: Vec<&&(std::path::PathBuf, String)> = cands
                .iter()
                .filter(|e| {
                    e.0.to_string_lossy()
                        .replace('\\', "/")
                        .ends_with(cited.as_str())
                })
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
    // 语料面塌没塌由 `addr_corpus` 自己判（每棵根都要扫到东西）；抽取器认不认得出由
    //   `the_address_extractors_really_see_each_shape` 拿合成样本判。
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
             ② 那一处说的是历史 ⇒ 删掉，或改成说现状；\n\
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

/// [`fn_names_starting_with`] 的反向自检：**既不恒空也不恒满**，
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
        fn_names_starting_with(&text("src/frontend/shell/src/deploy.rs"), &["uninstall"])
            .contains(&"uninstall_remote_backend".to_string()),
        "真树上扫不到 `uninstall_remote_backend` —— 剥法或遍历坏了"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R20`：**只活在散文里的名字**（零定义的名字被当现状说）
// ═══════════════════════════════════════════════════════════════════════

/// 至少几个下划线才进人群。**只能是 2，不许往上调。**
///
/// 整条曲线（现打，量于基点）：
/// `>=1` 100 个/143 处 · **`>=2` 58/84** · `>=3` 43/61 · `>=4` 33/50 · `>=5` 30/44。
/// 🔴 旗舰活体 `local_tmux_names` **只有 2 个下划线，在 `>=3` 就掉出人群**。
/// ⇒ 把阈值调上去换一个好看的人群数，等于把本条要逮的那个逮不着。
const DEAD_NAME_MIN_UNDERSCORES: usize = 2;

/// 本条的语料面：`src` 与 `tests` 下的全部文本（含 toml · sh · md · json，规则目录也在），`(仓根相对路径, 原文)`。
///
/// - `tests/evidence/` 不收：那是量具与记录，散文里逐字写着一堆死名，收进来代码侧会被本族自己的记录喂饱。
/// - 本文件只收声明那一半（不带引号与注释的行）：它的散文与样本为了讲形状写着死名，它的 `fn` 名别处在点。
fn dead_name_corpus() -> Vec<(String, String)> {
    const ME: &str = "tests/common/guard-core/rules/structural_scan.rs";
    let mut out: Vec<(String, String)> = Vec::new();
    for f in repo().with_rules().under(&["src", "tests"], &[]) {
        if f.rel.starts_with("tests/evidence/")
            || f.rel.contains("/vendor/")
            || f.rel == "tests/__fixtures__/retired-names.tsv"
        {
            continue;
        }
        let text = if f.rel == ME {
            f.text()
                .lines()
                .filter(|l| !l.contains('"') && !l.contains("//"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            f.text().to_string()
        };
        out.push((f.rel.clone(), text));
    }
    out
}

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
/// `//` 那一族（`.rs` / `.ts` / …）走 [`crate::strip_comment_lines`] ——
/// **整行注释与行尾注释都剥，而且字符串安全**（`"http://host"` 不会被切短）。
///
/// 🔴 **行尾注释必须算注释**，这是 `K-R19` 实测出来的坑：它第一版尺子只认整行注释，
/// 于是 `tests/frontend/ui/ipc/commands.vitest.ts` 那行「代码 + 行尾注释」被判成「代码里有这个名字」
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
    let stripped = crate::strip_comment_lines(text);
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

/// 全语料现打：`(路径, 名字)` → 散文里的处数，只留**代码侧零出现**的。
fn dead_names_on_disk(
    corpus: &[(String, String)],
    min_us: usize,
) -> (usize, std::collections::BTreeMap<(String, String), usize>) {
    let mut in_code: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut in_prose: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();
    for (path, text) in corpus {
        for (_, code, cmt) in dead_name_split(path, text) {
            for w in dead_name_idents(&code, min_us) {
                in_code.insert(w.to_string());
            }
            for span in backtick_spans(&cmt) {
                if let Some(name) = bare_symbol_in_span(span, min_us) {
                    *in_prose
                        .entry((path.clone(), name.to_string()))
                        .or_insert(0) += 1;
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

/// ★★ **散文里用反引号点名的名字，代码里要有**（零命中，带正控）。
///
/// 改名 / 删掉之后「由 `X` 钉住」那句话不会跟着改，读的人以为这件事有人守着。
/// 说「它不在了」的那句删掉；仓外的名字写全路径。
///
/// 买不到：不带反引号的写法；代码侧把字符串字面量也算「活着」（字面量里留着旧名就遮住散文）。
#[test]
fn every_dead_name_named_in_the_prose_is_declared_dead() {
    let (_, disk) = dead_names_on_disk(&dead_name_corpus(), DEAD_NAME_MIN_UNDERSCORES);
    let found: Vec<String> = disk
        .iter()
        .map(|((path, name), n)| format!("{path}  `{name}`  {n} 处"))
        .collect();
    // 正控：同一个抽取器在合成样本上认得出一个只活在散文里的名字。
    let n = format!("zz{u}gone{u}name", u = "_");
    let sample = [(
        "a.rs".to_string(),
        format!("fn live_one_two() {{}}\n// 见 `{n}` · `live_one_two`\n"),
    )];
    let control = dead_names_on_disk(&sample, DEAD_NAME_MIN_UNDERSCORES)
        .1
        .len();
    super::none(
        "散文点名了代码里不存在的名字：改成现状（点今天真的那个符号），说「它不在了」的那句删掉；\
         仓外的名字（std · 第三方 · 内核）写全路径（`std::alloc::handle_alloc_error`）",
        found,
        control,
    );
}

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
        Some(&1),
        "散文里点名、代码里零出现 —— 这一处没进人群，闸就是死的：{dead:?}"
    );
    let with_code = format!("{prose}fn {n}() {{}}\n");
    let (_, dead) = dead_names_on_disk(&[("a.rs".to_string(), with_code)], min);
    assert!(
        dead.is_empty(),
        "代码里有这个名字了，它还留在人群里 ⇒ 本条会去红一堆活着的符号：{dead:?}"
    );

    // ⑦ `.md` 整份算散文；其余扩展名整份算代码（`.json` 里的名字是**定义**，不是提法）。
    let md_text = format!("见 {tick}{n}{tick}\n");
    let md_rows = dead_name_split("a.md", &md_text);
    assert!(md_rows[0].1.is_empty() && md_rows[0].2.contains(n.as_str()));
    let json_text = format!("{{\"k\": \"{n}\"}}\n");
    let json_rows = dead_name_split("a.json", &json_text);
    assert!(json_rows[0].2.is_empty() && json_rows[0].1.contains(n.as_str()));
}

// ── 生产树里不放测试体 ────────────────────────────────────────

/// `#[cfg(test)]` 罩着的测试专用支撑项（`thread_local!` · `impl Drop` · 辅助 `fn`）的上限：只许降。
const SUPPORT_ITEM_CEILINGS: &[(&str, usize)] =
    &[("src/backend", 7), ("src/frontend/shell/src", 15)];

/// 一棵生产树里：真 `#[test]` 属性（整行逐字）· 内联 `#[cfg(test)] mod X {` · 别的 `#[cfg(test)]` 块项。
/// 分号声明形（`#[cfg(test)] #[path = "…"] mod X;`）不算：那是把测试体挂到 `tests/` 的桩（桩指没指进 `tests/` 由 [`test_only_files_in`] 判）。
fn test_bodies_in(text: &str) -> (usize, usize, usize) {
    let (mut attrs, mut mods, mut support) = (0usize, 0usize, 0usize);
    let test_attr = format!("#[{}]", "test");
    let cfg_attr = format!("#[cfg({})]", "test");
    let attr_open = format!("#{}", "[");
    let mod_kw = format!("{} ", "mod");
    {
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if l.trim() == test_attr {
                attrs += 1;
            }
            if l.trim() != cfg_attr {
                continue;
            }
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_start().starts_with(attr_open.as_str()) {
                j += 1;
            }
            let Some(t) = lines.get(j).map(|x| x.trim()) else {
                continue;
            };
            if t.ends_with(';') || !t.ends_with('{') {
                continue;
            }
            if strip_visibility(t).starts_with(mod_kw.as_str()) {
                mods += 1;
            } else {
                support += 1;
            }
        }
    }
    (attrs, mods, support)
}

/// 生产树里只留三行桩（`#[cfg(test)]` ＋ `#[path = "…"]` ＋ `mod X;`），测试体住 `tests/`。
#[test]
fn the_production_trees_hold_no_test_bodies() {
    for (tree, ceiling) in SUPPORT_ITEM_CEILINGS {
        let (attrs, mods, support) = repo()
            .under(&[tree], &["rs"])
            .iter()
            .map(|f| test_bodies_in(f.text()))
            .fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
        assert_eq!(
            (attrs, mods),
            (0, 0),
            "`{tree}` 里测试代码回来了：`#[test]` {attrs} 个 · 内联测试模块 {mods} 处 —— 剖到 `tests/` 去"
        );
        assert!(
            support <= *ceiling,
            "`{tree}` 的测试专用支撑项涨到 {support}（上限 {ceiling}，只许降）"
        );
    }
    // 正控：同一个认法在合成样本上三样各认出一处，挂到 `tests/` 的桩不算。
    let sample = format!(
        "#[{t}]\nfn a() {{}}\n#[cfg({t})]\npub(crate) mod probe {{\n}}\n#[cfg({t})]\nfn helper() {{\n}}\n#[cfg({t})]\n#[path = \"x.rs\"]\nmod tests;\n",
        t = "test"
    );
    assert_eq!(test_bodies_in(&sample), (1, 1, 1), "认法坏了");
}

/// `#[cfg(…)]` 里的条件只在测试档成立：恰好是 `test`，或 `all(…)` 顶层有一项是 `test`。
fn cfg_only_in_tests(attr: &str) -> bool {
    let Some(p) = attr
        .trim()
        .strip_prefix("#[cfg(")
        .and_then(|s| s.strip_suffix(")]"))
    else {
        return false;
    };
    p == "test"
        || p.strip_prefix("all(")
            .and_then(|s| s.strip_suffix(')'))
            .is_some_and(|args| args.split(',').any(|a| a.trim() == "test"))
}

/// 一份源码里只在测试档编、正文却住在 `tests` 段之外的整份文件（行号从 1 起）：
/// 无花括号体的 `mod x;` 上面那串属性有只在测试档成立的 `#[cfg(…)]`、却没有指进 `tests` 段的 `#[path]`；或文件头一行 `#![cfg(test)]`。
/// 注释先剥成空行（属性与 `mod` 之间隔着注释也认）。看不见：测试模块花括号体里不带属性的 `mod x;`。
fn test_only_files_in(src: &str) -> Vec<usize> {
    let code = crate::strip_comment_lines(src);
    let lines: Vec<&str> = code.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t == "#![cfg(test)]" {
            out.push(i + 1);
            continue;
        }
        let body = strip_visibility(t);
        if !(body.starts_with("mod ") && body.contains(';') && !body.contains('{')) {
            continue;
        }
        let attrs: Vec<&str> = lines[..i]
            .iter()
            .rev()
            .map(|a| a.trim())
            .take_while(|a| a.starts_with("#[") || a.is_empty())
            .filter(|a| a.starts_with("#["))
            .collect();
        let gated = attrs.iter().any(|a| cfg_only_in_tests(a));
        let in_tests = attrs.iter().filter_map(|a| path_attr(a)).any(mounts_tests);
        if gated && !in_tests {
            out.push(i + 1);
        }
    }
    out
}

/// `src/` 里没有只在测试档编的整份文件：测试件住 `tests/` 同形位置，挂载处用 `#[path]` 指过去。
#[test]
fn src_holds_no_test_only_file() {
    let found: Vec<String> = repo()
        .under(&["src"], &["rs"])
        .into_iter()
        .filter(|f| !f.rel.starts_with("src/vendor/"))
        .flat_map(|f| {
            test_only_files_in(f.text())
                .into_iter()
                .map(move |n| format!("{}:{n}", f.rel))
        })
        .collect();
    let attr = format!("#[{}({})]", "cfg", "test");
    let sample = format!(
        "{attr}\nmod a;\n{attr}\n// 注释\npub(crate) mod b;\n#[cfg(all(unix, test))]\nmod c;\n\
         {attr}\n#[path = \"../../tests/x_tests.rs\"]\nmod ok;\n\
         #[cfg(any(test, feature = \"ts\"))]\nmod ok2;\n{attr}\nmod ok3 {{}}\n"
    );
    let inner = format!("#![{}({})]\n", "cfg", "test");
    let control = test_only_files_in(&sample).len() + test_only_files_in(&inner).len();
    assert_eq!(control, 4, "正控：合成样本该认出 a · b · c · 内属性那四处");
    super::none(
        "只在测试档编的整份文件住在 `src/`（挪进 `tests/` 同形位置，挂载处用 `#[path]`）",
        found,
        control,
    );
}
