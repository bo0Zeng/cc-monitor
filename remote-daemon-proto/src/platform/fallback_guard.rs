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
//! `#[cfg(not(unix))]` 这类 fallback 块，断言块体里**不出现**裸 `true`。
//!
//! **允许什么**：`false`（保守方向，如 `send_sigusr1` —— 发不出去当没发，调用方本就容忍）·
//! `None`（「不知道」的正确表达）· `unimplemented!()` / `todo!()`（大声说没做）。
//!
//! **它挡不住什么**（如实登记，别再宣称完备 —— U3 在 `layering_guard` 上栽过这一次）：
//! - 只看 `true` 这一个字面量。写 `1 == 1`、`!false`、或者返回一个恒真的表达式都绕得过。
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
}
