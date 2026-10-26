use std::path::PathBuf;

fn src_root() -> PathBuf {
    // 住址唯一源：`guard_core::addr`。
    guard_core::addr::shell_src_root()
}

/// 某一行所在的函数名：往回找最近的一处 `fn <名>`（顶格或缩进都算）。
///
/// ⚠ 与 `cc_bus` 里那个「按名字取函数体」不是同一件事，故不共用：
/// 那个是「给名字要体」，这个是「给行要名字」。
fn enclosing_fn(lines: &[&str], at: usize) -> String {
    for l in lines[..=at].iter().rev() {
        if let Some(rest) = l.split(" fn ").nth(1).or_else(|| l.strip_prefix("fn ")) {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !n.is_empty() {
                return n;
            }
        }
    }
    "<找不到外层函数>".to_string()
}

/// 函数体：从 `at` 那行所在函数的签名行起，到下一个顶格行为止。
// ═════════════════════════════════════════════════════════════════════════════
// **为什么还是一条拨号 shell**：逐处写理由，两向相等
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求：「所有 SSH 由本机常驻后端持有」· 「其余一次性 exec 点……**逐处仍要核**」·
// 要求「9 处 shell 能换后端具名命令的逐处换，换不了的写理由并登记」。
//
// 人群 = monitor 生产段里 `connect_and_exec_capture(` 的调用点（「文件::外层函数」，原语自己的定义不算），**两向相等**。
// 一处都不剩：起 / 找 / 换那台的常驻后端 · 停那台都整趟在本机后端（各台的常驻流 `control/feeds.rs` · `machine-stop`）。

const STILL_SHELL: &[(&str, &str, &str)] = &[];

fn still_shell_population() -> std::collections::BTreeSet<(String, String)> {
    let files = guard_core::scan_tree!(&src_root(), &["rs"]);
    let mut out = std::collections::BTreeSet::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with("pub async fn connect_and_exec_capture(") {
                continue;
            }
            if l.contains("connect_and_exec_capture(") {
                out.insert((stem.clone(), enclosing_fn(&lines, i)));
            }
        }
    }
    out
}

#[test]
fn every_remaining_dial_shell_says_why_it_is_not_a_backend_command() {
    let found = still_shell_population();
    let want: std::collections::BTreeSet<(String, String)> = STILL_SHELL
        .iter()
        .map(|(f, n, _)| (f.to_string(), n.to_string()))
        .collect();
    assert_eq!(
        found, want,
        "monitor 里「经拨号链路跑 shell」的点与理由表对不上。\n\
         多出来的 ⇒ 先回答它为什么不能是那台后端的一条具名命令；\n\
         少了的 ⇒ 换掉了，删那一行。"
    );
}
