//! PowerShell profile 路径解析 + 块插入/卸载 + 命令名冲突检测。
//!
//! 〔MIG-3a · `设计/99 §2.1 ⑬` · 主会话 09-27 裁〕从 monitor `profile_installer.rs` 搬来**别名块那一半**：规划 · 围栏 · 装 / 卸
//! 今天在那台机器的后端里算、经它自己的文件管理面写（[`super`]）。用户级 PATH 那一格（本机后端的引导）仍住 monitor。
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
//! | 装「别名块」 | [`install_to_profile`] | 🔴 **零口** | `sftp::install_remote_ccm_helper`〔散文墓碑〕（〔AL2〕今天就是 [`install_to_profile`]，带远端 `origin`） |
//! | 查「你 rc 里那几行是旧的」 | `scan_legacy_profiles`〔散文墓碑〕（〔AL1d〕删了：每份候选各带块的现状） | 🔴 **零口** | —— |
//!
//! 补法有两条硬边界，两条都是**这件事的一半价值**：
//!
//! 1. **不许变成第四套。** 装进本机 rc 的内容与远端那个口来自**同一个常量**
//!    （[`CCM_WRAPPER_SNIPPET`]），合块与剥块走**同一份实现**
//!    （[`merge_profile_block`] / [`strip_profile_block`]），围栏是**同一对标记**
//!    （[`CCM_PROFILE_BEGIN`] / `..._END`）。〔W5-ALIAS〕这几样从前住 `sftp.rs`，今天住本模块尾部。本模块**一个字节的 snippet 都不生成**，
//!    也**没有第二套 merge/strip** —— 见 [`plan_install`] / [`plan_uninstall`]。
//! 2. **一个字节都不许删用户的行**（`K31` + 用户逐字「原本的配置要手动删除」）。
//!    「查」这一半的产物是 [`render_manual_cleanup_hint`]：**逐行指名 + 一段让他自己动手的提示**，
//!    产品自己不动手。理由不是保守，是**做不到**：那些行没有围栏，边界只有人知道
//!    （`K-R57` 现打：用户机器上 10 个真使用者全是裸行）。
//!
//! ⚠ **方言不是「猜路径」。** 路径始终由界面上的人选（「其它文件」一直是产品特性）。
//! `dialect.rs::Shell::of_target` 回答的是**另一个问题**：人选定了这份文件之后，往里写哪种语言。
//! 把 `function cc { … }` 写进 `~/.bashrc` 在任何情形下都不是对的答案 ——
//! 而本件之前这条路**只会**写 PowerShell。

use copy_core::copy_text;
use serde::Serialize;
use std::path::Path;

use crate::assets::door::{self, Door};
use crate::platform::shell::dialect::{self, Shell};

/// ⚠ `K-R62` 起是 `pub(crate)`：`fenced_block::FENCE_SHAPES` 那张账要**指**这一对，
/// 而不是抄一份字面量过去（抄一份就是第二个住址）。
pub(crate) const BEGIN_MARKER: &str = "# === cc-monitor BEGIN";
pub(crate) const END_MARKER: &str = "# === cc-monitor END";

/// cc function 模板源码（含 `{{COMMAND_NAME}}` placeholder）
const CC_TEMPLATE: &str = include_str!("../../../shared/cc.ps1.tpl");

/// 别名块里那个 `cc` 函数的名字（PowerShell 那一臂由它渲染；POSIX 那一臂的名字住 `src/shared/ccm-aliases.sh`，
/// 这里只拿它查「用户 rc 里有没有同名函数」）。
///
/// 〔AL1d · 第四波 4B〕从前是 `cc_integration_*` 三条命令的入参 `command_name`，而界面从来只传 `"cc"`
/// （`设计/71 §7` 那张表：「界面写死 `CC_COMMAND_NAME = "cc"`」）⇒ 一个没人用的自由度，收成这一个常量。
pub(crate) const CC_FUNCTION_NAME: &str = "cc";

/// 一份启动文件（rc / `$PROFILE`）里**别名块**的现状。
///
/// 〔AL1d · 第四波 4B〕别名块与别名文件那一行 source 装进的是**同一批**启动文件，候选从前却有两份来历
/// （`AL1d.md §1.2`）⇒ 今天只有一份：`account_aliases::StartupFile` 每份候选都带着这一格，
/// 由 [`block_state`] 在读回口那一次扫描里一起算出来。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlockState {
    /// 这份文件里有没有 cc-monitor 的别名块（**悬空的 BEGIN 也算在**，见 `block_presence`）。
    pub present: bool,
    /// 块头上的版本串（PowerShell 那一对才有；POSIX 那一对恒 `None`）。
    pub version: Option<String>,
    /// 〔TL1 · 4C〕块在、而版本串不是这一版模板的那个 ⇒ `true`（只有 PowerShell 那一对有版本串）。
    /// v3 起模板结尾多一行接上别名文件（`设计/71 §6.1`）—— 装着 v2 的人**重装一次**才带上那一行，界面据此提示。
    /// 〔HX2 · 4D〕v4 起 `__ccm_bind` 找 monitor 数据目录走唯一出口（渲染时填）—— 装着 v3 的人同样重装一次。
    /// 「这一版是哪个」只从模板本身读（[`current_block_version`]），不另写一份字面量。
    pub outdated: bool,
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
pub(crate) fn block_state(path: &Path, raw: &str) -> BlockState {
    let flavor = Shell::of_target(path);
    let content = strip_bom(raw);
    let (present, version) = block_presence(flavor, content);
    let outdated = present
        && flavor == Shell::PowerShell
        && version.as_deref() != current_block_version().as_deref();
    BlockState {
        present,
        version,
        outdated,
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
pub(crate) fn render_block(shell: Shell, with_cc: bool, home: &str) -> Result<String, String> {
    let what = copy_text("rsProfileInstaller.preview.what", &[]);
    plan_install(shell, "", CC_FUNCTION_NAME, with_cc, &what, home)
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
/// 也没有第二套 merge —— 内容是 [`CCM_WRAPPER_SNIPPET`]（= `src/shared/ccm-aliases.sh`
/// 本身），合块是 [`merge_profile_block`]，围栏是 [`CCM_PROFILE_BEGIN`] / `_END`。
/// ⇒ 本机与远端装进 rc 的**是同一份东西**（`K15` / `K36`），
/// 而不是「同一件事的第四个形状」（`K-R62 §0c` 那三套）。
///
/// `command_name` / `include_cc_function` **只对 PowerShell 那一臂有意义**：
/// POSIX 那一块的名字（`cc` / `cct`）住在 `src/shared/ccm-aliases.sh` 里，
/// 那份文件自己用 `declare -f` 让着用户已有的同名函数 —— 由它说了算，不由这里的参数说了算。
pub(crate) fn plan_install(
    flavor: Shell,
    existing: &str,
    command_name: &str,
    include_cc_function: bool,
    what: &str,
    home: &str,
) -> Result<String, String> {
    match flavor {
        Shell::PowerShell => {
            // 〔HX2〕数据目录解不出（`CCM_DATA_DIR` 给了但不是绝对路径 / 找不到家目录）⇒ 拒，不往 `$PROFILE` 里写一个猜的路径。
            // 〔MIG-3a〕这台后端按同一份规则推（`creds_core::store::monitor_data_dir`，常驻后端 hello 回显的那一对也是它推的）：
            //   `$PROFILE` 在这台上，`__ccm_bind` 找的也是这台上 monitor 的那个目录。
            let dir = monitor_data_dir(home)
                .ok_or_else(|| copy_text("rsProfileInstaller.ps.noDataDir", &[]))?;
            let code = render_cc_code(command_name, include_cc_function, &dir);
            replace_or_append_block(existing, &code, what)
        }
        Shell::Posix => merge_profile_block(existing, CCM_WRAPPER_SNIPPET, what),
    }
}

/// 这台机器上 monitor 的数据目录（`CCM_DATA_DIR` 优先，其次按 `home` 推；规则唯一一份在 `creds_core`）。
fn monitor_data_dir(home: &str) -> Option<std::path::PathBuf> {
    use creds_core::store::{monitor_data_dir, DATA_DIR_ENV};
    monitor_data_dir(
        std::env::var(DATA_DIR_ENV).ok().as_deref(),
        Some(std::path::PathBuf::from(home)),
    )
}

/// 纯函数：**卸**完之后这份文件该长什么样。同 [`plan_install`]，POSIX 那一臂借远端那一份。
pub(crate) fn plan_uninstall(flavor: Shell, existing: &str, what: &str) -> Result<String, String> {
    match flavor {
        Shell::PowerShell => strip_block(existing, what),
        Shell::Posix => strip_profile_block(existing, what),
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
                .any(|l| l.trim_start().starts_with(CCM_PROFILE_BEGIN)),
            None,
        ),
    }
}

/// rc 里**围栏之外**、指着 `ccm` 的一行是什么形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyRcKind {
    /// `名字() { … ccm … }` —— **真使用者**（`K-R57` 现打：用户机器上那 14 行里的 10 行）。
    Function,
    /// 注释行（`#` 打头）。指名它只为让读的人知道「这几行也提到了 ccm」，不催他删。
    Comment,
    /// 其它（`alias cc=…` · `export PATH=…/ccm` · 直接调一次 …）。
    Other,
}

/// rc 里**围栏之外**、指着 `ccm` 的一行。**原文原样带着**，因为产品要做的是指名，不是改写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyRcLine {
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

/// 这一行是不是 cc-monitor 自己的围栏标记（每一对都认）。
///
/// 认的是**共同前缀** `# === cc-monitor`，而不是某一对 —— 今天是 `profile_installer` 的 `BEGIN_MARKER`
/// 与 `sftp` 的 `CCM_PROFILE_BEGIN` 两对；〔TL1 · 4C〕从前还有 `account_aliases` 包 rc 里那一行 source 的第三对
/// （那一步退役了，用户盘上可能还留着那一块 —— 共同前缀照样认得它是**我们的**边界，不当成用户的裸行）。
/// 这一格问的是「这一行是不是**我们的**边界」，那个答案对每一对是同一个。
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
/// - 形状按 `名字() {` 认函数（同 [`builtin_alias_names`] 那一形）。
///   `function cc { … }` 这一写法会落进 [`LegacyRcKind::Other`] —— **漏的是分类，不是那一行**，
///   它仍然被指名。
pub(crate) fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {
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
        } else if let Some(n) = Shell::Posix.dialect().declared_function(l) {
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

// 〔OSA〕这里原来有 POSIX「`名字() {` ⇒ 名字」那一份认法 —— 定义函数的写法归方言，搬进
//   （`platform/shell/dialect.rs` 的 `ShellDialect::declared_function`），[`builtin_alias_names`] 与本扫描共用那一份。

/// 🔴 `KR62D2` 的产物：**一段让用户自己动手的提示。** 没有要清的就是空串。
///
/// **产品一个字节都不删**（`K31` + 用户逐字「原本的配置要手动删除」）。
/// 措辞刻意不是「请删除」：见 [`scan_legacy_rc_lines`] 的诚实边界那一节。
///
/// 「哪几行会把我们装的那块遮蔽掉」现算自 [`builtin_alias_names`]
/// （= `src/shared/ccm-aliases.sh` 本身），**这里不抄一份名字清单**。
pub(crate) fn render_manual_cleanup_hint(what: &str, hits: &[LegacyRcLine]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let builtin = builtin_alias_names();
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
/// 而**远端**那条同名功能一直有围栏（从前 `sftp.rs`：「profile 只能是 home 下的文件名」）。
///
/// # 为什么是「home 之内」而不是「home 下的裸文件名」
///
/// `$PROFILE` 的候选（`shell_dialect.rs` 的 PowerShell 那一臂）本来就是 `~/Documents/WindowsPowerShell/…ps1` 这种**子目录**里的路径，
/// 而「其它文件」是产品特性（用户可以指 `~/.config/fish/config.fish`）。
/// ⇒ 围栏只挡「跑出 home」这一类，**不缩小功能**。
///
/// # 〔AL2 · 第四波 4D〕拆成两层：**词法**（本函数，两侧都过）＋ **符号链接**（[`fence_on`]，只对本机）
///
/// 词法四条：① `~` / `~/x` 先展开（用户会手打这种）；② 必须是绝对路径；
/// ③ 不许含 `..`（不做「消解后再看」——直接拒绝更简单也更难绕）；④ 前缀必须是 home。
/// 全是**字符串**上的判断（与 `user_files::rel_under` 同一种算法）：`home` 是**那台机器**的后端答的
/// （`files-home`），而那台可能不是 monitor 这台 —— `std::path::Path::is_absolute` 在 Windows 上把 `/home/zbl/.bashrc`
/// 判成相对（没有盘符），`Path::join` 又用本机分隔符（`第四波记录/W5-ALIAS.md §2.2` · `AL2.md §2.5`）。
pub(crate) fn fence_lexical(home: &str, raw: &str) -> Result<String, String> {
    let expanded = if raw == "~" {
        home.to_string()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        door::join_under(home, rest)
    } else {
        raw.to_string()
    };
    let b = expanded.as_bytes();
    let absolute = expanded.starts_with('/')
        || expanded.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'/' | b'\\'));
    if !absolute {
        return Err(copy_text(
            "rsProfileInstaller.fence.notAbsolute",
            &[("raw", &format!("{:?}", raw))],
        ));
    }
    if expanded.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(copy_text(
            "rsProfileInstaller.fence.dotdot",
            &[("raw", &format!("{:?}", raw))],
        ));
    }
    let norm = |s: &str| s.replace('\\', "/");
    let h = norm(home);
    let h = h.trim_end_matches('/');
    let a = norm(&expanded);
    let inside = !h.is_empty() && (a == h || a.starts_with(&format!("{h}/")));
    if !inside {
        return Err(copy_text(
            "rsProfileInstaller.fence.outsideHome",
            &[
                ("raw", &format!("{:?}", raw)),
                ("home", &format!("{:?}", home)),
            ],
        ));
    }
    Ok(expanded)
}

/// 同一道围栏：词法（[`fence_lexical`]）＋ **符号链接逃逸**那一步。
///
/// 符号链接那一步：父目录已存在时用它的真身再查一次前缀 —— 挡掉 `~/link -> /etc` 这种逃逸（`install` 会跟着链接写过去）。
/// 〔MIG-3a〕围栏住进了那台机器的后端 ⇒ `canonicalize` 量的就是**那台自己的盘**，本机远端同一道（〔AL2〕从前住 monitor 时
/// 只对本机做，远端靠写口 `files_write::resolve_existing_in_root` 兜）。写口那一道照旧在。
pub(crate) fn fence(home: &str, raw: &str) -> Result<String, String> {
    let expanded = fence_lexical(home, raw)?;
    let (at, home_p) = (Path::new(&expanded), Path::new(home));
    if let Some(parent) = at.parent() {
        if let (Ok(real_parent), Ok(real_home)) = (parent.canonicalize(), home_p.canonicalize()) {
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

// 〔MIG-3a〕`K-R132` 那一节「装上了、能跑、用户敲不到」（用户级 PATH 那一格）留在 monitor（`profile_installer.rs`，本机后端的引导）。

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
// 那时的落盘原语 `atomic_write_string`〔散文墓碑〕走标准库的整份写，写的是**不带 BOM 的 UTF-8**
// （〔RW1〕今天落盘在后端 `files-put`，同样原样写字节、不加 BOM）。
// 而 **Windows PowerShell 5.1 把不带 BOM 的 `.ps1` 按系统 ANSI 代码页解**
// （这台机器上是 GBK）。一个 UTF-8 的 CJK 字符被当成 GBK 解，末尾会剩下一个
// **落单的前导字节**，它把紧随其后的换行吃掉 ⇒ 下一行被并进注释。
//
// ## 🔴 它**不是本件引入的** —— 发出去的 `v3.8.0` 上就有，而且更重
//
// 同一趟真机现打模板本身（`src/shared/cc.ps1.tpl`，逐字读数同住那份 evidence）：
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
    dialect::strip_bom(s)
}

/// 落盘的那一份：PowerShell 方言加 BOM，POSIX rc **一个字节都不加**（实现住方言那一格）。
fn encode_for_disk(flavor: Shell, content: &str) -> String {
    flavor.dialect().encode_for_disk(content)
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
/// monitor `profile_installer.rs` 的 `render_user_path_setup_command` / `render_user_path_removal_command` 生成（本机后端的引导那一格）。
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
/// POSIX 那边 `cct() { ccm --ccm-tmux "$@"; }`，而 **Windows 上没有 tmux** ⇒ 给它一个
/// 「名字在、行为不在」的壳比没有更坏（`K-R129` 那位用户正是照文案敲了 `cct`）。
///
/// ⚠ **`K-R132` 把「把 `cct` 从 Windows 文案里摘掉」随动到 `src/frontend/ui/launcher-diagnostics.ts`
/// 那一句上 —— 本轮现打，那个随动的前提是假的**：那一句只在 **POSIX rc** 那一臂印
/// （它的下拉只遍历 `AccountAliasReport::rc_candidates`〔散文墓碑〕（〔AL1〕今天是 `AliasListing::rc_candidates`），而那张表现算自
/// 那时 `account_aliases` 里那张 POSIX 候选表（〔AL1c〕今天住 `shell_dialect.rs` 的 POSIX 那一臂），**一份 PowerShell profile 都没有**），
/// 而那一臂的 `cct` 是**真有**的（`src/shared/ccm-aliases.sh` 里就定义着）。
/// PowerShell 那一臂是另一份文件（那时的 `src/frontend/ui/settings/cc_integration.ts` 的 `renderScanResult`；〔AL1c〕今天并进了
/// `src/frontend/ui/settings/machine-aliases.ts`，是 PowerShell 那一侧的别名块，生成的别名在 PowerShell 上照样不带 tmux 那一族），
/// 现打 `cct` **零命中**。⇒ **Windows 文案里今天一个 `cct` 都没有，没有东西要摘。**
/// 读数 · 量法 · 分母住 `tests/evidence/K-R135-摸底.md`。
///
/// 〔HX2 · RT1 F6〕`monitor_data_dir` 填进模板那一格 `{{MONITOR_DATA_DIR}}`（`__ccm_bind` 找 `ps-registry/` · `ps-await/` ·
/// `auto-launch.json` 的那个目录），按 PowerShell 单引号字面量写。它只有一个出口 —— `paths::resolve_monitor_data_dir`
/// （跟 `CCM_DATA_DIR`），由 [`plan_install`] 取了交进来。
/// 〔墓碑 —— 从前模板里自己写死一份 `Join-Path $env:USERPROFILE '.claude\claudecode-frontend'`：数据目录的第二个住址，
///  `CCM_DATA_DIR` 隔离跑时每次 `cc` 白等 3 s ＋ 一句「绑定超时」（`第四波记录/RT1.md §8` F6）。〕
pub(crate) fn render_cc_code(
    command_name: &str,
    include_cc_function: bool,
    monitor_data_dir: &Path,
) -> String {
    let safe_name = sanitize_command_name(command_name);
    let cc_block = if include_cc_function {
        // 🔴 `KR135D2`：**翻正的落点**在方言那一份（`platform/shell/dialect.rs::ps_wrapper_function`）；
        // 那个词现算自 `SUBCOMMAND_WORD`（`13b`：那个词的唯一住址），不写第二份字面量。
        dialect::ps_wrapper_function(&safe_name, crate::control::ccm::SUBCOMMAND_WORD)
    } else {
        String::new()
    };
    // 〔`R86`〕这里原先是一个三项拼装：会话级 PATH 那一段 ＋ 它的说明注释 ＋ `cc` 那一块。
    // 前两项删了（理由住上面那段横幅），于是**装进 profile 的东西只剩 `cc` 那一块**。
    CC_TEMPLATE
        .replace("{{CC_FUNCTION_BLOCK}}", &cc_block)
        .replace(
            "{{MONITOR_DATA_DIR}}",
            &dialect::ps_literal(&monitor_data_dir.to_string_lossy()),
        )
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
pub(crate) fn install_to_profile(
    d: &dyn Door,
    path: &Path,
    command_name: &str,
    include_cc_function: bool,
) -> Result<(), String> {
    let flavor = Shell::of_target(path);
    let what = path.display().to_string();
    let home = door::home(d)?;
    let rel = door::rel_under(&home, &what)?;
    door::edit(d, &home, &rel, true, true, |raw| {
        // 〔`K-R132`〕BOM 剥在**最靠近读的那一跳**；落盘那一份按方言再编码回去 ——
        // 读回比对比的是落盘那一份（比计划出来的那一份会恒差三个字节，当场回滚）。
        let existing = strip_bom(raw.unwrap_or(""));
        let updated = plan_install(
            flavor,
            existing,
            command_name,
            include_cc_function,
            &what,
            &home,
        )?;
        Ok(Some(encode_for_disk(flavor, &updated)))
    })
    .map(|_| ())
}

/// 卸载：整块删除 BEGIN/END 之间的内容（含 marker 行）。块外内容不动。
/// 文件不存在 ⇒ 什么都不做。读写经 `door`，规则同 [`install_to_profile`]（住后端）。
pub(crate) fn uninstall_from_profile(d: &dyn Door, path: &Path) -> Result<(), String> {
    let flavor = Shell::of_target(path);
    let what = path.display().to_string();
    let home = door::home(d)?;
    let rel = door::rel_under(&home, &what)?;
    door::edit(d, &home, &rel, true, false, |raw| {
        let Some(raw) = raw else { return Ok(None) };
        let stripped = plan_uninstall(flavor, strip_bom(raw), &what)?;
        Ok(Some(encode_for_disk(flavor, &stripped)))
    })
    .map(|_| ())
}

/// 〔TL1 · 4C〕这一版模板的块头版本串（`src/shared/cc.ps1.tpl` 第一行 `BEGIN vN` 那个 `vN`）—— 版本号的唯一住址是模板本身。
pub(crate) fn current_block_version() -> Option<String> {
    find_block_version(CC_TEMPLATE).1
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
        // 〔OSA〕两种写法的认法住方言（`ShellDialect::declared_function`）。
        if let Some(name) = flavor.dialect().declared_function(l) {
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
    super::fence::splice_in(
        existing,
        BEGIN_MARKER,
        END_MARKER,
        new_block,
        what,
        super::fence::Layout::PowerShell,
    )
}

/// 删除 ccm 块（如果有）。围栏损坏时 `Err` 中止而不是「当作没有块、原样返回」——
/// 后者让用户以为卸载干净了，而那个悬空的 BEGIN 下次安装就会吃掉它下面的内容。
/// 拼接同上走 `fenced_block::splice_out`。
fn strip_block(existing: &str, what: &str) -> Result<String, String> {
    super::fence::splice_out(
        existing,
        BEGIN_MARKER,
        END_MARKER,
        what,
        super::fence::Layout::PowerShell,
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

// ═══════════════════════════════════════════════════════════════════════════
// 〔W5-ALIAS · 第五波先行〕**别名块的真相**：从 `sftp.rs` 搬来（那份文件已经不做 SFTP，B §2 第 12 条）
// ═══════════════════════════════════════════════════════════════════════════
//
// 搬的是：POSIX rc 里那一对围栏 · 块的内容（`src/shared/ccm-aliases.sh`）· 自带的名字 · 合 / 剥 ·
// 远端装 / 卸那两条命令。一个字节的行为没变，只换住址 —— 本模块本来就是「别名块」那一族
// （[`plan_install`] / [`block_state`] / [`render_block`] / [`install_to_profile`]），从前反过来借 `sftp::` 的东西。

// F10：远端 cc/bash 集成——一键把 ccm wrapper 装进远端 ~/.bashrc（SS-H）。
// 写 ~/.bashrc 不是 Claude 数据（不触 INVARIANT §1），与本地 PowerShell profile 安装同性质。

/// 远端 ccm 块的 BEGIN/END 标记（镜像本地 profile_installer 的 `# === cc-monitor BEGIN/END`）。
/// 重装时整块替换、卸载时整块删；用户在块外的内容绝不动。
///
/// ⚠ `K-R62` 起是 `pub(crate)`：**本机 POSIX 那条路装的是同一个块**
/// （`profile_installer::plan_install` 的 `PosixRc` 臂走 [`merge_profile_block`]）。
/// 在那边抄一对同样的字符串就是第二个住址 —— 而「同一件事有两个住址」正是
/// `KR62D1` 那条「不许变成第四套」要挡的东西。名字里的 `remote` 是历史，
/// 今天它的意思是「**POSIX rc 里那一对围栏**」，本机远端共用。
pub(crate) const CCM_PROFILE_BEGIN: &str = "# === cc-monitor remote ccm BEGIN ===";
pub(crate) const CCM_PROFILE_END: &str = "# === cc-monitor remote ccm END ===";

/// 远端 ↗ 拉前用的 `ccm` wrapper（**后端拥有**，install 写它而非前端传入——见审计 S-1：
/// 写进 ~/.bashrc 的是被 shell **执行**的代码，绝不能让前端注入任意 bash）。
///
/// **必须与前端 `remote-section.ts::CCM_WRAPPER_SNIPPET`（面板展示/手动复制用）逐字一致。**
/// **单一来源**：`src/shared/ccm-aliases.sh`——前端 `remote-section.ts` 经 `?raw` import
/// 同一文件（修复历史漂移：Batch7 重构时只改了前端展示版，装进远端的还是老版）。
///
/// **F02 起本块只剩「别名层」**；`K-R48` 第二拍起它指向的那个 `ccm` 是 `local_backend::ccm_entry_shim`
/// （三行入口，转给后端本体），不再是一份 bash 实现。
/// 理由：shell 函数**优先于 PATH**，装成函数则与用户已有同名函数硬冲突且必然被遮蔽（实测）；
/// 且远端是 zsh/fish 时 `.bashrc` 根本不被 source，函数形态拿不到（审计 D2）。
/// ⚠ `K-R49` 起它是 `pub(crate)`：`account_aliases::collision_note` 要问
/// 「`cc` / `cct` 这几个名字是不是已经被自带的别名块占了」，
/// 而那个答案**只有这份文件说了算** —— 在那边抄一份名字清单就是第二个住址。
/// 〔`K-R58` 09-11：`cch` 从这份文件里删了 ⇒ 它**不再**被当作「已被占用」，
/// 用户可以自己定义一个 `cch`。**多一格自由，不是回归。**〕
pub(crate) const CCM_WRAPPER_SNIPPET: &str = include_str!("../../../shared/ccm-aliases.sh");

/// 自带别名块里**今天定义了哪几个名字** —— 现算，不写死（`13b`：闭集只许有一个住址，
/// 那个住址就是 `src/shared/ccm-aliases.sh` 自己）。
///
/// `account_aliases` 的撞名判据与本文件的文档对账判据都拿它当人群，
/// 于是「删/加一个别名」这件事**不需要同时去改两份名单**（改漏一份正是 `KR58D1`
/// 的失效方向）。
///
/// 🔴 〔`K-R62` 09-11〕**它从 `#[cfg(test)]` 转正了**，因为多了一个生产使用者：
/// `profile_installer::render_manual_cleanup_hint` 要回答「你 rc 里那几行裸的
/// `cc()` / `cct()`，会不会把我们装的那一块遮蔽掉」—— 那个答案**只有这份文件说了算**，
/// 在提示文案里抄一份名字清单就是第二个住址。转正**没有放宽任何东西**：
/// 它仍然现算自 [`CCM_WRAPPER_SNIPPET`]，一个字节的名单都没写死。
///
/// ⚠ **它认的形状写死在这里**：`<名>() {`（`()` 与 `{` 之间允许空白）。
/// 注释行里那两条示例（`#   zcc()  { … }`）靠「名字只许 `[A-Za-z0-9_]`」被剔掉 ——
/// 换一种写法（`function cc {`）它会**漏**，而漏出来的形状是「人群变空」，
/// 调用处一律先断 `!is_empty()`，不让它静默变成空真。
pub(crate) fn builtin_alias_names() -> Vec<String> {
    // 〔OSA〕认法住方言（`ShellDialect::declared_function`，POSIX 那一臂）。
    let mut v: Vec<String> = CCM_WRAPPER_SNIPPET
        .lines()
        .filter_map(|l| Shell::Posix.dialect().declared_function(l))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// 〔OSA〕我们自己那块别名块的正文 —— 撞名那一问交给方言认函数用（`ShellDialect::name_taken`）：
/// POSIX 是 [`CCM_WRAPPER_SNIPPET`] 本身；PowerShell 是模板渲染出来的那一份（与数据目录无关 —— 喂一个占位目录）。
pub(crate) fn own_block(shell: Shell) -> String {
    match shell {
        Shell::Posix => CCM_WRAPPER_SNIPPET.to_string(),
        Shell::PowerShell => render_cc_code(CC_FUNCTION_NAME, true, Path::new("/_")),
    }
}

/// 纯函数：把 `snippet` 合进 profile 内容的 BEGIN/END 块（可单测）。
/// - 已有**配对**块（BEGIN 后能找到 END）→ **整块替换**（幂等：`merge(merge(x))==merge(x)`）。
/// - 无 BEGIN → **追加**（块外内容原样保留）。
/// - **有 BEGIN 但其后无 END（损坏/截断/上次安装中断）→ `Err` 中止**（审计 B1：绝不用独立
///   `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，也不破坏文件）。
pub(crate) fn merge_profile_block(
    existing: &str,
    snippet: &str,
    what: &str,
) -> Result<String, String> {
    // **T04 第二步：配对判定改走 `fenced_block::find_pair`，与本机 profile 共用同一条规则。**
    //
    // **更正我原话「判定本身是对的…判定没变」——被实测证伪，9 个边界里 3 个变了**
    // （T04 审计②，它把旧 byte-find 实现逐字复制成 `old_merge` 并列对拍）：
    //   1. **行内 marker**（用户 profile 里有 `echo "…BEGIN…"` / `echo "…END…"`）：
    //      旧实现会**切断那个 echo 行、并把第二个 echo 行整行吃掉** —— 远端侧一个
    //      **我未申报就修掉了的数据丢失**。新实现按行 `trim_start().starts_with` 判，改成追加。
    //   2. **BEGIN 与 END 同一行**：旧能正确替换该行 → 新直接 Err（`find_pair` 认到 BEGIN
    //      就 `continue`，同行的 END 被跳过）。**这是退化**，虽符合"宁可报错"但当时未文档化未测试。
    //   3. **缩进 marker**：旧"保留 BEGIN 行缩进、丢 END 缩进"（不自洽）→ 新统一归一到列 0。
    // 三条现在都有测试锁死（见 `remote_merge_boundary_semantics_after_migration`）。
    //
    // 原实现是自己 `find(BEGIN)` 再在其后 `find(END)`——
    // 但本机侧漏了同一道保护，于是两侧对"围栏损坏"处置不一致、本机那边会**吃掉用户内容**。
    // 现在两侧同一个函数，判定不可能再漂移。
    // 〔AL1 · 2026-09-24〕配对之后怎么拼，**也只剩一份**：`fenced_block::splice_in`（`71 §12.5`）。
    //   本函数只答「POSIX rc 里这一块长什么样」（方言的内容与围栏），不再自己切行拼接。
    let block = format!(
        "{CCM_PROFILE_BEGIN}\n{}\n{CCM_PROFILE_END}\n",
        snippet.trim()
    );
    super::fence::splice_in(
        existing,
        CCM_PROFILE_BEGIN,
        CCM_PROFILE_END,
        &block,
        what,
        super::fence::Layout::Posix,
    )
}

/// 纯函数：从 profile 内容删掉 cc-monitor 的 BEGIN/END 块（可单测）。
/// - 有**配对**块（BEGIN 后找得到 END）→ 整块删，块前后用户内容原样保留。
/// - 无 BEGIN，或 BEGIN 后无 END（损坏）→ **原样返回**（宁可不删也不破坏文件）。
pub(crate) fn strip_profile_block(existing: &str, what: &str) -> Result<String, String> {
    // **T04 审计阻塞：这里原先没迁移，于是「卸」那半边被我从"两侧一致"改成了"两侧不一致"。**
    // 原实现在悬空 BEGIN 时 `return existing.to_string()` → 调用方判 `stripped == existing`
    // → 打印「远端 {profile} 里没有 ccm 块，无需卸载」。**那正是我在同一个 commit 里
    // 定义为 bug 的形态**，而且比本机那边更糟：它主动告诉用户"没问题"。
    //
    // 更要紧的是这是我**新造的漂移**：`af21ffb~1` 时两侧卸载都"原样返回"（一致），
    // `af21ffb` 之后本机 Err、远端静默 no-op（不一致）。我 commit 里那句
    // 「两侧不可能再漂移」**只对 install 半边成立，对 uninstall 半边方向相反**。
    // 现在两侧的装与卸四条路全走 `find_pair`。
    // 〔AL1〕拼接走 `fenced_block::splice_out`（与装那一半同一份，`71 §12.5`）。
    super::fence::splice_out(
        existing,
        CCM_PROFILE_BEGIN,
        CCM_PROFILE_END,
        what,
        super::fence::Layout::Posix,
    )
}

// 〔AL2 · 第四波 4D〕远端装 / 卸别名块那两条 Tauri 命令（连同只收 home 下裸文件名的那道小围栏）删了：
//   并进 `lib.rs` 的 `aliases_block_install` / `_remove`（带 `origin`，本机远端同一条）。

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/block_tests.rs"]
mod tests;
