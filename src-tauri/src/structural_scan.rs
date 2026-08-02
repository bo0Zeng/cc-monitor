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
        // **`min_checked = 0` 等于把要件 3 静默关掉**（T01 审计 I3）：`checked < 0` 恒假。
        // 文档写「`min_checked` 不是可选的」，但类型上它是——所以这里把它变成硬失败。
        if min_checked == 0 {
            return Err(format!(
                "{what}：min_checked 不得为 0——那等于关掉计数自检（要件 3），\
                 而扫描器失效时正是靠它报警"
            ));
        }
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
///
///   **前置条件，如实写明**（T01 审计 S8）：注释判定是**逐行**的朴素实现，
///   只看行首。因此它只适用于「`comment_prefix` 出现在行首就确实是整行注释」的文本，
///   并且要求文本里**没有 heredoc、没有跨行字符串**——否则 heredoc 正文里一行
///   `# tmux -t $bare` 会被当注释跳过，而它在 shell 里是要被真执行的。
///   当前唯一的调用点是 `shared/ccm`：已核实它**不含 heredoc**（全用 `printf`/单行赋值），
///   所以这一条成立。以后拿它扫别的文件，**先确认这条前置条件**，不成立就别用
///   `comment_prefix`（传 `None`，让注释里的示例也进枚举，宁可假红）。
/// - `allow`：返回 `true` 表示这一处是**已钉死的逃生口**，计入 `checked` 但不施加 `check`。
///   用它的地方**必须**同时调 [`pin_definition`] 把逃生口的定义钉死。
/// 取紧跟 marker 的**那一个 token**（到空白 / `;` / `|` / `&` / `)` 为止）。
///
/// 谓词只该看这个 token，**不该看一整个窗口**（T01 审计 S2 实测：窗口里出现
/// `"export A=b:c"` 这种诱饵，就能让裸目标零违规通过）。
pub fn first_token(rest: &str) -> &str {
    let t = rest.trim_start();
    let b = t.as_bytes();
    if b.is_empty() {
        return t;
    }
    // **必须是 shell 意义上的"一个参数"，不能在第一个空格处硬切**：
    // `$(sq "=$x:")` 与 `"=$x:"` 都是合法的单参数、内部含空格。第一版按空格切，
    // 把 `$(sq "=$x:")` 截成 `$(sq` 于是把合法写法判成违规（当场被测试抓到）。
    let mut i = 0usize;
    // `$(` … 匹配到配对的 `)`（支持一层嵌套足够覆盖本仓用法）
    if t.starts_with("$(") {
        let mut depth = 0i32;
        for (j, c) in t.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return &t[..j + 1];
                    }
                }
                _ => {}
            }
        }
        return t; // 不配对 → 整段交给谓词判（它会因缺 `=`/`:` 而报违规）
    }
    // 引号包裹 → 到配对的同种引号
    if b[0] == b'"' || b[0] == b'\'' {
        let q = b[0];
        if let Some(j) = t[1..].find(q as char) {
            return &t[..j + 2];
        }
        return t;
    }
    // 其余：到第一个 shell 分隔符
    while i < b.len() {
        let c = b[i] as char;
        // 引号也是终止符：裸词遇到 `"` / `'` 就结束（那是新引用段的开始）。
        // 真实形态 `seq="…tmux attach -t $t"` 里尾部那个引号是**赋值的闭合引号**，
        // 不属于目标——不这样切的话 token 会变成 `$t"`，把合法写法判成违规（实测抓到过）。
        if c.is_whitespace() || matches!(c, ';' | '|' | '&' | ')' | '"' | '\'') {
            break;
        }
        i += 1;
    }
    &t[..i]
}

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
            // **紧贴形态也要枚举**（T01 审计 S1，已独立复现）：marker 若写成 `"-t "`（带空格），
            // `-t$name` 这种 getopt 合法写法**完全不进枚举**——实测把 `shared/ccm` 里
            // `-t "=$x:"` 改成 `-t$x`，checked 11→10、violations 空、`require(4)` 照样通过，
            // 而那正是 F01 修掉的「打错兄弟会话」形态。所以 marker 只给 `"-t"`，
            // 空白与紧贴两种都由这里统一处理。
            // 排除 `-tmux`/`-timeout` 这类**更长的选项名**误命中：紧跟的字符若是字母数字或
            // `-`，说明这是别的选项，不是 `-t` 带值。
            if let Some(c) = rest.chars().next() {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    continue;
                }
            } else {
                continue; // 行尾就是 marker，没有目标
            }
            let tok = first_token(rest);
            let win: String = rest.chars().take(window).collect();
            checked += 1;
            if allow(rest) {
                continue;
            }
            // 谓词只看紧跟的那一个 token（S2）；窗口只用于报错展示。
            if let Err(e) = check(tok) {
                violations.push(format!(
                    "第 {} 行：{e}（token {tok:?}，窗口 {win:?}）",
                    lineno + 1
                ));
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
pub fn pin_definition(
    text: &str,
    definition: &str,
    assign_prefix: &str,
    what: &str,
) -> Result<(), String> {
    // **只 `contains` 是不够的**（T01 审计 S3，已独立复现）：在钉死的定义之后再追加一行
    // `t="$tmux_name"`，`contains` 仍然通过、扫描仍然全绿，而 `$t` 运行期已经是裸值了。
    // 所以除了「逐字存在」，还要断言**该变量在非注释行只被赋值一次**。
    if !text.contains(definition) {
        return Err(format!(
            "{what} 的定义必须逐字是 `{definition}`——它是被放行的间接目标的唯一来源，\
             改了它就能绕过整个结构性扫描"
        ));
    }
    let assigns = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.trim_start().starts_with(assign_prefix))
        .count();
    if assigns != 1 {
        return Err(format!(
            "{what} 在非注释行被赋值 {assigns} 次（应为 1 次）——多次赋值时后一次生效，\
             钉死第一处等于没钉：`{assign_prefix}…` 可以被改成裸值而扫描照样全绿"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U8a-2a：monitor 的每个源文件都要能被共享剥法（`guard_core`）剥干净。
    ///
    /// 与 daemon 侧 `every_daemon_file_strips_clean` 同一条，只是换了一棵树。
    /// 它同时是「列 0 右大括号这个收尾判据够不够用」的持续验证 —— 哪天有人在测试模块里
    /// 写了一段列 0 含右大括号的原始字符串，这里会红，那时再上真正的大括号配对。
    ///
    /// `min_files` 是**计数自检**：遍历坏掉时它会红，而不是静默扫 0 个文件通过。
    #[test]
    fn every_monitor_file_strips_clean() {
        // 地板 = **实测值**（2026-08-02：52 个 .rs）。松着放等于把灵敏度交出去
        // （同 `ci.yml` 那条 shellcheck 覆盖面棘轮的教条：棘的时候把实测构成一起写下）。
        guard_core::assert_tree_strips_clean(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            52,
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

    /// **用已知的绕过手法验证**（本轮新纪律）。这不是构造的边角——
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
}
