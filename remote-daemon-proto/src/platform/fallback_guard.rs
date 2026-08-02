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

    /// 会引出一个「本平台不是目标平台」分支的 cfg 写法。
    const FALLBACK_CFGS: &[&str] = &[
        "#[cfg(not(target_os = \"linux\"))]",
        "#[cfg(not(unix))]",
        "#[cfg(not(windows))]",
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
                if p.file_name().and_then(|n| n.to_str()) == Some("fallback_guard.rs") {
                    continue;
                }
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((
                    rel,
                    production_code(&std::fs::read_to_string(&p).expect("read")),
                ));
            }
        }
        out.sort();
        out
    }

    /// 从 `start` 处的 `{` 起按括号配平取出块体。
    fn block_after(code: &str, start: usize) -> Option<&str> {
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
        assert!(
            checked >= 3,
            "只检查到 {checked} 个 fallback 块 —— platform/ 里应当有若干（pid_alive / proc_starttime / \
             proc_cmdline / send_sigusr1 …）。扫不到就说明判据坏了，这条断言在空转"
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
