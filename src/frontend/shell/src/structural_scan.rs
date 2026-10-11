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
// 扫描报告 · 钉逃生口 · 符号地址 · 按动词收 fn 名这几把搬进了共享的 `guard_core`（足迹的申报表与判据进了后端，两侧要用同一把）。
pub(crate) use guard_core::path_before;
pub use guard_core::{
    fn_names_starting_with, pin_definition, symbol_addresses, ScanReport, SymbolAddress,
};

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

// ═══════════════════════════════════════════════════════════════════════════
// `K-R17`：源码里的**地址**这一族 —— 两个纯抽取器
//
// 「某个文件的某一行」这种地址**会烂**，而在本件之前盘上**没有任何判据**在它们
// 变馊时会响：08-25 / 09-01 / 09-02 三次撞见，三次都是顺带撞见的。
//
// 处方 08-25 就写在仓里了（`byte_cap_registry.rs` 那段订正，逐字）：
// 「⇒ 改成点**函数名**……行号是**每一轮都会变的量**，写进登记表下一轮自动变成假话。」
//
// ⚠⚠ **这里必须把「机器判得了什么」说死，别把射程写宽了一格**：
//
// 「这句引文说的还不还是被引行那件事」**本质上要读语义**，机器读不了。
// 机器判得了的只有下面这三样，一样都不涉及语义：
//   ㈠ 被引的**符号**在不在（`文件.rs::符号` ⇒ 那个文件里有没有这个声明）；
//   ㈡ 被引的**行号**越没越界（`文件.rs:行号` ⇒ 那个文件有没有这么多行）；
//   ㈢ 行号地址的**处数**有没有涨（棘轮）。
// 判不了的是**其余全部** —— 一个裸行号地址今天指得对不对，机器一个字也读不出来。
// ⇒ 出路不是让机器变聪明，是**把新写的地址逼进 ㈠ 那个子集**：符号地址的真伪
//   完全机器判得了，而行号地址的真伪完全判不了。棘轮就是那道逼迫。
//
// 同族先例（本件的形状抄它，只是把扫描面从散文换成源码）：
// `doc_claim_registry_tests.rs::every_code_symbol_named_in_the_docs_still_resolves`
// —— 它 08-06 就在守 `doc/` 里的符号地址了，**而源码这一侧一直没人守**。
// 那正是「处方写好了只落了一处」的机制答案：处方落进了一个**没有判据的人群**。
// ═══════════════════════════════════════════════════════════════════════════

/// 一处**行号地址**：`(引用点行号, 被引路径原样, 被引行号)`。
pub type LineAddress = (usize, String, usize);

/// 枚举 `text` 里每一处 `文件.rs:行号`（`文件.rs:行号-行号` 只取头一个数）。
///
/// `文件.rs::符号` **不会**被误收：`::` 后面不是数字。
pub fn line_addresses(text: &str) -> Vec<LineAddress> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let b = line.as_bytes();
        let mut from = 0usize;
        while let Some(k) = line[from..].find(".rs:") {
            let at = from + k;
            let mut e = at + 4;
            let mut n = 0usize;
            let mut any = false;
            while e < b.len() && (b[e] as char).is_ascii_digit() {
                n = n * 10 + (b[e] - b'0') as usize;
                any = true;
                e += 1;
            }
            if any {
                let p = path_before(line, at);
                if !p.is_empty() {
                    out.push((i + 1, format!("{p}.rs"), n));
                }
            }
            from = at + 4;
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/structural_scan_tests.rs"]
mod tests;
