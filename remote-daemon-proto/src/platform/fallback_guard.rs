//! U4a（2026-08-01）：**非目标平台的 fallback 分支，不许凭空返回一个「成功」值。**
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**各自只许有一句**，`readonly_guard::g6_scope_pins` 钉着）
//!
//! - **它守的性质是**：非目标平台的 fallback 分支，不许凭空返回一个看起来无害的「成功」值 —— 答不上来的问题要诚实地说不知道（`false` / `None` / `unimplemented!()`）。
//! - **它扫的人群是**：`src/platform/` 递归全部 `.rs`（跳过自身）的生产段里，每一个非主分支的平台 cfg 属性**紧跟着的那一个 item 或块**，判红条件只有两条：块体里出现裸 `true` · 块的**最后一个表达式**以 `Some(` / `Ok(` 打头。
//!
//! ⚠ **这两行今天双向同时错开，而且「它们是同一件事」钉不住** —— 三道护栏里最严重的一道，如实登记：
//! 人群比性质**大**（真正的平台实现也在人群里，而且它是 [`tests::the_platform_blocks_are_still_a_mixed_population`] 那条断言的样本）；
//! 人群比性质**小**（性质说的是「非目标平台的 fallback 分支」，人群只有 `platform/` 这一个目录）；
//! 而两者之间**没有可机检的桥**：性质是「诚实」这个语义判断，人群是「那个块的最后一行长什么样」这个文本形状。
//! ⇒ **今天靠纪律。** 它今天真能拦住的形状全表、以及**两条**今天就通过了的反例，在 [`g6_reach`]。
//!
//! # 〔`K-G6` `KG61`〕本护栏的定性：**今天既不构成阻塞，也不构成保护**
//!
//! - **不构成阻塞**：惯用写法（`#[cfg(平台)] mod x;` + 独立文件）整文件绕过；返回**计算值**的
//!   真实现也通过 —— `platform/paths.rs` 那个 Windows 分支今天就在人群里、就通过。
//! - **不构成保护**：它要挡的那个形状（非目标平台的回退块、块体一个裸 `true`）
//!   今天就在 `plugin/discover.rs` 活着，而人群够不着它。
//! - **它真守住的那一点点是**：`platform/` 这一层里、**内联写法**的、**字面**乐观值。
//!
//! ★★ 两条反例**分开列、不合成一条**：一条证「不构成阻塞」，一条证「不构成保护」——
//! 合成一条会让下一个人以为**一条反例就够**，而那正是本轮要治的病。
//!
//! # 这条护栏从一个真实的地雷来
//!
//! `pid_alive` 的非 Linux 分支曾经是：
//!
//! ```text
//! #[cfg(not(target_os = "linux"))]
//! { let _ = pid; true }   // “treat as alive so the cross-platform smoke still works”
//! ```
//!
//! 那个 `true` 是**判活的加表门**。恒真的后果是**会话永远不被归档**，
//! 而且没有任何信号说「这个平台上我根本不知道」。它在仓里活了很久，
//! U2 与 U3 两轮都识别出来、两轮都推迟（理由都是「改它 = 决定 Windows 语义」），
//! 到 U4a 才拆掉。
//!
//! **这类地雷的共同形状**：为了让代码「在别的平台上也编得过 / 也能跑一下」，
//! 给一个答不上来的问题编一个看起来无害的答案。而 `true` / `Some(..)` / `Ok(..)`
//! 恰恰是最危险的那几个 —— 它们让上层以为拿到了事实。
//!
//! # 判据
//!
//! 扫 `platform/` 的生产段，找 `#[cfg(not(target_os = "linux"))]` /
//! `#[cfg(not(unix))]` 这类 fallback 块，断言两件事：
//! ① 块体里**不出现**裸 `true`（全体扫）；
//! ② 块体**最后一个表达式**不是 `Some(..)` / `Ok(..)`〔08-06 补〕。
//!
//! ⚠ ② 是补上来的，理由是实测：把 `proc.rs` 非 Linux 那支的 `None` 改成 `Some(0)`，
//! 本守卫**全绿放过** —— 而下面那段头注早就把 `Some(..)`/`Ok(..)` 与 `true` 并列成
//! 「最危险的那几个」。**说了危险却没查**，是这一族最容易长出来的缝。
//! 那个 `0` 不是无害的：判活表按 `captured != current` 判 PID 复用，
//! 一个编造的 `0` 会让活着的进程被判成「已复用 ⇒ 已死」，正是误归档。
//! ② 只看块的**值**（最后一个表达式），不像 ① 那样全体扫 ——
//! 体内的 `if let Some(x)` 是合法解构，全体扫会误伤（已用对照变异验过不误红）。
//!
//! **允许什么**：`false`（保守方向，如 `send_sigusr1` —— 发不出去当没发，调用方本就容忍）·
//! `None`（「不知道」的正确表达）· `unimplemented!()` / `todo!()`（大声说没做）。
//!
//! # 为什么它是黑名单，而本仓的原则偏好白名单〔08-06 实测答复〕
//!
//! `structural_scan.rs` 头注写着本仓的偏好：**结构性扫描天然是白名单** ——
//! 枚举每一处出现、要求它们都满足好性质，新增的自动被纳入；黑名单则要求
//! 「预先想全所有坏写法」，审计曾用五种没想到的写法绕过 B04 那条守卫。
//! 按这条原则，本守卫（列坏值：`true` / `Some(..)` / `Ok(..)`）**看起来该改成白名单**。
//!
//! 08-06 量了一次：**改不了，因为人群不同质。** `platform/` 的 `#[cfg(平台)]` 块混着两类 ——
//! 一类是**诚实空壳**（`proc.rs` 的 `None` 与多行 `unimplemented!(…)`、`signal.rs` 的 `false`），
//! 另一类是**真正的平台实现**（`paths.rs` 的 `p.to_path_buf()` / `PathBuf::from(...)`）。
//! 白名单「块的值必须是 `false`/`None`/`unimplemented!()`」会把后者一律误红。
//!
//! ⇒ 结论不是「原则错了」，是**它有前提：枚举式白名单要求被枚举的人群同质**。
//! 这里不同质，所以黑名单是对的选择，代价（列不全）如实记在下面。
//! 那个前提由 `the_platform_blocks_are_still_a_mixed_population` 盯着：
//! 哪天这些块只剩诚实空壳一类，白名单就做得成了，那时回来改。
//!
//! **它挡不住什么**（如实登记，别再宣称完备 —— U3 在 `layering_guard` 上栽过这一次）：
//! - **等价改写绕得过**：`!false` / `1 == 1` / 任何恒真表达式。08-06 实测确认
//!   （把 `signal.rs` 的 `false` 改成 `!false`，全绿）。这条**刻意不追** ——
//!   完备性在这里做不到，而本守卫挡的是「顺手编一个乐观答案」这个真实高频形态。
//! - 只看 `platform/`。别处的 fallback 不管（但别处**不该有** fallback —— §1.1-1 说平台分支只许在这层）。
//! - 它挡的是「顺手编一个乐观答案」这个**真实且高频**的失败模式，不是一个完备证明。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 会引出一个**平台分支**的 cfg 写法 —— 正向与反向都要认。
    ///
    /// # 只认 `not(...)` 是本护栏第一版最大的洞
    ///
    /// Phase D 审计的变异 M6：把 `pid_alive` 的地雷用 **`#[cfg(windows)]`** 原样放回去
    /// （`{ let _ = pid; true }`），**fmt / 本护栏 / 跨 target check / 200 条测试四道门全绿**。
    ///
    /// 而 **U4b 写 Windows 实现时必然用的就是正向 cfg**。也就是说：这条护栏在它最该起作用的
    /// 那一刻是全瞎的。头注原本列了两条「挡不住什么」，**没有这一条** —— 而隔壁
    /// `layering_guard.rs` 一个 commit 前刚写着「自己刚批评过的形状不能自己再犯一遍」。
    const FALLBACK_CFGS: &[&str] = &[
        "#[cfg(not(target_os = \"linux\"))]",
        "#[cfg(not(unix))]",
        "#[cfg(not(windows))]",
        "#[cfg(windows)]",
        "#[cfg(target_os = \"windows\")]",
        "#[cfg(target_os = \"macos\")]",
    ];

    /// 主分支（Linux 原生实现）。**只有这两种**；其余平台 cfg 一律按回退处理。
    const PRIMARY_CFGS: &[&str] = &["#[cfg(target_os = \"linux\")]", "#[cfg(unix)]"];

    /// 一份源码里全部**平台**条件编译属性（`#[cfg(test)]` 之类不算）。
    pub(super) fn platform_cfgs(code: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("#[cfg(") {
            let i = from + rel;
            let Some(end) = code[i..].find(")]") else {
                break;
            };
            let attr = &code[i..i + end + 2];
            from = i + end + 2;
            let mentions_platform = ["windows", "unix", "target_os", "target_family"]
                .iter()
                .any(|k| attr.contains(k));
            if !mentions_platform || attr.contains("test") {
                continue;
            }
            if PRIMARY_CFGS.contains(&attr) {
                continue;
            }
            if !out.contains(&attr.to_string()) {
                out.push(attr.to_string());
            }
        }
        out
    }

    pub(super) fn platform_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("platform");
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).expect("read platform dir") {
                let p = e.expect("dir entry").path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                // 跳过本护栏自身：它的说明文字里逐字写着那个 `true`。
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                // 跳过本护栏自身：它的说明文字里逐字写着那个布尔字面量。
                // **按路径判不按裸文件名**（Phase D 审计）：跳过是**豁免**语义 = fail-open，
                // 落在 U3 判 `readonly_guard` 白名单有罪的那一侧（而登记表那种「认账」语义
                // 是 fail-closed、可以裸名 —— 判据看的是失败方向，见 no_timer_guard 的对照表）。
                if rel == "fallback_guard.rs" {
                    continue;
                }
                out.push((
                    rel,
                    production_code(&std::fs::read_to_string(&p).expect("read")),
                ));
            }
        }
        out.sort();
        out
    }

    /// 取 cfg 之后**那一个 item** 的文本。
    ///
    /// # 为什么不能只认花括号块
    ///
    /// 第一版是 `code[start..].find('{')` —— 从 cfg 之后找**全文下一个** `{`。
    /// Phase D 审计逮到两条后果：
    ///
    /// - **M7**：`#[cfg(not(target_os = "linux"))] pub(crate) const ASSUME_ALIVE: bool = true;`
    ///   —— cfg 字符串逐字命中，但常量行上没有 `{`，于是抓到的是**后面某个不相干函数的体**，
    ///   那行 `true` 从不进入检查面。**全绿。**
    /// - **更要紧的一条**：`pidwatch/mod.rs` 的 `#[cfg(not(target_os = "linux"))] mod fallback;`
    ///   同样无花括号 ⇒ 返回 `None` ⇒ **`pidwatch/fallback.rs` 一行都没被扫过**。
    ///   而 U4a 自己把 pidwatch 的 fallback 从「函数内 cfg 块」改成「整文件按 cfg 选」，
    ///   然后写了一个只理解前一种形状的护栏。今后有人把那个文件改成立刻调 `on_dead()`
    ///   （它自己头注表里标为「最坏」的选项），没有任何门禁会响。
    ///
    /// ⇒ 现在：先看 cfg 之后**先遇到 `{` 还是先遇到 `;`**。先 `;` ⇒ 是 item 声明，取到 `;` 为止。
    pub(super) fn block_after(code: &str, start: usize) -> Option<&str> {
        let tail = &code[start..];
        let brace = tail.find('{');
        let semi = tail.find(';');
        if let Some(sc) = semi {
            if brace.is_none_or(|b| sc < b) {
                return Some(&code[start..start + sc + 1]);
            }
        }
        let b = code[start..].find('{')? + start;
        let bytes = code.as_bytes();
        let (mut depth, mut i) = (0i32, b);
        while i < code.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&code[b..=i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    #[test]
    /// ★ 派生口径的**回归锚点**〔audit-0805 08-06〕。
    ///
    /// [`FALLBACK_CFGS`] 是换成派生之前那份手写清单。**不删它**（铁律 13：
    /// 删之前先证明它恒绿——而它这里恰恰不是恒绿的，它现在的岗位是回归锚）：
    /// 断言那 6 个历史已知形态在新口径下**仍被判为回退**。
    /// 派生逻辑哪天收窄（比如有人给 `platform_cfgs` 加一条过滤），这条当场红。
    #[test]
    fn the_derived_population_still_covers_every_historically_known_form() {
        for cfg in FALLBACK_CFGS {
            let synthetic = format!("{cfg}\nfn x() {{}}\n");
            assert!(
                platform_cfgs(&synthetic).iter().any(|c| c == cfg),
                "历史已知的回退形态 `{cfg}` 在派生口径下不再被认成回退 —— 口径收窄了"
            );
        }
        // 反向：主分支不许被当成回退（否则 Linux 原生实现会被要求「诚实降级」）。
        for cfg in PRIMARY_CFGS {
            let synthetic = format!("{cfg}\nfn x() {{}}\n");
            assert!(
                platform_cfgs(&synthetic).is_empty(),
                "主分支 `{cfg}` 被当成了回退分支"
            );
        }
    }

    /// **判红本体**：一个 cfg 块体今天会不会被判成「凭空造了一个成功值」。
    ///
    /// 返回命中的**说法**（可能同时命中两条）。
    ///
    /// # 〔`K-G6`〕为什么从判据体里抽出来
    ///
    /// `KG61` 要交一张「它今天真能拦住的形状全表」和一条「形状相同却通过了的反例」，
    /// 两者都要**喂给判据本体**。抽成纯函数是为了让那些一刀读数量到**被测者实际用的那个对象** ——
    /// 本文件下面那段自检逐字记过同一条纪律：量一个「同样构造」的副本，
    /// 副本一旦与本体分叉，红灯就开始骗人。**抽取是纯重构，两条判红条件一个字没动。**
    ///
    /// - 裸 `true` —— 用词边界避开 `true_x` / `is_true` 之类，**全体扫**。
    /// - 〔audit-0805 08-06〕`Some(..)` / `Ok(..)` 这两个 —— 本模块头注把它们与 `true`
    ///   并列为「最危险的那几个」，而检查**只查了 `true`**。
    ///   实测：把 `proc.rs` 非 Linux 那支的 `None` 改成 `Some(0)`，本守卫 3 passed 全绿。
    ///   那个 0 不是无害的：判活表按 `captured != current` 判 PID 复用，
    ///   一个编造的 `0` 会让活着的进程被判成「已复用 ⇒ 已死」——正是误归档。
    ///   ⚠ 只看**块体的最后一个表达式**（块的值），不像 `true` 那样全体扫：
    ///   体内的 `if let Some(x)` 是合法解构，全体扫会把它误伤成「凭空造值」。
    /// 块体的**最后一个有效表达式行**（= 块的值）。
    ///
    /// ⚠ **它不是一份「剥注释的 transformer」，别把它读成那个**：
    /// 它不产出一份剥干净的副本，它只回答「这个块的值那一行长什么样」，
    /// 跳过 `//` 行只是为了**不把一句注释当成块的值**
    /// —— 与 `structural_scan::TRANSFORMERS` 里那几条逐字登记为「不是剥法」的同形
    /// （`refusal_variants` 逐字：「跳过 doc 行只是为了不把注释当变体」）。
    /// ⇒ 因此它返回**借用的一行**而不是 `String`/`Vec`，
    /// 也因此不该、也确实不会被那条判据当成第二份剥法收走。
    pub(super) fn block_tail(body: &str) -> &str {
        body.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//") && *l != "{" && *l != "}")
            .next_back()
            .unwrap_or("")
            .trim_end_matches([';', '}'])
            .trim()
    }

    pub(super) fn fabricates(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let bare_true = body
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|t| t == "true");
        if bare_true {
            out.push("块体里出现裸 `true`".to_string());
        }
        let tail = block_tail(body);
        if tail.starts_with("Some(") || tail.starts_with("Ok(") {
            out.push(format!(
                "块体最后一个表达式是 `{tail}` —— 凭空造了一个「成功」值"
            ));
        }
        out
    }

    #[test]
    fn fallback_branches_must_not_fabricate_success() {
        let files = platform_sources();
        // 采集面自检：`platform/` 至少 5 个文件（proc/paths/signal/liveness/pidwatch/mod）。
        assert!(
            files.len() >= 5,
            "platform/ 只扫到 {} 个文件 —— 采集坏了，下面的断言是空转",
            files.len()
        );
        let mut bad: Vec<String> = Vec::new();
        let mut checked = 0usize;
        for (name, code) in &files {
            // 〔audit-0805 08-06〕**人群改成派生 + 默认拒绝**。
            //
            // 原来是 `FALLBACK_CFGS` 那 6 个**精确字符串**。实测：往 `proc.rs` 写
            //   `#[cfg(any(windows, target_os = "macos"))] fn probe(..) -> bool { true }`
            // ——一个在非 Linux 平台上**假装进程还活着**的回退，正是本护栏要挡的东西 ——
            // **四条判据全绿**，因为那个复合 cfg 不在清单里。
            //
            // 现在：扫出 `platform/` 里**每一个**平台条件编译属性，
            // 凡不是[`PRIMARY_CFGS`]登记的主分支，**一律按回退处理**。
            // ⇒ 新写法（复合 cfg / `target_family` / 别的组合）天然落网，
            // 不需要谁想起来把它加进清单。
            for cfg in platform_cfgs(code) {
                let cfg = cfg.as_str();
                let mut from = 0usize;
                while let Some(rel) = code[from..].find(cfg) {
                    let i = from + rel;
                    if let Some(body) = block_after(code, i + cfg.len()) {
                        checked += 1;
                        for what in fabricates(body) {
                            bad.push(format!("{name} 的 `{cfg}` {what}"));
                        }
                    }
                    from = i + cfg.len();
                }
            }
        }
        // 采集面自检：**数量相等，不是松地板**（Phase D 审计）。
        //
        // 第一版是 `checked >= 3`，而实测 `checked = 7` —— 余量 2.3 倍，
        // **4 个块可以静默掉出采集面而地板照绿**。而这个仓一个 commit 前刚在
        // `layering_guard` 里用整段论证否定过这种松地板、改成了数量相等。
        // 同一个仓、连续两个 commit 里的双标，比文件名那条实质得多。
        //
        // 这里独立数一遍「`platform/` 生产段里 FALLBACK_CFGS 出现了几次」，与 `checked` 比。
        // 〔audit-0805 08-06〕**这个独立计数也要用同一份派生人群**。
        //
        // 它原来数的是 `FALLBACK_CFGS` 那 6 个字符串的出现次数，而上面的循环
        // 已改成派生 ⇒ 两侧口径脱节：加一条复合 cfg 时，上面数 11、这里数 10，
        // 于是**红是红了，红的却是「计数对不上」而不是「伪造成功」** —— 诊断指错方向。
        // ⚠ 这正是本会话反复记的那条：**自检必须量被测者实际用的那个对象**，
        // 不能量一个「同样构造」的副本；副本一旦与本体分叉，红灯就开始骗人。
        let mut occurrences = 0usize;
        for (_, code) in &files {
            for cfg in platform_cfgs(code) {
                occurrences += code.matches(cfg.as_str()).count();
            }
        }
        assert_eq!(
            checked, occurrences,
            "扫到 {checked} 个 fallback 分支，而 platform/ 生产段里 cfg 出现了 {occurrences} 次 \
             —— 有分支没被取出来检查（多半是 `block_after` 认不了那种 item 形状）"
        );
        assert!(
            occurrences >= 5,
            "platform/ 只找到 {occurrences} 个平台分支 —— 采集坏了，这条断言在空转"
        );
        assert!(
            bad.is_empty(),
            "fallback 分支凭空返回了「成功」值：\n  {}\n\n\
             一个答不上来的问题不该有一个看起来无害的答案。可选的诚实表达：\n\
             · `false`（保守方向，如发信号失败当没发）\n\
             · `None`（「不知道」）\n\
             · `unimplemented!()` / `todo!()`（大声说没做，且给后来人一个编译器帮忙找的落点）\n\
             `pid_alive` 曾经就是这里的 `true`，它让会话永不归档且毫无信号，在仓里活了很久。",
            bad.join("\n  ")
        );
    }

    /// 〔audit-0805 08-06〕**前提触发器：`platform/` 的 cfg 块仍是混合人群。**
    ///
    /// 本守卫用黑名单而不是本仓偏好的白名单，理由只有一个：被枚举的人群不同质
    /// （诚实空壳 + 真正的平台实现混在一起，见头注）。这条盯着那个理由。
    ///
    /// ⚠ 它**自带一份取尾逻辑**，不复用上面那条的 —— 因为两者要的东西不同：
    /// 上面那条只看「块的值是不是 `Some(..)`/`Ok(..)`」，最后一行足够；
    /// 本条要**分类**，而诚实空壳里有**多行宏**（`unimplemented!(` 换行 `"…"` 换行 `)`），
    /// 只看最后一行会拿到收尾的 `)` 并把它误判成「真实现」——
    /// 第一版就是这么写的，实测**永远不会红**（把两处真实现换成空壳它照样绿）。
    #[test]
    fn the_platform_blocks_are_still_a_mixed_population() {
        let mut stub = 0usize;
        let mut real = 0usize;
        for (_, code) in platform_sources() {
            // 〔audit-0805 08-06〕同上：切到派生人群，免得同一模块留两套口径。
            for cfg in platform_cfgs(code.as_str()) {
                let cfg = cfg.as_str();
                let mut from = 0usize;
                while let Some(rel) = code[from..].find(cfg) {
                    let i = from + rel;
                    if let Some(body) = block_after(code.as_str(), i + cfg.len()) {
                        // 分三类，前两类都不算「真实现」：
                        // ① **item 声明**（`mod fallback;` / `pub(crate) use …`）—— 没有块体，
                        //    `block_after` 返回的是声明本身。不算。
                        // ② **诚实空壳** —— `false` / `None`，或块体里出现 `unimplemented!(` / `todo!(`。
                        //    ⚠ 宏要**看整个块体**而不是最后一行：`proc.rs` 那处是多行宏，
                        //    回溯只会拿到宏里的**消息字符串**（第一版就这么把它误判成「真实现」，
                        //    于是本条永远不会红 —— 变异实测确认过）。
                        // ③ 其余 = 真正的平台实现。
                        let is_item = !body.trim_start().starts_with('{');
                        let has_macro = body.contains("unimplemented!(") || body.contains("todo!(");
                        let last = body
                            .lines()
                            .map(str::trim)
                            .filter(|l| {
                                !l.is_empty() && !l.starts_with("//") && *l != "{" && *l != "}"
                            })
                            .next_back()
                            .unwrap_or("")
                            .trim_end_matches([';', '}'])
                            .trim();
                        if is_item {
                            // 不计入任何一类
                        } else if has_macro || last == "false" || last == "None" {
                            stub += 1;
                        } else {
                            real += 1;
                        }
                    }
                    from = i + cfg.len();
                }
            }
        }
        // ★ 自检：两类都数不到就是取块坏了，下面的判断没有意义。
        assert!(
            stub + real >= 4,
            "只从 `platform/` 取到 {} 个有值的 cfg 块 —— 取块坏了（08-06 实测：空壳 6 + 实现 2 = 8）",
            stub + real
        );
        assert!(
            real > 0,
            "`platform/` 的 cfg 块**只剩诚实空壳一类了**（空壳 {stub} · 实现 {real}）。\n\
             ⇒ 人群变同质了，本守卫可以从**黑名单**（列坏值）改成本仓偏好的**白名单**\n\
             （枚举每个块、要求它的值是 `false`/`None`/`unimplemented!()` 之一）——\n\
             那样「等价改写绕得过」那个洞会一起消失。请回来改，并删掉本条。"
        );
    }
}

/// 〔`K-G6` `KG61`〕**本护栏今天真能拦住的形状全表 + 两条今天就通过了的反例。**
///
/// # 两条反例**分开列**，因为它们证的不是同一件事（`§0c 裁三`）
///
/// - **甲** 证「**不构成阻塞**」：一个**真的** Windows 实现今天就在人群里、就通过 ——
///   所以「在 `platform/` 里写真 Windows 实现会当场红」这句话是假的。
/// - **乙** 证「**不构成保护**」：本护栏来历里那个地雷的**逐字同形**今天在生产段活着，
///   而人群够不着它。
///
/// ★★ 合成一条会让下一个人以为**一条反例就够** —— 而那正是本轮要治的病。
#[cfg(test)]
mod g6_reach {
    use super::tests::{block_after, fabricates, platform_cfgs, platform_sources};
    use crate::guard_support::production_code;

    /// 取某个 cfg 属性后面那个块（合成样本与真语料共用同一条取块逻辑）。
    fn block_of(code: &str, cfg: &str) -> String {
        let i = code
            .find(cfg)
            .unwrap_or_else(|| panic!("语料里找不到 `{cfg}` —— 反例的住址变了，回来重判"));
        block_after(code, i + cfg.len())
            .unwrap_or_else(|| panic!("`{cfg}` 后面取不到块 —— `block_after` 认不了这种 item 形状"))
            .to_string()
    }

    /// 全表①：它今天**真能拦住**的形状。`(形状, 样本块体)`
    const CATCHABLE: &[(&str, &str)] = &[
        ("块体里出现裸 `true`（全体扫，带词边界）", "{ let _ = pid; true }"),
        ("块的值是 `Some(..)`", "{\n    Some(0)\n}"),
        ("块的值是 `Ok(..)`", "{\n    Ok(())\n}"),
    ];

    /// 全表②：它今天**拦不住**的形状。`(形状, 样本块体, 为什么它不红)`
    ///
    /// ⚠ 后两条是**应当**通过的（诚实表达），列在这里是为了让全表既说清「漏了什么」，
    /// 也说清「哪些是它有意放行的」—— 只列漏洞的表读起来像一份缺陷清单，那会误导下一个人。
    const BLIND: &[(&str, &str, &str)] = &[
        (
            "等价改写的恒真值",
            "{\n    !false\n}",
            "判红条件是**字面** `true` / `Some(` / `Ok(`；任何恒真表达式都绕得过。\
             本模块头注逐字登记过：这条**刻意不追**，完备性在这里做不到。",
        ),
        (
            "恒真比较",
            "{\n    1 == 1\n}",
            "同上，等价改写那一族。",
        ),
        (
            "真实现返回计算值",
            "{\n    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())\n}",
            "**应当**通过：人群里混着真正的平台实现，判红只看那两个字面乐观值。\
             这也正是反例甲的形状。",
        ),
        (
            "诚实空壳 `false`",
            "{\n    false\n}",
            "**应当**通过：保守方向的诚实表达（发不出信号当没发）。",
        ),
        (
            "诚实空壳 `None`",
            "{\n    None\n}",
            "**应当**通过：`None` 就是「我不知道」的正确写法。",
        ),
    ];

    /// ★ 全表①：逐形喂给**判据本体**，逐形要求它红。
    #[test]
    fn every_catchable_shape_reds_when_fed_to_the_real_predicate() {
        assert_eq!(
            CATCHABLE.len(),
            3,
            "能拦住的形状从 3 条变成 {} 条了 —— 全表的分母变了。\n\
             变少 = 判红条件被拿掉了一条，那是放宽；变多 = 补了人群，同轮把这张表也补上。",
            CATCHABLE.len()
        );
        for (shape, sample) in CATCHABLE {
            assert!(
                !fabricates(sample).is_empty(),
                "判据对「{shape}」这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
    }

    /// ★ 全表②：逐形喂给判据本体，逐形要求它**不**红，并逐形写清为什么。
    #[test]
    fn every_blind_shape_passes_and_says_why() {
        assert_eq!(BLIND.len(), 5, "拦不住的形状表条数变了：{}", BLIND.len());
        for (shape, sample, why) in BLIND {
            assert!(
                fabricates(sample).is_empty(),
                "「{shape}」今天红了 —— 本护栏的射程变了，这张表说的话已经不成立"
            );
            assert!(
                why.trim().chars().count() >= 10,
                "「{shape}」没写清为什么它不红 —— 一张只列形状不给理由的表，下一轮没人敢动"
            );
        }
    }

    /// ★★ **反例甲**（证「不构成阻塞」）：一个**真的** Windows 实现，今天在人群里、今天通过。
    ///
    /// 它直接证伪「在 `platform/` 里写一个真的 Windows 实现 ⇒ 当场红」这句话。
    /// 准确说法是：**只有「内联 cfg 块 + 块尾是字面 `Ok(` / `Some(`」这一种写法会红**；
    /// 返回计算值、或用惯用的 `#[cfg(平台)] mod x;` + 独立文件，一个字都不用改护栏。
    #[test]
    fn counterexample_a_a_real_windows_impl_is_inside_the_population_and_still_passes() {
        let files = platform_sources();
        let (_, paths) = files
            .iter()
            .find(|(n, _)| n.as_str() == "paths.rs")
            .expect("`platform/paths.rs` 不在人群里了 —— 反例甲的住址变了，回来重判");
        let cfg = "#[cfg(windows)]";
        assert!(
            platform_cfgs(paths).iter().any(|c| c == cfg),
            "`paths.rs` 的 `{cfg}` 不再被派生人群认成回退分支 —— 口径变了"
        );
        let body = block_of(paths, cfg);
        // 它是**真实现**，不是诚实空壳：既不是 `false`/`None`，也没有「大声说没做」的宏。
        assert!(
            !body.contains("unimplemented!(") && !body.contains("todo!("),
            "反例甲变成诚实空壳了 —— 那它就不再证明「真实现也通过」，回来重判"
        );
        assert!(
            body.contains("PathBuf::from("),
            "反例甲里那个返回计算值的真实现不见了 —— 修掉了就同轮摘登记"
        );
        assert!(
            fabricates(&body).is_empty(),
            "反例甲今天**红了** —— 本护栏的射程变了（或那处实现改了写法），\
             「不构成阻塞」这个定性要回来重判"
        );
    }

    /// ★★ **反例乙**（证「不构成保护」）：本护栏来历地雷的**逐字同形**，今天在生产段活着，
    /// 而人群够不着它。
    ///
    /// # 两个断言合起来才是这条反例
    ///
    /// ① 把那段真代码喂给**判据本体**，它**会红** ⇒ **形状对得上**；
    /// ② 而它住 `plugin/`，不住 `platform/` ⇒ 人群够不着 ⇒ **今天通过**。
    /// 少了任何一半都证不出「不构成保护」：只有①是「假想的坏写法」，只有②是「一句范围声明」。
    ///
    /// ⚠ 它的后果是真的：那个函数是「这个路径今天是不是一个能跑的文件」，
    /// 恒真 ⇒ 在非 unix 平台上，一个**存在但不可执行**的候选会被当成可用插件。
    /// ⚠ **修它归谁，本件没定** —— 修掉之后本条会红（断言①不再成立），
    /// 那时**同轮摘登记**并回来重判本护栏的射程。那正是形态二要买的东西。
    #[test]
    fn counterexample_b_the_origin_mine_is_alive_outside_the_population() {
        let discover = production_code(include_str!("../plugin/discover.rs"));
        let cfg = "#[cfg(not(unix))]";
        let body = block_of(&discover, cfg);
        assert!(
            !fabricates(&body).is_empty(),
            "反例乙的形状变了：`plugin/discover.rs` 的 `{cfg}` 块体不再是「一个裸 `true`」。\n\
             ⇒ 要么它被修好了（好事 —— **同轮摘登记**），\n\
             要么判红条件被改松了（坏事 —— 那是放宽，先摆出全表再谈）。"
        );
        // ② 它今天通过，原因只有一个：人群按**目录**画，而它不在那个目录里。
        assert!(
            !platform_sources()
                .iter()
                .any(|(n, _)| n.contains("discover")),
            "`discover.rs` 进人群了 —— 那这条反例就不成立了，回来重判"
        );
        // ★ 同一段代码，换个住址就会红 —— 这一刀把「人群够不着」与「判据看不出」分开。
        //   缺了它，上面两条读起来像「它不该红」，而事实是「它该红而没人看得见」。
        assert!(
            !fabricates("{\n    true\n}").is_empty(),
            "连合成的同形样本都不红了 —— 那就不是人群问题，是判据本身坏了"
        );
    }
}

/// 〔`K-G6` `KG62`〕**钉住 `readonly_guard.rs` 的「性质行 / 人群行」各自只有一句 +
/// 收窄前那句绝对话不许回来。**
///
/// # 为什么钉在这里
///
/// `ratchet_guard.rs` 头注逐字给过理由：判据**不许与被扫的文本同住一个文件**
/// （它会在自己的注释里找到自己 ⇒ 恒绿）。
/// `readonly_guard.rs` 那半钉另外两道，本模块钉它 —— **三道两两互钉**，每一针都跨文件。
///
/// ⚠ 正确落点其实是 `ratchet_guard.rs`（本仓已有的那张针表），
/// 但它不在 `K-G6` `C` 拍的写区里 ⇒ 暂住这里，**已上报 PM**。
#[cfg(test)]
mod g6_scope_pins {
    /// 两行的标记 + 承重词。**运行时拼**，免得本文件把自己数进去。
    fn needles() -> (String, String, Vec<String>) {
        (
            format!("//! - **它守的{}**", "性质是"),
            format!("//! - **它扫的{}**", "人群是"),
            vec![
                format!("必须{}", "只读"),
                format!("绝不{}", "写"),
            ],
        )
    }

    /// ★ `readonly_guard.rs` 的性质行与人群行**各自恰好一行**。
    #[test]
    fn the_readonly_guard_states_its_property_and_population_exactly_once() {
        let src = include_str!("../readonly_guard.rs");
        let (prop, popu, _) = needles();
        for (what, mark) in [("性质行", &prop), ("人群行", &popu)] {
            let hits: Vec<&str> = src.lines().filter(|l| l.starts_with(mark.as_str())).collect();
            assert_eq!(
                hits.len(),
                1,
                "`readonly_guard.rs` 里以 `{mark}` 打头的{what}有 {} 行（应恰好 1 行）。\n\
                 **多了**就是同一道护栏又有了两句性质声明 —— 那正是本轮逮到的病：\n\
                 一句宽一句窄，判据只兑现窄的那句，而宽的那句被下游件逐字引用。",
                hits.len()
            );
            let body = hits[0].trim_start_matches(mark.as_str());
            assert!(
                body.trim().chars().count() >= 20,
                "`readonly_guard.rs` 的{what}只有 {} 字 —— 写不下去的性质声明等于没写",
                body.trim().chars().count()
            );
        }
    }

    /// ★★ 反向棘轮：`D1` 收窄**之前**那句绝对话的承重词，今天起在 `readonly_guard.rs` 里零命中。
    ///
    /// # 它治的是一次已经发生的「只修一半」
    ///
    /// `doc/INVARIANTS.md` §41.6 早就把那句标成**原措辞**并给了现措辞，
    /// 而 `readonly_guard.rs` 里同时留着收窄前的绝对句两处（头注一处 + 报错文案一处），
    /// 判据兑现的却是收窄后那句。⇒ 判据的性质行是假话，且下游件在逐字引用它。
    ///
    /// ⚠ **它挡不住换个措辞说同一句话**（`ratchet_guard` 登记过这条同族边界）。
    /// 它挡的是**顺手抄回来**，那是实际会发生的动作。
    #[test]
    fn the_pre_narrowing_absolute_does_not_come_back() {
        let src = include_str!("../readonly_guard.rs");
        let (_, _, forbidden) = needles();
        assert!(
            forbidden.len() >= 2,
            "承重词表只剩 {} 条 —— 本条此刻在空转",
            forbidden.len()
        );
        for word in &forbidden {
            assert!(
                !src.contains(word.as_str()),
                "`readonly_guard.rs` 里又出现了 `{word}` —— 那是 `D1` **收窄前**的说法，\n\
                 判据从来没兑现过它（它只认本 crate 源码文本里那三个命名空间的调用，\n\
                 既不认起进程、也不认依赖 crate）。\n\
                 ⇒ 要么把人群补齐到那句话上，要么就别写那句话。\n\
                 （`§0a` 四情形表：这一格叫 `补齐人群`，不叫「先把话说满」。）"
            );
        }
        // ★ 反空真：这几个承重词必须**真的**匹配得上收窄前那句原话，
        //   否则上面那几条断言靠「钉了几个谁也不会写的字」恒绿。
        //   原话在这里**运行时拼**，免得本文件自己成为那句假话的第三处住址。
        let historical = format!(
            "daemon 对被观测文件系统（`~/.claude` 等）**必须{}**——只 watch/scan/read，绝不{}。",
            "只读", "写"
        );
        for word in &forbidden {
            assert!(
                historical.contains(word.as_str()),
                "承重词 `{word}` 连收窄前那句原话都匹配不上 —— 它钉错了词，\
                 上面那几条零命中断言此刻说明不了任何事"
            );
        }
    }
}
