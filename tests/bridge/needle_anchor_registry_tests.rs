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
const CORPUS_SEEDS: &[&str] = &["read_to_string(", "scan_tree!"];

/// ★ **递减棘轮的上限**（08-06 全树实测 **33** 处）。
///
/// ⚠⚠ **它从 63 降到 35 不是因为还了债，是因为量准了。**
/// 原来的传递闭包按「RHS 里**提及**了语料变量」传，会跑飞（见 `is_direct_derivation`）；
/// 收紧成「直接派生」之后，28 处**本来就不属这一族**的命中退出了计数。
/// 记下这一句是因为「上限降了」默认会被读成「有人修了 28 处」——那是假的。
///
/// ⚠ 这个数里**有假阳性**：语料变量的传递闭包只看 `let` 那一行的右侧，
/// 于是「先从磁盘读了点什么、后面又 `let` 了个提到它的变量」会被一并算进来
/// （`pubkey.rs` 那处就是）。假阳性抬高了上限、削弱了它的锐度，但**不影响方向**：
/// 新增一处仍然会越界。逐条判真伪归下一轮，见 `ROADMAP §5` 诚实边界。
///
/// 只许降。修一处就把这个数调下来，**不许调上去让今天好过**。
const BARE_CONTAINS_CEILING: usize = 33;

/// 「匹配单位比事实小」这一族的**全部**原语，各带各的递减棘轮上限。
///
/// 〔audit-0805 08-06〕原来只有 `contains` 一条。本表把族圈全 ——
/// 判据的人群应当是**这个族**，不是族里最好数的那一种（本轮逮到的洞用的是 `matches`）。
/// 数字是 **08-06 全树实测值**，不是估的（先置 0 跑一次，从诊断里读出来再钉）。
/// ⚠ 这 24 处**没有**像 `contains` 那 34 处一样被逐条判过真伪 —— 那要另开一件。
/// 棘轮的意义在此刻就是「别再长」；分类是后补的活，不是立棘轮的前提。
const MATCHER_CEILINGS: &[(&str, usize)] = &[
    (".contains(", BARE_CONTAINS_CEILING),
    (".matches(", 13),
    (".find(", 8),
    (".rfind(", 0),
    (".starts_with(", 0),
    // 🔴 〔步 7c 2026-09-19〕**2 → 0（往下拧）。** 原来那 2 处是
    //    `path.ends_with(".rs")` / `.ends_with(".ts")` 这一形 —— 已登记豁免
    //    （[`needle_is_a_file_extension`]：后缀 + 扩展名撑不大，没有更严的写法可换）。
    //    摘掉那一形之后全仓真欠账是 0 ⇒ 上限就写 0：从此任何**非**扩展名的
    //    裸 `ends_with` 当场红。
    (".ends_with(", 0),
    (".split(", 1),
    (".strip_prefix(", 0),
];

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

fn corpus_vars(test_src: &str) -> BTreeSet<String> {
    let mut vars: BTreeSet<String> = BTreeSet::new();
    for _ in 0..DERIVE_DEPTH {
        let before = vars.len();
        for line in test_src.lines() {
            let Some((name, rhs)) = let_binding(line) else {
                continue;
            };
            let seeded = CORPUS_SEEDS.iter().any(|s| rhs.contains(s));
            let derived = vars.iter().any(|v| is_direct_derivation(rhs, v));
            if seeded || derived {
                vars.insert(name.to_string());
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
/// # 〔audit-0805 08-06〕原来这里把 `.contains(` 写死了
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
/// ⚠ 这条豁免是**收紧**，不是放宽：摘掉它之后 `.ends_with(` 的上限从 2 拧到了 **0**
/// （见 [`MATCHER_CEILINGS`]）。任何**非**扩展名的裸 `ends_with` 从此当场红。
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
    let vars = corpus_vars(test_src);
    if vars.is_empty() {
        return 0;
    }
    test_src
        .match_indices(mark)
        .filter(|(i, _)| {
            // 只算 needle 是**字符串字面量**的那些：`v.contains(&x)` 是别的意思。
            let after = test_src[i + mark.len()..].trim_start();
            if !(after.starts_with('"') && vars.contains(&receiver_before(test_src, *i))) {
                return false;
            }
            // 登记过的豁免：后缀匹配 + 扩展名 needle（撑不大，理由见上）。
            !(mark == ".ends_with(" && needle_is_a_file_extension(test_src, i + mark.len()))
        })
        .count()
}

/// ★ 正题：**递减棘轮** —— 语料变量上的裸 `contains` 只许比今天少。
#[test]
fn bare_contains_on_disk_corpora_only_goes_down() {
    let root = repo_root();
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` B 类〕**补上两棵测试树。**
    //
    // 本条数的是**测试段**里的裸匹配，而测试段剖分之后整批住进了 `<repo>/tests/`。
    // 上一版三棵根全在 `src/` 下 ⇒ 下面那个 `all_contains` 从 500+ 掉到 240，
    // 自检逐字报「取测试段那步坏了，下面的棘轮此刻是空转的」—— 红得对。
    // ⇒ 五棵**互不包含**的根（`§5.4b` 纪律 1）。两棵生产树仍然要扫：
    //   `all(test, …)` 那一形的测试辅助项还住在那儿。
    // ⚠ A 类同时治掉：原来是 `scan_tree!`（靠 `file!()` 摘自己），而语料现在**含本文件**
    //   ⇒ 摘除失效不再无害（本文件头注逐字写着 `.contains("` 当例子）。
    //   改成 `scan_tree_excluding` 的明写名单，摘不到就 panic。
    let mut files = Vec::new();
    for (sub, excluded) in [
        ("src/bridge/src", &[] as &[&str]),
        ("src/backend", &[]),
        ("src/bridge/crates", &[]),
        ("tests/bridge", &["needle_anchor_registry_tests.rs"]),
        ("tests/backend", &[]),
    ] {
        files.extend(guard_core::scan_tree_excluding(
            &root.join(sub),
            &["rs"],
            excluded,
        ));
    }
    // 抽取器自检 ①：遍历活着。
    assert!(
        files.len() >= 100,
        "只扫到 {} 个 .rs（**08-08 复核：真值 135**；原写「08-06 实测 200+」是假的 —— \
             那种记下来就不再有人核的数，正是本区在治的病）—— 遍历坏了，下面的棘轮此刻是空转的",
        files.len()
    );
    // 抽取器自检 ②：**与被棘轮的那个数无关**的一个量 —— 测试段里的 `.contains("` 总数。
    // 用它而不是给棘轮配个地板：地板会在「修好一批」时变红，那是在挡住进步（F18 的教训）。
    let mut all_contains = 0usize;
    let mut hits = 0usize;
    let mut by_file: Vec<(String, usize)> = Vec::new();
    let mut per_prim: std::collections::BTreeMap<&str, usize> = Default::default();
    for (path, src) in &files {
        // ★ **剥掉注释再数**〔08-06 Phase G 后续：逐条判真伪时撞出来的〕。
        //
        // 不剥的话，一条**解释「这里原来是裸 contains」的注释**会被算成一处欠账
        // （`polling_registry` 那条 F24 的更正注释就是：它逐字写着
        // `body.contains("sleep 1")`，而那处早已改成 `pin_line`）。
        // ⇒ 判据把**自己留下的病历**当成了病。
        //
        // ⚠ 这是 F12 那条老病（判据数到注释）在本扫描器里的复发 ——
        // 而它这次的方向是**假阳性**（虚高欠账），不是假绿。虚高一样有害：
        // 它让棘轮的那个数不再等于「还欠多少」，于是「只许降」失去意义。
        // 🔴 住 `tests/` 的文件**整份就是测试段** —— 对它们再走一遍 `test_source()`
        // 会返回空串（那份文件里没有 `#[cfg(test)]` 块），于是整棵测试树零命中地绿。
        // 这一格与 `scanning_guard_registry_tests::test_side_of` 是同一条口径。
        let whole = path.to_string_lossy().contains("/tests/");
        let test_src: String = (if whole {
            (*src).clone()
        } else {
            guard_core::test_source(src)
        })
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        all_contains += test_src.matches(".contains(\"").count();
        for (mark, _) in MATCHER_CEILINGS {
            let n = bare_matcher_on_corpus(&test_src, mark);
            if n > 0 {
                by_file.push((format!("{} {mark}", path.to_string_lossy()), n));
                *per_prim.entry(*mark).or_insert(0) += n;
                if *mark == ".contains(" {
                    hits += n;
                }
            }
        }
    }
    assert!(
        all_contains >= 300,
        "整棵树的测试段里只找到 {all_contains} 个 `.contains(\"`（08-06 实测 500+）\
             —— 取测试段那步坏了，下面的棘轮此刻是空转的"
    );

    by_file.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    assert!(
        hits <= BARE_CONTAINS_CEILING,
        "语料变量上的裸 `contains(\"…\")` 有 {hits} 处 > 棘轮上限 \
             {BARE_CONTAINS_CEILING}（08-06 全树实测 33）。\n\
             ★ 匹配单位（子串）比事实（整行 / 完整签名 / 一个词）小时，把事实撑大的改动\n\
             会从缝里溜过去而判据照样绿。本区实测四次，前三次都只在造变异时才看得见。\n\
             改用 `guard_core::find_pinned`（恰好一处 + 两侧有边界）/ `pin_line`（整行相等）/\n\
             `contains_word`（有边界、不要求唯一）。\n\
             ⚠ **不许把上限调上去让今天好过** —— 这是递减棘轮。\n\
             当前分布：\n{}",
        by_file
            .iter()
            .map(|(f, n)| format!("  {n:3}  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // ★〔audit-0805 08-06〕**族里其余原语各自一条棘轮**。
    //
    // 它们没有像 `contains` 那 34 处那样被逐条判过真伪 —— 那要另开一件。
    // 但**棘轮不需要分类，只需要一个今天的数**：先把「只许降」立起来，
    // 挡住这一族继续长；分类可以后补。
    // ⇒ 这是「降级做」，不是把标准放低：覆盖面从 1 种原语扩到 8 种。
    let over: Vec<String> = MATCHER_CEILINGS
        .iter()
        .filter(|(mark, _)| *mark != ".contains(") // 上面那条已经管了，且带完整诊断
        .filter_map(|(mark, ceiling)| {
            let n = per_prim.get(mark).copied().unwrap_or(0);
            (n > *ceiling).then(|| format!("  {mark}\"…\")  {n} 处 > 上限 {ceiling}"))
        })
        .collect();
    // 🔴 〔步 7c〕诊断里**逐处点名**。原来只打「N 处 > 上限 M」——
    // 那句话读完之后不知道该去改哪一行，而这一族的默认结局就是「把上限调上去」。
    // `by_file` 本来就带着 `路径 + 原语`，只是没打出来。
    let where_of = |mark: &str| -> String {
        by_file
            .iter()
            .filter(|(f, _)| f.ends_with(mark))
            .map(|(f, n)| format!("      {n:3}  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        over.is_empty(),
        "语料变量上的裸匹配又长了（与 `contains` 同族同险：**needle 被撑大时照样绿**）：\n{}\n\
             改用 `guard_core::find_pinned`（恰好一处 + 两侧有边界）/ `pin_line`（整行相等）/\n\
             `contains_word`（有边界）。⚠ **不许把上限调上去让今天好过** —— 这是递减棘轮。",
        over.iter()
            .map(|line| {
                let mark = line.trim_start().split('"').next().unwrap_or("").to_string();
                format!("{line}\n{}", where_of(&mark))
            })
            .collect::<Vec<_>>()
            .join("\n")
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
    for sub in ["src/bridge/src", "src/backend", "src/bridge/crates", "tests"] {
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
         `needle_is_a_file_extension` 一起删掉，并把 `.ends_with(` 的上限留在 0"
    );
}

/// 抽取器的**行为**自检：喂一份人造测试段，它必须只数该数的那一处。
///
/// 没有这条，上面那个 33 只是「今天碰巧数出来的一个数」——
/// 数错方向（比如把纯字面量夹具也算进来）时它照样在上限之下。
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

/// 接收者要取**紧挨点号的那个词**，不能把前面的东西也吃进来。
#[test]
fn the_receiver_is_the_identifier_right_before_the_dot() {
    let s = "foo.bar.contains(\"x\")";
    let at = s.find(".contains(").expect("夹具里得有它");
    assert_eq!(receiver_before(s, at), "bar");
}
