//! **匹配单位不许比事实小**〔audit-0805 F24，F+#4 开〕。
//!
//! # 它治的族（与 F23 是兄弟）
//!
//! F23 治的是「判据读到了**自己**」。本条治的是另一半：判据用来匹配的那个**单位**
//! （一段子串），比它要钉的**事实**（一整行 / 一个完整签名 / 一个具体的词）**小** ——
//! 于是任何把事实撑大的改动都从缝里溜过去，而判据照样绿。
//!
//! | 何处 | needle | 撑大成 | 后果 |
//! |---|---|---|---|
//! | F05 | `起流` | `起流程` | 阶段埋点判据认错阶段 |
//! | F16 | `remote-daemon-proto` | `remote-daemon-proto-X` | 「跨 target check 走 daemon 的 lock」这个前提没了却不红 |
//! | F19 | `exe_suffix: &str` | 另一个函数的**同名参数** | 断言指的不是它自称的那个函数 |
//! | 本轮 | `sleep 1` | `sleep 10` | `polling_registry` 整段头注按「每秒」算代价，周期改了不红 |
//!
//! ★ 前三次**没有一次是被「判据变红」发现的**，全靠变异；第四次（`sleep 1`）是本件
//! B′ 摸底时才量出来的，此前也没有任何判据看得见它。这一族的默认结局同样是**恒绿**。
//!
//! # 修法：给它带边界的匹配，然后钉住存量只许降
//!
//! `guard_core::find_pinned`（恰好一处 + 两侧有边界）· `guard_core::pin_line`（整行相等）·
//! `guard_core::contains_word`（有边界、不要求唯一）。
//! 本模块要求：**测试段里，凡是拿「从磁盘读来的语料」去做的裸 `contains("…")`，
//! 只许比今天少。** 新写的判据请走上面三个。
//!
//! # ★ 存量已**逐条判过真伪**（08-06，Phase G 之后那一轮）
//!
//! 判准是一条**可复用的规则**，不是逐条品味：
//!
//! | 形态 | 危不危险 | 为什么 |
//! |---|---|---|
//! | **存在性断言 / 抽取器自检** | **不危险** | 它只需要「有」。needle 小一点仍然为真 —— 结论不依赖唯一性（`src.len() > 10_000 && src.contains("ccm")`、段界自检 `ws.contains("members")`、逐文件 `prod.contains("route_call_error")` 都是这类） |
//! | **正向事实钉** | ★ **危险** | 它声称「就是这个」。needle 被撑大时**照样绿** = 假绿 |
//! | **负向断言**（`!x.contains(…)`） | 另一族 | 撑大导致的是**假红**，不是假绿。本刀不治，登记在 `ROADMAP §5` |
//!
//! 逐条过完 34 处的结论：**只有一处是正向事实钉** ——
//! `profile_installer::install_to_nonexistent_path_creates_file` 的
//! `content.contains("function cc")`（同文件模板能生成 `function ccm` 形态 ⇒ 改名后照样绿）。
//! **已改用 `contains_word`**，变异复验过（把模板改成 `function {0}X` ⇒ 当场红）。
//!
//! 其余全部是**存在性/自检**：needle 小不影响结论。⇒ **剩下的 33 处不是欠账，是分类完毕的存量**；
//! 棘轮继续挡「新增」，而不再暗示「这里还有 33 个 bug」。
//!
//! # ⚠ 本模块自己就是这一族的高危户
//!
//! 它靠字符串匹配去查字符串匹配。摸底时**量具自己先掉进去过一次**：
//! 用 `#[cfg(test)]` 找测试段起点，结果匹配到了 `guard-core` 头注里逐字写着的那串字符，
//! 于是把生产代码也当成测试段扫，数出来 112 处（真实 63）。
//!
//! ⚠ 另一处**别读错的数**：摸底时先收窄到 F23 圈出的 34 个扫描型判据文件，数出 **59**；
//! 本条落地时扫的是整棵树（那才是对的范围），实测 **63**。
//! 两个数都对，量的不是同一件事 —— 写下来是因为「59」在本模块早期注释里出现过，
//! 而「一个数字出现在两个地方就会漂」正是本区 E12 反复记的事。
//! ⇒ 这里取测试段一律走 `guard_core::test_source`（与剥生产段共用同一份区间判定），
//! 变量名比对一律走 `guard_core::contains_word`（带边界），**不许在本文件里写裸 `contains` 去判词**。

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
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

    /// 一份测试段里，「语料变量上的裸 `contains(\"…\")`」有几处。
    fn bare_contains_on_corpus(test_src: &str) -> usize {
        let vars = corpus_vars(test_src);
        if vars.is_empty() {
            return 0;
        }
        let mark = ".contains(";
        test_src
            .match_indices(mark)
            .filter(|(i, _)| {
                // 只算 needle 是**字符串字面量**的那些：`v.contains(&x)` 是别的意思。
                let after = test_src[i + mark.len()..].trim_start();
                after.starts_with('"') && vars.contains(&receiver_before(test_src, *i))
            })
            .count()
    }

    /// ★ 正题：**递减棘轮** —— 语料变量上的裸 `contains` 只许比今天少。
    #[test]
    fn bare_contains_on_disk_corpora_only_goes_down() {
        let root = repo_root();
        let mut files = Vec::new();
        for sub in [
            "src-tauri/src",
            "remote-daemon-proto/src",
            "src-tauri/crates",
        ] {
            files.extend(guard_core::scan_tree!(&root.join(sub), &["rs"]));
        }
        // 抽取器自检 ①：遍历活着。
        assert!(
            files.len() >= 100,
            "只扫到 {} 个 .rs（08-06 实测 200+）—— 遍历坏了，下面的棘轮此刻是空转的",
            files.len()
        );
        // 抽取器自检 ②：**与被棘轮的那个数无关**的一个量 —— 测试段里的 `.contains("` 总数。
        // 用它而不是给棘轮配个地板：地板会在「修好一批」时变红，那是在挡住进步（F18 的教训）。
        let mut all_contains = 0usize;
        let mut hits = 0usize;
        let mut by_file: Vec<(String, usize)> = Vec::new();
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
            let test_src: String = guard_core::test_source(src)
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            all_contains += test_src.matches(".contains(\"").count();
            let n = bare_contains_on_corpus(&test_src);
            if n > 0 {
                by_file.push((path.to_string_lossy().into_owned(), n));
                hits += n;
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
            bare_contains_on_corpus(fixture),
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
}
