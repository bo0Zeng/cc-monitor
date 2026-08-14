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

    /// ★★ **两个 `#[test]` 叠在同一个函数上 ⇒ 另一个函数悄悄不是测试了。**
    ///
    /// # 这不是洁癖，是本仓两天内出现两次的真实事故
    ///
    /// | 谁 | 代价 |
    /// |---|---|
    /// | `inbound_client.rs`（08-11，P2s 插判据时插错位置） | `the_hello_witness_can_only_come_from_a_hello_frame` **一个属性都没有 = 死代码**，而 `the_only_way_to_build_an_inbound_client_is_into_client` 的闭合论证逐字压在它身上 |
    /// | `tmux.rs`（`4d8cfc2`，更早） | `tmux_targets_use_exact_match` 同样掉了属性；实测 `cargo test tmux_targets_use_exact_match` 回 **`0 passed`** —— 它根本不存在 |
    ///
    /// # 为什么编译器拦不住
    ///
    /// `duplicate_macro_attributes` 只是 **warn**，而本仓 clippy 不带 `-D warnings`
    /// （`local_backend.rs` 自己记着这条）。⇒ 编得过、跑得过、**少跑一条判据没人知道**。
    ///
    /// # 本条扫什么
    ///
    /// 本仓风格是 `#[test]` 写在**头注之前**（`#[test]` → `/// …` → `fn`），
    /// 这让「按 fn 名找锚点再往前插」这种改法极易把新块插进「属性与它的 fn」之间。
    /// ⇒ 判据：一个 `#[test]` 之后，跳过头注/其它属性/空行，**下一行必须是 `fn`**。
    /// 若又遇到一个 `#[test]`，就是这个形态。
    #[test]
    fn no_two_test_attributes_land_on_the_same_function() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = guard_core::scan_tree_excluding_self(&root, &["rs"], file!());
        let mut offenders: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for (path, src) in &files {
            let lines: Vec<&str> = src.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                if l.trim() != "#[test]" {
                    continue;
                }
                seen += 1;
                for next in lines.iter().skip(i + 1) {
                    let t = next.trim();
                    if t.is_empty() || t.starts_with("///") || t.starts_with("//") {
                        continue;
                    }
                    if t == "#[test]" {
                        offenders.push(format!(
                            "  {}:{} —— 这个 `#[test]` 之后又是一个 `#[test]`",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            i + 1
                        ));
                        break;
                    }
                    if t.starts_with("#[") {
                        continue; // 别的属性（`#[cfg(...)]` / `#[ignore]`）合法
                    }
                    break; // 落到 `fn` 或别的东西：本条只管 `#[test]` 连着 `#[test]`
                }
            }
        }
        // 抽取器自检：真的扫到了测试（否则本条会零命中地绿）。
        assert!(
            seen > 200,
            "只扫到 {seen} 个 `#[test]` —— 扫描面坏了，本条在空转（08-11 实测全树 {} 个文件）",
            files.len()
        );
        // ── 第二半：**直接钉伤害**，不只钉症状 ────────────────────────────
        //
        // 上面那半认的是「`#[test]` 后面又是 `#[test]`」这个**形状**。
        // 但真正的伤害是「某个 `fn` 没有属性、于是不是测试」——两者不等价：
        // 08-11 我修完第一处之后，另一处照样掉了属性，而上面那半**一声不吭**。
        //
        // ⚠ 判据形状按实测收敛过两次：
        // ① 第一版往回扫时遇到非 `#[` 行就停 ⇒ 多行 `#[cfg_attr(…)]` 的 `)]` 把它挡住，
        //    把一条**活着的**判据（`tmux.rs` 的 `the_shell_gate_expression_agrees_with_the_golden_table`）
        //    报成死的。真阳率 0% ⇒ 按铁律 18 那种版本不许留。
        // ② 现版改成「往回扫到空行 / `}` / 上一个 fn 为止，这一段里有没有 `#[test]`」，
        //    全树实测 **0 误报**。
        //
        // 人群限「`mod …test… {` 之后」+「无参无返回的 `fn 名()`」——
        // 那正是判据的形状；带参的是夹具（`hello_frame(commands: &[&str])` 之类），不在人群里。
        let mut orphans: Vec<String> = Vec::new();
        for (path, src) in &files {
            let lines: Vec<&str> = src.lines().collect();
            let Some(start) = lines.iter().position(|l| {
                let t = l.trim();
                t.starts_with("mod ") && t.contains("test") && t.ends_with('{')
            }) else {
                continue;
            };
            for (i, l) in lines.iter().enumerate().skip(start) {
                let t = l.trim();
                if !(t.starts_with("fn ") && t.ends_with("() {")) {
                    continue;
                }
                let mut k = i;
                let mut has_attr = false;
                while k > 0 {
                    let prev = lines[k - 1].trim();
                    if prev.is_empty()
                        || prev == "\u{7d}"
                        || prev.starts_with("fn ")
                        || prev.starts_with("pub fn ")
                    {
                        break;
                    }
                    if prev.contains("#[test]") {
                        has_attr = true;
                        break;
                    }
                    k -= 1;
                }
                if !has_attr {
                    orphans.push(format!(
                        "  {}:{} —— `{}` 在测试段里、长得像判据，却没有 `#[test]`",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        i + 1,
                        t.trim_start_matches("fn ").trim_end_matches("() {")
                    ));
                }
            }
        }
        assert!(
            orphans.is_empty(),
            "这些函数住在测试段里、长得像判据，但**没有 `#[test]`，从不运行**：\n{}\n\n\
             ⇒ 它和别的判据长得一模一样，读的人会以为那条性质有人守着。\n\
             08-11 全树逮到三条这样的死判据（`the_hello_witness_can_only_come_from_a_hello_frame`、\n\
             `tmux_targets_use_exact_match`、以及一条我自己修出来的），最久的从 `4d8cfc2` 起就是死的。",
            orphans.join("\n")
        );

        assert!(
            offenders.is_empty(),
            "这些地方两个 `#[test]` 叠在同一个函数上：\n{}\n\n\
             ⇒ 后面某个 `fn` 因此**没有属性、不是测试、从不运行**，而它看起来和别的判据一模一样。\n\
             `duplicate_macro_attributes` 只是 warn，本仓 clippy 不带 `-D warnings` ⇒ 编得过、跑得过、没人知道。\n\
             修法：把被顶开的那段头注搬回它自己的 `fn` 前，并给掉了属性的那个 `fn` 补回 `#[test]`。",
            offenders.join("\n")
        );
    }

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
            // ★ audit-0805 F16：52 → **80**（今日实测）。
            // 余量 28 的时候，`backend/`(15) + `adapter/`(2) **整体掉出扫描面仍会绿** ——
            // 而「扫描面缩水」正是这条地板存在的全部理由。
            // ⚠ 棘轮纪律：只许升不许降；要降必须带「副本真退役」的证据。
            80,
        );
    }

    /// ★ **剥注释只许有一个权威实现**〔audit-0805 §5 3h，08-06〕。
    ///
    /// # 它挡的是什么
    ///
    /// 「判据数到注释」在本区犯过三次（F12 跨语言对拍 · F24 的裸 `contains` 计数 ·
    /// 1k 的属性回溯）。三次的补法都是**在自己文件里现写一个剥注释的小函数** ——
    /// 于是 08-06 一数：**四个具名私有实现**，而 §5 3h 当时写的是「三处」。
    ///
    /// 更糟的是它们**语义不同**：两份整行删、一份整行留空、一份按 marker 截断。
    /// 「同一个词在四个地方各是一个意思」正是 E3 要消灭的形状。
    ///
    /// # 今天的唯一例外，以及它凭什么是例外
    ///
    /// `profile_installer::strip_comments(src, marker)` 按**第一个 marker 截断整行**，
    /// 因此能吃掉**行尾注释**；共享原语刻意不这么做（会砍坏 `"http://host"` 这类字面量，
    /// 详见 `guard_core::strip_comment_lines` 头注）。它扫的是**自己生成的** shell/rc 片段，
    /// 语料可控 ⇒ 那个风险在它那里不存在。**这不是豁免，是另一种语义。**
    ///
    /// ⚠ 想再加一个 ⇒ 先问「共享原语为什么不够」，答得出来才加进下面这张表。
    /// ★〔audit-0805 08-06〕**按「函数做了什么」再扫一遍剥注释实现**（默认拒绝）。
    ///
    /// # 它补的洞
    ///
    /// 下面那条按**函数名的三种拼法**取样（`strip_line_comments` / `strip_comments` /
    /// `without_comments`）。实测：往 `utils.rs` 加一个逐字同形、只是改名叫
    /// `fn drop_comments` 的实现 ⇒ **那条判据全绿**。
    /// 「只许有一份共享实现」这条纪律，此前只对**三个名字**成立。
    ///
    /// # 人群怎么定的（量了两轮才收住）
    ///
    /// 第一轮按行为取样（函数体里有 `//` / `#` 过滤）⇒ **29 处**，
    /// 绝大多数是各判据**内联**的一次性过滤（`hits` / `wake_hits` / `ci_live_lines` …），
    /// 它们不是「另一份剥法」，红它们只会淹掉信号。
    /// 第二轮收紧成「**返回 String / Vec 的转换器**」⇒ **14 处**，其中确实混着
    /// 四个非剥法（表格解析、CI 段落抽取、host 别名解析、字段解析）——
    /// 于是不猜，**逐个登记**：是剥法的写明「共享原语为什么不够」，不是的写明它在做什么。
    ///
    /// ⚠ 登记时读出一处**真事**：`tool_registry::production_code` 用的是
    /// **按 `//` 截断整行**的语义，而共享原语头注逐字写着刻意不这么做
    ///（会砍坏 `"http://host"` 这类字面量）。今天那个文件里没有 `://` 字面量所以没事，
    /// 但那是**运气**，不是设计 —— 现在它至少被登记着。
    #[test]
    fn every_comment_stripping_transformer_is_registered() {
        /// `(文件::函数, 它是什么 / 共享原语为什么不够)`。
        const TRANSFORMERS: &[(&str, &str)] = &[
            ("lib.rs::strip_comment_lines", "★ **共享原语本体**（`guard_core`）"),
            ("lib.rs::production_code", "共享原语：剥注释 + 剥测试段"),
            // 08-08 删掉 `lib.rs::test_source`：它**根本不剥注释**（只是把测试段拼起来）。
            // 它当初被检出，是因为旧检测器取「函数体起点后 700 字符」的定长窗口，
            // 一路吃进了它的邻居 `production_code`（那个才剥）。⇒ **这一行是误登记**，
            // 而误登记的害处是具体的：登记表是「已知的第二份剥法」清单，
            // 混进一条不是剥法的，下一个人会照它去找一份并不存在的实现。
            (
                "e2e_gate_registry.rs::strip_comments",
                "**别的注释语法**：语料是 shell 脚本（`e2e/*.sh`），注释是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 语义上刻意只剥**整行注释**、不碰行尾注释（shell 里 `#` 可以出现在字符串中间，                 按 marker 截断会误伤 `pgrep` 模式里的 `#`）。要收口的正确做法是给共享原语加一个                 「注释前缀」参数，那是另一件事。",
            ),
            (
                "cc_bus.rs::non_test_code",
                "本地剥法：只服务本文件自己的零命中守卫，语料是本文件源码。⚠ 与共享原语重复，登记为待收口",
            ),
            (
                "hooks_diag.rs::non_test_code",
                "同 cc_bus：本文件自用。⚠ 与共享原语重复，登记为待收口",
            ),
            (
                "tmux_hook.rs::prod_code",
                "daemon 侧本地剥法（跨 crate 够不着 monitor 的 `guard_core`）",
            ),
            (
                "tool_registry.rs::production_code",
                "⚠ **按 `//` 截断整行**——共享原语刻意不这么做（会砍坏 `\"http://host\"`）。\
                 本文件今天没有 `://` 字面量所以没事，但那是运气不是设计。登记为待收口",
            ),
            (
                "session_name_registry.rs::production",
                "**多语言**剥法（`.rs` 走共享原语，`.ts`/shell 各有注释语法）——共享原语只管 Rust",
            ),
            ("ccm_invocation.rs::refusal_variants", "不是剥法：从 `enum Refusal` 的定义里抽变体名（跳过 doc 行只是为了不把注释当变体）"),
            ("agent_profile_parity.rs::rows", "不是剥法：解析对拍表的行"),
            ("gate2_parity.rs::rows", "不是剥法：解析 golden 表的行"),
            ("gate.rs::golden_rows", "不是剥法：daemon 侧解析同一张 golden 表"),
            // 08-08 第二刀：`live_lines` 已变成一句委托（改调 `strip_hash_comment_lines`）⇒
            // 它不再是一份剥法，登记删掉。**同一天里这张表两次告诉我「你在写第二份剥法」**：
            // 一次是内联的 `#` 过滤（登记表逮的），一次是 `sftp.rs` 读 `release.yml`（变异逮的）。
            (
                "lib.rs::strip_hash_comment_lines",
                "**共享原语本体**：`strip_comment_lines` 的 YAML/shell 兄弟（`#` 整行注释）。\
                 两个都住 guard-core —— 判据要读 `.yml`/`.sh` 时借这一份，别再各写一遍",
            ),
            // 08-07：原 `ci_job_block`/`ci_yml` 搬进同文件的 `pub(crate) mod ci_yaml`
            // （E3：`ci.yml` 的读取与切块只有一个家，`lockfile_conflict_guard` 也要用）。
            // 搬家当场被本条逮住（多出 `job_block`、少了那两个）—— 这正是默认拒绝该有的样子。
            ("shared_crate_registry.rs::job_block", "不是剥法：抽某个 job 的段落"),
            ("ssh_source.rs::parse_host_aliases", "不是剥法：解析 ssh config 的 Host 别名"),
            ("tool_registry.rs::declared_fields_of", "不是剥法：解析结构体字段声明"),
            // 〔U8c-3-r2 08-14〕**这一格是本条判据当场逮出来的**：08-04 那份剥法是**内联**的
            // （一串 `.lines().filter().map()`），本条看不见；把它抽成具名函数给两处共用时，
            // 本条立刻说「你有第二份剥法」。⇒ 收口的动作反而暴露了此前没被登记的欠账。
            (
                "launch_wire.rs::production_ts",
                "**整行那半已经是共享原语**（本函数转调 `strip_comment_lines`），多出来的只有\
                 **行尾 `//` 截断** —— 共享原语刻意不剥行尾（会砍坏 `\"http:` + `//host\"`），\
                 而本组判据必须剥（F10 逐字：行尾注释里的提及不算数）。\
                 ⚠ 与 `tool_registry.rs` 那条的差别：**那条的安全是运气，这条的是读数** ——\
                 `the_ts_comment_stripper_actually_strips` 对四份语料逐个断言不含该字面量",
            ),
        ];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根");
        let mut found: Vec<String> = Vec::new();
        for sub in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let file = f
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let mut from = 0usize;
                while let Some(i) = raw[from..].find("fn ") {
                    let at = from + i + 3;
                    from = at;
                    let name: String = raw[at..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if name.is_empty() {
                        continue;
                    }
                    let after = &raw[at + name.len()..];
                    let Some(arrow) = after.find("->") else {
                        continue;
                    };
                    if arrow > 200 {
                        continue;
                    }
                    let ret = after[arrow + 2..].trim_start();
                    if !(ret.starts_with("String") || ret.starts_with("Vec<")) {
                        continue;
                    }
                    let body_at = at + name.len() + arrow;
                    // ⚠ **按 char 取，不按字节切** —— 本仓 `digit_after` 头注逐字记过这个坑
                    // （中文注释里按字节 `saturating_sub` 会落在汉字中间当场 panic），
                    // 而我这一版还是先写成了字节切片，跑起来立刻炸在「残」字上。
                    // ⚠ **切到函数真正的结尾**，不是「起点后 700 字符」〔08-08〕：
                    // 定长窗口会一路吃进**下一个函数**。实测：把 `live_lines` 插在 `yml` 后面，
                    // `yml`（一句 `read_to_string` 而已）当场被判成「在剥注释」——
                    // 窗口里装的是它邻居的身体。**匹配单位比事实大**，本仓治了一轮又一轮的那一族。
                    // 按大括号配平切；配不平（宏里带不成对括号之类）就退回定长窗口，
                    // 那时宁可**多判**——多判会被登记表逼着看一眼，少判是静默漏。
                    let body: String = {
                        let rest: Vec<char> = raw[body_at..].chars().take(3000).collect();
                        let mut depth = 0i32;
                        let mut end = None;
                        for (i, c) in rest.iter().enumerate() {
                            match c {
                                '{' => depth += 1,
                                '}' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        end = Some(i + 1);
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                        rest[..end.unwrap_or(rest.len().min(700))].iter().collect()
                    };
                    // ⚠ **先剥掉整行注释再找**〔08-08〕：本检测器找的是「这段代码在剥注释」，
                    // 而它原来拿**函数体原文**去找 —— 于是**一句解释性注释里写出那个形态就会被算成实现**。
                    // 实测：`e2e_gate_registry::floored` 里有一行注释写着「第一版在这里内联了一个
                    // `starts_with('#')` 过滤」，本条当场把它判成第二份剥法，还建议我去登记它。
                    // ⇒ 与 plan-lint 判据 7 同一个教训：**判据要看围栏，不是看围栏的说明书**。
                    // 用共享原语剥（它认 `//` 那套 Rust 形态，正是这里要去掉的东西）。
                    let body = guard_core::strip_comment_lines(&body);
                    let body = body.as_str();
                    let dq = '"';
                    let strips = body.contains(&format!("starts_with({dq}//{dq})"))
                        || body.contains(&format!("find({dq}//{dq})"))
                        || body.contains("starts_with('#')")
                        || body.contains(&format!("trim_start_matches({dq}//{dq})"));
                    if strips {
                        found.push(format!("{file}::{name}"));
                    }
                }
            }
        }
        found.sort();
        found.dedup();
        // 抽取器自检：连共享原语本体都扫不到 ⇒ 遍历或形态坏了。
        assert!(
            found.contains(&"lib.rs::strip_comment_lines".to_string()),
            "连共享原语 `strip_comment_lines` 都没扫到 —— 抽取器坏了，下面的对拍会空绿：{found:?}"
        );
        let mut want: Vec<String> = TRANSFORMERS.iter().map(|(n, _)| (*n).to_string()).collect();
        want.sort();
        assert_eq!(
            found, want,
            "\n「返回 String/Vec 且会剥注释」的函数与登记表对不上。\n\
             **多出来的**：先问「共享原语 `guard_core::strip_comment_lines` 为什么不够」——\n\
             答得出来就登记进 `TRANSFORMERS` 并写明理由；答不出来就改成调它。\n\
             ⚠ 名字叫什么**不是判据**：换个名字的同一份剥法仍然是第二份剥法。\n\
             **少了的**：它被收口了 ⇒ 把登记删掉（登记表腐烂比没有登记更糟）。"
        );
    }

    #[test]
    fn comment_stripping_has_exactly_one_shared_implementation() {
        const REGISTERED: &[(&str, &str)] = &[
            (
                "profile_installer.rs",
                "按 marker 截断整行（能吃行尾注释），语料是自己生成的 shell/rc 片段、无 `://` 字面量风险",
            ),
            (
                "e2e_gate_registry.rs",
                "**别的注释语法**：语料是 shell 脚本（`e2e/*.sh`），注释前缀是 `#` ——                  共享原语 `strip_comment_lines` 只认 `//` / `*` / `/*`（Rust/JS），对 `#` 一行都剥不掉。                 ⚠ 刻意只剥**整行**：shell 里 `#` 会出现在字符串中间（本处语料就有 `pgrep` 模式），                 按 marker 截断会误伤。收口的正确做法是给共享原语加一个「注释前缀」参数，那是另一件事。",
            ),
        ];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<String> = Vec::new();
        for (f, raw) in guard_core::scan_tree!(&root, &["rs"]) {
            // 只看生产段之外也一样：私有剥法一律住在测试模块里，所以扫整份。
            for l in raw.lines() {
                let t = l.trim_start();
                if t.starts_with("fn strip_line_comments")
                    || t.starts_with("fn strip_comments")
                    || t.starts_with("fn without_comments")
                {
                    found.push(
                        f.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string(),
                    );
                }
            }
        }
        found.sort();
        found.dedup();

        // 抽取器自检：连登记在册的那一个都扫不到 ⇒ 遍历或形态坏了，下面的对拍会空绿。
        assert!(
            found.contains(&"profile_installer.rs".to_string()),
            "连 `profile_installer.rs` 里那个已登记的实现都没扫到 —— 遍历或形态坏了，\n\
             那样「没有新增」这个结论是零命中得来的，不是真的"
        );

        let extra: Vec<&String> = found
            .iter()
            .filter(|f| !REGISTERED.iter().any(|(r, _)| *r == f.as_str()))
            .collect();
        assert!(
            extra.is_empty(),
            "又出现了私有的剥注释实现：{extra:?}\n\
             ★ 先用 `guard_core::strip_comment_lines` —— 08-06 已把三份重复迁过去。\n\
             它**确实不够**时才加进本表，并写清是哪种语义上的不够（行尾注释？保行号？\n\
             别的注释语法？）。⚠ 只写文件名不算理由。\n\
             已登记：{REGISTERED:?}"
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

    /// 〔audit-0805 08-06〕**拼命令用的 shell 元字符黑名单，权威源恰好一处**（E3）。
    ///
    /// # 它不是理论风险 —— 同一族已经漂过一次
    ///
    /// `backend/control/payload.rs` 的头注逐字记着：U7-3 把**不可见字符表**收进 `acct-core`
    /// 让两个读 manifest 的地方共用，**而「拼命令」那条路当时没跟上** ——
    /// `history.rs` 一直用自己那张 U7-3 之前的旧表，缺 `U+1680` · `U+2000..200A` ·
    /// `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，是一处**纵深防御缺口**。
    ///
    /// 08-06 顺着「只被一处调用的生产函数」这条先验查到：**元字符表也是两份逐字副本**
    /// （`history.rs` 与 `payload.rs`），而**没有任何东西对拍它们**。
    /// ⇒ 按 E3 收成一处：`history.rs` 那份删掉、改为派生 `payload::is_command_unsafe_char`；
    /// 本条钉住「以后也只有一处」。
    ///
    /// ⚠ E3 逐字要求「判据钉的是**权威源恰好一个**，不是『有没有登记』」——
    /// 所以这里数的是**定义处数**，不是「两处内容一不一样」。
    /// 后者在两份都改错时照样绿，前者不会。
    #[test]
    fn the_shell_metachar_blacklist_has_exactly_one_home() {
        const NEEDLE: &str = "const SHELL_META_COMMON";
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut homes: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        for (path, src) in guard_core::scan_tree!(&root.join("src"), &["rs"]) {
            scanned += 1;
            let prod = guard_core::production_code(&src);
            if prod.lines().any(|l| {
                l.trim_start().starts_with(NEEDLE)
                    || l.trim_start().starts_with(&format!("pub(crate) {NEEDLE}"))
                    || l.trim_start().starts_with(&format!("pub {NEEDLE}"))
            }) {
                homes.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
        // ★ 抽取器自检：遍历坏了会让下面「恰好一处」变成「恰好零处」也叫不出来。
        // ⚠ 地板是**实测**的：`src-tauri/src` 下 86 个 `.rs`，`scan_tree!` 摘除调用者自己 ⇒ 85。
        //   第一版我拍了个 100 —— 判据一建就红。**拍出来的数与抄来的数一样会腐**，
        //   本会话已在别处记过多次，这次犯在自己刚写的自检上。
        assert!(
            scanned >= 80,
            "只扫到 {scanned} 个 .rs —— 遍历坏了，下面那条会零命中地绿（建判据当日实测 85）"
        );
        assert_eq!(
            homes.len(),
            1,
            "拼命令用的元字符黑名单**不是恰好一处**，实得：{homes:?}\n\n\
             ⚠ 同一族已经漂过一次：`payload.rs` 头注记着，`history.rs` 那张**不可见字符表**\n\
             曾停在 U7-3 之前的旧版本，缺六段 Unicode —— 一处纵深防御缺口。\n\
             ⇒ 要新增消费者就**派生**（`payload::is_command_unsafe_char`），别再抄一份表。\n\
             E3：判据钉的是「权威源恰好一个」，不是「两份内容一不一样」——\n\
             后者在两份都改错时照样绿。"
        );
        assert!(
            homes[0].ends_with("backend/control/payload.rs"),
            "权威源搬家了（现在在 {:?}）—— 搬可以，但请顺手把本条与两处头注的指向一起改。",
            homes[0]
        );
    }
    /// ★★ **位置比较型判据的三种坏法，做成一条常驻元判据**〔audit-0805 08-08，Phase G 第 81 件〕。
    ///
    /// 08-08 透镜五横扫全仓「比源码位置」的顺序断言，**四条老的里两条是洞**，
    /// 一般化出三种坏法，每一种都有活样本：
    ///
    /// | 坏法 | 活样本 | 后果 |
    /// |---|---|---|
    /// | ① 比的是**注释**不是代码 | `watcher.rs` 拿 `// --- Phase 1 …` 当扫描锚点 | 真扫描搬到注入之前、注释不动 ⇒ 判据全绿，而启动时活着的会话一个 pidfd 看守都没有 |
    /// | ② 比的是**任意一处**不是**那一处** | `local_backend.rs` 的 `rfind(".stop()")` · `kill.rs` 的裸 `kill-session`（生产段两处） | 退出臂里删掉 `.stop()`、别处留一处 ⇒ 全绿，daemon 变游魂进程 |
    /// | ③ **文本顺序 ≠ 执行顺序** | `inbound.rs` 把 `remove` 搬进新 task | 文本上仍在前面，实际什么时候跑没人保证 |
    ///
    /// ⇒ 本条把①②做成机检（③ 没有可靠的文本特征，留在各判据自己的反向自检里）：
    /// 语料必须过 `production_code`（不许比注释）· 不许 `rfind`（那是「任意一处」）·
    /// 锚点必须被**界定**（切一段 `arm_of`，或当场核一次唯一性）。
    ///
    /// ⚠ **人群刻意收窄到「语料是本仓 Rust 源码」**：全仓还有 4 条位置比较判据比的是
    /// **生成的命令串 / PowerShell 模板 / 文档**（`account_usage` 两条 · `profile_installer` 两条）——
    /// 对它们来说「过 `production_code`」根本不成立。先量误红面再定人群，
    /// 这是本工作区反复吃亏的地方（人群取宽 ⇒ 逼人往豁免表里塞条目 ⇒ 判据变废纸）。
    #[test]
    fn every_position_comparison_over_source_pins_and_bounds_its_anchors() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf();
        let mut files = guard_core::scan_tree!(&root.join("src-tauri/src"), &["rs"]);
        files.extend(guard_core::scan_tree!(
            &root.join("remote-daemon-proto/src"),
            &["rs"]
        ));

        let mut population = 0usize;
        let mut bad: Vec<String> = Vec::new();
        for (path, src) in &files {
            let name = path.to_string_lossy().replace('\\', "/");
            for body in src.split("\n    fn ").skip(1) {
                let fname = body.split('(').next().unwrap_or("").trim();
                let finds = body.matches(".find(").count() + body.matches(".rfind(").count();
                if finds < 2 {
                    continue;
                }
                // 比位置：`x < y` 这种形状（`_at` 命名或 assert! 里直接比两个局部）。
                let compares = body.contains("_at < ")
                    || body
                        .lines()
                        .any(|l| l.trim().starts_with("assert!(") && l.contains(" < "))
                    || body.lines().any(|l| {
                        let t = l.trim();
                        t.ends_with(" < types,")
                            || t.ends_with(" < scan_at,")
                            || t.ends_with(" < act,")
                    });
                if !compares {
                    continue;
                }
                // 语料是不是本仓 Rust 源码（否则「过 production_code」这条要求不成立）。
                let rust_corpus = body.contains("production_code(") || body.contains(".rs\")");
                if !rust_corpus {
                    continue;
                }
                population += 1;
                let mut why = Vec::new();
                if !body.contains("production_code(") {
                    why.push("语料没过 `production_code` ⇒ 它在比**注释**的位置（坏法①）");
                }
                if body.contains("rfind(") {
                    why.push("用了 `rfind` ⇒ 比的是**任意一处**，不是**那一处**（坏法②）");
                }
                // `find_pinned` = **恰好一处 + 两侧有边界**（`guard_core`）。它比这里原有的两种
                // 界定法都强：`arm_of` 只切段（段内仍可能有第二处），`matches().count()` 只核数量
                // 而不管边界。⇒ 认它〔08-11，P2s 翻面那条判据用的就是它〕。
                // ⚠ 补它不是放宽：不认的话，用更强原语的判据反而被判不合格，
                // 那会把人推回 `matches().count()` —— 而那条又踩 `needle_anchor_registry` 的棘轮。
                let bounded = body.contains("arm_of(")
                    || body.contains("find_pinned(")
                    || (body.contains("matches(") && body.contains(".count()"));
                if !bounded {
                    why.push(
                        "锚点没被界定 ⇒ 没切段（`arm_of`）也没核唯一性，\
                         第二处同名字面量出现时它会比到别处去（坏法②的另一半）",
                    );
                }
                if !why.is_empty() {
                    bad.push(format!(
                        "  {name}::{fname}\n      - {}",
                        why.join("\n      - ")
                    ));
                }
            }
        }
        // 抽取器自检：人群塌了的话下面那条就是一句废话。
        assert!(
            population >= 5,
            "只识别出 {population} 条「比源码位置」的判据（08-08 实测 6）—— \
             识别口径坏了，本条会零命中地绿"
        );
        assert!(
            bad.is_empty(),
            "这些位置比较型判据没守住三条纪律：\n{}\n\n\
             ★ 三种坏法各有活样本（08-08 实测，逐条写在本条头注的表里）：\n\
             ① 比注释（`watcher.rs` 曾拿一行 `// --- Phase 1 …` 当扫描锚点）；\n\
             ② 比任意一处（`local_backend.rs` 的 `rfind(\".stop()\")`；`kill.rs` 的裸 `kill-session` \n\
                在生产段有两处，命中对的那处**是排序运气**）；\n\
             ③ 文本顺序 ≠ 执行顺序（`inbound.rs` 把 `remove` 搬进新 task 就绕过去了）。\n\
             ⇒ 修法：语料先过 `production_code`；别用 `rfind`；\n\
             锚点要么切一段（`arm_of`）、要么当场核一次唯一性（`matches(..).count() == 1`）。",
            bad.join("\n")
        );
    }
}
