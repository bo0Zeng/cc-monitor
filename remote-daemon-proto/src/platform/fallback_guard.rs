//! U4a（2026-08-01）：**非目标平台的 fallback 分支，不许凭空返回一个「成功」值。**
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

    fn platform_sources() -> Vec<(String, String)> {
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
    fn block_after(code: &str, start: usize) -> Option<&str> {
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
            for cfg in FALLBACK_CFGS {
                let mut from = 0usize;
                while let Some(rel) = code[from..].find(cfg) {
                    let i = from + rel;
                    if let Some(body) = block_after(code, i + cfg.len()) {
                        checked += 1;
                        // 裸 `true` —— 用词边界避开 `true_x` / `is_true` 之类。
                        let fabricates = body
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .any(|t| t == "true");
                        if fabricates {
                            bad.push(format!("{name} 的 `{cfg}` 块体里出现裸 `true`"));
                        }
                        // 〔audit-0805 08-06〕`Some(..)` / `Ok(..)` 这两个 —— 本模块头注把它们
                        // 与 `true` 并列为「最危险的那几个」，而检查**只查了 `true`**。
                        // 实测：把 `proc.rs` 非 Linux 那支的 `None` 改成 `Some(0)`，本守卫 3 passed 全绿。
                        // 那个 0 不是无害的：判活表按 `captured != current` 判 PID 复用，
                        // 一个编造的 `0` 会让活着的进程被判成「已复用 ⇒ 已死」——正是误归档。
                        //
                        // ⚠ 只看**块体的最后一个表达式**（块的值），不像 `true` 那样全体扫：
                        // 体内的 `if let Some(x)` 是合法解构，全体扫会把它误伤成「凭空造值」。
                        let tail = body
                            .lines()
                            .map(str::trim)
                            .filter(|l| {
                                !l.is_empty() && !l.starts_with("//") && *l != "{" && *l != "}"
                            })
                            .next_back()
                            .unwrap_or("")
                            .trim_end_matches([';', '}'])
                            .trim();
                        if tail.starts_with("Some(") || tail.starts_with("Ok(") {
                            bad.push(format!(
                                "{name} 的 `{cfg}` 块体最后一个表达式是 `{tail}` —— 凭空造了一个「成功」值"
                            ));
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
        let mut occurrences = 0usize;
        for (_, code) in &files {
            for cfg in FALLBACK_CFGS {
                occurrences += code.matches(cfg).count();
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
            for cfg in FALLBACK_CFGS {
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
