//! T01 第 6 步：**结构性扫描**抽成可复用形式。
//!
//! ## 这是本仓质量最高的一处防线，抽取时不得降级
//!
//! 账本原话：「结构性扫描 > 固定 needle，且**已实证固定 needle 是空转的**」。
//! 出处是 `sftp.rs` 里那段注释记的一次真实教训——F04 的 D 审计实测：把 CLI 里的
//! `=名:` 精确目标**全改回裸目标**，`cargo test` **依旧全绿**（正向 needle 恰好都还命中，
//! 反向 needle 引用的是 CLI 里根本不存在的代码）。而裸目标正是 F01 修掉的那个
//! 「杀错/打错兄弟会话」的生产事故。
//!
//! ## 四个要件（缺一不可，这里逐条内建）
//!
//! 1. **枚举**：扫出文本里**每一个**结构特征的出现（不是找几个固定字符串）；
//! 2. **逐个断言**：对每一处出现施加同一个性质；
//! 3. **计数自检**：扫到 0 处 → 扫描器自己失效了，必须红。**这一条内建在
//!    [`ScanReport::require`] 里，调用方想忘都忘不掉**——本会话我写坏过四条结构性守卫，
//!    其中"恒绿/空转"占了两条；
//! 4. **钉死逃生口**：允许的间接变量，其定义必须**逐字**钉死（见 [`pin_definition`]）。
//!    否则它可以被改成裸值，从而合法地绕过前三条。这是最容易漏的一条。
//!
//! ## 为什么是白名单
//!
//! 本会话的教训：黑名单（"不准出现这些坏写法"）要求我预先想全所有坏写法，
//! 而审计只用五种我没想到的写法就绕过了 B04 那条守卫。
//! 结构性扫描天然是白名单——它枚举**每一处**出现并要求它们**都**满足好性质，
//! 新增的出现自动被纳入。这正是固定 needle 永远做不到的。

/// 一次扫描的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// 实际检查过的出现次数。
    pub checked: usize,
    /// 违反性质的出现（每条带可读描述）。
    pub violations: Vec<String>,
}

impl ScanReport {
    /// 断言这次扫描通过。**`min_checked` 不是可选的**——它就是要件 3。
    ///
    /// 扫到的数量少于 `min_checked` 时同样失败，措辞明确指向"扫描器可能失效了"
    /// 而不是"被测代码有问题"：这两种失败的排查方向完全不同，混在一起会浪费很多时间。
    pub fn require(&self, min_checked: usize, what: &str) -> Result<(), String> {
        if !self.violations.is_empty() {
            return Err(format!(
                "{what}：{} 处违反（共检查 {} 处）\n  - {}",
                self.violations.len(),
                self.checked,
                self.violations.join("\n  - ")
            ));
        }
        if self.checked < min_checked {
            return Err(format!(
                "{what}：只扫到 {} 处（期望至少 {min_checked} 处）——**扫描器可能失效了**，\
                 而不是被测代码变干净了。先查扫描器的枚举逻辑，别急着调低阈值。",
                self.checked
            ));
        }
        Ok(())
    }
}

/// 枚举 `text` 里每一处 `marker`，对其后 `window` 个字符施加 `check`。
///
/// - `comment_prefix`：以它开头（trim 后）的行整行跳过。注释里的用法示例不该算数
///   ——但**这也意味着注释里藏一个坏例子不会被抓**，这是有意的取舍：
///   假红比漏抓更容易让人把守卫关掉。
/// - `allow`：返回 `true` 表示这一处是**已钉死的逃生口**，计入 `checked` 但不施加 `check`。
///   用它的地方**必须**同时调 [`pin_definition`] 把逃生口的定义钉死。
pub fn scan_after_marker(
    text: &str,
    marker: &str,
    comment_prefix: Option<&str>,
    window: usize,
    allow: &dyn Fn(&str) -> bool,
    check: &dyn Fn(&str) -> Result<(), String>,
) -> ScanReport {
    let mut checked = 0usize;
    let mut violations = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        if let Some(cp) = comment_prefix {
            if line.trim_start().starts_with(cp) {
                continue;
            }
        }
        for (i, _) in line.match_indices(marker) {
            let rest = &line[i + marker.len()..];
            let win: String = rest.chars().take(window).collect();
            checked += 1;
            if allow(rest) {
                continue;
            }
            if let Err(e) = check(&win) {
                violations.push(format!("第 {} 行：{e}（窗口 {win:?}）", lineno + 1));
            }
        }
    }
    ScanReport {
        checked,
        violations,
    }
}

/// 钉死一个逃生口的定义（要件 4）。
///
/// 凡是 [`scan_after_marker`] 的 `allow` 放行的间接变量，它的定义必须逐字出现在文本里。
/// 不钉的话，`$t` 这类变量可以被改成裸值——**扫描照样全绿，而防线已经没了**。
pub fn pin_definition(text: &str, definition: &str, what: &str) -> Result<(), String> {
    if text.contains(definition) {
        Ok(())
    } else {
        Err(format!(
            "{what} 的定义必须逐字是 `{definition}`——它是被放行的间接目标的唯一来源，\
             改了它就能绕过整个结构性扫描"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tmux `-t` 目标的性质：窗口内先出现 `=` 再出现 `:`。
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
        let r = scan_after_marker(bad, "-t ", Some("#"), 48, &|_| false, &exact_target);
        assert_eq!(r.checked, 2);
        assert_eq!(r.violations.len(), 2, "两处裸目标都必须被抓");
        assert!(r.require(2, "tmux 目标").is_err());
    }

    #[test]
    fn accepts_the_three_legit_exact_forms() {
        let good = "a -t $(sq \"=$x:\") b\nc -t \"=$x:\" d\ne -t '=x:' f\n";
        let r = scan_after_marker(good, "-t ", Some("#"), 48, &|_| false, &exact_target);
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
        let r = scan_after_marker(t, "-t ", Some("#"), 48, &|_| false, &exact_target);
        assert_eq!(r.checked, 1, "注释行里的用法示例不算");
        assert!(r.violations.is_empty());
    }

    /// **要件 4**：放行的逃生口计入 checked（否则计数自检会被它稀释），但不施加性质。
    #[test]
    fn allowed_indirection_counts_but_is_not_checked() {
        let t = "tmux send-keys -t $t x\ntmux kill -t $name y\n";
        let allow = |rest: &str| rest.starts_with("$t ") || rest.starts_with("$t\"");
        let r = scan_after_marker(t, "-t ", Some("#"), 48, &allow, &exact_target);
        assert_eq!(r.checked, 2, "逃生口也要计数");
        assert_eq!(r.violations.len(), 1, "只有裸目标那处违规");
    }

    /// **要件 4 的另一半**：钉死逃生口的定义。不钉的话 `$t` 能被改成裸值、
    /// 扫描照样全绿而防线已经没了。
    #[test]
    fn pinning_the_escape_hatch_definition() {
        let def = r#"t="$(sq "=$tmux_name:")""#;
        let ok = format!("x\n{def}\ny\n");
        assert!(pin_definition(&ok, def, "$t").is_ok());
        // 被改成裸值 → 必须红
        let tampered = "x\nt=\"$tmux_name\"\ny\n";
        let e = pin_definition(tampered, def, "$t").unwrap_err();
        assert!(e.contains("绕过整个结构性扫描"));
    }

    #[test]
    fn violations_report_line_numbers_and_window() {
        let t = "行一\ntmux -t $bare x\n";
        let r = scan_after_marker(t, "-t ", Some("#"), 12, &|_| false, &exact_target);
        let e = r.require(1, "tmux 目标").unwrap_err();
        assert!(e.contains("第 2 行"), "要报行号，实得 {e}");
        assert!(e.contains("$bare"), "要报窗口内容");
        assert!(e.contains("共检查 1 处"));
    }
}
