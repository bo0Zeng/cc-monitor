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
// `doc_claim_registry.rs::every_code_symbol_named_in_the_docs_still_resolves`
// —— 它 08-06 就在守 `doc/` 里的符号地址了，**而源码这一侧一直没人守**。
// 那正是「处方写好了只落了一处」的机制答案：处方落进了一个**没有判据的人群**。
// ═══════════════════════════════════════════════════════════════════════════

/// 一处**符号地址**：`(引用点行号, 被引文件基名, 符号名, 是不是前缀形)`。
pub type SymbolAddress = (usize, String, String, bool);

/// 一处**行号地址**：`(引用点行号, 被引路径原样, 被引行号)`。
pub type LineAddress = (usize, String, usize);

/// 显式标记：这一处行号地址是**故意留着的历史反例**（订正段 / 墓碑），不许判。
///
/// 为什么要显式标记而不是从散文里认：`frame_cadence_guard.rs` 里那条判据的 `///` 量过同一格
/// —— 「试跑那条规则**今天**误红 7 处全是合法文本，把限定词表调到全绿就是曲线拟合」。
/// ⇒ 由写的人**声明**，不由判据**猜**。
///
/// ⚠ 〔`K-R20` 订正 09-03〕这一句原先写的是「`ratchet_guard.rs` **头注**量过同一格」，
/// 三处都假：**引了中间人**（`ratchet_guard.rs` 自己是转述，原话在 `frame_cadence_guard.rs`）·
/// **部位假**（那句话在一条 `///` 上，不在 `//!` 头注里）·
/// **「今天」掉了**（一个有日期的快照被引成了无时态的性质）。
/// 溯源三跳由 `K-R19` 现打、`K-R20` 收：原点 → `ratchet_guard.rs` → 本处。
pub const LINE_ADDRESS_TOMBSTONE: &str = "〔行号墓碑〕";

/// 显式标记：这一**行**里的那个符号名是**故意留着的历史名**（订正段 / 墓碑），不许判。
///
/// 它是 [`LINE_ADDRESS_TOMBSTONE`] 的兄弟，服务的是
/// [`tests::every_dead_name_named_in_the_prose_is_declared_dead`]。**两个刻意不合成一个**：
/// 合成一个 ⇒ 一处行号墓碑会顺手赦免同一行上的死名（反之亦然），
/// 而那是「匹配单位比事实大」——本仓 `find_pinned` 的头注整段在讲这一族。
///
/// # 🔴 为什么它是**前置**，不是收尾〔`K-R19` 实测，`KR20D1` 逐字点名〕
///
/// `K-R19` 把 6 处「零定义名字当现状说」订正落盘之后，**尺子读数一动没动**
/// （221/350 → 221/350）—— 因为**订正段里逐字引用了旧名字**：
/// 「这里原先写的是 `X`，全仓零定义」这句话本身，就又是一处 `X` 的注释提及。
/// ⇒ **不带墓碑的闸会永远红**，而一条永远红的闸第一天就得被调松，那是本族最坏的结局。
/// 所以标记必须先定下来，再谈装闸。
///
/// # 三个设计问题的答案（`KR20D1` 点名要答的）
///
/// ## ① 加在哪一级：**行级**
///
/// 标记与那个死名必须在**同一行**。三条理由：
///   · 与 [`LINE_ADDRESS_TOMBSTONE`] 同形 —— 那个形状已经在本仓跑通过一轮；
///   · **窗口按构造有界**。段级标记（「本段以下都是历史」）的窗口开到哪儿说不清，
///     而本仓 `KP4` 实测过：`[\s\S]*?` 那种无界窗口**一刀就被绕过**，
///     「有正则」买不到牙，「窗口有界 + 次数钉死」才买得到；
///   · 名字级（`` `x`〔墓碑〕 ``）读起来更碎，而行级已经够细 ——
///     一行里两个死名要一起赦免时，那两个本来就在讲同一件事。
///
/// ## ② 谁能加：**写那句话的人**，而防它变成「红了就贴标签」的逃生舱靠三道
///
///   · **它只赦免那一行那一处**。人群单位是「处」不是「名字」——
///     同一个死名在别处仍然当现状说的话，那一处照样红；
///   · **贴一个墓碑是一次会被看见的记账**：判据把墓碑处数**钉死在登记表里**，
///     贴了不登记 ⇒ 红，登记了盘上没有 ⇒ 也红（保鲜自检）。
///     ⇒ 它进 diff、可 `grep`，而 `K4` 逐字要求 PM **自己读 diff**；
///   · 存量**不许**靠贴墓碑了事 —— 见 ③。
///
/// ## ③ 存量怎么办：**不贴墓碑，走棘轮 + 登记表**
///
/// 现打人群 59 个名字 / 87 处（`>=2` 个下划线，量于本件基点 `38fa388`），
/// 其中绝大多数在本拍写区之外，贴不了；而「哪儿红就往哪儿贴标签」正是要防的那件事。
/// ⇒ 存量逐条登记进 `every_dead_name_named_in_the_prose_is_declared_dead` 的 `INVENTORY`
/// （`K-R17` 那条行号地址棘轮同形），**新写的一律红**；
/// 墓碑**只用于本轮真的改过的那几处订正段** —— 那才是它被造出来要解的那个问题。
///
/// # 🔴 它买不到什么（写明，别把射程写宽了一格）
///
/// 墓碑只声明「我知道这个名字不在代码里」，**判不了「这句话是不是当现状在说」**。
/// 有人把墓碑贴在一句现在时的话上，本条一声不吭 —— 那要读语义，机器读不了。
/// 这是本族**判不了**的那一半，不假装它覆盖了。
pub const PROSE_NAME_TOMBSTONE: &str = "〔散文墓碑〕";

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
}

/// 往前收一个 ASCII 路径 —— 遇到中文（多字节）自然停在字符边界上。
///
/// 抄 `doc_claim_registry.rs` 那条同族判据的收法：本仓的地址几乎都嵌在中文散文里。
fn path_before(line: &str, at: usize) -> &str {
    let b = line.as_bytes();
    let mut s = at;
    while s > 0 && is_path_char(b[s - 1] as char) {
        s -= 1;
    }
    &line[s..at]
}

/// 枚举 `text` 里每一处 `文件.rs::符号`。
///
/// **前缀形**（第四个返回值 `true`）：抽出来的符号名以 `_` 收尾。本仓真实出现两形，
/// 都不是腐坏，而是写法：
///   · 通配（`build_local_` 后面跟着 `*_command`）；
///   · 行折（`emit_daemon_` 与它的后半截被 `///` 换行拆开）。
/// ⇒ 这一档降级成「那个文件里有**某个**以它打头的声明」，**不许**当成找不到就报红。
pub fn symbol_addresses(text: &str) -> Vec<SymbolAddress> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let b = line.as_bytes();
        let mut from = 0usize;
        while let Some(k) = line[from..].find(".rs::") {
            let at = from + k;
            let base = {
                let p = path_before(line, at);
                let full = format!("{p}.rs");
                full.rsplit('/').next().unwrap_or_default().to_string()
            };
            let mut e = at + 5;
            while e < b.len() && {
                let c = b[e] as char;
                c.is_ascii_alphanumeric() || c == '_'
            } {
                e += 1;
            }
            let sym = line[at + 5..e].to_string();
            if !sym.is_empty() && base != ".rs" {
                let prefix = sym.ends_with('_');
                out.push((i + 1, base, sym, prefix));
            }
            from = at + 5;
        }
    }
    out
}

/// 枚举 `text` 里每一处 `文件.rs:行号`（`文件.rs:行号-行号` 只取头一个数）。
///
/// 带 [`LINE_ADDRESS_TOMBSTONE`] 标记的那一行**不进枚举**（形 B）。
/// `文件.rs::符号` **不会**被误收：`::` 后面不是数字。
pub fn line_addresses(text: &str) -> Vec<LineAddress> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.contains(LINE_ADDRESS_TOMBSTONE) {
            continue;
        }
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

/// 枚举 `text` 的**生产段**里所有以 `verbs` 任一动词打头的 `fn` 名（去重、有序）。
///
/// # 它服务的是哪一族判据〔`K-R63` 09-11〕
///
/// 一族「**声明缺口**」：某张表上写着「这一格今天盘上没有实现」（`None` / `false`），
/// 而那句话**没有任何东西核**。本仓的活体是 `tool_registry.rs::TOOLS` 的 `remote-daemon`：
/// 字段写着 `uninstallable: false`，而 `sftp.rs::uninstall_remote_daemon` 是设置面板上
/// 那个「卸载 daemon」按钮背后的实现，**一直都在** —— 假申报活了一个月，一格没红。
///
/// ⇒ 处方：申报「没有」的那一格，**去它家里扫一眼有没有一个没人认领的同族实现**。
///
/// # 🔴 射程写死，别读大一格
///
/// 它按**名字**认，一个字的语义都不读：
///   · 动词表由调用方给，**不是穷举** —— 叫别的名字的实现它一个都看不见；
///   · 它只说「那份文件的生产段里有一个这么打头的 `fn`」，
///     **说不出**那个 `fn` 是不是真在做那件事（反过来也一样）。
/// ⇒ 它买到的是「那句『没有』有人在核」，**不是**「那句『没有』一定是真的」。
///
/// 剥法走**共享原语** `guard_core::production_code`（剥注释 + 剥测试段）——
/// 本文件那条 `every_comment_stripping_transformer_is_registered` 逐字要求
/// 「先问共享原语为什么不够」，这里够。
pub fn fn_names_starting_with(text: &str, verbs: &[&str]) -> Vec<String> {
    let prod = guard_core::production_code(text);
    let mut out = Vec::new();
    for line in prod.lines() {
        let mut it = line.split_whitespace().peekable();
        while let Some(tok) = it.next() {
            if tok != "fn" {
                continue;
            }
            let Some(next) = it.peek() else { continue };
            let name: String = next
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && verbs.iter().any(|v| name.starts_with(v)) {
                out.push(name);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
#[path = "../../../tests/bridge/structural_scan_tests.rs"]
mod tests;
