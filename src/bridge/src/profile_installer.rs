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
//! | 装「别名块」 | [`install_to_profile`] | 🔴 **零口** | `sftp::install_remote_ccm_helper`〔散文墓碑〕（〔MC1〕今天 `sftp::install_remote_alias_block`） |
//! | 查「你 rc 里那几行是旧的」 | `scan_legacy_profiles`〔散文墓碑〕（〔AL1d〕删了：每份候选各带块的现状） | 🔴 **零口** | —— |
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
//! ⚠ **方言不是「猜路径」。** 路径始终由界面上的人选（「其它文件」一直是产品特性）。
//! `shell_dialect.rs::Shell::of_target` 回答的是**另一个问题**：人选定了这份文件之后，往里写哪种语言。
//! 把 `function cc { … }` 写进 `~/.bashrc` 在任何情形下都不是对的答案 ——
//! 而本件之前这条路**只会**写 PowerShell。

use crate::copy_table::copy_text;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::shell_dialect::Shell;

/// ⚠ `K-R62` 起是 `pub(crate)`：`fenced_block::FENCE_SHAPES` 那张账要**指**这一对，
/// 而不是抄一份字面量过去（抄一份就是第二个住址）。
pub(crate) const BEGIN_MARKER: &str = "# === cc-monitor BEGIN";
pub(crate) const END_MARKER: &str = "# === cc-monitor END";

/// cc function 模板源码（含 `{{COMMAND_NAME}}` placeholder）
const CC_TEMPLATE: &str = include_str!("../scripts/cc.ps1.tpl");

/// 别名块里那个 `cc` 函数的名字（PowerShell 那一臂由它渲染；POSIX 那一臂的名字住 `src/shared/ccm-aliases.sh`，
/// 这里只拿它查「用户 rc 里有没有同名函数」）。
///
/// 〔AL1d · 第四波 4B〕从前是 `cc_integration_*` 三条命令的入参 `command_name`，而界面从来只传 `"cc"`
/// （`设计/71 §7` 那张表：「界面写死 `CC_COMMAND_NAME = "cc"`」）⇒ 一个没人用的自由度，收成这一个常量。
pub const CC_FUNCTION_NAME: &str = "cc";

/// 一份启动文件（rc / `$PROFILE`）里**别名块**的现状。
///
/// 〔AL1d · 第四波 4B〕别名块与别名文件那一行 source 装进的是**同一批**启动文件，候选从前却有两份来历
/// （`AL1d.md §1.2`）⇒ 今天只有一份：`account_aliases::StartupFile` 每份候选都带着这一格，
/// 由 [`block_state`] 在读回口那一次扫描里一起算出来。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BlockState {
    /// 这份文件里有没有 cc-monitor 的别名块（**悬空的 BEGIN 也算在**，见 `block_presence`）。
    pub present: bool,
    /// 块头上的版本串（PowerShell 那一对才有；POSIX 那一对恒 `None`）。
    pub version: Option<String>,
    /// 块外已有的同名函数（与 [`CC_FUNCTION_NAME`] 同名）。
    pub conflicting_functions: Vec<String>,
    /// 🔴 〔`K-R62`〕**「你 rc 里这几行是旧的」那段话。** 空串 = 没有要清的。
    ///
    /// 它是 [`render_manual_cleanup_hint`] 的产物：**逐行指名**（行号 + 原文）
    /// 加一段给用户自己动手的说明。**产品一个字节都不删**（`K31` + 用户逐字
    /// 「原本的配置要手动删除」）—— 那些行没有围栏，边界只有人知道。
    ///
    /// ⚠ **只对 [`Shell::Posix`] 有内容**：它找的是**根本没有围栏的裸行**。
    /// 「整块装在了别的哪份里」是另一件事，今天由每份候选各自的 [`BlockState::present`] 照实答
    /// （〔AL1d〕从前 PowerShell 那一侧另有一段只查 `profile.ps1` 两份的遗留扫描，随候选收成一份删了）。
    pub manual_cleanup_hint: String,
}

/// **只读、纯函数**：一份启动文件的正文（盘上原样，BOM 在这里剥）→ 别名块的现状。
/// 方言按**这份文件自己**的扩展名定（`Shell::of_target`），不按调用方在问哪种 shell。
pub fn block_state(path: &Path, raw: &str) -> BlockState {
    let flavor = Shell::of_target(path);
    let content = strip_bom(raw);
    let (present, version) = block_presence(flavor, content);
    BlockState {
        present,
        version,
        conflicting_functions: find_conflicting_functions(flavor, content, CC_FUNCTION_NAME),
        manual_cleanup_hint: match flavor {
            Shell::PowerShell => String::new(),
            Shell::Posix => {
                render_manual_cleanup_hint(&path.to_string_lossy(), &scan_legacy_rc_lines(content))
            }
        },
    }
}

/// **纯**：别名块渲染成代码 —— 「往一份空文件里装一次，那份文件会变成什么」（BOM 那一层除外）。
///
/// 〔AL1d · 第四波 4B〕从前的预览只会 PowerShell 那一块（`render_cc_code`），POSIX 那一块没有预览。
/// 今天两种方言都答，而且答的是**装那一跳调的同一个** [`plan_install`] —— 「预览的就是写的那一份」
/// 由同一个函数保证，不是两份拼法对拍。`with_cc` 只对 PowerShell 那一臂有意义（同 [`plan_install`]）。
pub fn render_block(shell: Shell, with_cc: bool) -> Result<String, String> {
    plan_install(shell, "", CC_FUNCTION_NAME, with_cc, "预览")
}

// 〔AL1d · 第四波 4B〕这里原来是 `ProfileKind`（PS 5.1 / PS 7 / 自定义 三个标签）与 `ProfileScan`〔散文墓碑〕
// （一份 profile 的扫描结果，给终端集成那两条命令出参）。今天候选只有一份来历（`shell_dialect::ShellDialect::startup_files`），
// 扫描结果是 `account_aliases::StartupFile` ＋ 上面的 [`BlockState`]；「哪一份是 PS 5.1 的」这个标签没有消费者了。

// ═══════════════════════════════════════════════════════════════════════════
// `K-R62`：本机 POSIX 那一半 —— **装**（借远端那一份）与**查**（够得着裸行）
// ═══════════════════════════════════════════════════════════════════════════

/// 一份 profile 的**方言**：这份文件里该放哪种语言的内容、认哪一对围栏。
///
/// 〔AL1c · 第四波 4B〕它从前是本模块自己的一个两值枚举 ＋ 一个按扩展名判的函数（旧名见 `git log`）；
/// 今天是 `shell_dialect::Shell` —— 别名文件、别名块、source 那一行问的是**同一个问题**，
/// 不许有两个枚举各答一半。认法没变：`Shell::of_target` 按**文件扩展名**判，不按 `cfg!(windows)`。
///
/// 🔴 **它不猜路径。** 路径始终由界面上的人选（`account_aliases` 的 `§0e` 那条理由
/// 原样适用：`.bashrc` / `.zshrc` / fish 的 `config.fish` 写法不同，替人选一份是最坏的
/// 那条路）。它回答的是**人选定之后**的那个问题：往这份文件里写哪种语言。

/// 纯函数：**装**完之后这份文件该长什么样。落盘那一跳在 [`install_to_profile`]。
///
/// 🔴 **`Posix` 那一臂是 `KR62D1` 的正题**：它一个字节的 snippet 都不生成、
/// 也没有第二套 merge —— 内容是 `sftp::CCM_WRAPPER_SNIPPET`（= `src/shared/ccm-aliases.sh`
/// 本身），合块是 `sftp::merge_profile_block`，围栏是 `sftp::CCM_PROFILE_BEGIN/END`。
/// ⇒ 本机与远端装进 rc 的**是同一份东西**（`K15` / `K36`），
/// 而不是「同一件事的第四个形状」（`K-R62 §0c` 那三套）。
///
/// `command_name` / `include_cc_function` **只对 PowerShell 那一臂有意义**：
/// POSIX 那一块的名字（`cc` / `cct`）住在 `src/shared/ccm-aliases.sh` 里，
/// 那份文件自己用 `declare -f` 让着用户已有的同名函数 —— 由它说了算，不由这里的参数说了算。
pub fn plan_install(
    flavor: Shell,
    existing: &str,
    command_name: &str,
    include_cc_function: bool,
    what: &str,
) -> Result<String, String> {
    match flavor {
        Shell::PowerShell => {
            let code = render_cc_code(command_name, include_cc_function);
            replace_or_append_block(existing, &code, what)
        }
        Shell::Posix => {
            crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)
        }
    }
}

/// 纯函数：**卸**完之后这份文件该长什么样。同 [`plan_install`]，POSIX 那一臂借远端那一份。
pub fn plan_uninstall(flavor: Shell, existing: &str, what: &str) -> Result<String, String> {
    match flavor {
        Shell::PowerShell => strip_block(existing, what),
        Shell::Posix => crate::sftp::strip_profile_block(existing, what),
    }
}

/// 这份文件里**有没有** cc-monitor 的块，以及块里的版本串。
///
/// 两种方言认的是**两对不同的围栏**，一对都不许混：混了就是「装一个把另一个整块替换掉」
/// （`account_aliases` 那对刻意不同前缀，理由同源）。
fn block_presence(flavor: Shell, content: &str) -> (bool, Option<String>) {
    match flavor {
        Shell::PowerShell => find_block_version(content),
        // POSIX 那一对没有版本后缀 ⇒ 恒 `None`。判「在不在」与 PowerShell 同口径：
        // 只看有没有一行以 BEGIN 打头（**悬空的 BEGIN 也算在**——否则界面会说「未安装」
        // 且藏起卸载按钮，而点安装却报行号，那正是 T04 审计③ 治过的那一形）。
        Shell::Posix => (
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
/// # 为什么不是「给 `scan_legacy_profiles`〔散文墓碑〕的路径表加两行」
///
/// 那个函数（〔AL1d〕已删）认的是 [`find_block_version`]（**围栏**）。而 `K-R57` 现打用户本机：
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
        body.push_str(&copy_text(
            "rsProfileInstaller.hint.line",
            &[
                ("lineNo", &h.line_no.to_string()),
                ("text", &(h.text.trim_end()).to_string()),
                (
                    "mark",
                    &(if shadow {
                        copy_text("rsProfileInstaller.hint.shadowMark", &[])
                    } else {
                        String::new()
                    }),
                ),
            ],
        ));
    }
    let mut out = copy_text(
        "rsProfileInstaller.hint.head",
        &[
            ("what", &what.to_string()),
            ("count", &(hits.len()).to_string()),
            ("body", &body.to_string()),
        ],
    );
    if shadowed > 0 {
        out.push_str(&copy_text(
            "rsProfileInstaller.hint.shadowNote",
            &[("shadowed", &shadowed.to_string())],
        ));
    }
    out.push_str(&copy_text(
        "rsProfileInstaller.hint.whereToEdit",
        &[("what", &what.to_string())],
    ));
    out
}

/// **路径围栏：profile 只能落在用户 home 之内**〔audit-0805 08-08，Phase G 第 86 件〕。
///
/// # 为什么需要它
///
/// 那三条命令（〔AL1d〕今天是 `aliases_block_install` / `aliases_block_remove` ＋ `aliases_read` 的「其它文件」；
/// 从前叫 `cc_integration_*`〔散文墓碑〕）收的是 **webview 给的字符串**（前端那一格是用户可输入的
/// 文本框），此前**原样** `PathBuf::from` 就交给了安装器：装往那里写、
/// 文件不存在还会创建；卸会重写它；扫是任意路径的存在性探针。
/// 而**远端**那条同名功能一直有围栏（`sftp.rs`：「profile 只能是 home 下的文件名」）。
///
/// # 为什么是「home 之内」而不是「home 下的裸文件名」
///
/// `$PROFILE` 的候选（`shell_dialect.rs` 的 PowerShell 那一臂）本来就是 `~/Documents/WindowsPowerShell/…ps1` 这种**子目录**里的路径，
/// 而「其它文件」是产品特性（用户可以指 `~/.config/fish/config.fish`）。
/// ⇒ 围栏只挡「跑出 home」这一类，**不缩小功能**。
///
/// 三条规则：① `~` / `~/x` 先展开（用户会手打这种）；② 必须是绝对路径；
/// ③ 不许含 `..`（不做「消解后再看」——直接拒绝更简单也更难绕）；④ 前缀必须是 home。
/// 另外：父目录若已存在，用它的 canonical 形态再查一次前缀 —— 挡掉
/// `~/link -> /etc` 这种**符号链接逃逸**（`install` 会跟着链接写过去）。
pub fn fence_profile_path(raw: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| copy_text("rsProfileInstaller.fence.noHome", &[]))?;
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
        return Err(copy_text(
            "rsProfileInstaller.fence.notAbsolute",
            &[("raw", &format!("{:?}", raw))],
        ));
    }
    if expanded
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(copy_text(
            "rsProfileInstaller.fence.dotdot",
            &[("raw", &format!("{:?}", raw))],
        ));
    }
    if !expanded.starts_with(&home) {
        return Err(copy_text(
            "rsProfileInstaller.fence.outsideHome",
            &[
                ("raw", &format!("{:?}", raw)),
                ("home", &format!("{:?}", home)),
            ],
        ));
    }
    // 符号链接逃逸：父目录已存在时用它的真身再查一次。
    if let Some(parent) = expanded.parent() {
        if let (Ok(real_parent), Ok(real_home)) = (parent.canonicalize(), home.canonicalize()) {
            if !real_parent.starts_with(&real_home) {
                return Err(copy_text(
                    "rsProfileInstaller.fence.symlinkEscape",
                    &[
                        ("raw", &format!("{:?}", raw)),
                        ("realParent", &format!("{:?}", real_parent)),
                    ],
                ));
            }
        }
    }
    Ok(expanded)
}

// 〔AL1d · 第四波 4B〕这里原来住着 `$PROFILE` 的两份认法 ＋ 一段扫描：`discover_profiles` · `legacy_profile_paths` · `scan_legacy_profiles`〔散文墓碑〕
// （前者认 PS 5.1 / 7 的 `Microsoft.PowerShell_profile.ps1`；后者把同目录的 `profile.ps1` 当成「v1.7.0-1.7.1 装错的位置」），
// 以及扫一份的 `scan_path` / `scan_profile`〔散文墓碑〕。`$PROFILE` 在哪今天**只有** `shell_dialect.rs` 的 PowerShell 那一臂答（四份都列：
// `profile.ps1` 是合法的 AllHosts 位置，不是「装错了」—— 从前两处认法正是在这一格上互相矛盾，`调研/第四波记录/AL1d.md §1.3`）；
// 扫一份的判内容那一半是上面的 [`block_state`]，读盘那一次在 `account_aliases::rc_candidates_in`。

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
// 那时的落盘原语 `atomic_write_string`〔散文墓碑〕走 `std::fs::write`，写的是**不带 BOM 的 UTF-8**
// （〔RW1〕今天落盘在后端 `files-put`，同样原样写字节、不加 BOM）。
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
// 都认。⚠ **不能加在落盘那一层** —— 那一层还有别的用户（项目 `.mcp.json`、别名那份 shell 脚本；
// 〔RW1〕今天那一层是后端的 `files-put`，照样是这几家共用），给 JSON 和 `.sh` 加 BOM 是往别人身上引入同族的病。
// ⇒ 分岔点放在**方言**这一层（[`encode_for_disk`] / [`strip_bom`]），与
// 「写什么」那一处分岔（[`plan_install`]）同一条线。

// 〔AL1c · 第四波 4B〕这里原来住着 BOM 常量与「读时剥 / 写时按方言加」那一对（`K-R132`），
// 它们是**方言**的一格 ⇒ 搬去 `shell_dialect.rs`（`ShellDialect::encode_for_disk` / `::decode_from_disk`，
// BOM 常量的唯一住址也跟过去了）。分岔点仍在方言这一层，与「写什么」那一处分岔（[`plan_install`]）同一条线。

/// 读进来的那一份：把 BOM 剥掉再交给任何**判内容**的东西。
///
/// 🔴 不剥会坏两件事：① `find_block_version` 按 `strip_prefix(BEGIN_MARKER)` 认围栏，
/// 而 `\u{feff}# === cc-monitor BEGIN` 前缀对不上 ⇒ 界面说「未安装」、藏起卸载按钮，
/// 点安装却报行号（那正是 T04 审计③ 治过的那一形）；② BOM 会被当成用户内容
/// 原样写回文件中间。（这里**两种方言都剥**：它是「读」这一侧，宽进。）
fn strip_bom(s: &str) -> &str {
    crate::shell_dialect::strip_bom(s)
}

/// 落盘的那一份：PowerShell 方言加 BOM，POSIX rc **一个字节都不加**（实现住方言那一格）。
fn encode_for_disk(flavor: Shell, content: &str) -> String {
    flavor.dialect().encode_for_disk(content)
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
    .map_err(|e| {
        copy_text(
            "rsProfileInstaller.ps.spawnFailed",
            &[("e", &e.to_string())],
        )
    })?;
    if !out.status.success() {
        return Err(copy_text(
            "rsProfileInstaller.ps.exitCode",
            &[
                ("status", &format!("{:?}", out.status.code())),
                (
                    "detail",
                    &(String::from_utf8_lossy(&out.stderr).trim()).to_string(),
                ),
            ],
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(not(windows))]
fn run_user_path_powershell(_script: &str) -> Result<String, String> {
    Err(copy_text("rsProfileInstaller.userPath.notWindows", &[]))
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
                error: Some(copy_text("rsProfileInstaller.userPath.noBinDir", &[])),
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
        .ok_or(&copy_text("rsProfileInstaller.userPath.addNoDir", &[]))?;
    run_user_path_powershell(&script).map(|_| ())
}

/// `KR135D1` ③：**一个按钮撤**。跑的就是 [`render_user_path_removal_command`] 那段字节
/// —— **只摘自己那一格**（整格比，不碰用户 PATH 里别的东西）。
pub fn user_path_remove() -> Result<(), String> {
    let script = render_user_path_removal_command()
        .ok_or(&copy_text("rsProfileInstaller.userPath.removeNoDir", &[]))?;
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
/// （它的下拉只遍历 `AccountAliasReport::rc_candidates`〔散文墓碑〕（〔AL1〕今天是 `AliasListing::rc_candidates`），而那张表现算自
/// 那时 `account_aliases` 里那张 POSIX 候选表（〔AL1c〕今天住 `shell_dialect.rs` 的 POSIX 那一臂），**一份 PowerShell profile 都没有**），
/// 而那一臂的 `cct` 是**真有**的（`src/shared/ccm-aliases.sh` 里就定义着）。
/// PowerShell 那一臂是另一份文件（那时的 `src/settings/cc_integration.ts` 的 `renderScanResult`；〔AL1c〕今天并进了
/// `src/settings/machine-aliases.ts`，是 PowerShell 那一侧的别名块，生成的别名在 PowerShell 上照样不带 tmux 那一族），
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
/// v1.7.10 那四道安全加固（先备份 · 真原子替换 · 写后回读 · 盘上有字节却读到空就中止）
/// 〔RW1 · 第四波 09-24〕**住后端**（`files-put` ／ `files-peek`，本机与远端同一份规则）——
/// 本进程不再落盘。本函数只答方言那一半：[`plan_install`] ＋ [`encode_for_disk`]，读写经 `door`。
/// ⚠ 内容与盘上逐字相同时**一个字节都不写**。
pub async fn install_to_profile(
    door: &impl crate::user_files::Door,
    path: &Path,
    command_name: &str,
    include_cc_function: bool,
) -> Result<(), String> {
    let flavor = Shell::of_target(path);
    let what = path.display().to_string();
    let home = door.home().await?;
    let rel = crate::user_files::rel_under(&home, &what)?;
    crate::user_files::edit(door, &home, &rel, true, true, |raw| {
        // 〔`K-R132`〕BOM 剥在**最靠近读的那一跳**；落盘那一份按方言再编码回去 ——
        // 读回比对比的是落盘那一份（比计划出来的那一份会恒差三个字节，当场回滚）。
        let existing = strip_bom(raw.unwrap_or(""));
        let updated = plan_install(flavor, existing, command_name, include_cc_function, &what)?;
        Ok(Some(encode_for_disk(flavor, &updated)))
    })
    .await
    .map(|_| ())
}

/// 卸载：整块删除 BEGIN/END 之间的内容（含 marker 行）。块外内容不动。
/// 文件不存在 ⇒ 什么都不做。读写经 `door`，规则同 [`install_to_profile`]（住后端）。
pub async fn uninstall_from_profile(
    door: &impl crate::user_files::Door,
    path: &Path,
) -> Result<(), String> {
    let flavor = Shell::of_target(path);
    let what = path.display().to_string();
    let home = door.home().await?;
    let rel = crate::user_files::rel_under(&home, &what)?;
    crate::user_files::edit(door, &home, &rel, true, false, |raw| {
        let Some(raw) = raw else { return Ok(None) };
        let stripped = plan_uninstall(flavor, strip_bom(raw), &what)?;
        Ok(Some(encode_for_disk(flavor, &stripped)))
    })
    .await
    .map(|_| ())
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
fn find_conflicting_functions(flavor: Shell, content: &str, command_name: &str) -> Vec<String> {
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
        if flavor == Shell::Posix {
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
/// 在 existing 中替换 ccm 块；若不存在则追加。
///
/// 〔AL1 · 2026-09-24〕配对之后怎么拼**只剩一份**：`fenced_block::splice_in`，
/// PowerShell 那几处排版（保住原文件的 CRLF · 追加前空一行）是它的 `Layout::PowerShell` 那一臂。
/// 那条「必须保留原文件的 EOL 风格」的来历（早期 `lines().join("\n")` 静默把 CRLF 换成 LF，
/// 长度校验检不出、notepad 报行尾不一致）跟着搬过去了，判据仍是本文件的 CRLF 那几条。
fn replace_or_append_block(existing: &str, new_block: &str, what: &str) -> Result<String, String> {
    crate::fenced_block::splice_in(
        existing,
        BEGIN_MARKER,
        END_MARKER,
        new_block,
        what,
        crate::fenced_block::Layout::PowerShell,
    )
}

/// 删除 ccm 块（如果有）。围栏损坏时 `Err` 中止而不是「当作没有块、原样返回」——
/// 后者让用户以为卸载干净了，而那个悬空的 BEGIN 下次安装就会吃掉它下面的内容。
/// 拼接同上走 `fenced_block::splice_out`。
fn strip_block(existing: &str, what: &str) -> Result<String, String> {
    crate::fenced_block::splice_out(
        existing,
        BEGIN_MARKER,
        END_MARKER,
        what,
        crate::fenced_block::Layout::PowerShell,
    )
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

// 〔RW1 · 第四波 09-24〕这里原来是本机用户文件的原子写原语 `atomic_write_string`〔散文墓碑〕与它的
// 两份平台副本 `atomic_replace_path`〔散文墓碑〕（Windows `ReplaceFileW` 保 ACL · POSIX `rename`）。
// 用户裁「只允许后端的文件管理部分写文件」也管本机 ⇒ `$PROFILE` / rc / 别名文件 / 项目 `.mcp.json`
// 全改经后端写（`user_files`），三件零调用方 ⇒ 走。「Windows 上替换要保住 explicit ACE」那条性质
// 跟着写搬到了后端（`control/files_write.rs::swap_in` 的 `cfg(windows)` 那一支）。

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
