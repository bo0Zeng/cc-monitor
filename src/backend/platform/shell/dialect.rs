//! **shell 方言：那组平台接口，POSIX 与 PowerShell 各一份实现。**
//!
//! 住后端 OS 适配层（`platform/shell/`）。本层不往上依赖：
//! `ccm` 那个词与 `--` 分界由通用层作 [`Call`] 交进来，「我们自己那块别名块定义了哪些函数」由通用层交进那块的正文。
//!
//! 别名规则与方言住**那台机器的后端**：
//! 「这台说不说 PowerShell」「`PATH` 上有没有同名程序」「文档目录在哪」都是**这台自己**的事实。
//!
//! # 判准
//!
//! ```text
//! 「这个 shell 怎么写 / 文件落在哪 / 名字怎么认」 → 这里（翻译官）
//! 「一条别名合不合格 / 有一条不合格整批不写 / 围栏怎么拼 / 写的序列」 → 通用层（account_aliases · fenced_block · 后端 Door::put）
//! ```
//!
//! 通用层持有的是**结构**（名字 ＋ 一组 ccm 参数），不持有任何一种 shell 的文本 —— 一旦持有一段 POSIX 脚本，
//! 这里就翻译不了它（`src/backend/platform/shell/mod.rs` 头注那条「交给 PowerShell 是另一种语言」）。
//! ⇒ 本模块里**零规则**：组合规则、控制字符、重名、能力闸都不在这里；这里只答「这个 shell 里怎么写 / 怎么读」。
//!
//! # 方言由**目标**决定，不由宿主平台决定（末段）
//!
//! [`Shell::of_target`] 按目标文件的扩展名判（`.ps1` ⇒ PowerShell），刻意不看 `cfg!(windows)`：
//! Windows 上的 Git Bash 读的是 `~/.bashrc`（POSIX）；门禁跑在 Linux 上，按 `cfg` 分的话两条臂里恒有一条没人验。
//!
//! # 七问里不在这里的两问
//!
//! - `atomic_replace()`：写的序列（CAS · 备份 · 暂存旁名换名上位 · 回读 · 回滚）只住后端
//!   `files/write.rs::put_text`，Windows 那一支是就地覆盖保 ACE。方言只多答一格「落盘前要不要加 BOM」
//!   （[`ShellDialect::encode_for_disk`]）。
//! - `has_tmux()`：「不进这一族 —— 它是一项能力」⇒ 住通用层 `account_aliases::Caps`。
//!
//! # 🔴 PowerShell 那一臂的诚实边界
//!
//! 跑 cargo 的这台没有 `pwsh` / `powershell` ⇒ PowerShell 文本在这里的判据里**一次都没被
//! PowerShell 解析过**。它买到的只有：函数体逐字照 `src/shared/cc.ps1.tpl` 里那个 `cc` 的形状（Windows 真机上
//! 那一形 `parse-errors=0`）· 黄金串。

use crate::platform::fs::Ro;
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 「这是哪种 shell 的方言」—— **唯一一个**回答这一问的枚举。
///
/// 它取代了 `profile_installer` 里那个只给别名块用的方言枚举（那一族的旧名见本仓 `git log`）：
/// 别名文件、别名块、source 那一行，三件事问的是同一个问题，不许有两个枚举各答一半。
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "ShellKind",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    /// POSIX sh（bash / zsh 都 source 得了；fish 不行）。
    Posix,
    /// PowerShell（5.1 与 7 同一种写法）。
    #[serde(rename = "powershell")]
    PowerShell,
}

impl Shell {
    /// 按**目标文件**定方言：`.ps1` ⇒ PowerShell，其余一律 POSIX。
    ///
    /// ⚠ 为什么按扩展名而不是按 `cfg!(windows)`：**跑在哪台机器上**与**这份文件是什么**是两件事。
    /// 用户可以指任意一份 home 内的文件，而 PowerShell 的 profile 恒是 `.ps1`（`$PROFILE` 的四种取值全是）。
    pub fn of_target(path: &Path) -> Shell {
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("ps1") => Shell::PowerShell,
            _ => Shell::Posix,
        }
    }

    /// 这种方言的那份实现。
    pub fn dialect(self) -> &'static dyn ShellDialect {
        match self {
            Shell::Posix => &Posix,
            Shell::PowerShell => &PowerShell,
        }
    }
}

/// 调用时跟在别名后面的那些词交给谁：缺省交给 agent（接在 `--` 左边）；交给 ccm 时接在右边末尾。
/// 许不许交给 ccm 是通用层的规则（只有接回会话那一形），方言只照着写。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestTo {
    #[default]
    Agent,
    Ccm,
}

/// 一条别名调的是谁、argv 里哪个词把两半分开。**通用层给**（`control::ccm` 那两个常量），方言只照着写：
/// 适配层不往上依赖。
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    /// 被调的那个命令（`ccm_line::SUBCOMMAND_WORD`）。
    pub word: &'a str,
    /// 分两半的那个词（`ccm_line::argv::flag::END`）：左边交 claude，右边归 ccm。
    pub end: &'a str,
    /// ccm 落在家目录下哪儿（`/` 分隔的相对路径，`relay_route_core::BACKEND_LANDING_REL`）：不靠 `PATH` 的那一臂按它显式叫
    /// （PowerShell：别名块不动会话 `PATH`）。
    pub landing: &'a str,
}

/// 托管入口（`cch` · `<号>cch`）补全要的那几个词。**通用层给**（`ccm_line::argv` 的 `hosted_word` · `flag`），方言只照着写。
#[derive(Debug, Clone, Copy)]
pub struct HostedWords<'a> {
    /// 名字后面第一个词补这几个（`ls` · `attach`）。
    pub first: &'a [&'a str],
    /// 跟在这几个词后面的那一格补这台的托管终端名（`attach` · 它的短写 `a`）。
    pub takes_name: &'a [&'a str],
    /// 列托管终端那一下交给 ccm 的词（`--` `--hosts`，即 `cch ls`）：印出的每行第一段是名字、段与段之间两个空格；
    /// 没有两个空格的那一行（「无托管终端」）不是名字。
    pub list: &'a [&'a str],
}

/// 一份启动文件候选：**路径 ＋ 盘上不在时列不列**（「列不列不存在的」是方言的读法，表第一行）。
///
/// 方言**只给路径与列法**，一个字节的盘都不读：在不在、里面是什么由**那台机器的后端**答
/// （`account_aliases::rc_candidates_via` 经 `user_files::Door` 问 `Door::peek` / `files-stat`）——
/// 这里自己 `is_file()` / `is_dir()` 的话，量的是 monitor 这台的盘，拿去说远端是错的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupCandidate {
    /// 那台机器上的绝对路径（按那台 home 的写法拼，见 `user_files::join_under`）。
    pub path: String,
    pub listed: Listed,
    /// 哪一代 PowerShell 加载它（执行策略按代问）；POSIX 与人另指的那一份 ⇒ `None`。
    pub host: Option<super::PsHost>,
}

/// 一份候选**不在盘上时**列不列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listed {
    /// 不在就不列（POSIX 那四份：只列真有的）。
    IfFileExists,
    /// 不在也列（`$PROFILE` 常常要装的时候才建）。
    Always,
    /// 这个目录在才列，文件在不在都列（PowerShell 7 那两份：目录在 ⇒ 装过且至少跑过一次）。
    IfDirExists(String),
}

/// 🔴 **方言那组接口。** 每个方法都只回答「这个 shell 里怎么写 / 怎么读 / 文件在哪」；
/// 任何「合不合格」的判断都不许写进实现里（那是通用层的，两边一模一样）。
pub trait ShellDialect: Sync {
    // 别名块的排版按目标文件扩展名走 `profile_installer` 那一份，不经这里。

    /// 落盘前的编码。PowerShell 加 BOM（PS 5.1 把无 BOM 的 `.ps1` 按 ANSI 代码页解，
    /// 中文注释会吞掉下一行）；POSIX 一个字节都不加（`.bashrc` 开头多三个字节 ⇒ `sh` 把它当命令）。
    fn encode_for_disk(&self, content: &str) -> String;

    /// shell 启动时会执行的那几份文件（界面「别名块装进哪份」的候选），按优先级。
    /// 「列不列一份还不存在的文件」是读法，由实现答（POSIX 只列在的 · PowerShell 的 `$PROFILE` 常常要装时才建）。
    /// `home` 是**那台机器**的 home（后端 `files-home` 答的字符串）；这里一个字节的盘都不读（[`StartupCandidate`]）。
    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate>;

    // 别名块建不建 `$PROFILE`，归 `profile_installer` 那一份规则（source 那一行只住别名块里）。

    /// 我们自己那份别名文件在 home 下的相对路径（`/` 分隔，交给后端的 `rel` 就是它）。
    fn our_alias_file_rel(&self) -> &'static str;

    /// 这份启动文件是不是已经接上了我们那份（任何一种写法都认，别按整行比 —— 见 POSIX 那一臂的注释）。
    fn sources_our_file(&self, text: &str) -> bool;

    /// 我们那份别名文件的头注（整段注释行，含结尾换行）。
    fn file_header(&self) -> String;

    /// 一条（通用层判过合格的）别名在这个 shell 里的写法：名字 ＋ 预置参数 ＋ 把调用时的参数原样接在 `rest` 那一边。
    fn render_alias(&self, call: Call, name: &str, argv: &[String], rest: RestTo) -> String;

    /// 托管入口的补全（接在别名文件里那几条别名之后）：`names` 那几条（通用层判过：合下来带 `--hosted`）
    /// 首词补 [`HostedWords::first`]，跟在 [`HostedWords::takes_name`] 后面补这台的托管终端名（现问 ccm）。
    fn render_hosted_completion(&self, call: Call, names: &[String], words: HostedWords) -> String;

    /// 在终端里敲的那一行：按绝对路径叫 `program`（不靠 `PATH`）、带一个词 `word`（通用层判过合格的 `@名`）。
    fn render_start(&self, program: &str, word: &str) -> String;

    /// 这一行（已去掉行首空白）声明了哪个函数（别名块认函数只用这一份认法）。
    fn declared_function(&self, line: &str) -> Option<String>;

    /// 名字的合法字符集。
    fn name_is_valid(&self, name: &str) -> bool;

    /// 两个名字在这个 shell 里是不是同一个函数（PowerShell 大小写不敏感）。
    fn same_name(&self, a: &str, b: &str) -> bool;

    /// 这个名字是不是已经被占了（**只出声、不拦**）。报出来的话里带住址。
    /// `own_block` 是我们自己那块别名块的正文（通用层给：POSIX 是 `src/shared/ccm-aliases.sh`，PowerShell 是模板渲染出来的那一份）。
    ///
    fn name_taken(&self, name: &str, own_block: &str) -> Option<String>;
}

/// UTF-8 BOM。**闭集只有这一处住址**。
const UTF8_BOM: &str = "\u{feff}";

/// 读进来的那一份：把 BOM 剥掉再交给任何**判内容**的东西（两种方言都剥 —— 读的一侧宽，写的一侧严）。
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix(UTF8_BOM).unwrap_or(s)
}

/// `PATH` 上有没有一个叫这个名字的程序（带上这些扩展名之一；空串 = 不带扩展名）。
///
/// ⚠ **诚实边界**：查的是这台后端进程的 `PATH`，不是用户登录 shell 的 `PATH` ⇒ 会漏报，不会误报成「有」。
/// PowerShell 内建别名：小写名字 → 它指向的命令。问不到 ⇒ `Err(原因)`。
pub(crate) type PsAliases = Result<std::collections::BTreeMap<String, String>, String>;

/// 这台那一份（起一次、进程内缓存；这台没有 PowerShell ⇒ 说问不到，不当成「没撞」）。
fn ps_builtin_aliases() -> &'static PsAliases {
    static ONE: std::sync::OnceLock<PsAliases> = std::sync::OnceLock::new();
    ONE.get_or_init(|| {
        let r = ask_get_alias();
        // 问不到的原因（含 PowerShell 的原话）只记这一次日志；撞名提示那一句不带它。
        if let Err(e) = &r {
            tracing::warn!("PowerShell 自带别名问不到: {e}");
        }
        r
    })
}

/// `Get-Alias` 那一段的输出（每行 `名字<TAB>指向`）→ 表。名字按 PowerShell 的口径不分大小写（存小写）。
pub(crate) fn parse_alias_listing(text: &str) -> std::collections::BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.trim_end().split_once('\t'))
        .filter(|(n, _)| !n.trim().is_empty())
        .map(|(n, d)| (n.trim().to_ascii_lowercase(), d.trim().to_string()))
        .collect()
}

/// 这个名字撞没撞内建别名（撞了 ⇒ 那句话；问不到 ⇒ 说问不到，不当成「没撞」）。
pub(crate) fn builtin_alias_note(name: &str, aliases: &PsAliases) -> Option<String> {
    match aliases {
        Ok(m) => m.get(&name.to_ascii_lowercase()).map(|target| {
            copy_text(
                "rsShellDialect.ps.nameTakenBuiltinAlias",
                &[("name", name), ("target", target)],
            )
        }),
        Err(_) => Some(copy_text(
            "rsShellDialect.ps.builtinAliasUnknown",
            &[("name", name)],
        )),
    }
}

/// 问内建别名那一趟的期限：界面等别名那一族的预算是 30 s，收一档到 15 s。
const GET_ALIAS_WITHIN: crate::platform::child::Deadline =
    crate::platform::child::Deadline::secs(15);

/// `Get-Alias` 那一段：只读、不吃任何用户输入。
const GET_ALIAS_SCRIPT: &str = "Get-Alias | ForEach-Object { $_.Name + [char]9 + $_.Definition }";

/// 起一次 5.1 的 `-NoProfile -NonInteractive -Command <固定脚本>`（这条 argv 与不弹窗那一格住 `platform::shell`）。
/// `-NoProfile`：问的是**自带**那一份（用户 profile 里另加 / 删的别名不算）。这台没有 PowerShell ⇒ `Err`（说问不到）。
fn ask_get_alias() -> PsAliases {
    let cmd = crate::platform::shell::powershell_on(super::PsHost::Desktop, GET_ALIAS_SCRIPT)
        .ok_or_else(|| copy_text("rsShellDialect.ps.noPowerShellHere", &[]))?;
    let out = cmd.run(GET_ALIAS_WITHIN).map_err(|e| e.to_string())?;
    if !out.status.success() {
        let why = format!(
            "exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        tracing::warn!("问 PowerShell 内建别名没问成：{why}");
        return Err(why);
    }
    Ok(parse_alias_listing(&String::from_utf8_lossy(&out.stdout)))
}

/// 这个后端进程 `PATH` 上敲 `name` 会起来的那个文件（`windows` ＝ 按 PowerShell 的规矩找：`.com` · `.exe` · `.bat` · `.cmd` · `.ps1`）。
/// 找法只住 `platform::shell::find_session_command`；这里只多一条：我们自己放的别名链接不算撞名。
fn on_path(name: &str, windows: bool) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok();
    // 我们自己放的别名链接（`~/.cc-monitor/bin/<名>` → `ccm`，`assets/aliases/links.rs`）不算撞名：那就是这条别名本身。
    let ccm = relay_route_core::BACKEND_LANDING_REL.rsplit('/').next();
    let theirs = |cand: &std::path::Path| {
        cand.is_file() && !Ro::read_link(cand).is_ok_and(|t| t.to_str() == ccm)
    };
    match super::find_session_command(name, path.as_deref(), windows, &theirs) {
        super::SessionCommand::Found { file, .. } => Some(file),
        _ => None,
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*`。两种方言用**同一个**字符集：PowerShell 本来还收 `-`，不收是为了
/// 一份清单搬到哪台机器上都还是合法的名字。
fn portable_name(name: &str) -> bool {
    let mut cs = name.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ═══════════════════════════════════════════════════════════════════════════
// POSIX
// ═══════════════════════════════════════════════════════════════════════════

/// POSIX sh 那一份实现。
pub struct Posix;

/// POSIX 那一侧「source 那一行加进哪份」的候选（**只列真实存在的那几份**）。
///
/// ⚠ fish 的 `config.fish` **不在这里**，那不是遗漏：生成文件是 POSIX sh 的函数写法，
/// fish 根本 `source` 不了它。少列一个候选好过让人点一下之后 shell 报一屏语法错。
const POSIX_RC_CANDIDATES: &[&str] = &[".bashrc", ".zshrc", ".bash_profile", ".profile"];

/// POSIX 函数声明的头与尾。
/// ⚠ 住常量的理由：`structural_scan` 那把尺子按大括号配平切函数体，字面量里落单的 `{` 会让它切过界。
const POSIX_FN_HEAD: &str = "() { ";
/// `"$@"`（用户敲别名时跟的参数）落在 `--` 左边 —— 交给 claude；别名自己的 ccm 选项在它后面的 `--` 右边。
const POSIX_ARGS: &str = " \"$@\"";
const POSIX_FN_TAIL: &str = "; }";

/// 托管入口补全那一段（POSIX）。bash 照 `complete -C` 叫它：`$1` 命令名 · `$2` 正在敲的词 · `$3` 前一个词，印出来的每行是一个候选。
/// 函数体只用 POSIX sh 的写法（这份文件也可能经 `.profile` 被 dash 读进来）；`complete` 只在 bash 里挂（zsh 不挂）。
/// 名单那一行按两个空格切、第一段是名字；没有两个空格的那一行（「无托管终端」）不算。
/// ⚠ 住常量的理由同 [`POSIX_FN_HEAD`]：大括号在同一个字面量里配平。
const POSIX_HOSTED_COMPLETION: &str = r#"__ccm_hosted_complete() {
    case "$3" in
        "$1") for w in {{FIRST}}; do case "$w" in "$2"*) printf '%s\n' "$w" ;; esac; done ;;
        {{NAMED}}) {{CCM}} {{LIST}} 2>/dev/null | while IFS= read -r l; do case "$l" in *'  '*) n=${l%%'  '*}; case "$n" in "$2"*) printf '%s\n' "$n" ;; esac ;; esac; done ;;
    esac
}
if [ -n "${BASH_VERSION:-}" ]; then complete -C __ccm_hosted_complete {{NAMES}}; fi"#;

/// POSIX 那份别名文件在 home 下的相对路径。从 `account-aliases.sh` 改名而来，旧名不读不写不删。
/// 值只住契约 crate（monitor 的数据位置页按它列出）。
const POSIX_ALIAS_FILE_REL: &str = relay_route_core::POSIX_ALIASES_REL;

impl Posix {
    /// 一个参数要不要加引号：只由「安全字符」组成的原样放，其余一律 POSIX 单引号
    /// （实现只有一份：`shell_quote_core::posix_quote`）。
    fn word(w: &str) -> String {
        let safe = !w.is_empty()
            && w.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '%' | '+' | '=' | ',')
            });
        if safe {
            w.to_string()
        } else {
            shell_quote_core::posix_quote(w)
        }
    }
}

/// 别名那条 argv 按最后一个 `--` 切成两半（没有 ⇒ 右边 `None`）。两种方言渲染共用。
fn split_last_end<'a>(end: &str, argv: &'a [String]) -> (&'a [String], Option<&'a [String]>) {
    match argv.iter().rposition(|w| w == end) {
        Some(k) => (&argv[..k], Some(&argv[k + 1..])),
        None => (argv, None),
    }
}

impl ShellDialect for Posix {
    fn encode_for_disk(&self, content: &str) -> String {
        content.to_string()
    }

    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate> {
        POSIX_RC_CANDIDATES
            .iter()
            .map(|n| StartupCandidate {
                path: crate::platform::paths::join_under(home, n),
                listed: Listed::IfFileExists,
                host: None,
            })
            .collect()
    }

    fn our_alias_file_rel(&self) -> &'static str {
        POSIX_ALIAS_FILE_REL
    }

    /// 🔴 认的是**相对路径**，不是整行：`src/shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`
    /// （**没展开**），人自己手写的那一行可能是展开后的绝对路径 ⇒ 按整行比会漏认。
    fn sources_our_file(&self, text: &str) -> bool {
        text.contains(POSIX_ALIAS_FILE_REL)
    }

    fn file_header(&self) -> String {
        copy_text("rsShellDialect.posix.header", &[])
    }

    /// `'<路径>' @名`。
    fn render_start(&self, program: &str, word: &str) -> String {
        format!("{} {word}", shell_quote_core::posix_quote(program))
    }

    /// `名字() { ccm <参数…> "$@"; }`。`"$@"` 必须在最后 —— 那就是「参数附加器」的全部含义：
    /// 调用时再给的参数接在后面、后者胜。
    fn render_alias(&self, call: Call, name: &str, argv: &[String], rest: RestTo) -> String {
        let word = call.word;
        let (left, right) = split_last_end(call.end, argv);
        let mut out = format!("{name}{POSIX_FN_HEAD}{word}");
        for w in left {
            out.push(' ');
            out.push_str(&Self::word(w));
        }
        if rest == RestTo::Agent {
            out.push_str(POSIX_ARGS);
        }
        if let Some(right) = right {
            out.push_str(" --");
            for w in right {
                out.push(' ');
                out.push_str(&Self::word(w));
            }
        }
        if rest == RestTo::Ccm {
            out.push_str(POSIX_ARGS);
        }
        out.push_str(POSIX_FN_TAIL);
        out
    }

    /// [`POSIX_HOSTED_COMPLETION`] 填上词：ccm 照别名那样按 `PATH` 叫（别名块让它进 `PATH`）。
    fn render_hosted_completion(&self, c: Call, names: &[String], w: HostedWords) -> String {
        let join = |ws: &[&str], sep: &str| {
            ws.iter()
                .map(|x| Self::word(x))
                .collect::<Vec<_>>()
                .join(sep)
        };
        POSIX_HOSTED_COMPLETION
            .replace("{{FIRST}}", &join(w.first, " "))
            .replace("{{NAMED}}", &join(w.takes_name, "|"))
            .replace("{{CCM}}", &Self::word(c.word))
            .replace("{{LIST}}", &join(w.list, " "))
            .replace(
                "{{NAMES}}",
                &names
                    .iter()
                    .map(|n| Self::word(n))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
    }

    /// `名字() {` ⇒ `Some("名字")`。
    fn declared_function(&self, l: &str) -> Option<String> {
        let (name, rest) = l.split_once("()")?;
        if !rest.trim_start().starts_with('{') {
            return None;
        }
        let name = name.trim();
        (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .then(|| name.to_string())
    }

    fn name_is_valid(&self, name: &str) -> bool {
        portable_name(name)
    }

    fn same_name(&self, a: &str, b: &str) -> bool {
        a == b
    }

    /// 两条路各查一次，报出来的话里带住址，用户才知道自己在盖掉什么：
    /// ① `src/shared/ccm-aliases.sh` 里自带的那几个（今天是 `cc` / `cct`）——**问的是那份文件本身**
    ///    （通用层交进来的 `own_block`），不在这里抄一份名字清单；
    /// ② `PATH` 上真有一个同名程序 —— 🔴 `cc` 在多数机器上是 C 编译器（`/usr/bin/cc`），
    ///    而自带那份别名只检查「有没有同名**函数**」、不检查程序。
    fn name_taken(&self, name: &str, own_block: &str) -> Option<String> {
        if own_block
            .lines()
            .filter_map(|l| self.declared_function(l))
            .any(|n| n == name)
        {
            return Some(copy_text(
                "rsShellDialect.posix.nameTakenBuiltin",
                &[("name", name)],
            ));
        }
        on_path(name, false).map(|cand| {
            copy_text(
                "rsShellDialect.posix.nameTakenPath",
                &[("name", name), ("cand", &(cand.display()).to_string())],
            )
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PowerShell
// ═══════════════════════════════════════════════════════════════════════════

/// PowerShell 那一份实现（5.1 与 7 同一种写法）。
pub struct PowerShell;

/// PowerShell 那份别名文件在 home 下的相对路径（交给后端的 `rel` 用 `/`；PowerShell 两种分隔符都认）。
/// 值只住契约 crate（monitor 的数据位置页按它列出）。
const PS_ALIAS_FILE_REL: &str = relay_route_core::PS_ALIASES_REL;

/// PowerShell 单引号串里算「引号」的那几个字符：ASCII `'` 之外，PowerShell 还把 `‘ ’ ‚ ‛` 当成同一个引号
/// （它的词法器逐字如此）⇒ 值里出现任何一个都得双写，否则串在那里就断了。
const PS_QUOTES: &[char] = &['\'', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];

/// 函数体里那几行固定的（与 `src/shared/cc.ps1.tpl` 里的 `cc` 逐字同形：那一形在 Windows 真机上 `parse-errors=0`）。
const PS_PARAM_OPEN: &str = "    [CmdletBinding()] param(";
const PS_PARAM_ARG: &str =
    "        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs";
const PS_PARAM_CLOSE: &str = "    )";
/// `__ccm_bind` 是终端集成块里那一下「勾了自动打开就先把 cc-monitor 开起来」（v9 起它只做这件事，名字沿用：
/// 已生成的别名文件按这一行逐字认），留适配层 —— 通用层一个字不知道它。
/// 带守卫：终端集成块没装时它不存在，这一行就是空操作（与 POSIX 那一侧同效）。
const PS_BIND: &str =
    "    if (Get-Command __ccm_bind -CommandType Function -ErrorAction SilentlyContinue) { __ccm_bind }";
/// 函数块的开与收。住常量而不是函数体里的字面量：`structural_scan` 那把「会剥注释的函数」尺子按大括号配平切函数体，
/// 字面量里落单的一个 `{` 会让它切不准、一路吃进下一个函数而误报。
const PS_BLOCK_OPEN: &str = "{";
const PS_BLOCK_CLOSE: &str = "}";
/// 最后一行的尾巴：调用时再给的参数原样接在后面（与 `cc` 同一写法：数组变量交给原生程序时逐个展开）。
const PS_TAIL: &str = " $RemainingArgs";
/// 分隔 claude / ccm 两半的 `--`：写成单引号字面量（裸 `--` 是 PowerShell 自己的「参数到此为止」记号，会被它吃掉）。
const PS_END: &str = " '--'";

/// 托管入口补全那一段（PowerShell）：挂在每条托管别名的 `RemainingArgs` 上（别名函数那一格，[`PS_PARAM_ARG`]）。
/// 已敲的词照 `$commandAst` 数（正在敲的那个不算）：0 个 ⇒ 首词；1 个且是 `attach` 那一类 ⇒ 现问 ccm 的名单，每行按两个空格切、第一段是名字
/// （「无托管终端」那一行切不出两段，不算）。形状在 Win11 上 PowerShell 5.1 的 `TabExpansion2` 实测过（`cch ` ⇒ ls,attach · `cch a ` ⇒ 名字）。
/// ⚠ 住常量的理由同 [`PS_BLOCK_OPEN`]：大括号在同一个字面量里配平。
const PS_HOSTED_COMPLETION: &str = r#"Register-ArgumentCompleter -CommandName {{NAMES}} -ParameterName RemainingArgs -ScriptBlock {
    param($commandName, $parameterName, $wordToComplete, $commandAst, $fakeBoundParameters)
    $w = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.Extent.Text })
    if ($wordToComplete) { $w = @($w | Select-Object -SkipLast 1) }
    $out = if ($w.Count -eq 0) { {{FIRST}} } elseif ($w.Count -eq 1 -and $w[0] -in {{NAMED}}) { & {{CCM}} {{LIST}} 2>$null | ForEach-Object { $p = $_ -split '  '; if ($p.Count -gt 1) { $p[0] } } }
    $out | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}"#;

/// **PowerShell 单引号字面量的唯一出口**：`'…'` 包裹，[`PS_QUOTES`] 里每个字符都双写。
/// 后端生产段凡是把一个值放进 PowerShell 单引号里的都只调它（判据 `shell_home_guard.rs` 零命中）。
///
/// 别名参数也每个都走它、不像 POSIX 那样「安全字符裸放」：PS 5.1 对裸词有 `--%`（停止解析）与 `--x=a.b` 在 `.` 处被劈开两个坑。
pub fn ps_literal(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if PS_QUOTES.contains(&c) {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

impl PowerShell {
    /// 别名里叫 ccm 的那个词：`"$HOME\<落点>.exe"`（双引号：`$HOME` 在那台上展开、展开出来的值不再解析；
    /// 别名块不往会话 `PATH` 里加东西，新机器上 `PATH` 里没有 ccm）。
    fn invoked(call: Call) -> String {
        format!("\"$HOME\\{}.exe\"", call.landing.replace('/', "\\"))
    }
}

impl ShellDialect for PowerShell {
    fn encode_for_disk(&self, content: &str) -> String {
        format!("{UTF8_BOM}{content}")
    }

    /// `$PROFILE` 的四种取值里、这台机器上有意义的那几份：PS 5.1（Windows 自带）两份恒列；
    /// PS 7 两份只在 `Documents/PowerShell` 这个目录在时列（说明装过且至少跑过一次）。
    /// **文件不在也列**：`$PROFILE` 通常要装的时候才建。
    ///
    /// 🔴 **全仓只有这里答「`$PROFILE` 在哪」**：另立一份认法就是同一个事实两种说法。
    /// 判据 `dialect_tests.rs::the_profile_location_has_exactly_one_home` 数着这几个文件名 / 目录名只在这里出现。
    ///
    /// 「文档」目录优先问系统（OneDrive 会把它挪走），问到的不在这个 home 底下时退回 `home/Documents`
    /// （判据拿临时目录当 home，结构上碰不到真实家目录）。
    ///
    /// PS 7 那两份「目录在才列」交给调用方问那台后端（[`Listed::IfDirExists`]），这里不 `is_dir()`。
    /// 「文档目录在哪」是这台后端问自己的系统（`platform::paths::documents_dir`）—— 这一臂只在说 PowerShell 的
    /// 那台上走得到（不在 Windows 的后端在命令口显式拒，[`super::dialect_here`]），路径按这台的写法拼（`Path::join`）。
    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate> {
        let home = Path::new(home);
        let docs = crate::platform::paths::documents_dir()
            .filter(|d| d.starts_with(home))
            .unwrap_or_else(|| home.join("Documents"));
        let mut out = Vec::new();
        for (dir, always, host) in [
            ("WindowsPowerShell", true, super::PsHost::Desktop),
            ("PowerShell", false, super::PsHost::Core),
        ] {
            let d = docs.join(dir);
            let listed = if always {
                Listed::Always
            } else {
                Listed::IfDirExists(d.display().to_string())
            };
            for f in ["Microsoft.PowerShell_profile.ps1", "profile.ps1"] {
                out.push(StartupCandidate {
                    path: d.join(f).display().to_string(),
                    listed: listed.clone(),
                    host: Some(host),
                });
            }
        }
        out
    }

    fn our_alias_file_rel(&self) -> &'static str {
        PS_ALIAS_FILE_REL
    }

    /// 两种分隔符都认、大小写不敏感（Windows 路径）。
    fn sources_our_file(&self, text: &str) -> bool {
        let t = text.to_ascii_lowercase();
        t.contains(PS_ALIAS_FILE_REL) || t.contains(&PS_ALIAS_FILE_REL.replace('/', "\\"))
    }

    fn file_header(&self) -> String {
        copy_text("rsShellDialect.ps.header", &[])
    }

    /// `& '<路径>.exe' '@名'`（`@` 打头在 PowerShell 里是展开，要引）。
    fn render_start(&self, program: &str, word: &str) -> String {
        format!(
            "& {} {}",
            ps_literal(&format!("{program}.exe")),
            ps_literal(word)
        )
    }

    /// `function 名字 { param…; <有守卫的 __ccm_bind>; & "$HOME\…\ccm.exe" <预置> $RemainingArgs['--' <ccm 选项>] }`（交给 ccm 时 `$RemainingArgs` 在最后；
    /// 叫法见 [`PowerShell::invoked`]）。
    fn render_alias(&self, c: Call, name: &str, argv: &[String], rest: RestTo) -> String {
        let (left, right) = split_last_end(c.end, argv);
        let mut call = format!("    & {}", Self::invoked(c));
        for w in left {
            call.push(' ');
            call.push_str(&ps_literal(w));
        }
        if rest == RestTo::Agent {
            call.push_str(PS_TAIL);
        }
        if let Some(right) = right {
            call.push_str(PS_END);
            for w in right {
                call.push(' ');
                call.push_str(&ps_literal(w));
            }
        }
        if rest == RestTo::Ccm {
            call.push_str(PS_TAIL);
        }
        [
            format!("function {name} {PS_BLOCK_OPEN}"),
            PS_PARAM_OPEN.to_string(),
            PS_PARAM_ARG.to_string(),
            PS_PARAM_CLOSE.to_string(),
            PS_BIND.to_string(),
            call,
            PS_BLOCK_CLOSE.to_string(),
        ]
        .join("\n")
    }

    /// [`PS_HOSTED_COMPLETION`] 填上词：每个词走 [`ps_literal`]（`--` 也得引：裸写会被 PowerShell 吃掉），ccm 照别名那样按落点叫。
    fn render_hosted_completion(&self, c: Call, names: &[String], w: HostedWords) -> String {
        let list = |ws: &mut dyn Iterator<Item = &str>, sep: &str| {
            ws.map(ps_literal).collect::<Vec<_>>().join(sep)
        };
        PS_HOSTED_COMPLETION
            .replace(
                "{{NAMES}}",
                &list(&mut names.iter().map(String::as_str), ", "),
            )
            .replace("{{FIRST}}", &list(&mut w.first.iter().copied(), ", "))
            .replace("{{NAMED}}", &list(&mut w.takes_name.iter().copied(), ", "))
            .replace("{{CCM}}", &Self::invoked(c))
            .replace("{{LIST}}", &list(&mut w.list.iter().copied(), " "))
    }

    fn name_is_valid(&self, name: &str) -> bool {
        portable_name(name)
    }

    /// `function 名字` / `function<TAB>名字` ⇒ 名字（`[A-Za-z0-9_-]` 那一段，可能为空）。原 `block.rs::find_conflicting_functions` 的 PowerShell 那一臂。
    fn declared_function(&self, l: &str) -> Option<String> {
        let rest = l
            .strip_prefix("function ")
            .or_else(|| l.strip_prefix("function\t"))?;
        let rest = rest.trim_start();
        // 函数名到第一个非 [A-Za-z0-9_-] 字符为止
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }

    /// PowerShell 的函数名大小写不敏感：`Workcc` 与 `workcc` 是同一个函数，后定义的盖掉先定义的。
    fn same_name(&self, a: &str, b: &str) -> bool {
        a.eq_ignore_ascii_case(b)
    }

    /// ① 终端集成块（`src/shared/cc.ps1.tpl`）里定义的函数（`__ccm_bind` · `ssh`）——
    ///    问的是那份模板本身（`profile_installer::render_cc_code` 渲染出来的那一份），不抄名单；
    /// ② `PATH` 上的同名程序（按 PowerShell 认的那几种扩展名）。函数的优先级高于外部程序 ⇒ 你这条会赢。
    ///
    /// ③ 〔WIN2 #4 读数〕PowerShell 的**内建别名**（`ls` / `cd` / `cat` …）优先级**高于**函数 ——
    /// 撞上它们的别名定义了也敲不到。问这台：起一次 PowerShell 跑 `Get-Alias`、进程内缓存
    /// （[`ps_builtin_aliases`]），不编一份清单；问不到就说问不到。
    fn name_taken(&self, name: &str, own_block: &str) -> Option<String> {
        // 模板里定义了哪几个函数：通用层交进来渲染好的那一份（它喂一个占位数据目录 —— 与名字无关）。
        let ours = own_block.lines().any(|l| {
            l.trim_start()
                .strip_prefix("function ")
                .and_then(|r| r.split_whitespace().next())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
        if ours {
            return Some(copy_text(
                "rsShellDialect.ps.nameTakenIntegration",
                &[("name", name)],
            ));
        }
        // 这台不说 PowerShell ⇒ 没有内建别名可撞（这一臂在命令口已被 `dialect_here` 拒，判据直调方言时走到这里）。
        if crate::platform::shell::speaks_powershell() {
            if let Some(note) = builtin_alias_note(name, ps_builtin_aliases()) {
                return Some(note);
            }
        }
        on_path(name, true).map(|cand| {
            copy_text(
                "rsShellDialect.ps.nameTakenPath",
                &[("name", name), ("cand", &(cand.display()).to_string())],
            )
        })
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/shell/dialect_tests.rs"]
mod tests;
