//! PowerShell profile 路径解析 + 块插入/卸载 + 命令名冲突检测。
//!
//! ## 块标记
//!
//! cc-monitor 写入 profile 用明显的块边界，可被工具识别、原子替换、整块卸载：
//!
//! ```text
//! # === cc-monitor BEGIN v1 ===
//! ... cc function 内容 ...
//! # === cc-monitor END ===
//! ```
//!
//! 重装时找到 BEGIN/END 范围整块替换；卸载时整块删除。用户在块外的任何内容不动。
//!
//! ## 🔴 〔`K-R62` 09-11〕本模块从「PowerShell 专用」扩到**两种方言**
//!
//! 立件时现打的账（`K-R57` 摸底 → `K-R62 §0b`）：**本机 POSIX 那一格，装与查都缺。**
//!
//! | 面 | 本机 Windows | 本机 POSIX（本件之前） | 远端 POSIX |
//! |---|---|---|---|
//! | 装「别名块」 | [`install_to_profile`] | 🔴 **零口** | `sftp::install_remote_ccm_helper` |
//! | 查「你 rc 里那几行是旧的」 | [`scan_legacy_profiles`] | 🔴 **零口** | —— |
//!
//! 补法有两条硬边界，两条都是**这件事的一半价值**：
//!
//! 1. **不许变成第四套。** 装进本机 rc 的内容与远端那个口来自**同一个常量**
//!    （`sftp::CCM_WRAPPER_SNIPPET`），合块与剥块走**同一份实现**
//!    （`sftp::merge_profile_block` / `sftp::strip_profile_block`），围栏是**同一对标记**
//!    （`sftp::CCM_PROFILE_BEGIN` / `..._END`）。本模块**一个字节的 snippet 都不生成**，
//!    也**没有第二套 merge/strip** —— 见 [`plan_install`] / [`plan_uninstall`]。
//! 2. **一个字节都不许删用户的行**（`K31` + 用户逐字「原本的配置要手动删除」）。
//!    「查」这一半的产物是 [`render_manual_cleanup_hint`]：**逐行指名 + 一段让他自己动手的提示**，
//!    产品自己不动手。理由不是保守，是**做不到**：那些行没有围栏，边界只有人知道
//!    （`K-R57` 现打：用户机器上 10 个真使用者全是裸行）。
//!
//! ⚠ **方言不是「猜路径」。** 路径始终由界面上的人选（`ProfileKind::Custom` 一直是产品特性）。
//! [`flavor_of`] 回答的是**另一个问题**：人选定了这份文件之后，往里写哪种语言。
//! 把 `function cc { … }` 写进 `~/.bashrc` 在任何情形下都不是对的答案 ——
//! 而本件之前这条路**只会**写 PowerShell。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// ⚠ `K-R62` 起是 `pub(crate)`：`fenced_block::FENCE_SHAPES` 那张账要**指**这一对，
/// 而不是抄一份字面量过去（抄一份就是第二个住址）。
pub(crate) const BEGIN_MARKER: &str = "# === cc-monitor BEGIN";
pub(crate) const END_MARKER: &str = "# === cc-monitor END";

/// cc function 模板源码（含 `{{COMMAND_NAME}}` placeholder）
const CC_TEMPLATE: &str = include_str!("../scripts/cc.ps1.tpl");

/// PowerShell profile 类型标签。v1.7.2 起 UI 只用作显示提示，实际安装传 path。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub enum ProfileKind {
    /// Windows PowerShell 5.1（Windows 自带）→ Microsoft.PowerShell_profile.ps1
    Ps51,
    /// PowerShell 7.x（独立安装）→ Microsoft.PowerShell_profile.ps1
    Ps7,
    /// 用户自定义路径
    Custom,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct ProfileScan {
    pub kind: ProfileKind,
    pub path: String,
    pub exists: bool,
    /// 是否含 cc-monitor BEGIN/END 块
    pub has_ccm_block: bool,
    /// 块中的版本字符串（"v1" 等）
    pub ccm_block_version: Option<String>,
    /// 已有同名 function（非 ccm 块内的）
    pub conflicting_functions: Vec<String>,
    /// 🔴 〔`K-R62`〕**「你 rc 里这几行是旧的」那段话。** 空串 = 没有要清的。
    ///
    /// 它是 [`render_manual_cleanup_hint`] 的产物：**逐行指名**（行号 + 原文）
    /// 加一段给用户自己动手的说明。**产品一个字节都不删**（`K31` + 用户逐字
    /// 「原本的配置要手动删除」）—— 那些行没有围栏，边界只有人知道。
    ///
    /// ⚠ **只对 [`ProfileFlavor::PosixRc`] 有内容**；PowerShell 那一侧的遗留由
    /// [`scan_legacy_profiles`] 按**围栏**答（那是另一件事：它找的是**装错位置的整块**，
    /// 这一格找的是**根本没有围栏的裸行**）。
    pub manual_cleanup_hint: String,
    // **C03 大整数策略**：量纲是**字节数**——2^53-1 B ≈ **8 PB**。
    // PowerShell profile 是文本脚本，不可能接近它 ⇒ f64 精度足够。同 `SftpEntry.size` 那条论证。
    #[cfg_attr(test, ts(type = "number"))]
    pub size_bytes: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// `K-R62`：本机 POSIX 那一半 —— **装**（借远端那一份）与**查**（够得着裸行）
// ═══════════════════════════════════════════════════════════════════════════

/// 一份 profile 的**方言**：这份文件里该放哪种语言的内容、认哪一对围栏。
///
/// 🔴 **它不猜路径。** 路径始终由界面上的人选（`account_aliases` 的 `§0e` 那条理由
/// 原样适用：`.bashrc` / `.zshrc` / fish 的 `config.fish` 写法不同，替人选一份是最坏的
/// 那条路）。它回答的是**人选定之后**的那个问题：往这份文件里写哪种语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileFlavor {
    /// PowerShell profile。围栏是本模块的 [`BEGIN_MARKER`]/[`END_MARKER`]，
    /// 内容由 [`render_cc_code`] 按模板渲染。
    PowerShell,
    /// POSIX shell 的 rc（`.bashrc` / `.zshrc` / `.profile` …）。
    /// 围栏与内容**都借远端那一份**，本模块一个字节都不自己造。
    PosixRc,
}

/// 按**文件扩展名**定方言：`.ps1` ⇒ PowerShell，其余一律 POSIX rc。
///
/// ⚠ 为什么按扩展名而不是按 `cfg!(windows)`：**跑在哪台机器上**与**这份文件是什么**
/// 是两件事。`ProfileKind::Custom` 允许用户指任意一份 home 内的文件，
/// 而 PowerShell 的 profile 恒是 `.ps1`（`$PROFILE` 的四种取值全是）。
/// 按 `cfg` 分还有一个更硬的毛病：**沙箱里跑不到 Windows 那一臂**
/// （门禁在 Linux 上跑），于是两条臂里恒有一条没人验。
pub fn flavor_of(path: &Path) -> ProfileFlavor {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("ps1") => ProfileFlavor::PowerShell,
        _ => ProfileFlavor::PosixRc,
    }
}

/// 纯函数：**装**完之后这份文件该长什么样。落盘那一跳在 [`install_to_profile`]。
///
/// 🔴 **`PosixRc` 那一臂是 `KR62D1` 的正题**：它一个字节的 snippet 都不生成、
/// 也没有第二套 merge —— 内容是 `sftp::CCM_WRAPPER_SNIPPET`（= `shared/ccm-aliases.sh`
/// 本身），合块是 `sftp::merge_profile_block`，围栏是 `sftp::CCM_PROFILE_BEGIN/END`。
/// ⇒ 本机与远端装进 rc 的**是同一份东西**（`K15` / `K36`），
/// 而不是「同一件事的第四个形状」（`K-R62 §0c` 那三套）。
///
/// `command_name` / `include_cc_function` **只对 PowerShell 那一臂有意义**：
/// POSIX 那一块的名字（`cc` / `cct`）住在 `shared/ccm-aliases.sh` 里，
/// 那份文件自己用 `declare -f` 让着用户已有的同名函数 —— 由它说了算，不由这里的参数说了算。
pub fn plan_install(
    flavor: ProfileFlavor,
    existing: &str,
    command_name: &str,
    include_cc_function: bool,
    what: &str,
) -> Result<String, String> {
    match flavor {
        ProfileFlavor::PowerShell => {
            let code = render_cc_code(command_name, include_cc_function);
            replace_or_append_block(existing, &code, what)
        }
        ProfileFlavor::PosixRc => {
            crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)
        }
    }
}

/// 纯函数：**卸**完之后这份文件该长什么样。同 [`plan_install`]，POSIX 那一臂借远端那一份。
pub fn plan_uninstall(flavor: ProfileFlavor, existing: &str, what: &str) -> Result<String, String> {
    match flavor {
        ProfileFlavor::PowerShell => strip_block(existing, what),
        ProfileFlavor::PosixRc => crate::sftp::strip_profile_block(existing, what),
    }
}

/// 这份文件里**有没有** cc-monitor 的块，以及块里的版本串。
///
/// 两种方言认的是**两对不同的围栏**，一对都不许混：混了就是「装一个把另一个整块替换掉」
/// （`account_aliases` 那对刻意不同前缀，理由同源）。
fn block_presence(flavor: ProfileFlavor, content: &str) -> (bool, Option<String>) {
    match flavor {
        ProfileFlavor::PowerShell => find_block_version(content),
        // POSIX 那一对没有版本后缀 ⇒ 恒 `None`。判「在不在」与 PowerShell 同口径：
        // 只看有没有一行以 BEGIN 打头（**悬空的 BEGIN 也算在**——否则界面会说「未安装」
        // 且藏起卸载按钮，而点安装却报行号，那正是 T04 审计③ 治过的那一形）。
        ProfileFlavor::PosixRc => (
            content
                .lines()
                .any(|l| l.trim_start().starts_with(crate::sftp::CCM_PROFILE_BEGIN)),
            None,
        ),
    }
}

/// rc 里**围栏之外**、指着 `ccm` 的一行是什么形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyRcKind {
    /// `名字() { … ccm … }` —— **真使用者**（`K-R57` 现打：用户机器上那 14 行里的 10 行）。
    Function,
    /// 注释行（`#` 打头）。指名它只为让读的人知道「这几行也提到了 ccm」，不催他删。
    Comment,
    /// 其它（`alias cc=…` · `export PATH=…/ccm` · 直接调一次 …）。
    Other,
}

/// rc 里**围栏之外**、指着 `ccm` 的一行。**原文原样带着**，因为产品要做的是指名，不是改写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRcLine {
    /// 1 起的行号 —— 「逐行指名」的那个「行」。
    pub line_no: usize,
    /// **一整行的原文**，一个字节都没动。
    pub text: String,
    /// 形状。
    pub kind: LegacyRcKind,
    /// 它定义的那个函数名（只有 [`LegacyRcKind::Function`] 有）。
    pub name: Option<String>,
}

/// `ccm` 这三个字母在这一行里是不是**一个独立的词**。
///
/// ⚠ 要边界，不要裸 `contains`：`~/.cc-monitor/` 里没有 `ccm`，但 `ccmx` / `myccm` 有 ——
/// 匹配单位比事实小正是本仓那条递减棘轮在数的东西。
fn mentions_ccm(line: &str) -> bool {
    let b = line.as_bytes();
    let word = |c: u8| (c as char).is_ascii_alphanumeric() || c == b'_';
    let mut from = 0usize;
    while let Some(k) = line[from..].find("ccm") {
        let at = from + k;
        let left_ok = at == 0 || !word(b[at - 1]);
        let right = at + 3;
        let right_ok = right >= b.len() || !word(b[right]);
        if left_ok && right_ok {
            return true;
        }
        from = at + 3;
    }
    false
}

/// 这一行是不是 cc-monitor 自己的围栏标记（三对全认）。
///
/// 认的是**共同前缀** `# === cc-monitor`，而不是三对里的某一对 —— 三对分别是
/// `profile_installer` 的 `BEGIN_MARKER`、`sftp` 的 `CCM_PROFILE_BEGIN`、
/// `account_aliases` 的 `RC_BEGIN`。这一格问的是「这一行是不是**我们的**边界」，
/// 那个答案对三对是同一个。
fn fence_marker(line: &str) -> Option<bool> {
    let l = line.trim_start();
    if !l.starts_with("# === cc-monitor") {
        return None;
    }
    if l.contains("BEGIN") {
        Some(true)
    } else if l.contains("END") {
        Some(false)
    } else {
        None
    }
}

/// 🔴 `KR62D2` 的正题：**扫一份 POSIX rc 里围栏之外的裸行。**
///
/// # 为什么不是「给 `scan_legacy_profiles` 的路径表加两行」
///
/// 那个函数认的是 [`find_block_version`]（**围栏**）。而 `K-R57` 现打用户本机：
/// `~/.bashrc` 三种围栏**全部零命中**，那 14 行 ccm 相关**全是裸写的**
/// ⇒ **加路径解决不了「够不着裸行」**，只会让读数看起来像做完了。
/// ⇒ 这里换的是**判法**：按行走、跳过我们自己的围栏段、按**词**认 `ccm`。
///
/// # 它诚实的边界（写出来，别读大）
///
/// - 它认的是「**提到 ccm**」，不是「**这一行是旧的**」。一个在自己函数里调 `ccm` 的用户
///   （`shared/ccm-aliases.sh` 头注逐字鼓励这么做）也会被指名 —— 所以产物是
///   [`render_manual_cleanup_hint`] 那种「你自己定」的措辞，**不是** 「请删除」。
/// - 形状按 `名字() {` 认函数（同 `sftp::builtin_alias_names` 那一形）。
///   `function cc { … }` 这一写法会落进 [`LegacyRcKind::Other`] —— **漏的是分类，不是那一行**，
///   它仍然被指名。
pub fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {
    let mut out = Vec::new();
    let mut inside = false;
    for (i, line) in content.lines().enumerate() {
        if let Some(open) = fence_marker(line) {
            inside = open;
            continue;
        }
        if inside || !mentions_ccm(line) {
            continue;
        }
        let l = line.trim_start();
        let (kind, name) = if l.starts_with('#') {
            (LegacyRcKind::Comment, None)
        } else if let Some(n) = function_name_of(l) {
            (LegacyRcKind::Function, Some(n))
        } else {
            (LegacyRcKind::Other, None)
        };
        out.push(LegacyRcLine {
            line_no: i + 1,
            text: line.to_string(),
            kind,
            name,
        });
    }
    out
}

/// `名字() {` ⇒ `Some("名字")`。形状与 `sftp::builtin_alias_names` 认的那一形逐字相同。
fn function_name_of(l: &str) -> Option<String> {
    let (name, rest) = l.split_once("()")?;
    if !rest.trim_start().starts_with('{') {
        return None;
    }
    let name = name.trim();
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| name.to_string())
}

/// 🔴 `KR62D2` 的产物：**一段让用户自己动手的提示。** 没有要清的就是空串。
///
/// **产品一个字节都不删**（`K31` + 用户逐字「原本的配置要手动删除」）。
/// 措辞刻意不是「请删除」：见 [`scan_legacy_rc_lines`] 的诚实边界那一节。
///
/// 「哪几行会把我们装的那块遮蔽掉」现算自 `sftp::builtin_alias_names()`
/// （= `shared/ccm-aliases.sh` 本身），**这里不抄一份名字清单**。
pub fn render_manual_cleanup_hint(what: &str, hits: &[LegacyRcLine]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let builtin = crate::sftp::builtin_alias_names();
    let mut shadowed = 0usize;
    let mut body = String::new();
    for h in hits {
        let shadow = h
            .name
            .as_deref()
            .is_some_and(|n| builtin.iter().any(|b| *b == n));
        if shadow {
            shadowed += 1;
        }
        body.push_str(&format!(
            "  第 {} 行  {}{}\n",
            h.line_no,
            h.text.trim_end(),
            if shadow {
                "     ← 会赢过我们那一块"
            } else {
                ""
            }
        ));
    }
    let mut out = format!(
        "{what} 里有 {} 行提到 ccm，而它们都在 cc-monitor 的围栏之外。\n\
         围栏删法够不着裸行 —— 边界在哪只有你知道，所以 cc-monitor 一个字节都不会碰它们。\n\
         要不要删、删哪几行，由你自己定：\n\n{body}",
        hits.len()
    );
    if shadowed > 0 {
        out.push_str(&format!(
            "\n标了「会赢过我们那一块」的那 {shadowed} 行：cc-monitor 装的别名块用 `declare -f` \
             让着你已有的同名函数，而你这几行在它前面 —— 不删掉它们，装了那一块也不生效。\n"
        ));
    }
    out.push_str(&format!(
        "\n动手的地方：用你自己的编辑器打开 {what}，删掉你决定不要的那几行，再开一个新终端。\n"
    ));
    out
}

/// 解析当前用户实际安装的 PS profile 路径。
///
/// **关键**：用 `Microsoft.PowerShell_profile.ps1`（`$PROFILE` 默认指向，即
/// CurrentUserCurrentHost）而非 `profile.ps1`（CurrentUserAllHosts，对所有
/// host 包括 ISE / VSCode 集成 terminal 生效，但绝大多数用户不用这个）。
///
/// v1.7.0-1.7.1 错用 `profile.ps1` → PowerShell 启动时根本不读那个文件 →
/// cc 集成形同虚设。v1.7.2 修正到默认 `$PROFILE`。
///
/// **自动识别**：
///   - PS 5.1 永远显示（Windows 自带）
///   - PS 7.x 只在 `Documents/PowerShell/` 目录存在时显示（说明用户装过且至少跑过一次）
/// **路径围栏：profile 只能落在用户 home 之内**〔audit-0805 08-08，Phase G 第 86 件〕。
///
/// # 为什么需要它
///
/// 三条 `cc_integration_*` 命令收的是 **webview 给的字符串**（前端那一格是用户可输入的
/// 文本框），此前**原样** `PathBuf::from` 就交给了安装器：`install` 往那里写、
/// 文件不存在还会创建；`uninstall` 会重写它；`scan_path` 是任意路径的存在性/大小探针。
/// 而**远端**那条同名功能一直有围栏（`sftp.rs`：「profile 只能是 home 下的文件名」）。
///
/// # 为什么是「home 之内」而不是「home 下的裸文件名」
///
/// [`discover_profiles`] 自己就会返回 `~/WindowsPowerShell/…ps1` 这种**子目录**里的路径，
/// 而 [`ProfileKind::Custom`] 是产品特性（用户可以指 `~/.config/fish/config.fish`）。
/// ⇒ 围栏只挡「跑出 home」这一类，**不缩小功能**。
///
/// 三条规则：① `~` / `~/x` 先展开（用户会手打这种）；② 必须是绝对路径；
/// ③ 不许含 `..`（不做「消解后再看」——直接拒绝更简单也更难绕）；④ 前缀必须是 home。
/// 另外：父目录若已存在，用它的 canonical 形态再查一次前缀 —— 挡掉
/// `~/link -> /etc` 这种**符号链接逃逸**（`install` 会跟着链接写过去）。
pub fn fence_profile_path(raw: &str) -> Result<PathBuf, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "找不到 home 目录 —— 拒绝写任何 profile".to_string())?;
    fence_path_under(&home, raw)
}

/// 同一道围栏，**home 由调用方给**。
///
/// ⚠ `K-R49` 起抽出这一层，理由是**可测性**而不是通用性：调用方拿一个临时目录当 home，
/// 围栏的四条规则就能在**碰不到真实家目录**的前提下被真跑一遍
/// （〔用 08-29〕「你只能做产品, 不能动机器」）。上面那个入口一个字节的语义都没变 ——
/// 它只是把 `dirs::home_dir()` 填进来。
pub fn fence_path_under(home: &std::path::Path, raw: &str) -> Result<PathBuf, String> {
    let home = home.to_path_buf();
    let expanded: PathBuf = if raw == "~" {
        home.clone()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    };
    if !expanded.is_absolute() {
        return Err(format!(
            "refuse profile path: 必须是绝对路径（实得 {raw:?}）"
        ));
    }
    if expanded
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("refuse profile path: 不许含 `..`（实得 {raw:?}）"));
    }
    if !expanded.starts_with(&home) {
        return Err(format!(
            "refuse profile path: 只能落在 home 之内（实得 {raw:?}，home 是 {home:?}）"
        ));
    }
    // 符号链接逃逸：父目录已存在时用它的真身再查一次。
    if let Some(parent) = expanded.parent() {
        if let (Ok(real_parent), Ok(real_home)) = (parent.canonicalize(), home.canonicalize()) {
            if !real_parent.starts_with(&real_home) {
                return Err(format!(
                    "refuse profile path: 父目录经符号链接跑出了 home（{raw:?} → {real_parent:?}）"
                ));
            }
        }
    }
    Ok(expanded)
}

pub fn discover_profiles() -> Vec<(ProfileKind, PathBuf)> {
    let Some(home) = dirs::document_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    out.push((
        ProfileKind::Ps51,
        home.join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
    ));
    let ps7_dir = home.join("PowerShell");
    if ps7_dir.exists() {
        out.push((
            ProfileKind::Ps7,
            ps7_dir.join("Microsoft.PowerShell_profile.ps1"),
        ));
    }
    out
}

/// v1.7.0-1.7.1 错位 profile 路径（已废弃，仅用于检测是否有遗留块需要清理）。
fn legacy_profile_paths() -> Vec<(ProfileKind, PathBuf)> {
    let Some(home) = dirs::document_dir() else {
        return Vec::new();
    };
    vec![
        (
            ProfileKind::Ps51,
            home.join("WindowsPowerShell").join("profile.ps1"),
        ),
        (
            ProfileKind::Ps7,
            home.join("PowerShell").join("profile.ps1"),
        ),
    ]
}

/// 扫所有 v1.7.0-1.7.1 错位 profile 文件，看哪些含 cc-monitor 块（需要用户手动清理）。
pub fn scan_legacy_profiles() -> Vec<(ProfileKind, String)> {
    let mut out = Vec::new();
    for (kind, path) in legacy_profile_paths() {
        if !path.exists() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        // 〔`K-R132`〕同 `scan_profile`：剥 BOM 再判围栏。
        let (has_block, _) = find_block_version(strip_bom(&raw));
        if has_block {
            out.push((kind, path.to_string_lossy().into_owned()));
        }
    }
    out
}

/// 扫描任意路径的 profile 文件（v1.7.2 用户自定义路径用）。kind 字段标 Custom。
pub fn scan_path(path: &PathBuf, command_name: &str) -> ProfileScan {
    scan_profile(ProfileKind::Custom, path, command_name)
}

/// 扫描一个 profile 文件：是否存在 / 是否含 ccm 块 / 检测命令名冲突 /
/// 〔`K-R62`〕POSIX rc 上那几行**围栏之外的裸行**。
///
/// 🔴 **只读。** 它一个字节都不写 —— `K31`（只能做产品，不能动机器）在这一条上尤其硬：
/// 「查」这一半的全部产物是文字（[`ProfileScan::manual_cleanup_hint`]），动手的是用户。
pub fn scan_profile(kind: ProfileKind, path: &PathBuf, command_name: &str) -> ProfileScan {
    let path_str = path.to_string_lossy().into_owned();
    if !path.exists() {
        return ProfileScan {
            kind,
            path: path_str,
            exists: false,
            has_ccm_block: false,
            ccm_block_version: None,
            conflicting_functions: Vec::new(),
            manual_cleanup_hint: String::new(),
            size_bytes: 0,
        };
    }
    let flavor = flavor_of(path);
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    // 〔`K-R132`〕BOM 剥在最靠近读的那一跳 —— 不剥，`find_block_version` 的
    // `strip_prefix(BEGIN_MARKER)` 会在第一行就对不上前缀 ⇒ 界面说「未安装」。
    let content = strip_bom(&raw).to_string();
    // `size_bytes` 报的是**盘上那份**的大小（含 BOM）—— 它是给人看「这文件多大」的，
    // 不是内容判定的输入。
    let size_bytes = raw.len() as u64;
    let (block_present, block_version) = block_presence(flavor, &content);
    let conflicts = find_conflicting_functions(flavor, &content, command_name);
    let manual_cleanup_hint = match flavor {
        ProfileFlavor::PowerShell => String::new(),
        ProfileFlavor::PosixRc => {
            render_manual_cleanup_hint(&path_str, &scan_legacy_rc_lines(&content))
        }
    };
    ProfileScan {
        kind,
        path: path_str,
        exists: true,
        has_ccm_block: block_present,
        ccm_block_version: block_version,
        conflicting_functions: conflicts,
        manual_cleanup_hint,
        size_bytes,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 🔴 `K-R132`：**装上了、能跑、用户敲不到** —— 这一段就是那条缺陷的修法
// ═══════════════════════════════════════════════════════════════════════════
//
// `K-R129` 在真机上（干净本地用户 · Release 上 `v3.8.0` 那份字节）实敲
// 三条安装路 × 三种 shell × 三个名字，**每一格都找不到**；而 `ccm.exe` 真在
// `%USERPROFILE%\.cc-monitor\bin\`。⇒ 唯一原因是**那个目录不在 PATH 上**。
//
// **病因不是 v3.8.0 的回归，是从来就没有过这个机制**：全仓一个 Windows PATH
// 写入点都没有（`setx` / `SetEnvironmentVariable` / `EnvVarUpdate` / `AddToPath`
// 扫 `*.rs *.ts *.nsi *.nsh *.wxs *.ps1 *.json` ⇒ 命中 0，量于本树 `95b6c93`）。
//
// ## 为什么补在这里，而不是补进安装器（`.nsi` / `.wxs`）
//
// 用户 `K33` 逐字：「**如果有需要动用户 alias 的就生成命令让用户自己填。
// 像是原本的填 PowerShell profile 和 bashrc 一样。**」
// ⇒ **产品生成、用户应用**，不是产品替用户改他的环境（同向 `INVARIANTS.md §41.6`）。
// 而「生成一段让用户装进自己 profile 的东西」这台机器**今天就在这里**
// （[`install_to_profile`]：两种方言 · 备份 → 原子替换 → 读回逐字比对 → 不符回滚）
// ⇒ 照 `K-R62` 那条走：**让这台已有的安装器多吐一行 PATH，不新起第四套**。
//
// ## 🔴 〔`R86` 09-15 用户裁〕**那条路后来被裁掉了 —— 这张表记的正是它为什么不够**
//
// | shell | 读不读 PowerShell `$PROFILE` | 往 profile 里塞一段 PATH 管不管用 |
// |---|---|---|
// | PowerShell | 读 | 管 —— 但**只管这一个进程** |
// | `cmd` | **不读**（它根本没有 profile 这个概念） | **不管** |
// | Git Bash | 不读（它读 `~/.bashrc`，那是 POSIX 那一臂） | 不管 |
//
// ⇒ 三格里只有一格通，而**「一格通、两格不通」比「三格都不通」更难查** ——
// 用户会把它读成「装好了」。⇒ 用户当场裁掉这条路（`R86` 逐字「既然要加用户 path，
// 这个就没用了」）：**要让三格都通只有改用户级 PATH 一条路**，
// 而那一步**由用户点一下**（`R85` 逐字「应该让用户手动点击加，也能管理删除」）。
// 命令文本住 [`render_user_path_setup_command`] 与 [`render_user_path_removal_command`]，
// 状态那一格住 [`user_path_has_our_bin`]。

// ═══════════════════════════════════════════════════════════════════════════
// 🔴 `K-R132` 真机现打逮到的**第二条缺陷**：BOM-less UTF-8 ＋ PS 5.1 ＝ 吞掉一行
// ═══════════════════════════════════════════════════════════════════════════
//
// ## 它是怎么被逮到的（不是推理出来的，是先修不动才回头查的）
//
// 本件先把 PATH 那一段加进块里，在 win11 真机上装好、开一个新 PowerShell ——
// **`ccm` 照旧找不到**。回头查：`$ccmBinDir` 是空的，而 `$env:PATH` 前面多了一个 `;`
// ⇒ 赋值那一行**根本没执行**，而 `if` 那一行执行了。
//
// 用 PowerShell 自己的 `Get-Content` 读回来（真机逐字，住 `evidence/K-R132-摸底.md`）：
//
// ```text
// line 88 : # cc-monitor锛氳 `ccm` 鍦ㄨ繖涓?PowerShell …銆擪-R132銆曘€?$ccmBinDir = Join-Path …
// ```
//
// **注释行与它下面那一行被并成了一行** ⇒ 赋值落进了注释里。
//
// ## 机制
//
// [`atomic_write_string`] 走 `std::fs::write`，写的是**不带 BOM 的 UTF-8**。
// 而 **Windows PowerShell 5.1 把不带 BOM 的 `.ps1` 按系统 ANSI 代码页解**
// （这台机器上是 GBK）。一个 UTF-8 的 CJK 字符被当成 GBK 解，末尾会剩下一个
// **落单的前导字节**，它把紧随其后的换行吃掉 ⇒ 下一行被并进注释。
//
// ## 🔴 它**不是本件引入的** —— 发出去的 `v3.8.0` 上就有，而且更重
//
// 同一趟真机现打模板本身（`scripts/cc.ps1.tpl`，逐字读数同住那份 evidence）：
// PowerShell 的分析器在 `__ccm_bind` 里看到的 `$ccmDir` / `$deadline` / `$oldTitle`
// **各少一处赋值** —— 它们的赋值行都紧跟在一行 CJK 注释后面，全被吞了。
// ⇒ 在 CJK 代码页的 Windows 上，**「终端集成」那套窗口绑定一直是坏的**，
// 而它坏得很安静（`parse-errors=0`，没有任何报错）。
//
// ⚠ **分母**：我量的是**一台**机器（win11 VM，系统代码页 GBK）。
// 「所有 CJK locale 都这样」我没量，别那么写；英文 locale（代码页 1252）上
// 这条机制不成立 —— 那也正是它一直没被发现的原因。
//
// ## 修法：**只给 PowerShell 那一支加 BOM**
//
// BOM 是 Microsoft 自己对 PS 5.1 脚本的建议编码，PS 5.1 / PS 7 / Notepad / VSCode
// 都认。⚠ **不能加在 [`atomic_write_string`] 里** —— 那个函数还有两个调用方
// （`mcp.rs` 写 `.mcp.json`、`account_aliases.rs` 写一份 shell 脚本），
// 给 JSON 和 `.sh` 加 BOM 是往别人身上引入同族的病。
// ⇒ 分岔点放在**方言**这一层（[`encode_for_disk`] / [`strip_bom`]），与
// 「写什么」那一处分岔（[`plan_install`]）同一条线。

/// UTF-8 BOM。**闭集只有这一处住址**〔`13b`〕。
const UTF8_BOM: &str = "\u{feff}";

/// 读进来的那一份：把 BOM 剥掉再交给任何**判内容**的东西。
///
/// 🔴 不剥会坏两件事：① `find_block_version` 按 `strip_prefix(BEGIN_MARKER)` 认围栏，
/// 而 `\u{feff}# === cc-monitor BEGIN` 前缀对不上 ⇒ 界面说「未安装」、藏起卸载按钮，
/// 点安装却报行号（那正是 T04 审计③ 治过的那一形）；② BOM 会被当成用户内容
/// 原样写回文件中间。
fn strip_bom(s: &str) -> &str {
    s.strip_prefix(UTF8_BOM).unwrap_or(s)
}

/// 落盘的那一份：PowerShell 方言加 BOM，POSIX rc **一个字节都不加**。
///
/// POSIX 那一侧为什么绝不能加：`.bashrc` 开头多三个字节，`sh` 会把它当成命令
/// （`$'﻿': command not found`），而更坏的是 `#!/bin/sh` 这一形会整个失效。
fn encode_for_disk(flavor: ProfileFlavor, content: &str) -> String {
    match flavor {
        ProfileFlavor::PowerShell => format!("{UTF8_BOM}{content}"),
        ProfileFlavor::PosixRc => content.to_string(),
    }
}

/// 本机 `ccm` 入口所在目录的 **Windows 写法**（`%USERPROFILE%` 之下的相对路径）。
///
/// 目录本身取自 [`crate::tool_registry::local_ccm_bin_dir_rel`]（唯一住址是那张表），
/// 这里只做一件事：把 `/` 换成 `\`。**本函数体内没有任何目录字面量。**
// 🔴 〔`K-R135` 09-15〕**没有生产调用方是暂时的，而且原因是写区** ——
// 它的调用方是那一条新 IPC 命令（界面那一格「加 / 撤 / 现在状态」），
// 而登记一条 IPC 命令要同拍动 `lib.rs` 的 `generate_handler!` 与
// `parity_ledger.rs` 那张双向相等的表，**两份都在 `K-R135` 的写区之外**。
// ⇒ 本件把机制与判据做完，那一跳交回 PM（件文件 `§8`）。
// ⚠ **接上去的那一拍要把这个 `allow` 摘掉** —— 留着它，下一次真的没人用了也看不出来。
#[allow(dead_code)]
fn ccm_bin_dir_windows() -> Option<String> {
    crate::tool_registry::local_ccm_bin_dir_rel().map(|d| d.replace('/', "\\"))
}

// 🔴 〔`R86` 09-15 用户裁〕**这里原本住着两个函数，本件把它们整个删掉了** ——
// 一个把我们那个 bin 目录塞进**本会话**的 `$env:PATH`，一个给它配一段说明注释。
// 用户逐字：「既然要加用户 path，这个就没用了。」
// ⚠ 旧名字刻意不写在这里：`structural_scan` 有一条判据在数「散文点名了一个代码里
// 根本不存在的符号」，而**指向不存在的判据比没有注释更坏**（testing.md 诚实边界 4）。
// 要查它们逐字长什么样，去 `git log -S` 那两个函数体，或读 `K-R132` 的件文件。
//
// ## PM 认，而且理由比「没用了」更硬一条：**留着它会一直制造那个「半通」状态**
//
// 那一段只动**这个 PowerShell 进程自己**的 `$env:PATH` ⇒
// `K-R129` / `K-R132` 两轮真机 3×3 表里最难看的那一格（**PowerShell 里能、`cmd` 里不能**）
// 就是它造出来的。**「看起来装好了、换个终端就没了」比「哪儿都没有」更难查。**
// ⇒ 删掉之后状态变**二值**：没点按钮 ⇒ 哪儿都敲不到；点了 ⇒ 哪儿都敲得到。
//
// ## 🔴 停止生成 ≠ 已有用户 profile 里那一段会自己消失
//
// `v3.8.0` 与 `main` 现在这一版**已经会往用户 profile 里写那一段**。现成机制接得住：
// 那一段住在 `BEGIN/END` 围栏里，[`install_to_profile`] **每次装 / 修复整块重写**、
// [`uninstall_from_profile`] 整块摘掉 ⇒ **只要用户再走一次装或卸，旧那段就没了。**
// ⚠ **但「一直没再装过」的用户会留着它** —— 它本身无害（只是把一个目录再前置一次），
// **可对没点过按钮的那位用户，半通状态会继续保留。别当它自动消失。**
//
// ⚠ 不为它做「检测到旧块」的提示，理由是**那条提示够不着它要治的人**：
// 会看见提示的人是**打开了这一格**的人，而那位用户点一下「加」就两头都好了；
// 真正留着旧块的是**再也没打开过这个面板**的人 —— 提示对他恒不可见。
// ⇒ 多一条恒静默的 UI 不如把状态做成二值。

/// 🔴 `KR132D1` 的结论落地：**让用户自己跑一次的那条命令**（路线①，用户级 PATH）。
///
/// 用户 `K33` 逐字「**生成命令让用户自己填**」⇒ 产品**只生成这段文字**，一个字节都不执行。
/// 这是三种 shell 里唯一一条 `cmd` 也认的路（`cmd` 不读任何 profile）。
///
/// # 两个经典地雷，这条命令都绕开了 —— 而绕开的方式是判据在数的
///
/// 1. **不用 `setx`。** `setx` 把值截断在 1024 字符，而它给的提示是一句警告、
///    退出码照样 0 ⇒ 一条**静默截断用户 PATH** 的命令。
/// 2. **只读 `'User'` 那一档，不读 `$env:PATH`。** 进程里的 `$env:PATH` 是
///    **机器级 ＋ 用户级拼起来的那一份**；拿它当新值写回用户级，会把整条系统 PATH
///    **复制进用户 PATH**（此后系统 PATH 的任何更新对这个用户都不再生效）。
///    这两条一起构成了 Windows 上「改 PATH 改坏机器」的绝大多数病例。
///
/// # 它不写 `'Machine'`
///
/// 机器级要管理员，而且卸载时留下的垃圾是**全机**的。用户级不需要管理员，
/// 卸载时也只影响这一个用户。
///
/// ⚠ **要重开终端才生效** —— 已经开着的进程拿的是自己启动那一刻的环境块副本。
/// 这句话**刻意不在这段命令里**（这里只放**能跑的那几行**）——
/// 它归**显示这段命令的那一侧**（界面那一格）去配说明。
/// 别把说明混进可执行文本里：混进去，用户复制一整段就会连注释一起跑。
// 🔴 〔`K-R135` 09-15〕**没有生产调用方是暂时的，而且原因是写区** ——
// 它的调用方是那一条新 IPC 命令（界面那一格「加 / 撤 / 现在状态」），
// 而登记一条 IPC 命令要同拍动 `lib.rs` 的 `generate_handler!` 与
// `parity_ledger.rs` 那张双向相等的表，**两份都在 `K-R135` 的写区之外**。
// ⇒ 本件把机制与判据做完，那一跳交回 PM（件文件 `§8`）。
// ⚠ **接上去的那一拍要把这个 `allow` 摘掉** —— 留着它，下一次真的没人用了也看不出来。
#[allow(dead_code)]
pub fn render_user_path_setup_command() -> Option<String> {
    let dir = ccm_bin_dir_windows()?;
    Some(format!(
        "$d = Join-Path $env:USERPROFILE '{dir}'\n\
         $p = [Environment]::GetEnvironmentVariable('Path', 'User')\n\
         if (($p -split ';') -notcontains $d) {{\n\
         \x20   [Environment]::SetEnvironmentVariable('Path', (@($p, $d) | Where-Object {{ $_ }}) -join ';', 'User')\n\
         }}\n"
    ))
}

/// 🔴 `KR135D1` 的另一半（`R85` 逐字「**也能管理删除**」）：**把我们那一段从用户级
/// PATH 上摘掉的那条命令。** 「加」是上面那一条，这一条是「撤」。
///
/// # 🔴 撤这一侧会**再撞一次**同样那两个地雷 —— 逐条写出它是怎么绕的
///
/// 1. **不用 `setx`**：它把值截断在 1024 字符，提示只是一句警告、**退出码照样 0**。
///    ⚠ 撤这一侧踩它的后果比加那一侧**更重**：加那一次截断掉的是「多出来的尾巴」，
///    而撤是**整条重写**，截断掉的是用户本来就有的那一截。
/// 2. **读回来的是 `'User'` 那一档，不是 `$env:PATH`。** 进程里那份是机器级 ＋ 用户级
///    拼起来的；撤的时候拿它当基准写回用户级，后果与加那一侧**一模一样**
///    —— 整条系统 PATH 被复制进用户 PATH。
///    ★ **撤比加更容易踩这一个**，因为「先读一下现在的 PATH」在撤的语境里读起来非常顺手。
///
/// # 🔴 第三条，它是撤这一侧**独有**的：**只摘自己那一段，不碰别的**
///
/// 过滤条件 `-ne $d` 是**整格**比较（PowerShell 的 `-ne` 对字符串默认大小写不敏感，
/// 与加那一侧的 `-notcontains` 是**同一种相等**）⇒ 摘掉的**恰好**是我们加进去的那一格。
///
/// **刻意不用** `-replace` / `-like` / `.Replace()` —— 那三种都是**子串**口径：
/// 用户 PATH 上存在 `…\.cc-monitor\bin-old`（任何以我们那一段为前缀的目录）时会被一起打掉，
/// 而那正是「碰了用户 PATH 里别的东西」。同一个病在加那一侧的形状是「误判成已经在了」，
/// 在撤这一侧的形状是**误删用户的目录** —— 后者不可逆。
///
/// ⚠ **连空项都不许顺手清**：过滤条件里**只有** `-ne $d` 一条，**没有**
/// `Where-Object {{ $_ }}` 那种「顺手把空项也滤掉」。PATH 上的空项在 Windows 上
/// 语义是「当前目录」，删掉它是一次**我们没被要求做的改动**（哪怕它算个改进）。
/// ⇒ 除我们那一格之外**逐字复原**，这是「只摘自己那一段」的字面意思。
///
/// ⚠ 摘完剩下空串是**正常**的：`.NET` 把空串当「删掉这个变量」，
/// 而那正是「我们那一格是用户级 PATH 上唯一一格」时该有的结果 —— 不是清空了用户的 PATH
/// （**机器级那一档一个字节都没碰**，用户下次开终端仍然有完整的系统 PATH）。
///
/// ⚠ **要重开终端才看得到** —— 与加那一侧同理，已经开着的进程拿的是自己启动那一刻的
/// 环境块副本。这句话不放进可执行文本里（放进去，用户复制一整段就会连注释一起跑）。
// 🔴 〔`K-R135` 09-15〕**没有生产调用方是暂时的，而且原因是写区** ——
// 它的调用方是那一条新 IPC 命令（界面那一格「加 / 撤 / 现在状态」），
// 而登记一条 IPC 命令要同拍动 `lib.rs` 的 `generate_handler!` 与
// `parity_ledger.rs` 那张双向相等的表，**两份都在 `K-R135` 的写区之外**。
// ⇒ 本件把机制与判据做完，那一跳交回 PM（件文件 `§8`）。
// ⚠ **接上去的那一拍要把这个 `allow` 摘掉** —— 留着它，下一次真的没人用了也看不出来。
#[allow(dead_code)]
pub fn render_user_path_removal_command() -> Option<String> {
    let dir = ccm_bin_dir_windows()?;
    Some(format!(
        "$d = Join-Path $env:USERPROFILE '{dir}'\n\
         $p = [Environment]::GetEnvironmentVariable('Path', 'User')\n\
         $kept = @($p -split ';' | Where-Object {{ $_ -ne $d }})\n\
         [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')\n"
    ))
}

/// 🔴 `KR135D1` 的第一样：**`~/.cc-monitor\bin` 在不在用户级 PATH 上。**
///
/// 这是那一格的**判定内核**（`现算，不缓存` 指的是调用方每次重新读一遍 `'User'`
/// 那一档再问它，本函数自己不持有任何状态）。
///
/// - `user_path_raw`：从 **`'User'` 那一档**读回来的原始串。
///   🔴 **不许传 `$env:PATH`**（`§0b` 第 2 条）—— 那一份是机器级 ＋ 用户级拼起来的，
///   拿它判「在不在**用户级**上」会把「机器级上有」读成「用户级上有」，
///   于是「撤」那个按钮点下去什么都没发生，而界面还说它撤掉了。
/// - `dir_abs`：`%USERPROFILE%` 展开之后我们那个目录。
///
/// # 🔴 相等口径与生成的那两条命令**必须是同一种**，否则界面会撒谎
///
/// 加那一条用 `-notcontains $d`、撤那一条用 `-ne $d`，两者在 PowerShell 里都是
/// **整格 · 大小写不敏感**。这里逐字照同一种：按 `;` 切开比**整格**，
/// 用 `eq_ignore_ascii_case`。
///
/// ⚠ **刻意不做路径规范化**（不砍末尾 `\`、不解析 `..`、不 `canonicalize`）——
/// 理由是**一致压过聪明**：规范化之后本函数会说「已经在了」，而那两条命令
/// （它们不规范化）会说「不在」⇒ 界面显示 ✓、点一下却又多出一格重复。
/// **两边同样地笨，比一边聪明一边笨要好。**
/// ⇒ 代价如实写在这里：用户手写过一格**带末尾反斜杠**的同一个目录时，这一格显示「不在」，
/// 点「加」会再插一格。要治它得三处一起改（本函数 ＋ 两条命令），不许只改这一处。
///
/// ⚠ `dir_abs` 为空时**恒回 `false`** —— 不然 PATH 上任何一个空项（连着两个 `;`）
/// 都会与它相等，于是「拿不到目录」会被读成「已经装好了」。
// 🔴 〔`K-R135` 09-15〕**没有生产调用方是暂时的，而且原因是写区** ——
// 它的调用方是那一条新 IPC 命令（界面那一格「加 / 撤 / 现在状态」），
// 而登记一条 IPC 命令要同拍动 `lib.rs` 的 `generate_handler!` 与
// `parity_ledger.rs` 那张双向相等的表，**两份都在 `K-R135` 的写区之外**。
// ⇒ 本件把机制与判据做完，那一跳交回 PM（件文件 `§8`）。
// ⚠ **接上去的那一拍要把这个 `allow` 摘掉** —— 留着它，下一次真的没人用了也看不出来。
#[allow(dead_code)]
pub fn user_path_has_our_bin(user_path_raw: &str, dir_abs: &str) -> bool {
    if dir_abs.is_empty() {
        return false;
    }
    user_path_raw
        .split(';')
        .any(|seg| seg.eq_ignore_ascii_case(dir_abs))
}

/// 生成将要写入的代码（替换 placeholder）。
///
/// - `include_cc_function = true`：装 `__ccm_bind` helper **加上** `function {name}`
///   （适合 profile 里没有自定义 cc 的新用户，一键 work）。
/// - `include_cc_function = false`：只装 `__ccm_bind` helper（适合用户已有自定义
///   `function cc`——避免覆盖用户原有 cd/代理/etc 逻辑，用户自己在 cc 开头加
///   `__ccm_bind` 一行调用即可）。
///
/// 🔴 〔`R86` 09-15〕**PATH 那一段不在这里了** —— 生成它的那两个函数整个删了
/// （理由住本文件上面那段横幅）⇒ 这一块现在**只装 `cc`**。
/// 用户级 PATH 那件事换成**用户点一下**（`R85`），命令文本由
/// [`render_user_path_setup_command`] / [`render_user_path_removal_command`] 生成。
///
/// 🔴 〔`KR135D2` 09-15〕**`cc` 翻正了：它现在走 `ccm`，不再直呼 `claude`。**
///
/// 上一轮（`K-R132`）这一行是 `& claude $RemainingArgs`，并被登记成一处**反向锚点** ——
/// 那不是遗忘，是刻意钉住的一处不一致。本轮把它翻正，它当初那三条暂缓理由逐条到期：
///
/// 1. `K33` 逐字「**所有命令只许有一处**，其他都是根据传参来调用」＋ `K28`
///    「前端不许自己发明对外行为 —— 一切对外都经后端」⇒ 答案本来就没有悬念。
/// 2. 翻正之前，同一个名字 `cc` 在 PowerShell 与 POSIX
///    （`shared/ccm-aliases.sh` 的 `cc() { ccm "$@"; }`）上是**两个不同的东西**：
///    账号 / 工作目录 / agent 选择这几维在 Windows 上整条够不着。
/// 3. 它第一条暂缓理由是「`& ccm` 要 `ccm` 找得到，而那个前提刚修完没复验」——
///    本件把那个前提从「profile 里一段只管本进程的 PATH」换成**用户级 PATH**
///    （`R85` 那一下点击）⇒ 前提换了一个，不再压在会话级那一段上。
///
/// ⚠ **诚实边界，别读大**：本件**没有**在真机上把 `cc` 真跑过一趟。这一行今天证得住的
/// 只有「**它生成的文本指向 `ccm`**」，证不了「敲下去真起得来」。
///
/// 🔴 〔`KR132D3` 另一半，本轮**复打后维持**〕**`cct` 这一臂刻意不生成。**
/// POSIX 那边 `cct() { ccm --tmux "$@"; }`，而 **Windows 上没有 tmux** ⇒ 给它一个
/// 「名字在、行为不在」的壳比没有更坏（`K-R129` 那位用户正是照文案敲了 `cct`）。
///
/// ⚠ **`K-R132` 把「把 `cct` 从 Windows 文案里摘掉」随动到 `src/launcher-diagnostics.ts`
/// 那一句上 —— 本轮现打，那个随动的前提是假的**：那一句只在 **POSIX rc** 那一臂印
/// （它的下拉只遍历 `AccountAliasReport::rc_candidates`，而那张表现算自
/// `account_aliases::RC_CANDIDATES`，**一份 PowerShell profile 都没有**），
/// 而那一臂的 `cct` 是**真有**的（`shared/ccm-aliases.sh` 里就定义着）。
/// PowerShell 那一臂是另一份文件（`src/settings/cc_integration.ts` 的 `renderScanResult`），
/// 现打 `cct` **零命中**。⇒ **Windows 文案里今天一个 `cct` 都没有，没有东西要摘。**
/// 读数 · 量法 · 分母住 `evidence/K-R135-摸底.md`。
pub fn render_cc_code(command_name: &str, include_cc_function: bool) -> String {
    let safe_name = sanitize_command_name(command_name);
    let cc_block = if include_cc_function {
        // 🔴 `KR135D2`：**这一行就是翻正的落点。** `{word}` 现算自 `CCM_ENTRY_WORD`
        // （`13b`：那个词的唯一住址），不写第二份字面量。
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        format!(
            "\nfunction {safe_name} {{\n    [CmdletBinding()] param(\n        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs\n    )\n    __ccm_bind\n    & {word} $RemainingArgs\n}}\n"
        )
    } else {
        String::new()
    };
    // 〔`R86`〕这里原先是一个三项拼装：会话级 PATH 那一段 ＋ 它的说明注释 ＋ `cc` 那一块。
    // 前两项删了（理由住上面那段横幅），于是**装进 profile 的东西只剩 `cc` 那一块**。
    CC_TEMPLATE.replace("{{CC_FUNCTION_BLOCK}}", &cc_block)
}

/// idempotent 安装：把 cc function 块写到 profile，已有 ccm 块则原地替换。
/// 用户在 BEGIN/END 块外的内容完全不动。
///
/// `include_cc_function = false` 时只装 `__ccm_bind` helper，不抢 cc function 名。
///
/// v1.7.10 安全加固（修 v1.7.9 留下的"profile 写坏"事故）：
///  1. 文件存在时**必先备份**到 `<path>.ccm-backup-<ms>` 再动笔
///  2. 写入走 `MoveFileExW(REPLACE_EXISTING)` 真原子（之前是 remove + rename
///     非原子，rename 失败会留下空文件 + 原内容丢失）
///  3. 写完立即 read 回来校验长度 == 期望长度；不匹配从 backup 回滚
///  4. 若 `path.exists()` 但 read 出空字符串（OneDrive placeholder / 文件锁等
///     罕见情况），**直接 abort 不写**——避免 existing="" + 块追加 = 用户内容被冲掉
pub fn install_to_profile(
    path: &PathBuf,
    command_name: &str,
    include_cc_function: bool,
) -> Result<(), String> {
    // 〔`K-R62`〕**写什么**按方言分岔（见 [`plan_install`]），
    // **怎么落盘**这一整套（备份 → 原子替换 → 读回逐字比对 → 不符回滚）两种方言共用一份。
    // ⇒ 补上 POSIX 那一格**没有**多出第四套安装器，只是这一台安装器学会了第二种方言。
    let flavor = flavor_of(path);

    let (existing, did_exist) = if path.exists() {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read existing profile failed: {e}"))?;
        let on_disk_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        // 防御：文件在磁盘上有内容但读到 "" —— 罕见但要拦下来。OneDrive / 杀软介入
        // 可能让 read_to_string 在某些情况下返回 Ok("")。继续走会用 "" + new code
        // 覆盖原内容（v1.7.9 事故的可能子原因之一）。
        if on_disk_size > 0 && raw.is_empty() {
            return Err(format!(
                "profile 文件 {} 在磁盘上有 {} 字节但读到空内容（可能被 OneDrive/杀软锁定）。\
                 取消安装。请先在文件资源管理器里确认文件可读后重试。",
                path.display(),
                on_disk_size
            ));
        }
        // 〔`K-R132`〕BOM 剥在**最靠近读的那一跳**：往下所有判内容的东西
        // （围栏、冲突函数、裸行扫描、合块）拿到的都是没有 BOM 的那一份。
        (strip_bom(&raw).to_string(), true)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create profile dir failed: {e}"))?;
        }
        (String::new(), false)
    };

    let updated = plan_install(
        flavor,
        &existing,
        command_name,
        include_cc_function,
        &path.display().to_string(),
    )?;

    // 写之前先备份原文件（即使没动 BEGIN/END 块外的内容，也防 atomic_write 异常）
    let backup_path = if did_exist && !existing.is_empty() {
        let backup = backup_path_for(path);
        std::fs::copy(path, &backup)
            .map_err(|e| format!("backup profile to {} failed: {e}", backup.display()))?;
        Some(backup)
    } else {
        None
    };

    // 〔`K-R132`〕落盘的那一份 ≠ 计划出来的那一份：PowerShell 方言前面多一个 BOM。
    // **读回校验比的必须是落盘那一份** —— 比 `updated` 会恒差三个字节，当场回滚。
    let on_disk_bytes = encode_for_disk(flavor, &updated);

    if let Err(e) = atomic_write_string(path, &on_disk_bytes) {
        // 写入失败：尝试从 backup 恢复
        if let Some(b) = &backup_path {
            let _ = std::fs::copy(b, path); // best-effort
        }
        return Err(format!(
            "write profile failed: {e}\n备份保留在: {}",
            backup_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(无备份，原文件不存在)".into())
        ));
    }

    // 写完读回来校验。**T01：从「只比长度」升级为「内容级比对」。**
    // 旧实现是 `written.len() != updated.len()`——同长度的损坏（字节翻转 / 编码变形 /
    // 行尾 LF↔CR 等长替换）会被静默放过，而这里写的是用户的 shell profile，
    // 写坏的后果是下次开终端就炸。远端侧（`sftp.rs`）一直比的是内容，本机侧此前更弱。
    // 写入（含备份与写失败时的恢复）在上面已做完——那一段各落点不同，不上提。
    // 这里把「读回 → 比对 → 回滚」交给统一实现，与远端 SFTP 侧共用同一套判定语义。
    crate::verified_write::verify_and_rollback(
        &on_disk_bytes,
        || {
            std::fs::read_to_string(path)
                .map_err(|e| format!("{e}（请检查 {} 内容）", path.display()))
        },
        || {
            if let Some(b) = &backup_path {
                let _ = std::fs::copy(b, path);
            }
        },
    )?;

    Ok(())
}

fn backup_path_for(path: &PathBuf) -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut p = path.clone();
    let fname = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile".to_string());
    p.set_file_name(format!("{fname}.ccm-backup-{ms}"));
    p
}

/// 卸载：整块删除 BEGIN/END 之间的内容（含 marker 行）。块外内容不动。
///
/// v1.7.10：同 install 加 backup + 写后校验，避免卸载半途坏文件。
pub fn uninstall_from_profile(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let raw =
        std::fs::read_to_string(path).map_err(|e| format!("read existing profile failed: {e}"))?;
    let on_disk_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if on_disk_size > 0 && raw.is_empty() {
        return Err(format!(
            "profile 文件 {} 在磁盘上有 {} 字节但读到空内容，取消卸载。",
            path.display(),
            on_disk_size
        ));
    }
    let flavor = flavor_of(path);
    // 〔`K-R132`〕同 [`install_to_profile`]：BOM 剥在最靠近读的那一跳。
    let existing = strip_bom(&raw).to_string();
    // 〔`K-R62`〕同 [`install_to_profile`]：剥哪一对围栏按方言分岔，落盘那一套共用。
    let stripped = plan_uninstall(flavor, &existing, &path.display().to_string())?;
    let on_disk_bytes = encode_for_disk(flavor, &stripped);
    if on_disk_bytes == raw {
        return Ok(()); // 没有块（而且编码也已经是对的），无需写
    }

    let backup = backup_path_for(path);
    std::fs::copy(path, &backup)
        .map_err(|e| format!("backup profile to {} failed: {e}", backup.display()))?;

    if let Err(e) = atomic_write_string(path, &on_disk_bytes) {
        let _ = std::fs::copy(&backup, path);
        return Err(format!(
            "write profile failed: {e}\n备份保留在: {}",
            backup.display()
        ));
    }
    // 同上走统一校验。卸载路径此前也只比长度——剥离别名块写坏同样弄坏用户的 shell 配置。
    crate::verified_write::verify_and_rollback(
        &on_disk_bytes,
        || {
            std::fs::read_to_string(path)
                .map_err(|e| format!("{e}（请检查 {} 内容）", path.display()))
        },
        || {
            let _ = std::fs::copy(&backup, path);
        },
    )?;
    Ok(())
}

// === 内部 helpers ===

/// 找文件中第一个 cc-monitor 块的版本字符串（"v1" 等）。
fn find_block_version(content: &str) -> (bool, Option<String>) {
    for line in content.lines() {
        // T04 审计③：与 `find_pair` 同口径（`trim_start`）。不加的话缩进的悬空 BEGIN 会让
        // `has_ccm_block=false` → UI 说"未安装"**且隐藏卸载按钮**，而点安装却 Err 报行号。
        if let Some(rest) = line.trim_start().strip_prefix(BEGIN_MARKER) {
            // rest 可能是 " v1 ===" 之类
            let trimmed = rest.trim().trim_end_matches('=').trim();
            // trimmed = "v1"
            return (
                true,
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                },
            );
        }
    }
    (false, None)
}

/// 扫描 profile 找跟 command_name 同名的 function 定义（在 BEGIN/END 块外的）。
fn find_conflicting_functions(
    flavor: ProfileFlavor,
    content: &str,
    command_name: &str,
) -> Vec<String> {
    let safe = sanitize_command_name(command_name);
    let mut inside_ccm_block = false;
    let mut hits = Vec::new();
    // 简单 line-based regex 替代：检查 "function <name>" 模式
    for line in content.lines() {
        let l = line.trim_start();
        // 〔`K-R62`〕三对围栏一起认（`fence_marker` 的共同前缀就包含 [`BEGIN_MARKER`]）——
        // 只认 PowerShell 那一对的话，我们自己装进 rc 的 `cc() { ccm "$@"; }`
        // 会被当成「用户已有的同名函数」报成冲突。
        if let Some(open) = fence_marker(line) {
            inside_ccm_block = open;
            continue;
        }
        if inside_ccm_block {
            continue;
        }
        // 〔`K-R62`〕**两种方言的函数写法不同**：PowerShell 是 `function cc {`，
        // POSIX sh 是 `cc() {`。此前只认前一形 ⇒ 在 rc 上恒空，
        // 而「恒空」与「真的没冲突」在界面上一模一样。
        if flavor == ProfileFlavor::PosixRc {
            if let Some(name) = function_name_of(l) {
                if name.eq_ignore_ascii_case(&safe) {
                    hits.push(safe.clone());
                    break;
                }
            }
            continue;
        }
        // 简化匹配：以 "function" 开头 + 空白 + 同名（后跟空白/{/(）
        if let Some(rest) = l
            .strip_prefix("function ")
            .or_else(|| l.strip_prefix("function\t"))
        {
            let rest = rest.trim_start();
            // 取 function 后面的标识符
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len());
            let name = &rest[..end];
            if name.eq_ignore_ascii_case(&safe) {
                hits.push(safe.clone());
                break;
            }
        }
    }
    hits
}

/// 找已有 cc-monitor 块的范围（line index, inclusive）。
///
/// **T04 第二步：改走 `fenced_block::find_pair`，与远端 profile 共用同一条判定。**
/// 原实现在「有 BEGIN 但其后没有 END」时返回 `None` → 调用方走**追加**分支，
/// 而第二次安装时那个损坏的 BEGIN 会与新块的 END 配上对、**吃掉两者之间的用户代码**
/// （实测见 `repro_local_eats_user_content_on_damaged_fence`）。
/// 远端侧（`sftp::merge_profile_block`）当初被审计 B1 要求在同一情形 Err 中止，
/// 本机侧漏了这道保护——写的都是"下次开终端就炸"级别的文件。
fn find_block_range(content: &str, what: &str) -> Result<Option<(usize, usize)>, String> {
    crate::fenced_block::find_pair(content, BEGIN_MARKER, END_MARKER, what)
}

/// 检测 existing 用的行尾风格。包含任何 `\r\n` 就视为 CRLF（Windows 用户 profile
/// 默认值——notepad / VSCode / git autocrlf=true 三大来源都是 CRLF）。
fn detect_eol(s: &str) -> &'static str {
    if s.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// 把任意 EOL 风格的文本归一到指定 EOL：先全 → LF，再按需 → CRLF。
fn rewrite_eol(content: &str, target: &str) -> String {
    if target == "\n" {
        content.replace("\r\n", "\n")
    } else {
        content.replace("\r\n", "\n").replace('\n', "\r\n")
    }
}

/// `\n` / `\r\n` 都判 true（`\r\n` 的最后一个字符就是 `\n`）。
fn ends_with_eol(s: &str) -> bool {
    s.ends_with('\n')
}

/// 在 existing 中替换 ccm 块；若不存在则追加。
///
/// **必须保留原文件的 EOL 风格**：Windows 用户 profile 默认 CRLF；早期版本用
/// `existing.lines().join("\n")` 静默把 CRLF → LF，length 校验检不出（两边都已
/// LF），用户用 notepad 看会被"行尾不一致"警告/ git diff 整文件标红。
/// 改用 `split_inclusive('\n')` 保留终止符，新 block 按 detected EOL 重写。
fn replace_or_append_block(existing: &str, new_block: &str, what: &str) -> Result<String, String> {
    let eol = detect_eol(existing);
    let block = rewrite_eol(new_block.trim_end_matches(|c| c == '\r' || c == '\n'), eol);
    if let Some((begin, end)) = find_block_range(existing, what)? {
        // split_inclusive('\n') 与 .lines() 索引一致：都按 '\n' 切，索引位置相同；
        // 区别只是 split_inclusive 把 '\n'（及前一个 '\r'）保留在切片内部。
        let lines: Vec<&str> = existing.split_inclusive('\n').collect();
        let before: String = lines[..begin].concat();
        let after: String = if end + 1 < lines.len() {
            lines[(end + 1)..].concat()
        } else {
            String::new()
        };
        let mut out = String::new();
        if !before.is_empty() {
            out.push_str(&before);
            if !ends_with_eol(&before) {
                out.push_str(eol);
            }
        }
        out.push_str(&block);
        out.push_str(eol);
        if !after.is_empty() {
            out.push_str(&after);
            if !ends_with_eol(&after) {
                out.push_str(eol);
            }
        }
        Ok(out)
    } else {
        // 追加
        let mut out = existing.to_string();
        if !out.is_empty() && !ends_with_eol(&out) {
            out.push_str(eol);
        }
        if !out.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&block);
        out.push_str(eol);
        Ok(out)
    }
}

/// 删除 ccm 块（如果有）。
///
/// **卸载路径也走同一条配对判定**（T04 第二步）：围栏损坏时 `Err` 中止而不是
/// "当作没有块、原样返回"。后者看着无害，实则让用户以为卸载干净了，
/// 而那个悬空的 BEGIN 还留在文件里——下次安装就会吃掉它下面的内容。
fn strip_block(existing: &str, what: &str) -> Result<String, String> {
    let eol = detect_eol(existing);
    let Some((begin, end)) = find_block_range(existing, what)? else {
        return Ok(existing.to_string());
    };
    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    let before: String = lines[..begin].concat();
    let after: String = if end + 1 < lines.len() {
        lines[(end + 1)..].concat()
    } else {
        String::new()
    };
    let mut out = String::new();
    if !before.is_empty() {
        out.push_str(&before);
        if !ends_with_eol(&before) {
            out.push_str(eol);
        }
    }
    if !after.is_empty() {
        out.push_str(&after);
        if !ends_with_eol(&after) {
            out.push_str(eol);
        }
    }
    // 防止文件结尾多空行：保留至多一个尾 EOL
    let double = format!("{eol}{eol}");
    while out.ends_with(&double) {
        let new_len = out.len() - eol.len();
        out.truncate(new_len);
    }
    Ok(out)
}

/// 命令名只允许字母数字下划线（防注入）。
fn sanitize_command_name(name: &str) -> String {
    let trimmed = name.trim();
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "cc".to_string()
    } else {
        cleaned
    }
}

/// 原子写文件：写 .tmp 后用 `ReplaceFileW` 一步替换，**保留 dst 原有 ACL**。
///
/// v1.7.9 及之前用三步 `write(tmp) -> remove(path) -> rename(tmp, path)` 非原子，
/// 中途失败原文件丢失。v1.7.10 一开始改 MoveFileExW，但 MoveFileExW 仍把 tmp
/// 文件 ACL 写到 dst —— 如果 dst 父目录没给当前用户 explicit ACE（如用户把
/// Documents 重定向到非默认盘），用户自己都会读不了。
///
/// 正解：`ReplaceFileW(dst, src, ...)` —— 这个 API 专门做"原子替换内容但保留
/// dst 的 ACL / ADS / 创建时间"。Windows 文档明确推荐用它替换配置文件。
///
/// dst 不存在时 ReplaceFileW 会失败，fallback 到普通 rename（首次安装场景）。
/// tmp 文件名加 PID + 时间戳避免并行写碰撞。
pub(crate) fn atomic_write_string(path: &PathBuf, content: &str) -> std::io::Result<()> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut tmp = path.clone();
    let fname = tmp
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile.ps1".to_string());
    tmp.set_file_name(format!("{fname}.ccm-tmp-{ms}-{}", std::process::id()));
    std::fs::write(&tmp, content)?;
    let r = atomic_replace_path(&tmp, path);
    if r.is_err() {
        // 替换失败：清掉 tmp 不留垃圾
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

#[cfg(windows)]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);

    if !dst.exists() {
        // dst 不存在 ReplaceFileW 会失败；首次安装直接 rename（新文件 ACL 继承
        // 父目录，这是 Windows 创建文件的正常行为，没东西可保留）
        return std::fs::rename(src, dst);
    }

    unsafe {
        ReplaceFileW(
            PCWSTR(dst_w.as_ptr()),
            PCWSTR(src_w.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

#[cfg(test)]
mod tests {
    /// ★ 围栏本身的行为：**跑出 home 的一律拒绝，home 之内的照常放行**。
    ///
    /// ⚠ 正例那一半不是凑数：围栏收得太紧会**悄悄砍掉 `ProfileKind::Custom`**
    /// （用户指 `~/.config/fish/config.fish` 这种），那是把一个洞换成一个回归。
    #[test]
    fn the_profile_fence_keeps_writes_inside_home() {
        let home = dirs::home_dir().expect("测试需要 home");
        // 正例：home 下的裸文件名 · home 子目录 · `~` 前缀（用户会手打）· 不存在的新文件
        for ok in [
            home.join(".bashrc").to_string_lossy().to_string(),
            home.join(".config/fish/config.fish")
                .to_string_lossy()
                .to_string(),
            "~/.zshrc".to_string(),
            home.join("no-such-file-yet.rc")
                .to_string_lossy()
                .to_string(),
        ] {
            assert!(
                super::fence_profile_path(&ok).is_ok(),
                "围栏拒了一个合法路径：{ok:?} —— 收太紧会砍掉 `ProfileKind::Custom` 这个特性"
            );
        }
        // 反例：绝对路径跑出 home · 相对路径 · `..` 逃逸
        for bad in [
            "/etc/profile".to_string(),
            "/tmp/x.rc".to_string(),
            ".bashrc".to_string(),
            format!("{}/../../etc/profile", home.to_string_lossy()),
            // ⚠ **这一条是变异逼出来的**：上一行那种 `..` 逃逸其实是被**符号链接那条腿**
            // 接住的（父目录 `/etc` 存在 ⇒ canonicalize 成功 ⇒ 前缀检查发现跑出去了）。
            // 把上级目录检查整个删掉，判据**照样绿** —— 因为反例挑得不对。
            // 父目录**不存在**时 canonicalize 会失败、那条腿被跳过，只剩这一条挡着：
            format!(
                "{}/no-such-dir/../../../etc/profile",
                home.to_string_lossy()
            ),
        ] {
            let r = super::fence_profile_path(&bad);
            assert!(
                r.is_err(),
                "围栏放行了 {bad:?} —— 那三条命令会往它写/重写/探测存在性"
            );
            assert!(
                r.unwrap_err().starts_with("refuse profile path"),
                "拒绝理由要能一眼看出是围栏拒的（调用方与用户都要读它）"
            );
        }
    }

    /// ★★ **三条 `cc_integration_*` 命令都必须先过路径围栏**〔audit-0805 08-08，Phase G 第 86 件〕。
    ///
    /// # 洞：本机这条路没有围栏，而远端那条有
    ///
    /// `cc_integration_install(path: String, command_name: String, …)` 把 webview 给的路径
    /// **原样** `PathBuf::from` 交给 [`install_to_profile`]（文件不存在就创建 —— 本文件
    /// 另有一条判据逐字叫 `install_to_nonexistent_path_creates_file`），
    /// `cc_integration_uninstall` 会**重写**那个文件，`cc_integration_scan_path` 是任意路径的
    /// **存在性/大小探针**。三条都不看路径。
    ///
    /// 而**远端**那条同名功能有围栏：`sftp.rs` 逐字「profile 只能是 home 下的文件名
    ///（如 `.bashrc` / `.zshrc`）」。⇒ **同一形态在另一半有围栏、这一半没有**
    ///（F52/F44 同族；后果那一侧与 F47「任意本机文件删除」同族）。
    ///
    /// ⚠ 写进去的内容也不是全常量：`command_name` 来自调用方，会被渲进那段 shell 代码。
    ///
    /// # 围栏取「在 home 之内」，不取「home 下的裸文件名」
    ///
    /// 远端那条可以严到「裸文件名」，本机不行：`discover_profiles()` 自己就会返回
    /// `~/WindowsPowerShell/Microsoft.PowerShell_profile.ps1` 这种**子目录**里的路径，
    /// 而 `ProfileKind::Custom` 是产品特性（用户可以指 `~/.config/fish/config.fish`）。
    /// ⇒ 围栏只挡「跑出 home」这一类，**不缩小功能**。
    #[test]
    fn every_profile_command_passes_through_the_fence() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .expect("读不到 lib.rs");
        let prod = guard_core::production_code(&lib);
        const CMDS: &[&str] = &[
            "cc_integration_install",
            "cc_integration_uninstall",
            "cc_integration_scan_path",
        ];
        let fence = format!("{}_profile_path", "fence");
        for cmd in CMDS {
            let at = prod
                .find(&format!("fn {cmd}("))
                .unwrap_or_else(|| panic!("`lib.rs` 里找不到 `{cmd}` —— 命令改名了就把本条一起改"));
            // 切到该命令函数体的收尾（花括号配平；两个字符字面量成对出现）。
            let bytes = prod.as_bytes();
            let open = (at..bytes.len())
                .find(|&i| bytes[i] == b'{')
                .expect("找不到函数体起点");
            let mut depth = 0i32;
            let mut end = bytes.len();
            for i in open..bytes.len() {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            let body = &prod[open..end];
            assert!(
                body.len() > 40,
                "`{cmd}` 切出来只有 {} 字节 —— 配平切错了，本条会零命中地绿",
                body.len()
            );
            assert!(
                body.contains(&fence),
                "`{cmd}` 没有过路径围栏 `{fence}`。\n\
                 ★ 它收的是 **webview 给的任意路径**：`install` 会往那里写（不存在就创建）、\n\
                 `uninstall` 会重写它、`scan_path` 是存在性/大小探针。\n\
                 ⚠ 远端那条同名功能**有**围栏（`sftp.rs`：「profile 只能是 home 下的文件名」）——\n\
                 同一形态在另一半有围栏、这一半没有，正是本区反复逮到的形状。\n\
                 实得这一段：{body:?}"
            );
        }
    }

    /// **围栏损坏时中止，而不是吃掉用户内容**（T04 第二步修的真 bug）。
    ///
    /// 修前实测：`装一次` 走追加分支（用户代码还在），`装两次` 时那个损坏的 BEGIN
    /// 与新块的 END 配上对，两者之间的 `function cc { }` **被整段替换掉**。
    /// 远端侧（`sftp::merge_profile_block`）当初被审计 B1 要求在同一情形 Err 中止，
    /// 本机侧漏了这道保护——写的都是"下次开终端就炸"级别的文件。
    /// panic 也要清 tempdir。**这是我自己踩的**：用审计那两个变异反验证时测试 panic，
    /// 末尾的 `remove_dir_all` 走不到，`/tmp` 下留了两个目录。`Drop` 不受 panic 影响。
    struct TmpDir(std::path::PathBuf);
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tmpdir(tag: &str) -> TmpDir {
        let d = std::env::temp_dir().join(format!(
            "ccm-fence-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("建 tempdir");
        TmpDir(d)
    }

    /// **围栏损坏时一个字节都不许写** —— 真行为断言（T07 审计① 换掉的那条安慰剂）。
    ///
    /// ## 上一版是安慰剂，两个变异实证
    ///
    /// 上一版按**字节偏移顺序**扫自身源码（`body.find(fence) < body.find(write)`）。
    /// 审计用两个**编译得过且真写盘**的变异让它保持绿：
    /// ① 在围栏前插 `std::fs::write(path, "MUTANT CLOBBER\n")` —— 扫描不认这个 API 名
    ///    → 函数返回「已中止」而**用户文件已被清成 `"MUTANT CLOBBER\n"`**；
    /// ② 把同一个 `std::fs::copy` 挪进窗口之外的 helper → **泄漏文件真的产生**。
    /// 它还能被**注释文本**骗红（提到 `"atomic_write_string"` 的注释就让它 FAILED），
    /// 而且 `str::find` 只取第一处 `copy`（窗口内各有 3 处）
    /// —— **恰好命中真备份纯属排序运气**。
    ///
    /// **顺序/长相是代理指标，不是性质。** 换成直接量：造一个坏围栏文件、调真函数、
    /// 断言 **Err 且文件字节与调用前逐字相同**。用 tempdir，不碰任何用户文件。
    #[test]
    fn damaged_fence_leaves_the_file_byte_identical() {
        let td = tmpdir("bad");
        let dir = &td.0;
        let path = dir.join("Microsoft.PowerShell_profile.ps1");

        // 坏围栏：有 BEGIN、没有配对 END，**下面是用户自己的代码**
        let original =
            "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host mine }\n";
        std::fs::write(&path, original).expect("写 tempdir 样本");
        let before = std::fs::read(&path).expect("读原文");

        for (what, r) in [
            ("install", install_to_profile(&path, "cc", true)),
            ("uninstall", uninstall_from_profile(&path)),
        ] {
            // Ok 是 `()`；意外成功会被下面的字节断言 + 「应因围栏损坏中止」两条同时抓住
            let e = match r {
                Ok(()) => "(意外返回 Ok —— 围栏损坏本该中止)".to_string(),
                Err(e) => e,
            };
            let after = std::fs::read(&path).expect("读回");
            assert_eq!(
                after,
                before,
                "{what}：围栏损坏时文件必须逐字未变。实得 {:?}（错误/返回：{e}）",
                String::from_utf8_lossy(&after)
            );
            assert!(
                e.contains("找不到配对的 END"),
                "{what}：应因围栏损坏中止，实得：{e}"
            );
            // 顺带：不许留下备份/临时文件（上一版变异② 的泄漏形态）
            let leaked: Vec<String> = std::fs::read_dir(&dir)
                .expect("列 tempdir")
                .filter_map(|x| x.ok())
                .map(|x| x.file_name().to_string_lossy().into_owned())
                .filter(|n| n != "Microsoft.PowerShell_profile.ps1")
                .collect();
            assert!(leaked.is_empty(), "{what}：不该留下 {leaked:?}");
        }
    }

    /// 反向自检：围栏**完好**时这条路必须真能写成（否则上一条可能是"什么都不做"而恒绿）。
    #[test]
    fn intact_fence_actually_writes() {
        let td = tmpdir("ok");
        let path = td.0.join("Microsoft.PowerShell_profile.ps1");
        std::fs::write(&path, "# mine\n").expect("写样本");
        let before = std::fs::read_to_string(&path).unwrap();
        install_to_profile(&path, "cc", true).expect("围栏完好时应写成");
        let after = std::fs::read_to_string(&path).unwrap();
        assert_ne!(after, before, "围栏完好时必须真写进去");
        assert!(after.contains("# mine"), "块外内容要保留：{after}");
        assert!(after.contains("cc-monitor BEGIN"), "块要写进去：{after}");
    }

    #[test]
    fn damaged_fence_aborts_instead_of_eating_user_content() {
        const BLOCK: &str = "# === cc-monitor BEGIN v1 ===\nNEW\n# === cc-monitor END ===";
        // 用户 profile：有个损坏的 BEGIN（上次安装中断/手改坏），**下面是用户自己的代码**
        let damaged = "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { }\n";
        // **修后：第一次就 Err 中止，用户内容一个字节都不动。**
        let e = replace_or_append_block(damaged, BLOCK, "C:/x/profile.ps1").unwrap_err();
        assert!(e.contains("找不到配对的 END"), "{e}");
        assert!(e.contains("已中止"), "要让用户知道我们没动文件：{e}");
        // T04 审计⑥：`what` 现在传**真路径**而不是类别名（调用方手里一直有它）。
        // 原断言写的是类别名，与它自己的注释"要说清是哪个文件"不符。
        assert!(
            e.contains("C:/x/profile.ps1"),
            "要说清是哪个文件的真路径：{e}"
        );
        // 修前实测的退化链（留作记录，见 fenced_block 模块文档）：
        //   装一次 → 追加，用户代码还在；装两次 → 损坏的 BEGIN 与新块的 END 配对
        //   → `function cc { }` **被吃掉**。
    }
    use super::*;

    #[test]
    fn render_cc_code_with_function() {
        let out = render_cc_code("ccm", true);
        assert!(out.contains("function ccm"));
        assert!(out.contains("__ccm_bind"));
        assert!(!out.contains("{{CC_FUNCTION_BLOCK}}"));
        assert!(out.contains("BEGIN v2"));
        assert!(out.contains("cc-monitor END"));
    }

    #[test]
    fn render_cc_code_helper_only() {
        // 用户已有自定义 function cc 时只装 __ccm_bind helper，不生成 function cc
        let out = render_cc_code("cc", false);
        assert!(out.contains("__ccm_bind"));
        // 按词，不按子串〔§5 2l〕：同族的 `strip_block_removes_only_block` 已实测过
        // 这个陷阱 —— 语料里一出现 `function ccm`，`contains` 就假红。这里今天还碰不到，
        // 但**同一种写法只该有一个答案**，不留一处等着下次踩。
        assert!(!guard_core::contains_word(&out, "function cc"));
        assert!(!out.contains("{{CC_FUNCTION_BLOCK}}"));
        assert!(out.contains("BEGIN v2"));
    }

    #[test]
    fn sanitize_command_name_strips_specials() {
        assert_eq!(sanitize_command_name("cc"), "cc");
        assert_eq!(sanitize_command_name("my-cc"), "mycc");
        assert_eq!(sanitize_command_name("cc; rm -rf /"), "ccrmrf");
        assert_eq!(sanitize_command_name(""), "cc");
        assert_eq!(sanitize_command_name("   "), "cc");
    }

    #[test]
    fn find_conflict_in_user_function() {
        let content = r#"
function Get-Stuff { Write-Host x }
function cc { Write-Host my-cc }
"#;
        let conflicts = find_conflicting_functions(ProfileFlavor::PowerShell, content, "cc");
        assert_eq!(conflicts, vec!["cc".to_string()]);
    }

    #[test]
    fn find_conflict_ignores_inside_ccm_block() {
        // 同名 function 在 ccm 块里 → 不算冲突（是我们自己写的）
        let content = r#"
function Get-Stuff { Write-Host x }
# === cc-monitor BEGIN v1 ===
function cc { __ccm_bind; & claude $args }
# === cc-monitor END ===
"#;
        let conflicts = find_conflicting_functions(ProfileFlavor::PowerShell, content, "cc");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn find_block_version_v1() {
        let content = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===\n";
        let (present, ver) = find_block_version(content);
        assert!(present);
        assert_eq!(ver, Some("v1".to_string()));
    }

    #[test]
    fn replace_existing_block_keeps_other_content() {
        let existing = r#"# user stuff before
Set-Alias g git

# === cc-monitor BEGIN v1 ===
function cc { Write-Host old-version }
# === cc-monitor END ===

# user stuff after
$PSDefaultParameterValues = @{}
"#;
        let new_block =
            "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host new-version }\n# === cc-monitor END ===";
        let out = replace_or_append_block(existing, new_block, "C:/x/profile.ps1").unwrap();
        assert!(out.contains("Set-Alias g git"));
        assert!(out.contains("$PSDefaultParameterValues = @{}"));
        assert!(out.contains("new-version"));
        assert!(!out.contains("old-version"));
    }

    #[test]
    fn append_to_empty_profile() {
        let new_block =
            "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host hi }\n# === cc-monitor END ===";
        let out = replace_or_append_block("", new_block, "C:/x/profile.ps1").unwrap();
        assert!(out.starts_with("# === cc-monitor BEGIN"));
        assert!(out.ends_with("END ===\n"));
    }

    #[test]
    fn append_to_existing_profile_with_no_block() {
        let existing = "Set-Alias g git\n";
        let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
        let out = replace_or_append_block(existing, new_block, "C:/x/profile.ps1").unwrap();
        assert!(out.starts_with("Set-Alias g git"));
        assert!(out.contains("BEGIN v1"));
    }

    #[test]
    fn strip_block_removes_only_block() {
        let existing = r#"Set-Alias g git
function ccm { Write-Host "我自己的，别动" }
# === cc-monitor BEGIN v1 ===
function cc { Write-Host hi }
# === cc-monitor END ===
$PSDefaultParameterValues = @{}
"#;
        let out = strip_block(existing, "C:/x/profile.ps1").unwrap();
        assert!(out.contains("Set-Alias g git"));
        assert!(out.contains("$PSDefaultParameterValues"));
        assert!(!out.contains("BEGIN"));
        // ★ **`contains` 不行，要按词**〔audit-0805 §5 2l，08-06〕：
        // 用户自己那句 `function ccm` 会让 `contains("function cc")` 命中 ⇒ **假红**
        // （代码是对的：strip_block 只该删自己的块，用户的同前缀函数必须原样留着）。
        // 上面那行语料就是为这条加的 —— 换回 `contains` 会当场红。
        assert!(!guard_core::contains_word(&out, "function cc"));
        // 反向：用户那个同前缀的函数**必须还在**。少了这一条，上面那句会被人
        // 「顺手」改回 contains 再把语料删掉，于是两边一起退回原样。
        assert!(out.contains("function ccm"), "用户自己的同前缀函数被删掉了");
    }

    #[test]
    fn strip_block_no_op_when_no_block() {
        let content = "Set-Alias g git\n";
        assert_eq!(strip_block(content, "C:/x/profile.ps1").unwrap(), content);
    }

    #[test]
    fn install_preserves_crlf_line_endings() {
        // Windows 用户 profile 普遍 CRLF（notepad/VSCode/git autocrlf 三大来源）。
        // 早期 .lines().join("\n") 会静默把 CRLF → LF。这里验保留。
        let crlf = "# my profile\r\nSet-Alias g git\r\nfunction prompt { 'PS> ' }\r\n";
        let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
        let out = replace_or_append_block(crlf, new_block, "C:/x/profile.ps1").unwrap();
        // 用户原内容仍带 CRLF
        assert!(
            out.contains("# my profile\r\n"),
            "用户首行 CRLF 丢了：{out:?}"
        );
        assert!(
            out.contains("Set-Alias g git\r\n"),
            "用户 alias CRLF 丢了：{out:?}"
        );
        // 新插入的 ccm 块也应是 CRLF
        assert!(
            out.contains("# === cc-monitor BEGIN v1 ===\r\n"),
            "新块的 BEGIN 行 EOL 不是 CRLF：{out:?}"
        );
        // 整文件**不应出现**任何裸 \n（除了作为 \r\n 的一部分）
        let bare_lf_count = out.matches('\n').count() - out.matches("\r\n").count();
        assert_eq!(bare_lf_count, 0, "出现了裸 LF，CRLF 被破坏：{out:?}");
    }

    #[test]
    fn strip_block_preserves_crlf_line_endings() {
        let crlf = "Set-Alias g git\r\n\
                    # === cc-monitor BEGIN v1 ===\r\n\
                    function cc {}\r\n\
                    # === cc-monitor END ===\r\n\
                    function prompt { 'PS> ' }\r\n";
        let out = strip_block(crlf, "C:/x/profile.ps1").unwrap();
        assert!(out.contains("Set-Alias g git\r\n"));
        assert!(out.contains("function prompt"));
        assert!(!out.contains("BEGIN"));
        let bare_lf_count = out.matches('\n').count() - out.matches("\r\n").count();
        assert_eq!(bare_lf_count, 0, "出现了裸 LF：{out:?}");
    }

    #[test]
    fn lf_only_file_stays_lf() {
        let lf = "Set-Alias g git\n";
        let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
        let out = replace_or_append_block(lf, new_block, "C:/x/profile.ps1").unwrap();
        // 已是 LF 的文件不强加 CRLF
        assert!(!out.contains("\r\n"), "纯 LF 文件被改成了 CRLF：{out:?}");
    }

    // === v1.7.10：install / uninstall end-to-end 落地保护测试 ===

    fn tmp_profile() -> PathBuf {
        let mut p = std::env::temp_dir();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("ccm-test-{ms}-{}.ps1", std::process::id()));
        p
    }

    #[test]
    fn install_preserves_existing_user_content() {
        let p = tmp_profile();
        let user_content = "# my profile\nSet-Alias g git\nfunction prompt { 'PS> ' }\n";
        std::fs::write(&p, user_content).unwrap();

        install_to_profile(&p, "cc", false).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(
            after.contains("Set-Alias g git"),
            "用户原有 alias 被冲掉了！after={after}"
        );
        assert!(after.contains("function prompt"));
        assert!(after.contains("# === cc-monitor BEGIN"));
        assert!(!after.is_empty());

        // 备份文件应该存在
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        let has_backup = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.ccm-backup-"))
            });
        assert!(has_backup, "应该生成 .ccm-backup-<ts> 备份文件");

        // 清理
        let _ = std::fs::remove_file(&p);
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    #[test]
    fn install_to_nonexistent_path_creates_file() {
        let p = tmp_profile();
        // 不预先创建
        assert!(!p.exists());
        install_to_profile(&p, "cc", true).unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        // ★ **带边界**〔audit-0805 F24 存量清账〕：`contains("function cc")` 是**正向事实钉**，
        // 而 `function ccm` 也含有它 —— 安装器若改成生成 `function ccm`（同文件 `:727` 就有
        // 那个形态），本条**照样绿**。这是「匹配单位比事实小」里最贵的那种：假绿。
        assert!(
            guard_core::contains_word(&content, "function cc"),
            "装出来的 profile 里没有 `function cc` 这个**完整的词** —— \
             若是改成了 `function ccm` 之类，本条与它保护的行为都要一起重判"
        );
        assert!(content.contains("__ccm_bind"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reinstall_replaces_block_keeps_user_content() {
        let p = tmp_profile();
        std::fs::write(&p, "Set-Alias g git\n").unwrap();

        install_to_profile(&p, "cc", false).unwrap();
        // 第二次装：之前的块应该被原地替换，用户内容仍在
        install_to_profile(&p, "cc", true).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("Set-Alias g git"));
        assert!(after.contains("function cc"));
        // 只应该有一个 BEGIN 块
        assert_eq!(after.matches("# === cc-monitor BEGIN").count(), 1);

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// v1.7.10：验证 install_to_profile 保留 dst 上的 explicit ACE。
    ///
    /// 复现 v1.7.9 zbl 事故场景：原 profile 上有 explicit ACE（用户自己加的或
    /// 系统给的），如果 atomic_replace 用 MoveFileExW / rename，explicit ACE
    /// 会被 tmp 文件的"继承父目录" ACL 覆盖丢失。用 ReplaceFileW 应该保留。
    ///
    /// 用 icacls 给文件加 `Everyone:(R)` explicit ACE，install 后跑 icacls
    /// 看这条 ACE 是否还在。
    #[cfg(windows)]
    #[test]
    fn install_preserves_explicit_acl_entries() {
        let p = tmp_profile();
        std::fs::write(&p, "Set-Alias g git\n").unwrap();

        // 给文件加 explicit Everyone:(R) ACE
        let add = std::process::Command::new("icacls")
            .arg(&p)
            .arg("/grant")
            .arg("Everyone:(R)")
            .output();
        let add = match add {
            Ok(o) if o.status.success() => o,
            _ => {
                // icacls 不可用（测试环境少见）—— 跳过
                let _ = std::fs::remove_file(&p);
                return;
            }
        };
        assert!(add.status.success(), "icacls /grant failed: {:?}", add);

        // 跑 install
        install_to_profile(&p, "cc", false).unwrap();

        // 看 explicit ACE 还在不在（icacls 输出里不带 (I) 标记的那条）
        let out = std::process::Command::new("icacls")
            .arg(&p)
            .output()
            .expect("icacls run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // 期望看到 Everyone:(R) 不带 (I) 前缀 —— 是 explicit ACE
        let has_explicit_everyone = stdout
            .lines()
            .any(|line| line.contains("Everyone:(R)") && !line.contains("(I)(R)"));
        assert!(
            has_explicit_everyone,
            "explicit Everyone:(R) ACE 应该被 ReplaceFileW 保留！icacls 输出:\n{stdout}"
        );

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    #[test]
    fn uninstall_strips_block_keeps_user_content() {
        let p = tmp_profile();
        let user = "# mine\nSet-Alias g git\n";
        std::fs::write(&p, user).unwrap();
        install_to_profile(&p, "cc", true).unwrap();
        uninstall_from_profile(&p).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("Set-Alias g git"));
        assert!(!after.contains("# === cc-monitor"));
        assert!(!after.contains("__ccm_bind"));

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // `K-R62`：本机 POSIX 那一格
    // ═══════════════════════════════════════════════════════════════════════

    /// 「这份文本里**有整整一行**逐字等于它」。
    ///
    /// ⚠ 刻意不是 `contains`：那种比法的**匹配单位（子串）比事实（一整行）小**——
    /// 一行被截断、或被多贴了半句，子串比法照样绿（同 `account_aliases` 那条纪律）。
    fn pinned(hay: &str, needle: &str) -> bool {
        hay.lines().any(|l| l == needle)
    }

    /// 一份**只有裸行、一个围栏都没有**的 rc —— `K-R57` 现打用户 `~/.bashrc` 的形状。
    ///
    /// 那 14 行里 10 行是真使用者，`名字() { ccm <修饰...> "$@"; }`，另 4 行是注释。
    /// 这里照抄那个形状（名字取同一批），**并且刻意一个 BEGIN/END 都不放**。
    const BARE_RC: &str = "\
# 我自己的一些设置
export EDITOR=vim

# ccm 启动器
cc()   { ccm \"$@\"; }
cct()  { ccm --tmux \"$@\"; }
oo()   { ccm --agent codex \"$@\"; }
zcc()  { ccm --account z \"$@\"; }
alias  ccx='ccm --base'

# 与 ccm 无关的一行
gs() { git status; }
";

    /// ★★ `KR62D1` 的正题：**本机 POSIX 那条路与远端那个口，产物逐字节相同。**
    ///
    /// **死值验**：让本机这一臂改用一份**自己的**副本（或自己写一套 merge）⇒ 这一条当场红，
    /// 因为它比的不是「像不像」，是**恒等于远端那份实现在同一份输入上的产物**。
    #[test]
    fn the_local_posix_port_is_byte_for_byte_the_remote_one() {
        let what = "/home/u/.bashrc";
        for existing in ["", "export A=1\n", BARE_RC] {
            let got = plan_install(ProfileFlavor::PosixRc, existing, "cc", true, what)
                .expect("本机 POSIX 装口");
            let want =
                crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)
                    .expect("远端那个口");
            assert_eq!(
                got, want,
                "本机 POSIX 装进 rc 的东西与远端那个口不再是同一份 —— \
                 `KR62D1` 要买的一半正是「补这一格的时候没有变成第四套」。\
                 若这里改成了一份自己的 snippet / 一套自己的 merge，就把 `K-R62 §0c` 那三套变成了四套。"
            );
            // 卸那一半同样恒等（装了又卸回得去，是同一对围栏才可能成立）。
            let back = plan_uninstall(ProfileFlavor::PosixRc, &got, what).expect("本机 POSIX 卸口");
            assert_eq!(
                back,
                crate::sftp::strip_profile_block(&got, what).expect("远端卸口"),
                "卸那一半漂了 —— 装与卸必须是同一对围栏，否则装得进去卸不干净"
            );
        }
    }

    /// ★★ `KR62D1`：**全树只有一份 snippet**，而且它不住在本模块。
    ///
    /// 上一条比的是「产物一样」，一份**逐字节相同的副本**能骗过它；这一条按源码数**住址**。
    /// 两条一起才关得住「不许出现第二份 snippet」。
    #[test]
    fn the_alias_snippet_has_exactly_one_home_in_the_rust_tree() {
        let files = guard_core::scan_tree!(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &["rs"]
        );
        // ⚠ needle **拼出来**，本行不留那个宏名加左括号的字面形 —— 留了，
        //   `cross_half_edge_registry` 会把本处当成一处「解析不出路径的 include」而红
        //   （它按「宏名 + `(`」数调用，路径解析不出来就逼人登记）。
        //   判据自己不许在别人的人群里留一个假身影。
        let needle = format!("{}!(\"../../shared/ccm-aliases.sh\")", "include_str");
        let mut homes: Vec<String> = Vec::new();
        for (path, src) in &files {
            if guard_core::production_code(src).contains(&needle) {
                homes.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        assert_eq!(
            homes,
            vec!["sftp.rs".to_string()],
            "`shared/ccm-aliases.sh` 在 Rust 侧的住址应当**恰好一处**（`sftp.rs` 的 \
             `CCM_WRAPPER_SNIPPET`），实得 {homes:?}。多一处就是第二份 snippet —— \
             它们会各自漂，而漂了之后本机与远端装进去的东西就不是同一个了。"
        );
    }

    /// ★★ `KR62D1`：**本模块没有第二套 merge/strip，也没有第二对 POSIX 围栏。**
    ///
    /// 死值验：在本模块里另写一个 `fn merge_posix_block(...)` 并让 `plan_install` 调它 ⇒
    /// 下面第一条断言（POSIX 那一臂必须调远端那两个函数）当场红。
    #[test]
    fn the_posix_arm_borrows_the_remote_implementation_instead_of_growing_a_second_one() {
        let src = guard_core::production_code(include_str!("profile_installer.rs"));
        for call in [
            "crate::sftp::merge_profile_block(",
            "crate::sftp::strip_profile_block(",
            "crate::sftp::CCM_WRAPPER_SNIPPET",
        ] {
            assert_eq!(
                src.matches(call).count(),
                1,
                "`{call}` 在本模块的生产段里应当恰好出现 1 次 —— \
                 0 次 = 本机那条路不再借远端那一份（第四套长出来了）；\
                 >1 次 = 有第二个调用点，那是下一个漂移源"
            );
        }
        // POSIX 那一对围栏的**字面量**不许在本模块出现：出现即第二个住址。
        assert!(
            !src.contains("remote ccm BEGIN"),
            "本模块里出现了 POSIX 那一对围栏的字面量 —— 它只许有一个住址\
             （`sftp::CCM_PROFILE_BEGIN`），抄一份进来就是 `K13` 那族「一个性质两把尺子」"
        );
    }

    /// ★ `KR62D1` 的落盘那一跳：**装完之后用户原有的每一行都还在，而且块是真的块。**
    #[test]
    fn installing_into_a_posix_rc_keeps_every_user_line() {
        let td = tmpdir("posix-install");
        let p = td.0.join(".bashrc");
        std::fs::write(&p, BARE_RC).expect("写夹具");
        install_to_profile(&p, "cc", true).expect("装进 POSIX rc");
        let after = std::fs::read_to_string(&p).expect("读回");
        for line in BARE_RC.lines() {
            assert!(
                pinned(&after, line),
                "用户原来那一行没了：{line:?} —— `K31`：一个字节都不许删用户的行"
            );
        }
        // 装进去的确实是那份 snippet（按整行比，不按子串）。
        for line in crate::sftp::CCM_WRAPPER_SNIPPET.trim().lines() {
            assert!(pinned(&after, line), "别名块少了一行：{line:?}");
        }
        // 幂等：再装一次一个字节都不变。
        install_to_profile(&p, "cc", true).expect("再装一次");
        assert_eq!(
            std::fs::read_to_string(&p).expect("读回"),
            after,
            "第二次安装改了文件 —— 不幂等的安装器会在 rc 里堆出两份块"
        );
        // 扫得出「已装」，而且这一格此前在 POSIX 上恒 false。
        let scan = scan_path(&p, "cc");
        assert!(
            scan.has_ccm_block,
            "装完却扫不出块 —— 界面会说「未安装」且藏起卸载按钮"
        );
        // 卸得干净，用户的行还在。
        uninstall_from_profile(&p).expect("卸");
        let stripped = std::fs::read_to_string(&p).expect("读回");
        assert!(
            !stripped.contains("cc-monitor remote ccm"),
            "卸完还剩围栏残骸"
        );
        for line in BARE_RC.lines() {
            assert!(pinned(&stripped, line), "卸载吃掉了用户那一行：{line:?}");
        }
        for entry in std::fs::read_dir(&td.0).unwrap().filter_map(|e| e.ok()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    /// ★ 方言按**文件是什么**定，不按**机器是什么**定；而且路径仍然由人选。
    #[test]
    fn the_flavour_follows_the_file_not_the_machine() {
        use std::path::Path;
        for ps in [
            "/home/u/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
            "C:/x/profile.PS1",
        ] {
            assert_eq!(flavor_of(Path::new(ps)), ProfileFlavor::PowerShell, "{ps}");
        }
        for rc in [
            "/home/u/.bashrc",
            "/home/u/.zshrc",
            "/home/u/.profile",
            "/home/u/.config/fish/config.fish",
        ] {
            assert_eq!(flavor_of(Path::new(rc)), ProfileFlavor::PosixRc, "{rc}");
        }
    }

    /// ★★ `KR62D2` 的正题：**一份没有任何围栏的 rc，那些裸行逐行指得出来。**
    ///
    /// **死值验**：把查法换回「只认围栏」⇒ 这一条当场红。
    /// 本条自带那把反向尺子（下面第一段）：**围栏那条路在同一份输入上恒空** ——
    /// 所以「只加两条路径给 `scan_legacy_profiles`」在这里不可能变绿。
    #[test]
    fn bare_lines_with_no_fence_at_all_are_named_line_by_line() {
        // ① 反向尺子：围栏那条路在这份输入上**什么都看不见**。
        assert!(
            !find_block_version(BARE_RC).0,
            "这份夹具里居然有围栏 —— 那它就证不了「够不着裸行」这件事"
        );
        assert_eq!(
            crate::fenced_block::find_pair(
                BARE_RC,
                crate::sftp::CCM_PROFILE_BEGIN,
                crate::sftp::CCM_PROFILE_END,
                "夹具"
            ),
            Ok(None),
            "围栏配对在这份输入上必须是「没有」—— 这正是 `K-R57` 现打用户机器的形状"
        );

        // ② 正题：逐行指名，行号与原文都要对。
        let hits = scan_legacy_rc_lines(BARE_RC);
        let got: Vec<(usize, &str)> = hits.iter().map(|h| (h.line_no, h.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (4, "# ccm 启动器"),
                (5, "cc()   { ccm \"$@\"; }"),
                (6, "cct()  { ccm --tmux \"$@\"; }"),
                (7, "oo()   { ccm --agent codex \"$@\"; }"),
                (8, "zcc()  { ccm --account z \"$@\"; }"),
                (9, "alias  ccx='ccm --base'"),
                (11, "# 与 ccm 无关的一行"),
            ],
            "逐行指名对不上 —— 行号错一位，用户按着它去删就会删错行"
        );

        // ③ 分类：四条函数、两条注释、一条 alias。分类错了提示的措辞就会错。
        let kinds: Vec<LegacyRcKind> = hits.iter().map(|h| h.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LegacyRcKind::Comment,
                LegacyRcKind::Function,
                LegacyRcKind::Function,
                LegacyRcKind::Function,
                LegacyRcKind::Function,
                LegacyRcKind::Other,
                LegacyRcKind::Comment,
            ]
        );
        assert_eq!(
            hits.iter()
                .filter_map(|h| h.name.clone())
                .collect::<Vec<_>>(),
            vec!["cc", "cct", "oo", "zcc"]
        );

        // ④ 与 ccm 无关的行一条都不许进来（`gs() { git status; }` 与 `export EDITOR=vim`）。
        assert!(
            !hits.iter().any(|h| h.text.contains("git status")),
            "把与 ccm 无关的行也指名了 —— 那会让用户去删他自己的东西"
        );
    }

    /// ★ `KR62D2`：**我们自己那一块不算「你的旧行」。**
    ///
    /// 三对围栏都要认：`profile_installer` 的、`sftp` 的、`account_aliases` 的。
    /// 认漏一对 ⇒ 装完之后界面立刻回头指着我们自己刚写的那几行说「这是旧的」。
    #[test]
    fn our_own_fenced_block_is_never_reported_as_the_users_old_lines() {
        let what = "/home/u/.bashrc";
        let installed =
            plan_install(ProfileFlavor::PosixRc, "export A=1\n", "cc", true, what).expect("装一次");
        let hits = scan_legacy_rc_lines(&installed);
        assert!(
            hits.is_empty(),
            "刚装完就把自己那一块指名成「你的旧行」了：{:?}",
            hits.iter().map(|h| h.line_no).collect::<Vec<_>>()
        );
        // 另两对围栏同样要让开。
        let mixed = format!(
            "# === cc-monitor aliases BEGIN v1 ===\n\
             if [ -r \"$HOME/.cc-monitor/account-aliases.sh\" ]; then . \"$HOME/.cc-monitor/account-aliases.sh\"; fi\n\
             # === cc-monitor aliases END ===\n\
             cc() {{ ccm \"$@\"; }}\n"
        );
        let hits = scan_legacy_rc_lines(&mixed);
        assert_eq!(
            hits.iter().map(|h| h.line_no).collect::<Vec<_>>(),
            vec![4],
            "围栏内外分不开 —— 只有第 4 行那条裸的才是用户自己的"
        );
    }

    /// ★★ `KR62D2` 的产物：**一段让用户自己动手的提示，而产品一个字节都不删。**
    #[test]
    fn the_hint_names_every_line_and_the_product_deletes_nothing() {
        let hits = scan_legacy_rc_lines(BARE_RC);
        let hint = render_manual_cleanup_hint("/home/u/.bashrc", &hits);
        assert!(!hint.is_empty(), "有裸行却生成了一段空提示");
        for h in &hits {
            assert!(
                hint.contains(&format!("第 {} 行", h.line_no)),
                "提示里没点名第 {} 行",
                h.line_no
            );
            assert!(
                pinned(
                    &hint,
                    &format!("  第 {} 行  {}", h.line_no, h.text.trim_end())
                ) || hint.lines().any(|l| l.starts_with(&format!(
                    "  第 {} 行  {}",
                    h.line_no,
                    h.text.trim_end()
                ))),
                "提示里那一行的原文被改写了 —— 用户要照着它去自己文件里认行"
            );
        }
        // 「会赢过我们那一块」这一格现算自 `shared/ccm-aliases.sh`，不是抄的名单。
        let builtin = crate::sftp::builtin_alias_names();
        assert!(
            !builtin.is_empty(),
            "自带别名名单是空的 —— 下面那一格会变成空真"
        );
        assert!(
            hint.contains("会赢过我们那一块"),
            "夹具里有 `cc()` / `cct()` 两条与自带块同名，提示必须说清「不删就不生效」"
        );
        // 措辞：**不许**是「请删除」。产品指名，不替人做决定。
        assert!(
            hint.contains("由你自己定"),
            "措辞必须把决定权留给用户 —— 我们够不着边界，猜一个边界去删是最坏的那条路"
        );
        assert!(
            hint.contains("一个字节都不会碰"),
            "提示要明说产品不动手（`K31` + 用户逐字「原本的配置要手动删除」）"
        );
        // 没有裸行时是空串（界面靠它决定这一块出不出现）。
        assert!(render_manual_cleanup_hint("/home/u/.bashrc", &[]).is_empty());
    }

    /// ★★ `K31`：**「查」这一路一个字节都不写。**
    ///
    /// 不是读注释读出来的：真落一份夹具、扫一遍、逐字节比 + 比 mtime，
    /// 并且断言目录里没多出任何文件（备份文件也算「动了机器」）。
    #[test]
    fn scanning_a_rc_changes_not_a_single_byte_on_disk() {
        let td = tmpdir("posix-scan");
        let p = td.0.join(".bashrc");
        std::fs::write(&p, BARE_RC).expect("写夹具");
        let before = std::fs::read(&p).expect("读");
        let before_n = std::fs::read_dir(&td.0).unwrap().count();

        let scan = scan_path(&p, "cc");
        assert!(!scan.manual_cleanup_hint.is_empty(), "扫出来的提示是空的");
        assert!(!scan.has_ccm_block, "这份夹具里没有块");
        assert_eq!(
            scan.conflicting_functions,
            vec!["cc".to_string()],
            "POSIX 那一形的同名函数（`cc() {{`）此前恒扫不出来 —— 那一格是空的"
        );

        assert_eq!(std::fs::read(&p).expect("读回"), before, "扫描改了文件内容");
        assert_eq!(
            std::fs::read_dir(&td.0).unwrap().count(),
            before_n,
            "扫描在目录里留下了东西（备份 / 临时文件）—— 那也是「动机器」"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R132`：**装上了、能跑、用户敲不到** —— 这四条判据盘的是哪一半
    // ═══════════════════════════════════════════════════════════════════════
    //
    // ## 🔴 诚实边界，先写在这里，别读大
    //
    // `K-R129` 那一形（**真装完、真开一个终端、真敲一次**）**这四条一条都盘不住**，
    // 而且不是「今天还没写」，是**在构造上盘不住**：它要一台 Windows、要真跑一遍
    // NSIS `/S` / MSI `/qn`、要开一个新会话。本门禁的 npm / tsc / e2e / cargo
    // 全跑在 Linux 沙箱里 —— `scripts/gate.sh` 的 `GATE_BLIND` 里
    // `windows-runner` 那一条早就逐字写着这件事。
    //
    // ⇒ **这四条盘的是「片段生成」那一半**：我们**要写进用户 profile 的那几行**
    // 长不长得对、指不指得到真落点、有没有踩 Windows 改 PATH 的那两个经典地雷。
    // **「写进去之后终端里敲得到吗」仍然只有真机答得了**，本件的真机读数住
    // `evidence/K-R132-摸底.md`。
    //
    // ⚠ 别把这段话读成「所以这几条没用」：`K-R129` 那条缺陷的**直接死因**就是
    // 「这几行里根本没有 PATH 这回事」，而那一半恰好是这里盘得住的。

    /// ★★ 〔`KR135D1b` / `R86` 09-15〕**这一条是上一轮那条判据翻过来的那一面。**
    ///
    /// 上一轮那条判据（`K-R132` 立的）钉的是**反过来**的事：
    /// 「块里**必须有**一段把我们那个 bin 目录放上 `$env:PATH` 的代码」。
    /// `R86` 把那一段删了 ⇒ **那条判据失去了对象**。件文件逐字要求
    /// **别留一条永远绿的空判据** ⇒ 这里不是删掉了事，是**翻正成棘轮**：那一段不许再回来。
    ///
    /// # 它守的不是风格，是那个「半通」状态
    ///
    /// 会话级 `$env:PATH` 只对 PowerShell 有效（`cmd` 不读任何 profile、
    /// Git Bash 读 `~/.bashrc`）⇒ 一旦有人把它加回来，`K-R129`/`K-R132` 两轮真机
    /// 3×3 表里那个「**PowerShell 里能、`cmd` 里不能**」的格子当场复活，
    /// 而**「看起来装好了、换个终端就没了」比「哪儿都没有」更难查**。
    ///
    /// # 地板先证（testing.md 判据规则 7：先证够得到，否则「零违例」是空真）
    ///
    /// 两道地板，缺一不可：① 渲染出来不是空的、围栏在、placeholder 填掉了；
    /// ② **正向**那一道 —— 块里真有 `__ccm_bind`。少了 ②，
    /// 「把整个模板掏空」这一刀照样让下面每一条「不含」成立。
    ///
    /// # 诚实边界
    ///
    /// 它只看**我们生成的那一块**。用户自己在围栏外写的任何 `$env:PATH` 一概不管
    /// （那是用户的文件，`K31`）；`v3.8.0` 已经写进某人 profile 里的那段**旧块**
    /// 也不在射程内 —— 那一格靠「用户再走一次装或卸就整块重写」接住，
    /// 逐字住本文件上面那段横幅，**不是自动消失**。
    ///
    /// # 死值验（`KR135D1b` 刀①）
    ///
    /// 往 `render_cc_code` 的拼装处加回一段 `$env:PATH = "<我们的 bin 目录>;$env:PATH"` ⇒ 本条红。
    #[test]
    fn the_powershell_block_never_touches_the_session_path_again() {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        for include_cc in [true, false] {
            let out = render_cc_code("cc", include_cc);
            // ── 地板①：这份东西本身得是真的 ──────────────────────────────
            assert!(
                !out.trim().is_empty(),
                "渲染出来是空的 —— 下面每一条「不含」都成了空真"
            );
            assert!(
                out.contains(BEGIN_MARKER) && out.contains(END_MARKER),
                "围栏没了：这一块装进去就卸不掉（include_cc_function={include_cc}）"
            );
            assert!(
                !out.contains("{{CC_FUNCTION_BLOCK}}"),
                "placeholder 没被填掉（include_cc_function={include_cc}）"
            );
            // ── 地板②：正向那一道 —— 模板没被掏空 ────────────────────────
            assert!(
                guard_core::contains_word(&out, "__ccm_bind"),
                "连 `__ccm_bind` 都不在了 —— 模板被掏空，下面的「不含」全是空真\n{out}"
            );
            // ── 棘轮：会话级 PATH 那一段不许再回来 ────────────────────────
            let touches: Vec<&str> = out.lines().filter(|l| l.contains("$env:PATH")).collect();
            assert!(
                touches.is_empty(),
                "这一块又在动会话级 `$env:PATH` 了（include_cc_function={include_cc}）。\n\
                 `R86` 逐字把这一段删了，理由是它造出「PowerShell 里能、`cmd` 里不能」\
                 那个半通状态 —— 要让 `{word}` 在**所有**终端里都找得到，\
                 走的是**用户级** PATH 那一条（`render_user_path_setup_command`，\
                 由用户点一下）。\n实得 {} 行：\n{:#?}",
                touches.len(),
                touches
            );
            // 持久化那一族更不许出现在 profile 里：这一块每开一个会话跑一次，
            // 把写注册表放在这里 = 每开一个终端就改一次用户的环境（`K33` 明禁）。
            assert!(
                !out.contains("SetEnvironmentVariable"),
                "profile 块里出现了 `SetEnvironmentVariable` —— 这一块每开一个会话跑一次，\
                 放在这里等于每开一个终端改一次用户环境。\n{out}"
            );
        }
    }

    /// ★★ `KR132D2` 的第二半：**PATH 上写的那个目录，就是我们真放 `ccm` 下去的那个。**
    ///
    /// # 为什么单独一条：上一条只证「这两处一致」，这一条证「它们对得上现实」
    ///
    /// 上一条比的是**我们生成的那段命令**与 `tool_registry` 的申报 —— **两边同时改错**
    /// 照样全绿。真落点住在**第三处**：`local_daemon.rs` 里算 `extract_dir` 的那一行，
    /// 它就是 `install_local_ccm_entry` 的 `dir` 实参。⇒ 这一条去读**那一行源码**。
    ///
    /// # 诚实边界
    ///
    /// 它按**源码文本**对拍，不是跑起来量的（跑起来要一台 Windows）。
    /// 所以它认得的是「那一行长什么样」；有人改成用一个中间变量拼路径，
    /// 它会**红**而不是静默放过 —— 那是有意的：这一格宁可误红逼人来看，
    /// 也不要在「PATH 指向一个空目录」这件事上假绿。
    ///
    /// # 死值验（`KR132D2` 刀④）
    ///
    /// 把 `tool_registry` 里 `ccm` 本机载体的落点改成别的目录 ⇒ 本条红
    /// （而上一条**不红** —— 它是自洽的）。
    #[test]
    fn the_path_line_points_at_the_directory_we_really_install_ccm_into() {
        let dir = crate::tool_registry::local_ccm_bin_dir_rel().expect("申报的 bin 目录");
        // `.cc-monitor/bin` ⇒ `.join(".cc-monitor").join("bin")` —— 真落点那一行的形状。
        let want: String = dir
            .split('/')
            .map(|seg| format!(".join(\"{seg}\")"))
            .collect::<Vec<_>>()
            .join("");

        // 真落点：`install_local_ccm_entry` 的 `dir` 实参是 `local_daemon.rs` 算的 `extract_dir`。
        //
        // ⚠ **比之前先把空白抹掉**：`ccm_probe.rs` 那一处是
        // `h.join(".cc-monitor")\n            .join("bin")`（rustfmt 断的行）——
        // 按原文 `contains` 会**漏掉它**，而漏掉的那一形正是「判据够不着」，
        // 不是「那一处不存在」。这一刀是现打出来的：第一版就栽在这里。
        let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let want = squeeze(&want);
        let hosts: [(&str, &str); 2] = [
            ("local_daemon.rs", include_str!("local_daemon.rs")),
            ("ccm_probe.rs", include_str!("ccm_probe.rs")),
        ];
        for (name, raw) in hosts {
            let prod = squeeze(&guard_core::production_code(raw));
            assert!(
                prod.contains(&want),
                "`{name}` 的生产段里找不到 `{want}` —— 也就是说\
                 「PATH 上写的那个目录」与「盘上真放 ccm 的那个目录」已经分家了。\n\
                 PATH 那一侧现算自 `tool_registry` 申报的 `{dir}`；\
                 要么那张表改错了，要么真落点搬了家而这一侧没跟。\n\
                 ⚠ 指错目录与根本没补 PATH，在用户终端上是**同一个结果**（命令找不到）。"
            );
        }
    }

    /// ★★ `KR132D1` ＋〔`KR135D1` 09-15 **扩到「撤」那一侧**〕：
    /// **那两条要在用户机器上改 PATH 的命令，一条都不许踩 Windows 的那两个经典地雷。**
    ///
    /// 上一轮这条判据只数「加」那一条。`R85` 用户逐字「**也能管理删除**」把「撤」那一半
    /// 加进来了，而件文件 `§0b` 逐字点名：**做「撤」那一半会再撞一次同一个坑**
    /// ⇒ 这里把人群从**一条**扩成**两条**，两条走同一批断言。
    ///
    /// | 地雷 | 后果 | 撤这一侧为什么更重 |
    /// |---|---|---|
    /// | `setx` | 值截断在 1024 字符，只给一句警告、**退出码照样 0** ⇒ 静默截断用户 PATH | 加那一次截掉的是「多出来的尾巴」，撤是**整条重写** ⇒ 截掉的是用户本来就有的那一截 |
    /// | 拿 `$env:PATH` 当新值写回用户级 | 进程里那份是**机器级 ＋ 用户级拼起来的** ⇒ 整条系统 PATH 被复制进用户 PATH，此后系统 PATH 的更新对这个用户永久失效 | 「先读一下现在的 PATH」在撤的语境里读起来**格外顺手** |
    ///
    /// # 撤这一侧独有的第三条：**只摘自己那一段**
    ///
    /// 过滤必须是**整格**（`-ne $d`）。`-replace` / `-like` / `.Replace(` 全是**子串**口径，
    /// 用户 PATH 上有 `…\.cc-monitor\bin-old` 时会被一起打掉 —— 同一个病在加那一侧
    /// 只是「误判成已经在了」，在撤这一侧是**误删用户的目录**，不可逆。
    /// ⚠ 还要数一条**反向**的：不许出现 `Where-Object { $_ }` 那种「顺手把空项也滤掉」
    /// —— 空项在 Windows 上语义是「当前目录」，删它是一次我们没被要求做的改动。
    ///
    /// # 死值验（`KR135D1` 刀①/②）
    ///
    /// ① 把 [`render_user_path_setup_command`] 改成 `setx PATH "%PATH%;…"` ⇒ 本条红；
    /// ② 把 [`render_user_path_removal_command`] 的 `-ne $d` 改成
    ///    `-notlike "*$d*"` ⇒ 本条红（撤那一侧的整格断言）。
    #[test]
    fn the_generated_path_command_edits_only_the_user_scope_and_never_via_setx() {
        let both = [
            (
                "加",
                render_user_path_setup_command().expect("生成的「加」命令"),
            ),
            (
                "撤",
                render_user_path_removal_command().expect("生成的「撤」命令"),
            ),
        ];
        for (which, cmd) in &both {
            // ── 地板：它确实在写 PATH，而不是一段无关的文字 ──────────────
            assert!(
                !cmd.trim().is_empty(),
                "「{which}」生成出来是空的 —— 下面全是空真"
            );
            assert!(
                cmd.contains("SetEnvironmentVariable('Path', ") && cmd.contains("'User')"),
                "「{which}」没有在写**用户级** PATH：\n{cmd}"
            );
            assert!(
                cmd.contains("GetEnvironmentVariable('Path', 'User')"),
                "「{which}」的新值不是从**用户级**那一档读出来的 —— 拿 `$env:PATH`\
                 （机器级＋用户级拼起来的那一份）当基准，会把整条系统 PATH 复制进用户 PATH。\n{cmd}"
            );
            // ── 两个地雷，两侧同一批 ─────────────────────────────────────
            for landmine in ["setx", "'Machine'", "\"Machine\"", "$env:PATH"] {
                assert!(
                    !cmd.contains(landmine),
                    "「{which}」那条命令里出现了 `{landmine}` —— 见本判据头注那张表。\n{cmd}"
                );
            }
        }
        // ── 加：幂等（跑两次不许把目录塞两遍）──────────────────────────
        let add = &both[0].1;
        assert!(
            add.contains("-notcontains $d"),
            "「加」跑第二次会重复追加：\n{add}"
        );
        // ── 撤：整格，而且**只**摘我们那一格 ──────────────────────────
        let del = &both[1].1;
        assert!(
            del.contains("-ne $d"),
            "「撤」不是按**整格**比的 —— 子串口径会把 `<我们那段>-old` 之类一起删掉，\
             而那是删用户的东西，不可逆。\n{del}"
        );
        for substr_op in ["-replace", "-like", "-notlike", "-match", ".Replace("] {
            assert!(
                !del.contains(substr_op),
                "「撤」用了子串口径 `{substr_op}` —— 见本判据头注「撤这一侧独有的第三条」。\n{del}"
            );
        }
        assert!(
            !del.contains("Where-Object { $_ }"),
            "「撤」顺手把 PATH 上的空项也滤掉了 —— 空项在 Windows 上语义是「当前目录」，\
             删它是一次**我们没被要求做的改动**。除我们那一格之外要逐字复原。\n{del}"
        );
        // ── 产品这一侧今天一个字节都不执行 ────────────────────────────
        //
        // 🔴 **这条断言的理由在 `R85` 之后变了，别照旧读**：上一轮它的理由是
        //   「产品自己跑它就变成了替用户改他的环境」；`R85` 用户逐字
        //   「**应该让用户手动点击加**」⇒ **点击即执行是允许的**（`§0c`：`K33` 禁的是
        //   产品**替**用户决定，用户点一下就是用户自己决定）。
        //   它今天仍然绿，是因为**那一跳还没接进来** —— 接它要动
        //   `lib.rs`（`generate_handler!`）· `parity_ledger.rs`（双向相等）·
        //   `write_site_registry.rs`（`SPAWNS` 默认拒绝），**三份都在 `K-R135` 的写区之外**
        //   ⇒ 本件把机制与判据做完、那一跳交回 PM（`§8`）。
        //   ⚠ 接进来的那一拍**必须同拍**改这条断言与那三份登记，别让它继续挡路
        //   （testing.md 规则 12：出路是重新裁定，不是悄悄删掉）。
        let prod = guard_core::production_code(include_str!("profile_installer.rs"));
        assert!(
            !prod.contains("Command::new("),
            "本模块的生产段里起了进程。今天这一条仍然成立是因为**执行那一跳还没接进来**；\
             真要接（`R85` 允许点击即执行），同拍要做的三件事逐字写在本断言上面那段注释里 —— \
             改了这里却没改那三份登记，门禁会在别的格子红，而红的原因读起来与这里无关。"
        );
    }

    /// ★★ 〔`KR135D1` 09-15〕**那一格的「现在状态」：`~/.cc-monitor\bin` 在不在
    /// 用户级 PATH 上 —— 它与那两条命令必须用**同一种相等**。**
    ///
    /// # 为什么这一条单独存在：不一致的代价是**界面撒谎**
    ///
    /// 加用 `-notcontains $d`、撤用 `-ne $d`，两者在 PowerShell 里都是
    /// **整格 · 大小写不敏感**。状态那一格要是比它们**聪明**（做了路径规范化），
    /// 就会出现「显示 ✓、点一下又多出一格重复」；要是比它们**笨**，
    /// 就会出现「显示 ✗、点了没反应」。**两边同样地笨，比一边聪明一边笨要好。**
    ///
    /// # 地板先证
    ///
    /// 第一条断言是**正向**的（整格命中要认得出）—— 少了它，
    /// 一个恒回 `false` 的实现能让下面每一条 `!` 断言全绿。
    ///
    /// # 死值验（`KR135D1` 刀③）
    ///
    /// 把 [`user_path_has_our_bin`] 的 `split(';').any(整格相等)` 换成
    /// `user_path_raw.contains(dir_abs)` ⇒ 本条红（`-old` 那两格）。
    #[test]
    fn the_user_path_status_uses_the_same_equality_as_the_generated_commands() {
        let dir = r"C:\Users\me\.cc-monitor\bin";
        // ── 地板：整格命中真认得出（否则下面全是空真）──────────────────
        assert!(
            user_path_has_our_bin(&format!(r"C:\Windows;{dir};C:\foo"), dir),
            "整格命中都认不出来 —— 判据够不着被测对象了，先修实现"
        );
        assert!(user_path_has_our_bin(dir, dir), "独占一格认不出");
        assert!(
            user_path_has_our_bin(&format!(r"C:\Windows;{dir}"), dir),
            "在末尾认不出"
        );
        // ── 大小写不敏感：PowerShell 的 -contains / -ne 就是这样 ────────
        assert!(
            user_path_has_our_bin(r"C:\Windows;c:\users\me\.cc-monitor\BIN;C:\foo", dir),
            "大小写不一样就认不出了 —— 而那两条命令认得出 ⇒ 状态与按钮会各说各话"
        );
        // ── 比整格，不比子串：`-old` 这一形正是要挡的那个 ──────────────
        assert!(
            !user_path_has_our_bin(&format!("{dir}-old"), dir),
            "子串比法：`<我们那段>-old` 被读成「已经在了」，然后「加」那个按钮什么都不做"
        );
        assert!(!user_path_has_our_bin(
            &format!(r"C:\Windows;{dir}-old"),
            dir
        ));
        assert!(!user_path_has_our_bin(&format!(r"{dir}2;C:\x"), dir));
        // ── 末尾反斜杠：**刻意判 false**（代价写在函数头注里）──────────
        assert!(
            !user_path_has_our_bin(&format!(r"{dir}\"), dir),
            "这一格刻意与那两条命令**同样地笨** —— 它们也不砍末尾反斜杠。\
             要改成规范化，三处（本函数 ＋ 两条命令）必须同拍一起改"
        );
        // ── 拿不到目录时恒 false：不许把「拿不到」读成「已经装好了」──────
        assert!(
            !user_path_has_our_bin(r"C:\a;;C:\b", ""),
            "空目录与 PATH 上的空项相等了"
        );
        assert!(!user_path_has_our_bin("", ""));
        assert!(!user_path_has_our_bin("", dir));
    }

    /// ★★ 〔`KR135D3` 09-15〕**那份共用的 POSIX 别名 snippet，真的把两边申报的
    /// `ccm` 目录都放上了 PATH，而且本机那个赢。**
    ///
    /// # 病是什么（`R80 §二` 的 `R2`，现打出来的，不是推理）
    ///
    /// `shared/ccm-aliases.sh` 是**一份文件、两个消费者**：本机走
    /// [`plan_install`] 的 POSIX 方言合进用户选的那份 rc，远端走
    /// `sftp::install_remote_ccm_helper` 合进远端 rc —— **合进去的是逐字同一份文本**。
    /// 而两边的 `ccm` 落点**不是同一个目录**（`tool_registry::TOOLS` 现算：
    /// 本机 `.cc-monitor/bin`、远端 `.local/bin`）。
    /// ⇒ 那一行只写一个目录时，**它只可能对其中一边是对的**。
    /// 上一轮它只写 `~/.local/bin` ⇒ **对远端对、对本机错**，
    /// 而那正是 `R80` 逐字记下的那条预判（干净机器上 `ccm`/`cc`/`cct` 全 command not found）。
    ///
    /// # 修法为什么是「两个都加」而不是「改成本机那个」
    ///
    /// 改成本机那个只是把错换到另一边。这份 snippet **必须只有一个住址**
    /// （同文件另有一条判据在数 `include_str!` 恰好一处）⇒ 不能按边分叉
    /// ⇒ 两个都上，各自那台机器上只有一个真的存在（不存在的那个在 PATH 上无害）。
    ///
    /// # 顺序是承重的，不是风格
    ///
    /// `.cc-monitor/bin` 必须**赢过** `.local/bin`：后者是用户那份**旧的** `ccm`
    /// 住的地方（`K34` 逐字「原本的配置要手动删除」，产品一个字节都不删）。
    /// 它输了的那一形有人在数：`ccm_probe::classify_path_ccm` 的 `NotOurs`。
    ///
    /// # 🔴 诚实边界 —— 这一条**证不了** POSIX 那一臂通了
    ///
    /// 它量的是「这份 snippet 被 `source` 之后 `$PATH` 长什么样」，
    /// **不是**「干净 Linux 机上装完真敲得到 `ccm`」—— 后者要一台干净 Linux 机，
    /// 本件没有。⇒ `R80` 的 `R2` 今天仍然**既没被证实也没被证伪**，
    /// 被治掉的只是它的**静态成因**。别把这一条读成「验过了」。
    ///
    /// ⚠ 它跑在 `cfg(unix)` 下（要 `bash` 来 `source`）。Windows 上这条不出声 ——
    /// 而那**不是漏**：这份 snippet 本来就只装进 POSIX rc。
    ///
    /// # 死值验（`KR135D3` 刀①）
    ///
    /// 把那一行改回只有 `$HOME/.local/bin` ⇒ 本条红（本机那个目录不在 PATH 上）。
    #[cfg(unix)]
    #[test]
    fn the_shared_alias_snippet_really_puts_both_ccm_dirs_on_path_local_first() {
        let local = crate::tool_registry::local_ccm_bin_dir_rel().expect("本机申报的 bin 目录");
        let remote = crate::tool_registry::remote_ccm_bin_dir_rel().expect("远端申报的 bin 目录");
        assert_ne!(
            local, remote,
            "两边落点变成同一个了 —— 那本条的前提（一份文本、两个落点）就没了。\
             这不是放宽它的理由：回到 `KR135D3` 重裁一次"
        );
        let td = tmpdir("aliassnip");
        let home = td.0.join("h");
        std::fs::create_dir_all(&home).expect("造假家目录");
        let snip = td.0.join("snippet.sh");
        std::fs::write(&snip, crate::sftp::CCM_WRAPPER_SNIPPET).expect("写 snippet");

        // **量真实输出，不量源码**（`brief` 第 5 条）：真 source 一趟，问 `$PATH`。
        let run = |times: usize| -> String {
            let dots = ". '".to_string() + &snip.display().to_string() + "'; ";
            let script = dots.repeat(times) + "printf '%s' \"$PATH\"";
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(&script)
                .env("HOME", &home)
                .env("PATH", "/usr/bin:/bin")
                .output()
                .expect("跑 bash —— 沙箱里没有 bash 的话这条判据够不着被测对象");
            assert!(
                out.status.success(),
                "source 那份 snippet 直接失败了（这本身就是一条缺陷）：\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).to_string()
        };

        let path = run(1);
        // ── 地板：真读到东西了，而且原来的 PATH 没被吃掉 ──────────────
        assert!(
            path.contains("/usr/bin"),
            "原来的 PATH 不见了 —— 这一行把用户的 PATH 覆盖掉了，而它只该往前插。\n实得：{path}"
        );
        let want_local = format!("{}/{local}", home.display());
        let want_remote = format!("{}/{remote}", home.display());
        let segs: Vec<&str> = path.split(':').collect();
        for (who, want) in [("本机", &want_local), ("远端", &want_remote)] {
            assert!(
                segs.contains(&want.as_str()),
                "{who}那个 `ccm` 落点不在 PATH 上：`{want}`。\n\
                 ⚠ 只写一个目录时这一行**只可能对其中一边是对的** —— 见本判据头注。\n\
                 实得 {} 段：{segs:?}",
                segs.len()
            );
        }
        // ── 顺序：本机那个要赢过远端那个（旧的 ccm 住在远端那个目录里）──
        let i_local = segs
            .iter()
            .position(|s| *s == want_local)
            .expect("本机那格");
        let i_remote = segs
            .iter()
            .position(|s| *s == want_remote)
            .expect("远端那格");
        assert!(
            i_local < i_remote,
            "`{want_local}`（我们放下去的那一份）排在 `{want_remote}`（你那份旧的住处）后面 —— \
             那样赢的是旧的那个，而 `ccm_probe::classify_path_ccm` 会把它记成 `NotOurs`。\n\
             实得：{segs:?}"
        );
        // ── 幂等：source 两趟不许让 PATH 长一截 ────────────────────────
        assert_eq!(
            run(2),
            path,
            "source 两趟 PATH 变了 —— 「已经在就不插」那一道破了，嵌套 shell 会让 PATH 无限变长"
        );
    }

    /// ★★ 〔`KR135D2` 09-15〕**`cc` 翻正了 —— 这一条是那处反向锚点翻过来的那一面。**
    ///
    /// 上一轮那条判据（`K-R132` 立的，名字里逐字写着「仍然绕过后端、而这是登记过的、
    /// 不是忘了」）钉的是「今天这一行就是 `& claude`」这处**已登记的不一致**。
    /// testing.md 判据规则 12：反向锚点的合法出路只有「**重新裁定**」——
    /// `KR135D2` 就是那一次重新裁定，于是它连名字一起翻正。
    ///
    /// # 现在钉的是什么：**两臂的 `cc` 走同一条路**
    ///
    /// PowerShell 那一臂 `& ccm $RemainingArgs`、POSIX 那一臂 `cc() { ccm "$@"; }`
    /// —— `K33`「所有命令只许有一处」＋ `K26`「`ccm` 就是后端的原生命令行入口」＋
    /// `K28`「一切对外都经后端」。⚠ 两边都**现读**（一边读渲染产物、一边读
    /// `CCM_WRAPPER_SNIPPET`），一个名字都不抄第二份。
    ///
    /// # 🔴 它同拍仍然钉住「别只改一边」
    ///
    /// `launcher_identity_registry` 把这一行登记成 `T2` 的锚点。`K-R132` 上一轮现打验过：
    /// 动这一行会让**本条 ＋ 那条 `T2`** 同时红。本轮两处一起改了，
    /// 而这条性质**没变** —— 下一个人再动它，仍然是两条一起红。
    ///
    /// # 🔴 诚实边界（别读大）
    ///
    /// 它证的是「**生成的文本指向 `ccm`**」，**不是**「敲下去真起得来」。
    /// 后者要一台 Windows，而本门禁跑在 Linux 沙箱里（`GATE_BLIND` 的 `windows-runner`）。
    /// **本件没有在真机上把 `cc` 跑过一趟**，这一格如实记在 `§8`。
    ///
    /// # `cct` 那一半（维持上一轮的棘轮）
    ///
    /// POSIX 那一臂里 `cct() { ccm --tmux "$@"; }`，而 **Windows 上没有 tmux**
    /// ⇒ 这一臂**刻意不生成 `cct`**：给它一个「名字在、行为不在」的壳比没有更坏
    /// （`K-R129` 那位用户正是照文案敲了 `cct`）。**这半条是棘轮，不是发现。**
    ///
    /// # 死值验（`KR135D2` 刀①）
    ///
    /// 把那一行改回 `& claude $RemainingArgs` ⇒ 本条 ＋ `T2` 那条同时红。
    #[test]
    fn the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one() {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        let out = render_cc_code("cc", true);
        assert!(
            pinned(&out, "function cc {"),
            "地板没了：这一支本来就该生成 `function cc`\n{out}"
        );
        // ── 正题：那一行走 `ccm` ─────────────────────────────────────────
        assert!(
            pinned(&out, &format!("    & {word} $RemainingArgs")),
            "PowerShell 那一臂的 `cc` 不走 `{word}` 了。`K33`「所有命令只许有一处」＋ \
             `K28`「一切对外都经后端」—— 改回直呼别的东西，等于让同一个名字\
             在两个平台上是两件东西（账号 / 工作目录 / agent 选择这几维在 Windows 上\
             整条够不着）。\n{out}"
        );
        // ── 反面：这一块里**只许有这一条**调用行 ─────────────────────────
        //
        // ⚠ 不写成 `!out.contains("claude")`：模板里本来就有两处 `claude`
        //   （`.claude\claudecode-frontend` 这个路径、以及一句注释）⇒ 那样写第一天就是红的，
        //   而「第一天就红的判据」的唯一出路是放宽它。⇒ 人群收成「调用行」这一形。
        let invokes: Vec<String> = out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.starts_with("& "))
            .collect();
        assert_eq!(
            invokes,
            vec![format!("& {word} $RemainingArgs")],
            "这一块里的**调用行**应当恰好一条、而且走 `{word}`。\n\
             多一条 = 旁边又加了一条别的路（后一条赢，而读的人看见前一条）；\n\
             0 条 = `cc` 什么都不调了。\n逐字：\n{out}"
        );
        // ── POSIX 那一臂现读，不抄：抄一份就是第二个住址 ──────────────────
        assert!(
            crate::sftp::CCM_WRAPPER_SNIPPET
                .lines()
                .any(|l| l.trim_start().starts_with("cc()") && guard_core::contains_word(l, word)),
            "POSIX 那一臂的 `cc` 不走 `{word}` 了 —— 本判据钉的是**两臂一致**，\
             一致地错也是红"
        );
        // ── `cct`：这一臂不发明它 ────────────────────────────────────────
        for rendered in [render_cc_code("cc", true), render_cc_code("cc", false)] {
            assert!(
                !guard_core::contains_word(&rendered, "function cct"),
                "这一臂生成了 `cct`，而 Windows 上没有 tmux —— 见本判据头注"
            );
        }
    }

    /// ★★ `KR132D2` 的第三半，**真机逮到的那一条**：
    /// **PowerShell profile 落盘要带 BOM，POSIX rc 一个字节都不许带。**
    ///
    /// # 这不是风格，是真机上一条「静默吞掉一行代码」的路
    ///
    /// win11 真机现打（读数住 `evidence/K-R132-摸底.md`）：
    /// 本块写成不带 BOM 的 UTF-8 之后，Windows PowerShell 5.1 按系统 ANSI 代码页
    /// （那台机器是 GBK）解它，一行 CJK 注释**把它下面那一行吃掉** ——
    /// `$ccmBinDir = …` 落进了注释里，于是 `$env:PATH` 被赋成 `";" + 原值`，
    /// 而 `parse-errors=0`、**一个报错都没有**。
    /// 同一趟还现打到：发出去的 `v3.8.0` 模板里 `$ccmDir` / `$deadline` / `$oldTitle`
    /// 三处赋值也是这么被吃掉的 ⇒ CJK 代码页上「终端集成」那套绑定一直是坏的。
    ///
    /// # 诚实边界（别读大）
    ///
    /// - **这条判据证不了「PS 5.1 从此解对了」** —— 那要一台 Windows，而本门禁
    ///   跑在 Linux 沙箱里（`GATE_BLIND` 的 `windows-runner`）。它证的是
    ///   **我们落盘的字节形状对**，而「形状对 ⇒ PS 解得对」那一步是真机量出来的。
    /// - **分母是一台机器**（系统代码页 GBK）。英文代码页上那条机制不成立。
    ///
    /// # 死值验（`KR132D2` 刀②）
    ///
    /// 把 [`encode_for_disk`] 的 PowerShell 那一支改成原样返回 ⇒ 本条红。
    #[test]
    fn the_powershell_profile_lands_with_a_bom_and_the_posix_rc_never_does() {
        let td = tmpdir("bom");
        // ── PowerShell 那一支：有 BOM，而且装两趟只有一个 ────────────────
        let ps = td.0.join("Microsoft.PowerShell_profile.ps1");
        std::fs::write(&ps, "# 我自己的一行\nWrite-Host hi\n").expect("写夹具");
        install_to_profile(&ps, "cc", true).expect("装第一趟");
        let b1 = std::fs::read(&ps).expect("读回");
        assert_eq!(
            &b1[..3],
            &[0xEF, 0xBB, 0xBF],
            "PowerShell profile 落盘没有 BOM —— PS 5.1 会按 ANSI 代码页解它，\
             而那条路在真机上**吃掉过一整行可执行代码**（见本判据头注）"
        );
        install_to_profile(&ps, "cc", true).expect("装第二趟");
        let b2 = std::fs::read(&ps).expect("读回");
        assert_eq!(
            b2, b1,
            "装两趟字节不一样 —— 幂等破了（多半是 BOM 叠了一层）"
        );
        assert_eq!(
            b2.windows(3).filter(|w| *w == [0xEF, 0xBB, 0xBF]).count(),
            1,
            "文件里有不止一个 BOM —— 剥 BOM 那一跳漏了某条路"
        );
        // 用户块外的内容一个字节都没丢（BOM 这一改最容易伤到的就是它）。
        let text = String::from_utf8(b2.clone()).expect("UTF-8");
        assert!(pinned(strip_bom(&text), "# 我自己的一行"), "用户内容丢了");
        assert!(pinned(&text, "Write-Host hi"), "用户内容丢了");
        // 卸干净之后 BOM 还在、块没了、用户内容还在。
        uninstall_from_profile(&ps).expect("卸");
        let after = std::fs::read_to_string(&ps).expect("读回");
        assert!(!after.contains(BEGIN_MARKER), "卸了之后围栏还在：\n{after}");
        assert!(pinned(&after, "Write-Host hi"), "卸载吃掉了用户内容");

        // ── POSIX 那一支：一个 BOM 都不许有 ──────────────────────────────
        let rc = td.0.join(".bashrc");
        std::fs::write(&rc, BARE_RC).expect("写夹具");
        install_to_profile(&rc, "cc", true).expect("装 rc");
        let rb = std::fs::read(&rc).expect("读回");
        assert_ne!(
            &rb[..3],
            &[0xEF, 0xBB, 0xBF],
            "往 POSIX rc 里写了 BOM —— `sh` 会把那三个字节当成一条命令，\
             这是把 PowerShell 那一侧的药灌给了另一个病人"
        );
        assert!(
            !rb.windows(3).any(|w| w == [0xEF, 0xBB, 0xBF]),
            "rc 里任何位置都不许出现 BOM"
        );
    }
}

/// U6a：把 **PS↔monitor 握手**的顺序与数字钉在 `doc/IPC-PROTOCOL.md` 上。
///
/// # 为什么需要这个
///
/// U6a 逐图核时序图时发现：图上画的是 **v2 修复之前**的握手 —— 顺序是旧的
/// （先写 await 文件、后设窗口标题）、deadline 写 800ms（实际早已是 3000ms）、
/// notify debouncer 写 100ms（实际 50ms）、monitor 侧的 ≤600ms 重试**整个没画**。
///
/// 也就是说：文档描述的是那个**已经被修掉的 bug 的行为**。照图重新实现一遍 PS 侧，
/// 会精确复刻 v2.21 那个「每个新 shell 首次 `cc` 固定烧满超时」的故障。
///
/// 顺序那一条尤其不能只靠注释：它是**两个文件之间的时序约束**，两边看起来都合理，
/// 只有合起来看才错。
#[cfg(test)]
mod handshake_doc_guard {
    const IPC_DOC: &str = include_str!("../../doc/IPC-PROTOCOL.md");
    const BIND_RS_RAW: &str = include_str!("bind.rs");
    const ARCH_DOC: &str = include_str!("../../doc/ARCHITECTURE.md");

    /// ★ **判据一律看剥掉注释之后的代码**。
    ///
    /// D 审计把这三条护栏**全部攻破**，手法都一样：把值改坏，再在旁边加一行
    /// 「沿革：以前是 …3000…」的注释 —— 护栏读的是整份原文，注释就把它喂饱了。
    /// 实测 deadline 3000→800、轮询 30→250、debouncer 50→500、
    /// 重试 12×50→3×10（旧模板用户唯一的活路缩成 30ms），**四条全绿**。
    ///
    /// daemon 那边的护栏早就走 `guard_support::production_code` 剥注释，
    /// 那个模块的注释里逐字写着「不剥的话守卫会被解释它自己的那段散文喂饱」。
    /// **同一个坑，隔一个 crate 又踩了一遍。**
    fn strip_comments(src: &str, line_comment: &str) -> String {
        src.lines()
            .map(|l| match l.find(line_comment) {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn bind_rs() -> String {
        strip_comments(BIND_RS_RAW, "//")
    }

    fn tpl() -> String {
        strip_comments(super::CC_TEMPLATE, "#")
    }

    /// v2 竞态修复的核心：**先设标题、再写 await 文件**。
    ///
    /// 反过来的话，monitor 的 notify 在文件落地瞬间就 EnumWindows 找 marker，
    /// **扫得越快越容易找不到窗口** ⇒ 删 await 走失败路径 ⇒ 绑定成败全凭时序运气。
    #[test]
    fn ps_template_sets_the_window_title_before_writing_the_await_file() {
        let t = &tpl();
        let title = t
            .find("$Host.UI.RawUI.WindowTitle = $marker")
            .expect("模板里找不到设标题那行 —— 抽取坏了还是握手改了？");
        let write = t
            .find("WriteAllText($awaitFile")
            .expect("模板里找不到写 await 文件那行 —— 抽取坏了还是握手改了？");
        assert!(
            title < write,
            "cc.ps1.tpl 把顺序换回了「先写文件、后设标题」—— 那正是 v2.21 \
             『每个新 shell 首次 cc 固定烧满超时』的成因（doc/IPC-PROTOCOL.md \
             § 跨进程握手时序图 有整段说明）。设标题@{title} 写文件@{write}"
        );
    }

    /// 模板里的 deadline / 轮询步长必须在协议文档里出现（防「改了代码忘了改图」）。
    #[test]
    fn handshake_timings_in_the_template_appear_in_the_protocol_doc() {
        let t = &tpl();
        let deadline = between(t, "AddMilliseconds(", ")").expect("模板里没有 deadline");
        let poll = between(t, "Start-Sleep -Milliseconds ", "\n").expect("模板里没有轮询步长");

        // 这两个数曾双双漂移：文档停在 800ms，实现早已 3000ms。
        assert!(
            IPC_DOC.contains(&format!("{deadline}ms")),
            "PS 握手 deadline 是 {deadline}ms，但 doc/IPC-PROTOCOL.md 里没有 `{deadline}ms` \
             —— 文档还停在旧数字上（上一次是 800ms）"
        );
        assert!(
            IPC_DOC.contains(&format!("{poll}ms")),
            "PS 轮询步长是 {poll}ms，但 doc/IPC-PROTOCOL.md 里没有 `{poll}ms`"
        );
    }

    /// monitor 侧同理：debouncer 窗口 + 「找不到窗口就重试」的总时长。
    ///
    /// 重试那一条是**旧模板用户唯一的活路**（老 profile 不会自动更新），
    /// 图上却整个没画 —— 少画它等于把兼容性保障说成不存在。
    /// ★ **每一份描述这个握手的文档**都必须写当前顺序，不只 IPC-PROTOCOL.md。
    ///
    /// U6a 实测：同一个顺序被写在**五处** —— `cc.ps1.tpl`（真相）· `bind.rs` 模块头 ·
    /// `doc/IPC-PROTOCOL.md` 时序图 · `doc/ARCHITECTURE.md` · `cc_integration.ts` 的 UI 文案。
    /// v2 反转顺序时**只改了真相那一处**，另外四处全停在旧顺序上。
    /// 只钉一份文档，剩下几份照样把人教回旧写法。
    #[test]
    fn every_doc_that_describes_the_handshake_states_the_current_order() {
        // 判据：一段文字若同时提到 `ps-await` 与 `WindowTitle`，就算「在描述这个握手」，
        // 那它必须把 `WindowTitle` 写在 `ps-await` 前面（= v2 的真实顺序）。
        // 必须从**描述握手那一节**起算，不能拿全文首次出现比 —— IPC-PROTOCOL.md 有一整节
        // 就叫 `ps-await/<PID>.json`，排在时序图**之前**，全文首现比法会恒红（实测踩到）。
        for (name, doc, anchor) in [
            ("doc/IPC-PROTOCOL.md", IPC_DOC, "## 跨进程握手时序图"),
            ("doc/ARCHITECTURE.md", ARCH_DOC, "### marker 握手"),
        ] {
            let at = doc
                .find(anchor)
                .unwrap_or_else(|| panic!("{name} 里找不到锚点 {anchor:?} —— 文档被大改了？"));
            let doc = &doc[at..];
            let (Some(title), Some(await_file)) = (doc.find("WindowTitle"), doc.find("ps-await"))
            else {
                panic!("{name} 的握手节里找不到那两个关键词 —— 抽取坏了还是文档被大改了？");
            };
            assert!(
                title < await_file,
                "{name} 把握手顺序写成了「先写 ps-await、后设 WindowTitle」—— 那是 v2 之前的旧顺序，\n\
                 照它实现会复刻 v2.21『每个新 shell 首次 cc 固定烧满超时』。\n\
                 真相源是 src-tauri/scripts/cc.ps1.tpl（先设标题、后写文件）。"
            );
        }
    }

    /// ★ 把这四个数**直接钉死**。
    ///
    /// # 为什么「数字出现在文档里」不够
    ///
    /// D 审计实测：把 deadline 从 3000 退回 **800**，上面那条护栏**不红** ——
    /// 因为 `doc/IPC-PROTOCOL.md` 自己的沿革括号里就写着「v2 之前 deadline 是 800ms」。
    /// **文档的 changelog 把旧值供着，判据就被它喂饱了。**
    ///
    /// 那条护栏的立项理由是「文档停在 800、实现早已 3000」—— 它管的是**文档滞后**。
    /// 反方向（**实现退回旧值**）得靠这条钉死。两条一起才闭合。
    ///
    /// # 改这些数怎么办
    ///
    /// 它们是**协议的一部分**（PS 与 monitor 两侧必须对齐，且旧模板用户靠重试兜底）。
    /// 要改就三处一起改：实现 · 本 pin · `doc/IPC-PROTOCOL.md` 的时序图。
    /// 本 pin 红了不是"更新一下数字"，是提醒你**这是一次协议变更**。
    #[test]
    fn handshake_timings_match_their_pinned_values() {
        let t = tpl();
        let bind = bind_rs();
        let g = |src: &str, a: &str, b: &str| -> u32 {
            between(src, a, b)
                .unwrap_or_else(|| panic!("抽不到 {a:?} —— 抽取坏了，本断言在空转"))
                .parse()
                .unwrap_or_else(|e| panic!("{a:?} 抽到的不是整数：{e}"))
        };
        assert_eq!(
            g(&t, "AddMilliseconds(", ")"),
            3000,
            "PS 握手 deadline 变了。v2 从 800 抬到 3000 是为了覆盖 monitor 冷启动 ——\n\
             退回去会让「monitor 没在跑时第一次 cc」重新烧满超时。"
        );
        assert_eq!(
            g(&t, "Start-Sleep -Milliseconds ", "\n"),
            30,
            "PS 轮询步长变了"
        );
        assert_eq!(
            g(&bind, "new_debouncer(Duration::from_millis(", ")"),
            50,
            "notify debouncer 变了"
        );
        let n = g(&bind, "for _ in 0..", " {");
        let step = g(
            &bind,
            "std::thread::sleep(std::time::Duration::from_millis(",
            ")",
        );
        assert_eq!(
            (n, step),
            (12, 50),
            "找不到窗口时的重试节奏变了。**那是旧模板用户唯一的活路** ——\n\
             老 profile 不会自动更新，它们靠这 600ms 兜住「标题还没设上」的窗口。\n\
             D 审计把它缩成 3×10ms=30ms，四条护栏当时全绿。"
        );
    }

    #[test]
    fn monitor_side_timings_appear_in_the_protocol_doc() {
        let bind = bind_rs();
        let debounce =
            between(&bind, "new_debouncer(Duration::from_millis(", ")").expect("找不到 debouncer");
        assert!(
            IPC_DOC.contains(&format!("{debounce}ms")),
            "notify debouncer 是 {debounce}ms，doc/IPC-PROTOCOL.md 里没有 —— \
             图上曾长期写着 100ms"
        );

        // 重试：`for _ in 0..12 { sleep(50ms) }` ⇒ 总 600ms。两个数都得对得上。
        let n: u32 = between(&bind, "for _ in 0..", " {")
            .expect("找不到重试次数")
            .parse()
            .expect("重试次数不是整数");
        let step: u32 = between(
            &bind,
            "std::thread::sleep(std::time::Duration::from_millis(",
            ")",
        )
        .expect("找不到重试步长")
        .parse()
        .expect("重试步长不是整数");
        let total = n * step;
        assert!(
            IPC_DOC.contains(&format!("{total}ms")) && IPC_DOC.contains(&format!("{n} × {step}")),
            "monitor 找不到窗口时重试 {n} × {step}ms = {total}ms，\
             doc/IPC-PROTOCOL.md 必须同时写出总时长 `{total}ms` 和拆分 `{n} × {step}`（当前缺其一）"
        );
    }

    /// 抽第一处 `a`…`b` 之间的内容。抽不到返回 None —— 调用方一律 expect，
    /// 免得夹具悄悄退化成空转（本仓有先例：抽取器改坏后抽到 0 个、断言全绿）。
    fn between<'a>(hay: &'a str, a: &str, b: &str) -> Option<&'a str> {
        let s = hay.find(a)? + a.len();
        let e = hay[s..].find(b)? + s;
        Some(hay[s..e].trim())
    }
}
