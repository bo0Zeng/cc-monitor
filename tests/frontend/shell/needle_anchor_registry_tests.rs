use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 「语料变量」的**种子**：值直接来自磁盘。
///
/// 刻意**不含** `production_code(` / `production_source(` —— 那两个也常拿字面量夹具当输入
/// （`guard-core` 与 `profile_installer` 的单测就是），把它们当种子会把纯夹具断言也算进来。
/// 摸底实测（收窄到 34 个扫描型判据文件时）：含它们 73 处，只留磁盘种子 59 处。
///
/// # 🔴 这张表**漏掉了剖分之后本仓钦定的那个遍历原语**
///
/// 原文逐字只有 `["read_to_string(", "scan_tree!"]`。而 `scan_tree!` 只是一个宏，
/// 它展开成 `scan_tree_excluding_self(` —— 那个函数**号称**按 `file!()` 摘掉调用者自己，
/// 而那一刀在本仓的判据上**一处都不生效**（判据一律由 `#[path]` 挂载 ⇒ `file!()` 给的是
/// 带 `..` 的折返路径 ⇒ 后缀比**恒不命中**；逐字读数住 `scanning_guard_registry` 头注）。
/// ** 条 73 因此把「摘掉自己」换成了明写名单**，于是新写的遍历一律是
/// `guard_core::scan_tree_excluding(`（现打：`tests/frontend/shell` 29 处 · `tests/backend` 12 处 ·
/// `src/common` 1 处），而那个字面量**这张表一个都不含** ——
/// `scan_tree_excluding(` 里没有 `scan_tree!`，也没有 `read_to_string(`。
/// ⇒ 每一条改用新原语的判据，都**按构造**从这条棘轮的人群里掉了出去，而掉出去是静默的。
///
/// ★ 这正是本模块头注那一族的**镜像**（`scanning_guard_registry` 里逐字记过）：
/// 那一族是「判据在自己的登记表里找到了自己」，这一格是「**登记表根本没去找它**」。
/// 两边的默认结局都是恒绿。
///
/// ⚠ **仍然刻意不含的两个**，连理由一起写出来（这不是遗漏）：
/// · `files_by_extension(` / `shell_scripts(` —— 它们返回的是**路径**，不是文本语料
///   （`guard-core` 那两处头注逐字写着「这个只要路径」）。路径上的裸后缀匹配正是
///   [`needle_is_a_file_extension`] 已登记的那条豁免那一形，不是本区治的「needle 被撑大」。
/// · `include_str!` —— 它**是**真磁盘语料（编译期读文件），本区漏了它是**第三个洞**，
///   而它的人群比上面那一个大一个量级（现打：`tests/frontend/shell` 362 处 · `tests/backend` 158 处）。
///   🔴 **本拍刻意不补**：补它要连着逐处处置，那是另一件；读数与处置建议
///   写在 `P28` 的交付报告里，**别把这一行读成「那条路安全」**。
const CORPUS_SEEDS: &[&str] = &[
    "read_to_string(",
    "scan_tree!",
    "scan_tree_excluding(",
    "scan_tree_excluding_self(",
];

/// 「匹配单位比事实小」这一族的**全部**原语：语料变量上拿字符串字面量去够的这几种，风险一模一样
/// （needle 被撑大 ⇒ 判据照样绿）。
const MARKS: &[&str] = &[
    ".contains(",
    ".matches(",
    ".find(",
    ".rfind(",
    ".starts_with(",
    ".ends_with(",
    ".split(",
    ".strip_prefix(",
];

/// 欠账名单：今天还在的每一处「语料变量上的裸匹配」，一行一处（`住址\t接收者.原语("字面量")`，同一行可重复）。
/// 判法是**两向相等**：新写一处 ⇒ 红（改用 `guard_core::find_pinned` / `pin_line` / `contains_word`）；
/// 修掉一处 ⇒ 红（把那一行删掉）。名单住旁边那份文本（`needle_anchor_debt.txt`），一次性从现状生成。
const DEBT: &str = include_str!("needle_anchor_debt.txt");

/// 一个 `let` 绑定的名字与右侧表达式（右侧只取本行，多行 `let` 的首行足够判种子）。
fn let_binding(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start().strip_prefix("let ")?;
    let rest = rest.strip_prefix("mut ").unwrap_or(rest);
    let eq = rest.find('=')?;
    let name = rest[..eq].split(':').next()?.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    // ★ `_` **不是变量**，不许当传递闭包的中转站〔08-06 F05 下半撞出来的〕。
    //
    // 实测：`let _ = std::fs::read_to_string(…)` 会把 `_` 写进语料变量集，
    // 而 `_` 作为**独立词**在闭包参数里遍地都是（`|_| …`、`|_, _|`）⇒
    // 下一轮闭包把**几乎每一个** `let` 都卷进来（一个文件里从 0 个语料变量涨到 150 个），
    // 计数从 63 暴涨到 71，判据从「量这一族」退化成「量所有 `contains`」。
    //
    // ⚠ 这是**判据自己跑飞**，不是被测代码变坏 —— 而它的表现和真红一模一样。
    // 修法是「`_` 不是变量」这条**语言事实**，不是「把 `_` 加进黑名单」那种魔法名单
    // （本模块头注刻意反对名单：名单挡不住第 N+1 个名字）。
    if name.chars().all(|c| c == '_') {
        return None;
    }
    Some((name, &rest[eq + 1..]))
}

/// 测试段里哪些变量装着**从磁盘读来的语料**（传递闭包，跑到不动点）。
///
/// ⚠ 刻意**不用魔法变量名单**（`body` / `src` / `all` …）：那本身就是一次
/// 「匹配单位比事实小」—— 名单挡不住第 N+1 个名字，而且它错了看不出来。
/// 传递**只走一层**（种子 → 直接派生），不做不动点。
///
/// ★ 原来是跑到不动点的，08-06 撞出它会**跑飞**：
/// `let n = prod.matches(…).count()` 把一个**数**写进了语料变量集，
/// 而 `n` 这种短名在 RHS 里遍地都是 ⇒ 下一轮几乎每个 `let` 都被卷进来
/// （一个文件里 0 → 166 个语料变量，全树计数 63 → 71）。
/// 根因是传递**不看派生出来的还是不是文本**，而「是不是文本」在这个层面判不了。
///
/// ⇒ 只走一层：`let body = &ccm[..]` 这种直接切片仍然认得，更深的链认不到。
/// **欠算是已登记的诚实边界**（本模块头注：这个数是下界不是全集）；
/// 而跑飞不是欠算 —— 它让判据**从「量这一族」退化成「量所有 `contains`」**，
/// 表现却和真红一模一样。宁可欠算。
const DERIVE_DEPTH: usize = 2;

/// 一个 `for` 头部的**模式绑定名**与被迭代的表达式（右侧只取本行）。
///
/// # 🔴 [`let_binding`] 只认 `let`，而本仓取语料的主流写法不是 `let`
///
/// 逐字：`let_binding` 第一句是 `strip_prefix("let ")` ⇒ 一条
/// `for (path, src) in guard_core::scan_tree_excluding(…)` 里的 `src`
/// **永远进不了语料变量集**，于是它上面的裸 `contains("…")` 一处都不算。
/// 现打全仓这一形有 **46 处**（`for (…, …) in <取语料的原语>`）。
///
/// ⚠ 要紧的是它**不只漏新原语**：`scan_tree!` 本来就在 [`CORPUS_SEEDS`] 里，
/// 可只要它写在 `for` 头上而不是 `let` 右边，这条棘轮照样看不见 ——
/// ⇒ 这两个洞是**独立的两个**，补一个不会顺带补掉另一个。
///
/// # 射程边界（两侧都写出来）
///
/// - **接得住**：`for (p, src) in …` · `for src in …` · `for (mut a, b) in …` ·
///   `for (p, src) in &files`（`files` 是语料变量 ⇒ 走 [`is_direct_derivation`]）·
///   多行调用（右侧只取本行，而种子字面量就在 `for` 那一行上）。
/// - **接不住**：`for` 头跨行写（`for (p, src) in\n    guard_core::scan_tree…`）——
///   那时本行右侧没有种子。现打全仓 0 处，**登记为已知欠算**，不是已守。
/// - **`_` 不算变量**，同 [`let_binding`] 那条语言事实（否则 `|_|` 遍地都是，闭包会跑飞）。
///
/// ⚠ 元组里**每一个**名字都登记，包括拿到路径的那个（`path` / `p` / `f`）。
/// 那是刻意的**从严**：`PathBuf` 上压根没有 `contains`，
/// 而它真会命中的 `path.ends_with(".rs")` 恰好是 [`needle_is_a_file_extension`]
/// 已登记的那条豁免 ⇒ 多登记一个名字不会造出假账。
fn for_pattern_bindings(line: &str) -> Option<(Vec<&str>, &str)> {
    let rest = line.trim_start().strip_prefix("for ")?;
    // `in` 取**第一个**独立的词形（两侧带空格）：`for i in 0..n` 与 `for (a, b) in xs` 同形。
    let at = rest.find(" in ")?;
    let (pat, rhs) = (&rest[..at], &rest[at + " in ".len()..]);
    let names: Vec<&str> = pat
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .filter_map(|raw| {
            let t = raw.trim().trim_start_matches(['&', '*']).trim();
            let t = t.strip_prefix("mut ").unwrap_or(t).trim();
            let ok = !t.is_empty()
                && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !t.chars().all(|c| c == '_');
            ok.then_some(t)
        })
        .collect();
    (!names.is_empty()).then_some((names, rhs))
}

fn corpus_vars(test_src: &str) -> BTreeSet<String> {
    let mut vars: BTreeSet<String> = BTreeSet::new();
    for _ in 0..DERIVE_DEPTH {
        let before = vars.len();
        for line in test_src.lines() {
            // 一行要么是 `let`、要么是 `for` 头，两条路同一个判准：
            // 右侧命中种子，或右侧是某个语料变量的**直接派生**。
            let bound: Vec<(&str, &str)> = match let_binding(line) {
                Some((name, rhs)) => vec![(name, rhs)],
                None => match for_pattern_bindings(line) {
                    Some((names, rhs)) => names.into_iter().map(|n| (n, rhs)).collect(),
                    None => continue,
                },
            };
            for (name, rhs) in bound {
                let seeded = CORPUS_SEEDS.iter().any(|s| rhs.contains(s));
                let derived = vars.iter().any(|v| is_direct_derivation(rhs, v));
                if seeded || derived {
                    vars.insert(name.to_string());
                }
            }
        }
        if vars.len() == before {
            break;
        }
    }
    vars
}

/// RHS 是不是**从 `var` 这份文本直接切/借出来的**。
///
/// ★ 原来的规则是「RHS 里**提及**了 `var`」，08-06 撞出它会**跑飞**：
/// 一个大文件里总有某个 `let` 提到语料变量，而短名（`n` / `s` / `t` / `q`）一旦进集合，
/// 下一轮就把几乎所有 `let` 都卷进来（实测一个文件 0 → 166 个语料变量，
/// 全树计数 63 → 71）。**判据从「量这一族」退化成「量所有 `contains`」，
/// 而它的表现和真红一模一样。**
///
/// ⇒ 收紧成「**直接派生**」：RHS 去掉前导 `&`/`*`/空格后，**以 `var` 开头**，
/// 且紧跟的是非标识符字符（`[` / `.` / `,` / `)` / 空白 / 结尾）。
/// 覆盖真实形状 `&ccm[a..b]` / `body.trim()` / `src[at..]`；
/// 不覆盖 `format!("{a}{corpus}")` 这类拼接（**欠算**，已在头注登记为下界）。
///
/// ⚠ 用「以它开头」而不是「包含它」—— 那正是本区 **F24** 那一族：
/// 匹配单位（提及）比事实（派生）大，把不相干的也吃了进来。
fn is_direct_derivation(rhs: &str, var: &str) -> bool {
    let body = rhs.trim_start_matches([' ', '&', '*']);
    let Some(after) = body.strip_prefix(var) else {
        return false;
    };
    !after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `<接收者>.contains("` 里的接收者标识符（紧挨在点号前的那个词）。
fn receiver_before(hay: &str, dot_at: usize) -> String {
    let head = &hay[..dot_at];
    let mut chars: Vec<char> = head
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    chars.reverse();
    chars.into_iter().collect()
}

/// 一份测试段里，「语料变量上的裸 `<原语>(\"…\")`」有几处。
///
/// # 原来这里把 `.contains(` 写死了
///
/// 本模块治的族叫「**匹配单位比事实小**」，而 `contains` 只是这个族的**一个成员**：
/// `matches` / `find` / `starts_with` / `ends_with` / `split` / `strip_prefix` 拿字符串字面量
/// 去够语料时，**风险一模一样**（needle 被撑大 ⇒ 照样绿）。
///
/// 实测这不是理论风险：**本轮在 `polling_registry` 逮到的那个洞用的正是 `matches`**
/// （`src.matches("{api}(")`，一个空格就出圈），而本模块**从头到尾看不见它** ——
/// 治这一族的判据，自己的人群只取了族里的一种形态。
///
/// 全树测试段实测：`contains` 555 · `find` 104 · `starts_with` 88 · `matches` 35 ·
/// `strip_prefix` 28 · `ends_with` 25 · `split` 14 · `rfind` 6。
/// ⇒ 只盯 `contains` 等于放掉一半以上的族成员。
/// 一处 `.ends_with("…")` 的 needle 是不是**纯扩展名**（`.rs` / `.ts` / `.d.ts` / `.vitest.ts`）。
///
/// # 为什么这一形要从「裸匹配欠账」里摘出去〔步 7c 2026-09-19 登记，带理由〕
///
/// 本区棘轮治的风险**只有一个**：`匹配单位（子串）比事实小 ⇒ 把事实撑大的改动
/// 从缝里溜过去而判据照样绿`（头注那张表三个实例：`起流`→`起流程` ·
/// `src/backend`→`src/backend-X` · 同名参数）。
///
/// **后缀匹配 + 扩展名 needle 撑不大**：`ends_with` 把 needle 钉在字符串**末尾**，
/// 而扩展名后面按定义没有东西。`x.ends_with(".test.ts")` 不会被 `.test.tsx` 命中
/// （那不是以 `.test.ts` 结尾）。⇒ 这一形上 `find_pinned` / `pin_line` / `contains_word`
/// **一个都不适用**，也没有更严的写法可换 —— 它不是欠账，是这件事的正确写法。
///
/// ⚠ 这条豁免是**收紧**，不是放宽：扩展名形之外，任何裸 `ends_with` 都进欠账名单
/// （见 [`DEBT`]），新写一处当场红。
/// 下面 [`the_extension_suffix_exemption_is_still_load_bearing`] 是它的保鲜自检：
/// 哪天全仓一处都不再用这一形，这条豁免必须删掉，不许留着替真欠账挡枪。
fn needle_is_a_file_extension(test_src: &str, after_mark: usize) -> bool {
    let after = test_src[after_mark..].trim_start();
    let Some(rest) = after.strip_prefix('"') else {
        return false;
    };
    let Some(end) = rest.find('"') else {
        return false;
    };
    let lit = &rest[..end];
    // 形状：`.` 打头，其余只许是小写字母 / 数字 / 再来一个 `.`（`.d.ts` / `.vitest.ts`）。
    lit.len() >= 2
        && lit.starts_with('.')
        && lit[1..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.')
}

fn bare_matcher_on_corpus(test_src: &str, mark: &str) -> usize {
    bare_matcher_hits(test_src, mark).len()
}

/// 每一处命中写成 `接收者.原语("字面量")`（不带行号：行号每一轮都变，字面量与接收者才是那一处的身份）。
fn bare_matcher_hits(test_src: &str, mark: &str) -> Vec<String> {
    let vars = corpus_vars(test_src);
    if vars.is_empty() {
        return Vec::new();
    }
    test_src
        .match_indices(mark)
        .filter_map(|(i, _)| {
            // 只算 needle 是**字符串字面量**的那些：`v.contains(&x)` 是别的意思。
            let after = test_src[i + mark.len()..].trim_start();
            let recv = receiver_before(test_src, i);
            if !(after.starts_with('"') && vars.contains(&recv)) {
                return None;
            }
            // 登记过的豁免：后缀匹配 + 扩展名 needle（撑不大，理由见上）。
            if mark == ".ends_with(" && needle_is_a_file_extension(test_src, i + mark.len()) {
                return None;
            }
            Some(format!("{recv}{mark}{}", string_literal(after)))
        })
        .collect()
}

/// `"…"` 打头的那段字面量原文（含两侧引号；转义的引号不算收尾；没收尾 ⇒ 到本行末）。
fn string_literal(after: &str) -> &str {
    let b = after.as_bytes();
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return &after[..=i],
            b'\n' => return &after[..i],
            _ => i += 1,
        }
    }
    after
}

/// ★ 正题：**递减棘轮** —— 语料变量上的裸 `contains` 只许比今天少。
#[test]
fn bare_matchers_on_disk_corpora_equal_the_debt_list() {
    let root = repo_root();
    // 测试段整批住 `<repo>/tests/`；两棵生产树仍然要扫（`all(test, …)` 那一形的测试辅助项还住在那儿）。
    // 每份文件带着「整份就是测试段」这一位：按它从哪棵根扫出来定（根名是本文件写死的 `/` 串）。
    let mut files = Vec::new();
    for (sub, excluded) in [
        ("src/frontend/shell/src", &[] as &[&str]),
        ("src/backend", &[]),
        ("src/common", &[]),
        ("tests/frontend/shell", &["needle_anchor_registry_tests.rs"]),
        ("tests/frontend/filewin", &[]),
        ("tests/backend", &[]),
        ("tests/comms", &[]),
    ] {
        let whole = sub.starts_with("tests/");
        let got = guard_core::scan_tree_excluding(&root.join(sub), &["rs"], excluded);
        // ★ 抽取器自检：哪一棵根一份都没扫到 ⇒ 那一块零命中地绿。
        assert!(!got.is_empty(), "`{sub}` 下一份 .rs 都没扫到 —— 遍历坏了");
        files.extend(got.into_iter().map(|(path, src)| (path, src, whole)));
    }
    let mut now: Vec<String> = Vec::new();
    let mut all_contains = 0usize;
    for (path, src, whole) in &files {
        // 剥掉注释再数：解释「这里原来是裸 contains」的注释不是欠账。
        // 住 `tests/` 的文件**整份就是测试段**（对它们再取 `test_source()` 会得到空串）。
        let test_src: String = (if *whole {
            (*src).clone()
        } else {
            guard_core::test_source(src)
        })
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
        all_contains += test_src.matches(".contains(\"").count();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        for mark in MARKS {
            for hit in bare_matcher_hits(&test_src, mark) {
                now.push(format!("{rel}\t{hit}"));
            }
        }
    }
    // ★ 抽取器自检：取测试段那步坏了 ⇒ 一个 `.contains("` 都见不到，下面的相等是空转的。
    assert!(
        all_contains > 0,
        "整棵树的测试段里一个 `.contains(\"` 都没有 —— 取测试段那步坏了"
    );
    now.sort();
    let mut want: Vec<String> = DEBT
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    want.sort();
    let (mut extra, mut gone) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < now.len() || j < want.len() {
        match (now.get(i), want.get(j)) {
            (Some(a), Some(b)) if a == b => {
                i += 1;
                j += 1;
            }
            (Some(a), Some(b)) if a < b => {
                extra.push(a.clone());
                i += 1;
            }
            (Some(a), None) => {
                extra.push(a.clone());
                i += 1;
            }
            (_, Some(b)) => {
                gone.push(b.clone());
                j += 1;
            }
            (None, None) => break,
        }
    }
    assert!(
        extra.is_empty(),
        "语料变量上又多了裸匹配（匹配单位比事实小：needle 被撑大时照样绿）：\n{}\n\
         改用 `guard_core::find_pinned`（恰好一处 + 两侧有边界）/ `pin_line`（整行相等）/ `contains_word`（有边界）。\n\
         ⚠ 不许把这几行抄进 `needle_anchor_debt.txt` 让它绿 —— 名单只许删行。",
        extra.iter().map(|l| format!("+ {l}")).collect::<Vec<_>>().join("\n")
    );
    assert!(
        gone.is_empty(),
        "名单里这几处盘上已经没有了（修掉了 / 改写了）—— 把它们从 `needle_anchor_debt.txt` 里删掉：\n{}",
        gone.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")
    );
}

/// ★ 豁免表保鲜 ＋ 行为自检：[`needle_is_a_file_extension`] 那条豁免**今天真的在承重**，
/// 而且**只**遮扩展名这一形。
///
/// 豁免是**欠账**，不是免检章（同 `structural_scan::EXCEPTIONS` 的保鲜自检一个道理）：
/// · 没有一处在用它 ⇒ 删掉它，别留着替真欠账挡枪；
/// · 它遮住了非扩展名的 needle ⇒ 那就是放宽，当场红。
#[test]
fn the_extension_suffix_exemption_is_still_load_bearing() {
    // ① 行为：扩展名形摘掉、非扩展名形留下。
    let fixture = "\
            let disk = std::fs::read_to_string(p).unwrap();\n\
            assert!(disk.ends_with(\".rs\"));\n\
            assert!(disk.ends_with(\".vitest.ts\"));\n\
            assert!(disk.ends_with(\"fn main() {}\"));\n";
    assert_eq!(
        bare_matcher_on_corpus(fixture, ".ends_with("),
        1,
        "扩展名那两处该被豁免、最后那处（真 needle）该留下 —— \
         这一格错了说明豁免的形状判定偏了（要么放宽、要么误伤）"
    );

    // ② 保鲜：全仓真的还有人在用这一形。用不到了就该把豁免删掉。
    let root = repo_root();
    let mut used = 0usize;
    for sub in [
        "src/frontend/shell/src",
        "src/backend",
        "src/common",
        "tests",
    ] {
        for (_, src) in guard_core::scan_tree_excluding(&root.join(sub), &["rs"], &[]) {
            let mut i = 0usize;
            while let Some(j) = src[i..].find(".ends_with(") {
                let at = i + j + ".ends_with(".len();
                if needle_is_a_file_extension(&src, at) {
                    used += 1;
                }
                i = at;
            }
        }
    }
    assert!(
        used > 0,
        "全仓一处扩展名形的 `ends_with` 都没有了 —— 把那条豁免连同 \
         `needle_is_a_file_extension` 一起删掉"
    );
}

/// 抽取器的**行为**自检：喂一份人造测试段，它必须只数该数的那一处。
///
/// 没有这条，欠账名单只是「今天碰巧扫出来的一份」——
/// 数错方向（比如把纯字面量夹具也算进来）时它照样能和名单对上。
#[test]
fn the_extractor_counts_only_disk_corpora() {
    let fixture = "\
            let disk = std::fs::read_to_string(p).unwrap();\n\
            let derived = &disk[3..];\n\
            let fixture_only = \"literal text\";\n\
            assert!(disk.contains(\"a\"));\n\
            assert!(derived.contains(\"b\"));\n\
            assert!(fixture_only.contains(\"c\"));\n\
            assert!(disk.contains(&other));\n";
    let vars = corpus_vars(fixture);
    assert!(vars.contains("disk"), "种子变量没认出来：{vars:?}");
    assert!(
        vars.contains("derived"),
        "传递闭包没跟上 —— `derived` 是从 `disk` 切出来的：{vars:?}"
    );
    assert!(
        !vars.contains("fixture_only"),
        "把纯字面量夹具当成磁盘语料了：{vars:?}"
    );
    // ★ 08-06 撞出的跑飞形状：**提及**不算派生。
    let mention = "\
            let disk = std::fs::read_to_string(p).unwrap();\n\
            let msg = format!(\"{disk} 之外的话\");\n\
            let sliced = &disk[1..];\n";
    let mv = corpus_vars(mention);
    assert!(mv.contains("sliced"), "直接切片该算派生：{mv:?}");
    assert!(
        !mv.contains("msg"),
        "**提及**被当成了派生：{mv:?} —— 那正是让闭包跑飞的规则（一个文件 0 → 166 个语料变量）"
    );
    assert_eq!(
        bare_matcher_on_corpus(fixture, ".contains("),
        2,
        "该数的是 `disk.contains(\"a\")` 与 `derived.contains(\"b\")` 两处：\
             字面量夹具那处不算，`contains(&other)`（needle 不是字面量）也不算"
    );
}

/// ★ **负对照**：`P28` 补的这两个洞，**旧口径在同一份夹具上是零命中**。
///
/// 形态照 `stream_source::write_half_guard::the_shared_stripper_keeps_the_part_this_guard_must_scan`
/// —— 判的不是产品性质，是「**这把尺子够得着我说它够得着的那一段**」。
///
/// 🔴 **为什么不能只断言「语料变量集非空」**：旧口径在下面这两份夹具上，
/// 语料变量集**就是空集** ⇒ 一条非空型的地板在这个方向上**恒不响**。
/// 所以两半都用**相等**（`assert_eq!` 到具体处数）＋ 对旧口径的**零命中**断言。
#[test]
fn the_extractor_sees_the_for_pattern_and_the_designated_tree_primitive() {
    // ── 洞 ①：种子表漏了剖分之后钦定的那个遍历原语 ──
    let by_let = "\
            let files = guard_core::scan_tree_excluding(&root, &[\"rs\"], &[]);\n\
            assert!(files.contains(\"needle\"));\n";
    assert_eq!(
        bare_matcher_on_corpus(by_let, ".contains("),
        1,
        "`scan_tree_excluding(` 没被当成语料种子 —— 用这个原语取语料的判据**整批**掉出人群"
    );
    // 负对照：**旧的那张种子表**在这一行上够不着。旧的两个种子逐字写在这里 ——
    // 本文件已在 [`bare_matchers_on_disk_corpora_equal_the_debt_list`] 的排除名单上
    // （摘不到就 panic），写出来不会自匹配。
    let seed_line = by_let.lines().next().expect("夹具第一行");
    for old_seed in ["read_to_string(", "scan_tree!"] {
        assert!(
            !seed_line.contains(old_seed),
            "旧种子 `{old_seed}` 居然命中了这一行 —— 这条对照失去意义，重新确认原语拼写"
        );
    }

    // ── 洞 ②：只认 `let`，不认 `for` 模式绑定 ──
    // ⚠ 与洞 ① **独立**：这里用的 `scan_tree!` 本来就在种子表里，
    //   只因为它写在 `for` 头上而不是 `let` 右边，旧口径照样一处都看不见。
    let by_for = "\
            for (path, src) in guard_core::scan_tree!(&root, &[\"rs\"]) {\n\
                assert!(src.contains(\"needle\"));\n\
                assert!(path.starts_with(\"x\"));\n\
            }\n";
    assert_eq!(
        bare_matcher_on_corpus(by_for, ".contains("),
        1,
        "`for (path, src) in …` 里的 `src` 没进语料变量集"
    );
    assert_eq!(
        bare_matcher_on_corpus(by_for, ".starts_with("),
        1,
        "元组里拿到路径的那个名字也该登记 —— 刻意从严，理由见 [`for_pattern_bindings`] 头注"
    );
    // 负对照：旧口径的入口是 [`let_binding`]，拿它**现打**这一行，必须是 `None`。
    let for_head = by_for.lines().next().expect("夹具第一行");
    assert!(
        let_binding(for_head).is_none(),
        "`let_binding` 居然认了 `for` 头 —— 这条对照失去意义，旧口径没有瞎过"
    );

    // ── 边界：`_` 不是变量，两条路共用那条语言事实 ──
    //
    // 🔴 **这一格的第一版是空真的，死值验刀 3 当场逮到** ——
    // 原文断的是「`bare_matcher_on_corpus(夹具, ".contains(") == 0`」，而夹具里的接收者
    // 叫 `whatever`：把 `_` 登记成变量之后，`whatever` 照样不在集合里 ⇒ 那个 0 **本来就成立**，
    // 刀砍下去一声不响（`5 passed`）。⇒ 断言换成直接落在被判的那个函数上。
    let anon = "\
            for (_, _) in guard_core::scan_tree_excluding(&root, &[\"rs\"], &[]) {\n\
                assert!(whatever.contains(\"needle\"));\n\
            }\n";
    let anon_head = anon.lines().next().expect("夹具第一行");
    assert!(
        for_pattern_bindings(anon_head).is_none(),
        "`for (_, _)` 把 `_` 登记成了变量 —— 那正是让闭包跑飞的那一形\
         （[`let_binding`] 头注逐字记过 0 → 166 那一次）"
    );
    assert!(
        corpus_vars(anon).is_empty(),
        "整份夹具里只有 `_` 被绑定，语料变量集却非空：{:?}",
        corpus_vars(anon)
    );
}

/// 接收者要取**紧挨点号的那个词**，不能把前面的东西也吃进来。
#[test]
fn the_receiver_is_the_identifier_right_before_the_dot() {
    let s = "foo.bar.contains(\"x\")";
    let at = s.find(".contains(").expect("夹具里得有它");
    assert_eq!(receiver_before(s, at), "bar");
}
