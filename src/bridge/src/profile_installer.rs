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
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
/// 也没有第二套 merge —— 内容是 `sftp::CCM_WRAPPER_SNIPPET`（= `src/shared/ccm-aliases.sh`
/// 本身），合块是 `sftp::merge_profile_block`，围栏是 `sftp::CCM_PROFILE_BEGIN/END`。
/// ⇒ 本机与远端装进 rc 的**是同一份东西**（`K15` / `K36`），
/// 而不是「同一件事的第四个形状」（`K-R62 §0c` 那三套）。
///
/// `command_name` / `include_cc_function` **只对 PowerShell 那一臂有意义**：
/// POSIX 那一块的名字（`cc` / `cct`）住在 `src/shared/ccm-aliases.sh` 里，
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
///   （`src/shared/ccm-aliases.sh` 头注逐字鼓励这么做）也会被指名 —— 所以产物是
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
/// （= `src/shared/ccm-aliases.sh` 本身），**这里不抄一份名字清单**。
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
// 用 PowerShell 自己的 `Get-Content` 读回来（真机逐字，住 `tests/evidence/K-R132-摸底.md`）：
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
pub fn user_path_has_our_bin(user_path_raw: &str, dir_abs: &str) -> bool {
    if dir_abs.is_empty() {
        return false;
    }
    user_path_raw
        .split(';')
        .any(|seg| seg.eq_ignore_ascii_case(dir_abs))
}

/// 🔴 `KR135D1`：**问「现在状态」的那条命令**（第三条，只读）。
///
/// 它吐两行：**第 1 行是我们那个 bin 目录的绝对路径**（`%USERPROFILE%` 展开之后），
/// **第 2 行是 `'User'` 那一档的原始 PATH 串**。
/// 目录路径与 PATH 串都**不可能含换行** ⇒ 按行切是安全的，不需要发明分隔符。
///
/// # 为什么要它把目录也一起吐回来
///
/// [`user_path_has_our_bin`] 比的是**整格**，而 PATH 上那一格是**绝对路径**；
/// 我们这一侧只知道 `%USERPROFILE%` **相对**的那一段（`tool_registry` 申报的就是相对路径）。
/// ⇒ 让**同一个 PowerShell 进程**用 `Join-Path $env:USERPROFILE` 展开，
/// 与「加」「撤」那两条命令**用的是同一句展开**（三条都写着同一个 `Join-Path`）
/// —— 在 Rust 这一侧自己拼一次 `%USERPROFILE%` 就是那个值的第二个住址。
///
/// # 两个地雷在这一侧同样适用
///
/// 它**只读 `'User'` 那一档**：读 `$env:PATH` 会把「机器级上有」读成「用户级上有」，
/// 于是「撤」那个按钮点下去什么都没发生、而界面还说它撤掉了。
/// 判据与另外两条走**同一批断言**。
pub fn render_user_path_probe_command() -> Option<String> {
    let dir = ccm_bin_dir_windows()?;
    Some(format!(
        "$d = Join-Path $env:USERPROFILE '{dir}'\n\
         Write-Output $d\n\
         Write-Output ([Environment]::GetEnvironmentVariable('Path', 'User'))\n"
    ))
}

/// 用户级 PATH 那一格的**现状**。`R85` 逐字要的三样里的第一样，**现算，不缓存**。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPathStatus {
    /// 这台机器有没有「用户级 PATH」这一档 —— **它是 Windows 独有的**。
    /// `false` 时下面三格一律不许被读成「没装」（见 [`user_path_status`] 头注）。
    pub supported: bool,
    /// 我们那个 bin 目录的**绝对路径**（探针展开回来的那一行）。探不动 ⇒ `None`。
    pub dir: Option<String>,
    /// 在不在用户级 PATH 上。**探不动时恒 `false`，而那时 `error` 非空** ——
    /// 两者要一起读，别单看这一格。
    pub on_user_path: bool,
    /// 「加」那条命令的逐字文本（给不想点按钮的人复制；与按钮跑的**是同一份字节**）。
    pub add_command: Option<String>,
    /// 「撤」那条命令的逐字文本。
    pub remove_command: Option<String>,
    /// 探不动时的原话。🔴 **探不动 ≠ 不在 PATH 上** —— 界面必须把这一格显示出来，
    /// 不许把它静默成「未安装」（同 `launcher-diagnostics` 那条「扫不动不许静默」）。
    pub error: Option<String>,
}

/// 🔴 `R88` ＋ `KR135D1`：**本模块唯一一处起进程。**
///
/// # 为什么产品这一侧要起它（`R85` / `R88`，不是顺手）
///
/// `R85` 用户逐字「**应该让用户手动点击加，也能管理删除**」⇒ **点击即执行是允许的**
/// （`§0c`：`K33` 禁的是产品**替**用户决定，**用户点一下就是用户自己决定**）。
/// 而「现在状态」那一格**根本不可能靠用户去跑** —— 那正是 `R88` 推翻 `R87`
/// 「本件不需要起进程」那句假前提的地方。
///
/// # 为什么是起 PowerShell，而不是 Rust 直接写注册表
///
/// `R88` 裁定（两条，按份量）：
/// 1. **直接写注册表会造出第二份 PATH 编辑实现** —— 而走 Tauri 命令的理由本身就是
///    「别给同一族动作另起一条路」。⇒ 这一跳跑的**就是我们生成给用户看的那段字节**，
///    「点按钮」与「自己复制去跑」**逐字同一份**，实现真的只有一处（`K33`）。
/// 2. `[Environment]::SetEnvironmentVariable(…,'User')` **自带 `WM_SETTINGCHANGE` 广播**；
///    自己写注册表就得自己记得广播，忘了的后果是「改了、新开的终端看不到，要重登录」
///    —— **那正是本件在杀的那个形状**（`R86` 逐字「看起来装好了、换个终端就没了」）。
///    用一个会重新制造本病的手法去治本病，不行。
///
/// # argv 的形状（`write_site_registry::SPAWNS` 那一行逐字记的就是这个）
///
/// `powershell.exe -NoProfile -NonInteractive -Command <脚本>`。
/// - **`-NoProfile` 是承重的**：不读用户自己的 profile ⇒ 这一跳的行为不被用户配置左右
///   （而且本件刚刚才把我们自己那一段从 profile 里删掉，再去读 profile 是自相矛盾）。
/// - **`-NonInteractive`**：绝不弹提示等人回车 —— 界面点一下不许挂住。
/// - `<脚本>` **只可能是本模块那三个 `render_*` 函数的输出**，不吃任何用户输入；
///   里面唯一的变量是 `tool_registry` 申报的那个目录。判据在数这件事。
///
/// 非 Windows 上**不起进程**，直接如实回错 —— 那台机器上根本没有「用户级 PATH」这一档。
#[cfg(windows)]
fn run_user_path_powershell(script: &str) -> Result<String, String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped());
    // 三条策略（`00 §1.5.2`）：
    // · `Hidden` —— 🔴 **先前是裸 `.output()`，也就是没人回答过这个问题**：`-NonInteractive`
    //   只保证它不等人回车，**挡不住 Windows 给它新开一个控制台窗口**。用户点一下
    //   「加到 PATH」就闪一个黑框，而这一跳的全部意义是「点一下、悄悄改好」。
    // · `JobKillOnClose` —— 就地等它退；`SetEnvironmentVariable` 那段若起了别的东西，
    //   不许留在后面。
    // · `Captured` —— stderr **是返回值的一部分**（下面那句 `退出码 …；stderr：…`
    //   逐字要用它），不是被丢了。
    let out = spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Captured,
    )
    .and_then(|c| c.wait_with_output())
    .map_err(|e| format!("起不来 powershell.exe：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "powershell 退出码 {:?}；stderr：{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(not(windows))]
fn run_user_path_powershell(_script: &str) -> Result<String, String> {
    Err("这台机器上没有「用户级 PATH」这一档 —— 它是 Windows 独有的".to_string())
}

/// `KR135D1` ①：**现在状态**。每调一次真跑一趟探针，**不缓存**。
///
/// 🔴 **探不动时不许假装「不在 PATH 上」**：那时 `on_user_path = false` 而 `error` 非空，
/// 界面要显示 `error` 那一句。把「问不出来」显示成「没装」，用户会去点「加」，
/// 而那一下同样会失败 —— 两次失败之间他学不到任何东西。
pub fn user_path_status() -> UserPathStatus {
    let add_command = render_user_path_setup_command();
    let remove_command = render_user_path_removal_command();
    let probe = match render_user_path_probe_command() {
        Some(p) => p,
        None => {
            return UserPathStatus {
                supported: cfg!(windows),
                dir: None,
                on_user_path: false,
                add_command,
                remove_command,
                error: Some(
                    "问不出本机 `ccm` 的 bin 目录 —— 它现算自 `tool_registry` 那张表里 \
                     `ccm` 那条本机载体的落点。取不到 = 那张表被改坏了"
                        .to_string(),
                ),
            };
        }
    };
    if !cfg!(windows) {
        return UserPathStatus {
            supported: false,
            dir: None,
            on_user_path: false,
            add_command,
            remove_command,
            error: None,
        };
    }
    match run_user_path_powershell(&probe) {
        Ok(raw) => {
            let mut lines = raw.lines();
            let dir = lines.next().unwrap_or("").trim().to_string();
            let user_path = lines.next().unwrap_or("").trim_end();
            UserPathStatus {
                supported: true,
                on_user_path: user_path_has_our_bin(user_path, &dir),
                dir: if dir.is_empty() { None } else { Some(dir) },
                add_command,
                remove_command,
                error: None,
            }
        }
        Err(e) => UserPathStatus {
            supported: true,
            dir: None,
            on_user_path: false,
            add_command,
            remove_command,
            error: Some(e),
        },
    }
}

/// `KR135D1` ②：**一个按钮加**。跑的就是 [`render_user_path_setup_command`] 那段字节。
pub fn user_path_add() -> Result<(), String> {
    let script = render_user_path_setup_command()
        .ok_or("问不出本机 `ccm` 的 bin 目录 —— 不发明一个目录往用户 PATH 上写")?;
    run_user_path_powershell(&script).map(|_| ())
}

/// `KR135D1` ③：**一个按钮撤**。跑的就是 [`render_user_path_removal_command`] 那段字节
/// —— **只摘自己那一格**（整格比，不碰用户 PATH 里别的东西）。
pub fn user_path_remove() -> Result<(), String> {
    let script = render_user_path_removal_command()
        .ok_or("问不出本机 `ccm` 的 bin 目录 —— 不拿一个猜出来的目录去改用户 PATH")?;
    run_user_path_powershell(&script).map(|_| ())
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
///    （`src/shared/ccm-aliases.sh` 的 `cc() { ccm "$@"; }`）上是**两个不同的东西**：
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
/// 而那一臂的 `cct` 是**真有**的（`src/shared/ccm-aliases.sh` 里就定义着）。
/// PowerShell 那一臂是另一份文件（`src/settings/cc_integration.ts` 的 `renderScanResult`），
/// 现打 `cct` **零命中**。⇒ **Windows 文案里今天一个 `cct` 都没有，没有东西要摘。**
/// 读数 · 量法 · 分母住 `tests/evidence/K-R135-摸底.md`。
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
#[path = "../../../tests/bridge/profile_installer_tests.rs"]
mod tests;

/// U6a：把 **PS↔monitor 握手**的顺序与数字钉在 `src/doc/IPC-PROTOCOL.md` 上。
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
#[path = "../../../tests/bridge/profile_installer_handshake_doc_guard.rs"]
mod handshake_doc_guard;
