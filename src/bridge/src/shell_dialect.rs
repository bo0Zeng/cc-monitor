//! 〔AL1c · 第四波 4B · 2026-09-24〕**shell 方言：`设计/71 §4.4` 那组平台接口，POSIX 与 PowerShell 各一份实现。**
//!
//! # 判准（`71 §4.1` · 条 33）
//!
//! ```text
//! 「这个 shell 怎么写 / 文件落在哪 / 名字怎么认」 → 这里（翻译官）
//! 「一条别名合不合格 / 有一条不合格整批不写 / 围栏怎么拼 / 写的序列」 → 通用层（account_aliases · fenced_block · 后端 files-put）
//! ```
//!
//! 通用层持有的是**结构**（名字 ＋ 一组 ccm 参数），不持有任何一种 shell 的文本 —— 一旦持有一段 POSIX 脚本，
//! 这里就翻译不了它（`71 §4.2`，`src/backend/platform/shell.rs` 头注那条「交给 PowerShell 不是另一种写法，是另一种语言」）。
//! ⇒ 本模块里**零规则**：V1–V5、控制字符、重名、能力闸都不在这里；这里只答「这个 shell 里怎么写 / 怎么读」。
//!
//! # 方言由**目标**决定，不由宿主平台决定（`71 §4.4` 末段）
//!
//! [`Shell::of_target`] 按目标文件的扩展名判（`.ps1` ⇒ PowerShell），刻意不看 `cfg!(windows)`：
//! Windows 上的 Git Bash 读的是 `~/.bashrc`（POSIX）；门禁跑在 Linux 上，按 `cfg` 分的话两条臂里恒有一条没人验。
//!
//! # `71 §4.4` 七问里不在这里的两问
//!
//! - `atomic_replace()`：〔RW1〕写的序列（CAS · 备份 · 暂存旁名换名上位 · 回读 · 回滚）只住后端
//!   `control/files_write.rs::put_text`，Windows 那一支是就地覆盖保 ACE。方言只多答一格「落盘前要不要加 BOM」
//!   （[`ShellDialect::encode_for_disk`]）。
//! - `has_tmux()`：`71 §4.4` 逐字「不进这一族 —— 它是一项能力」⇒ 住通用层 `account_aliases::Caps`。
//!
//! # 🔴 PowerShell 那一臂的诚实边界
//!
//! 本机没有 `pwsh` / `powershell`，Win11 虚拟机不许碰（`99 §2 ⑤` 未拍）⇒ PowerShell 文本在这里**一次都没被
//! PowerShell 解析过**。它买到的只有：函数体逐字照 `scripts/cc.ps1.tpl` 里那个 `cc` 的形状（`K-R132` 真机上
//! 那一形 `parse-errors=0`）· 黄金串 · 与 POSIX 臂同契约的对拍（同一份清单两边渲染再各自读回，得回同一份清单）。

use crate::copy_table::copy_text;
use std::path::{Path, PathBuf};

/// 「这是哪种 shell 的方言」—— **唯一一个**回答这一问的枚举。
///
/// 〔AL1c〕它取代了 `profile_installer` 里那个只给别名块用的方言枚举（那一族的旧名见本仓 `git log`）：
/// 别名文件、别名块、source 那一行，三件事问的是同一个问题，不许有两个枚举各答一半。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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

/// 解析回来的一条：`(名字, 参数)`，或者一行认不出的原文 ＋ 原因（**不静默丢**）。
pub type Parsed = Result<(String, Vec<String>), String>;

/// 〔AL2 · 第四波 4D〕一份启动文件候选：**路径 ＋ 盘上不在时列不列**（「列不列不存在的」是方言的读法，`71 §4.4` 表第一行）。
///
/// 方言**只给路径与列法**，一个字节的盘都不读：在不在、里面是什么由**那台机器的后端**答
/// （`account_aliases::rc_candidates_via` 经 `user_files::Door` 问 `files-peek` / `files-stat`）——
/// 从前这里自己 `is_file()` / `is_dir()`，量的是 monitor 这台的盘，拿去说远端是错的（`第四波记录/W5-ALIAS.md §2.2`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupCandidate {
    /// 那台机器上的绝对路径（按那台 home 的写法拼，见 `user_files::join_under`）。
    pub path: String,
    pub listed: Listed,
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

/// 🔴 **`71 §4.4` 那组接口。** 每个方法都只回答「这个 shell 里怎么写 / 怎么读 / 文件在哪」；
/// 任何「合不合格」的判断都不许写进实现里（那是通用层的，两边一模一样）。
pub trait ShellDialect: Sync {
    // 〔TL1 · 4C〕墓碑：这里从前有一格「围栏块的排版」—— 唯一的读者是代装 rc 那一行的那一跳（退役，`71 §6.1`）。
    //   别名块那一侧的排版按目标文件扩展名走 `profile_installer` 那一份，不经这里。

    /// 落盘前的编码。PowerShell 加 BOM（`K-R132`：PS 5.1 把无 BOM 的 `.ps1` 按 ANSI 代码页解，
    /// 中文注释会吞掉下一行）；POSIX 一个字节都不加（`.bashrc` 开头多三个字节 ⇒ `sh` 把它当命令）。
    fn encode_for_disk(&self, content: &str) -> String;

    /// 读进来的那一份在交给任何**判内容**的东西之前要剥掉什么（[`Self::encode_for_disk`] 的逆）。
    /// PowerShell 剥 BOM；POSIX 原样（用户 rc 开头那三个字节不是我们的，写回去也原样留着）。
    fn decode_from_disk<'a>(&self, raw: &'a str) -> &'a str;

    /// shell 启动时会执行的那几份文件（界面「别名块装进哪份」的候选），按优先级。
    /// 「列不列一份还不存在的文件」是读法，由实现答（POSIX 只列在的 · PowerShell 的 `$PROFILE` 常常要装时才建）。
    /// 〔AL2〕`home` 是**那台机器**的 home（后端 `files-home` 答的字符串）；这里一个字节的盘都不读（[`StartupCandidate`]）。
    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate>;

    // 〔TL1 · 4C〕墓碑：这里从前有一格「用户选的那份启动文件还不在时，装 source 那一行要不要新建它」——
    //   代装那一行的那一跳退役了（`71 §6.1`：source 那一行只住别名块里），这一格零调用方 ⇒ 删。
    //   （别名块自己那一侧建不建 `$PROFILE`，归 `profile_installer` 那一份规则。）

    /// 我们自己那份别名文件在 home 下的相对路径（`/` 分隔，交给后端的 `rel` 就是它）。
    fn our_alias_file_rel(&self) -> &'static str;

    /// 「执行我们那份文件」在这个 shell 里怎么写（**一行**）。文件不在时必须是空操作。
    /// 〔TL1 · 4C〕今天只给人看（选的那份启动文件没接上时，报告里给出这一行、由人自己决定贴不贴）；自动接上那一行住别名块里。
    fn source_line(&self, our_file: &str) -> String;

    /// 这份启动文件是不是已经接上了我们那份（任何一种写法都认，别按整行比 —— 见 POSIX 那一臂的注释）。
    fn sources_our_file(&self, text: &str) -> bool;

    /// 我们那份别名文件的头注（整段注释行，含结尾换行）。
    fn file_header(&self) -> String;

    /// 一条（通用层判过合格的）别名在这个 shell 里的写法：名字 ＋ 预置参数 ＋ 把调用时的参数原样接在后面。
    fn render_alias(&self, name: &str, argv: &[String]) -> String;

    /// 把我们那份文件的正文（BOM 已剥）读回成一条条。注释 / 空行跳过；认不出的原文带原因。
    fn parse_file(&self, text: &str) -> Vec<Parsed>;

    /// 名字的合法字符集。
    fn name_is_valid(&self, name: &str) -> bool;

    /// 两个名字在这个 shell 里是不是同一个函数（PowerShell 大小写不敏感）。
    fn same_name(&self, a: &str, b: &str) -> bool;

    /// 这个名字是不是已经被占了（**只出声、不拦**）。报出来的话里带住址。
    ///
    /// 〔AL2〕`look_on_path`：要不要查 `PATH` 上的同名程序。查的是 **monitor 这个进程**的 `PATH` ⇒ 只有那台机器就是
    /// monitor 所在这台时才说得了（调用方按 `origin` 给）；远端给 `false`，只查自带别名块（`第四波记录/W5-ALIAS.md §2.2`）。
    fn name_taken(&self, name: &str, look_on_path: bool) -> Option<String>;

    /// 一个参数能不能**原样**到达 `ccm`（传参那一跳这个 shell 会不会改坏它）。
    fn arg_is_passable(&self, word: &str) -> Result<(), String>;
}

/// UTF-8 BOM。**闭集只有这一处住址**〔`13b`〕。
const UTF8_BOM: &str = "\u{feff}";

/// 读进来的那一份：把 BOM 剥掉再交给任何**判内容**的东西（两种方言都剥 —— 读的一侧宽，写的一侧严）。
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix(UTF8_BOM).unwrap_or(s)
}

/// `PATH` 上有没有一个叫这个名字的程序（带上这些扩展名之一；空串 = 不带扩展名）。
///
/// ⚠ **诚实边界**：查的是 monitor 这个进程的 `PATH`，不是用户登录 shell 的 `PATH` ⇒ 会漏报，不会误报成「有」。
fn on_path(name: &str, exts: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let cand = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}.{ext}"))
            };
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
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

/// POSIX 函数声明的头与尾（写与读回共用这一对 —— 渲染与解析不许各写一份字面量）。
/// ⚠ 住常量还有一个理由：`structural_scan` 那把尺子按大括号配平切函数体，字面量里落单的 `{` 会让它切过界。
const POSIX_FN_HEAD: &str = "() { ";
/// 〔V151〕`"$@"`（用户敲别名时跟的参数）落在 `--` 左边 —— 交给 claude；别名自己的 ccm 选项在它后面的 `--` 右边。
const POSIX_ARGS: &str = " \"$@\"";
const POSIX_FN_TAIL: &str = "; }";

/// POSIX 那份别名文件在 home 下的相对路径。〔RW1〕从 `account-aliases.sh` 改名而来，旧名不读不写不删。
const POSIX_ALIAS_FILE_REL: &str = ".cc-monitor/aliases.sh";

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

    /// 把中段按 POSIX 单引号规则切成词，顺便证明引号是配平的。
    ///
    /// ⚠ 反斜杠**只**在 `'\''`（关引号 + 转义的引号 + 开引号）这一形里放行 ——
    /// 它是 `shell_quote_core::posix_quote` 唯一会产出的反斜杠；别处出现一律拒。
    fn split_words(mid: &str) -> Result<Vec<String>, String> {
        let mut words: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut started = false;
        let mut in_q = false;
        let mut it = mid.chars();
        while let Some(c) = it.next() {
            match c {
                '\'' => {
                    in_q = !in_q;
                    started = true;
                }
                ' ' if !in_q => {
                    if started {
                        words.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                '\\' if !in_q => {
                    // 只认 `'\''`：此刻引号刚被上一个 `'` 关掉，后面必须是 `'` 再 `'`。
                    if it.next() != Some('\'') || it.next() != Some('\'') {
                        return Err(copy_text("rsShellDialect.posix.backslash", &[]));
                    }
                    cur.push('\'');
                    in_q = true;
                    started = true;
                }
                _ => {
                    cur.push(c);
                    started = true;
                }
            }
        }
        if in_q {
            return Err(copy_text("rsShellDialect.quote.unbalanced", &[]));
        }
        if started {
            words.push(cur);
        }
        Ok(words)
    }

    /// 生成文件里的一行 → 一条。认两种调用词：裸 `ccm`，以及从前那种 `"${CCM:-<路径>}"`
    /// （`K-R69` 的 `ccmInvocation`〔散文墓碑〕吐过，那一格随 TS 生成器退役）—— 读回之后一律按裸 `ccm` 重写。
    fn parse_line(line: &str) -> Parsed {
        let rest = line
            .strip_suffix(POSIX_FN_TAIL)
            .ok_or(&copy_text("rsShellDialect.posix.badTail", &[]))?;
        let (name, body) = rest
            .split_once(POSIX_FN_HEAD)
            .ok_or(&copy_text("rsShellDialect.posix.badShape", &[]))?;
        // 〔V151〕`ccm <左…> "$@"[ -- <右…>]`：`"$@"` 是分界（最后一处；用户的值经 [`Self::word`] 单引号，不会长成它）。
        let at = body
            .rfind(POSIX_ARGS)
            .ok_or(&copy_text("rsShellDialect.posix.badTail", &[]))?;
        let (lead, tail) = (&body[..at], &body[at + POSIX_ARGS.len()..]);
        let right: Option<Vec<String>> = match tail.strip_prefix(" --") {
            None if tail.is_empty() => None,
            None => return Err(copy_text("rsShellDialect.posix.badShape", &[])),
            Some(r) => Some(Self::split_words(r.trim_start())?),
        };
        let mut words = Self::split_words(lead)?.into_iter();
        let head = words.next().unwrap_or_default();
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        if head != word && !head.starts_with("\"${CCM:-") {
            return Err(copy_text(
                "rsShellDialect.posix.notCcm",
                &[("word", &word.to_string())],
            ));
        }
        Ok((name.to_string(), join_last_end(words.collect(), right)))
    }
}

/// 〔V151〕别名那条 argv 按最后一个 `--` 切成两半（没有 ⇒ 右边 `None`）。两种方言渲染共用。
fn split_last_end(argv: &[String]) -> (&[String], Option<&[String]>) {
    match argv.iter().rposition(|w| w == "--") {
        Some(k) => (&argv[..k], Some(&argv[k + 1..])),
        None => (argv, None),
    }
}

/// [`split_last_end`] 的逆：两种方言读回共用。
fn join_last_end(mut left: Vec<String>, right: Option<Vec<String>>) -> Vec<String> {
    if let Some(r) = right {
        left.push("--".into());
        left.extend(r);
    }
    left
}

impl ShellDialect for Posix {
    fn encode_for_disk(&self, content: &str) -> String {
        content.to_string()
    }

    fn decode_from_disk<'a>(&self, raw: &'a str) -> &'a str {
        raw
    }

    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate> {
        POSIX_RC_CANDIDATES
            .iter()
            .map(|n| StartupCandidate {
                path: crate::user_files::join_under(home, n),
                listed: Listed::IfFileExists,
            })
            .collect()
    }

    fn our_alias_file_rel(&self) -> &'static str {
        POSIX_ALIAS_FILE_REL
    }

    /// `[ -r … ]` 那道是承重的：文件还没生成 / 被用户删掉时它是个 no-op，
    /// 而不是让用户每开一个终端就看见一行 `No such file or directory`。
    ///
    /// ⚠ 写成 `if … then … fi` 而不是 `[ -r … ] && . …`，理由是**退出码**：
    /// 后者在文件不存在时整行返回 1，而这一行在 `src/shared/ccm-aliases.sh` 里是**最后一行**
    /// ⇒ `source` 那份片段会以非零收场。`if` 那一形恒返回 0。
    fn source_line(&self, our_file: &str) -> String {
        format!("if [ -r \"{our_file}\" ]; then . \"{our_file}\"; fi")
    }

    /// 🔴 认的是**相对路径**，不是整行：`src/shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`
    /// （**没展开**），而 [`Self::source_line`] 带的是展开后的绝对路径 ⇒ 按整行比，
    /// 一个已经装了 ccm 别名块的人会被判成「还没 source 过」，于是又被追加一行（重复追加那条病）。
    fn sources_our_file(&self, text: &str) -> bool {
        text.contains(POSIX_ALIAS_FILE_REL)
    }

    fn file_header(&self) -> String {
        copy_text("rsShellDialect.posix.header", &[])
    }

    /// `名字() { ccm <参数…> "$@"; }`。`"$@"` 必须在最后 —— 那就是「参数附加器」的全部含义：
    /// 调用时再给的参数接在后面、后者胜。
    fn render_alias(&self, name: &str, argv: &[String]) -> String {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        let (left, right) = split_last_end(argv);
        let mut out = format!("{name}{POSIX_FN_HEAD}{word}");
        for w in left {
            out.push(' ');
            out.push_str(&Self::word(w));
        }
        out.push_str(POSIX_ARGS);
        if let Some(right) = right {
            out.push_str(" --");
            for w in right {
                out.push(' ');
                out.push_str(&Self::word(w));
            }
        }
        out.push_str(POSIX_FN_TAIL);
        out
    }

    fn parse_file(&self, text: &str) -> Vec<Parsed> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| Self::parse_line(l).map_err(|why| format!("{l}（{why}）")))
            .collect()
    }

    fn name_is_valid(&self, name: &str) -> bool {
        portable_name(name)
    }

    fn same_name(&self, a: &str, b: &str) -> bool {
        a == b
    }

    /// 两条路各查一次，报出来的话里带住址，用户才知道自己在盖掉什么：
    /// ① `src/shared/ccm-aliases.sh` 里自带的那几个（今天是 `cc` / `cct`）——**问的是那份文件本身**
    ///    （`profile_installer::builtin_alias_names`），不在这里抄一份名字清单；
    /// ② `PATH` 上真有一个同名程序 —— 🔴 `cc` 在多数机器上是 C 编译器（`/usr/bin/cc`），
    ///    而自带那份别名只检查「有没有同名**函数**」、不检查程序。
    fn name_taken(&self, name: &str, look_on_path: bool) -> Option<String> {
        if crate::profile_installer::builtin_alias_names().contains(&name) {
            return Some(copy_text(
                "rsShellDialect.posix.nameTakenBuiltin",
                &[("name", &name.to_string())],
            ));
        }
        if !look_on_path {
            return None;
        }
        on_path(name, &[""]).map(|cand| {
            copy_text(
                "rsShellDialect.posix.nameTakenPath",
                &[
                    ("name", &name.to_string()),
                    ("cand", &(cand.display()).to_string()),
                ],
            )
        })
    }

    /// 单引号能表达一切（控制字符由通用规则拒）⇒ 恒过。
    fn arg_is_passable(&self, _word: &str) -> Result<(), String> {
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PowerShell
// ═══════════════════════════════════════════════════════════════════════════

/// PowerShell 那一份实现（5.1 与 7 同一种写法）。
pub struct PowerShell;

/// PowerShell 那份别名文件在 home 下的相对路径（交给后端的 `rel` 用 `/`；PowerShell 两种分隔符都认）。
const PS_ALIAS_FILE_REL: &str = ".cc-monitor/aliases.ps1";

/// PowerShell 单引号串里算「引号」的那几个字符：ASCII `'` 之外，PowerShell 还把 `‘ ’ ‚ ‛` 当成同一个引号
/// （它的词法器逐字如此）⇒ 值里出现任何一个都得双写，否则串在那里就断了。
const PS_QUOTES: &[char] = &['\'', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];

/// 函数体里那几行固定的（与 `scripts/cc.ps1.tpl` 里的 `cc` 逐字同形：那一形在 `K-R132` 真机上 `parse-errors=0`）。
const PS_PARAM_OPEN: &str = "    [CmdletBinding()] param(";
const PS_PARAM_ARG: &str =
    "        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs";
const PS_PARAM_CLOSE: &str = "    )";
/// 〔`71 §4.6 ②`〕`__ccm_bind` 是**拉前的握手**，留适配层 —— 通用层一个字不知道它。
/// 带守卫：终端集成块没装时它不存在，这一行就是空操作（与 POSIX 那一侧「没有握手」同效）。
const PS_BIND: &str =
    "    if (Get-Command __ccm_bind -CommandType Function -ErrorAction SilentlyContinue) { __ccm_bind }";
/// 函数块的开与收。住常量而不是函数体里的字面量：`structural_scan` 那把「会剥注释的函数」尺子按大括号配平切函数体，
/// 字面量里落单的一个 `{` 会让它切不准、一路吃进下一个函数（`parse_file`）而误报。
const PS_BLOCK_OPEN: &str = "{";
const PS_BLOCK_CLOSE: &str = "}";
/// 最后一行的尾巴：调用时再给的参数原样接在后面（与 `cc` 同一写法：数组变量交给原生程序时逐个展开）。
const PS_TAIL: &str = " $RemainingArgs";
/// 〔V151〕分隔 claude / ccm 两半的 `--`：写成单引号字面量（裸 `--` 是 PowerShell 自己的「参数到此为止」记号，会被它吃掉）。
const PS_END: &str = " '--'";

impl PowerShell {
    /// 一个参数：**每个都单引号**（引号字符双写）。
    ///
    /// 为什么不像 POSIX 那样「安全字符裸放」：PS 5.1 对裸词有两个已知坑 —— `--%` 是停止解析记号；
    /// `--x=a.b` 这一形会在 `.` 处被劈成两个参数 —— 而 W1（真 Windows）买不到 ⇒ 取最保守的一形。
    fn word(w: &str) -> String {
        let mut out = String::from("'");
        for c in w.chars() {
            if PS_QUOTES.contains(&c) {
                out.push(c);
            }
            out.push(c);
        }
        out.push('\'');
        out
    }

    /// `'a' 'b''c'` → `["a", "b'c"]`。只认 [`Self::word`] 产出的那一形（每个词都是一对单引号）。
    fn split_words(s: &str) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        let mut it = s.chars().peekable();
        loop {
            while it.peek() == Some(&' ') {
                it.next();
            }
            let Some(open) = it.next() else { break };
            if !PS_QUOTES.contains(&open) {
                return Err(copy_text("rsShellDialect.ps.notQuoted", &[]));
            }
            let mut cur = String::new();
            loop {
                match it.next() {
                    None => return Err(copy_text("rsShellDialect.quote.unbalanced", &[])),
                    Some(c) if PS_QUOTES.contains(&c) => {
                        if it.peek() == Some(&c) {
                            it.next();
                            cur.push(c);
                        } else {
                            break;
                        }
                    }
                    Some(c) => cur.push(c),
                }
            }
            if !matches!(it.peek(), None | Some(' ')) {
                return Err(copy_text("rsShellDialect.ps.missingSpace", &[]));
            }
            out.push(cur);
        }
        Ok(out)
    }

    /// `& ccm '…' $RemainingArgs['--' '…']` → 参数（〔V151〕`$RemainingArgs` 是分界：左边交 claude，右边 `'--'` 之后归 ccm）。
    fn parse_call(line: &str) -> Result<Vec<String>, String> {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        let head = format!("    & {word}");
        let bad = || copy_text("rsShellDialect.ps.badCall", &[("word", &word.to_string())]);
        let body = line.strip_prefix(&head).ok_or_else(bad)?;
        let at = body.rfind(PS_TAIL).ok_or_else(bad)?;
        let (lead, tail) = (&body[..at], &body[at + PS_TAIL.len()..]);
        let right = match tail.strip_prefix(PS_END) {
            None if tail.is_empty() => None,
            None => return Err(bad()),
            Some(r) => Some(Self::split_words(r)?),
        };
        Ok(join_last_end(Self::split_words(lead)?, right))
    }
}

impl ShellDialect for PowerShell {
    fn encode_for_disk(&self, content: &str) -> String {
        format!("{UTF8_BOM}{content}")
    }

    fn decode_from_disk<'a>(&self, raw: &'a str) -> &'a str {
        strip_bom(raw)
    }

    /// `$PROFILE` 的四种取值里、这台机器上有意义的那几份：PS 5.1（Windows 自带）两份恒列；
    /// PS 7 两份只在 `Documents/PowerShell` 这个目录在时列（说明装过且至少跑过一次）。
    /// **文件不在也列**：`$PROFILE` 通常要装的时候才建。
    ///
    /// 🔴 〔AL1d · 第四波 4B〕**全仓只有这里答「`$PROFILE` 在哪」**（`调研/第四波记录/AL1d.md §2.3`）：
    /// 从前另有四处认法（终端集成的两份发现表、TS 自己换文件名推 AllHosts、数据页探备份目录那张表），
    /// 其中一份还把 `profile.ps1` 判成「装错了的遗留」而这里把它列成合法候选 —— 同一个事实三种说法。
    /// 判据 `shell_dialect_tests.rs::the_profile_location_has_exactly_one_home` 数着这几个文件名 / 目录名只在这里出现。
    ///
    /// 「文档」目录优先问系统（OneDrive 会把它挪走），问到的不在这个 home 底下时退回 `home/Documents`
    /// （判据拿临时目录当 home，结构上碰不到真实家目录）。
    ///
    /// 〔AL2 · 第四波 4D〕PS 7 那两份「目录在才列」从前在这里 `is_dir()`，今天交给调用方问那台后端（[`Listed::IfDirExists`]）。
    /// ⚠ **诚实边界**：「文档目录在哪」仍是 monitor 这个进程问系统（`dirs::document_dir()`），不是问后端 ——
    /// 这一臂今天只有本机（远端 × PowerShell 在命令口显式拒，`account_aliases::dialect_promised` · `01 §6.7b` 表 B），
    /// 本机 = monitor 所在那台；路径按本机的写法拼（`Path::join`）也是这个缘故。
    fn startup_candidates(&self, home: &str) -> Vec<StartupCandidate> {
        let home = Path::new(home);
        let docs = dirs::document_dir()
            .filter(|d| d.starts_with(home))
            .unwrap_or_else(|| home.join("Documents"));
        let mut out = Vec::new();
        for (dir, always) in [("WindowsPowerShell", true), ("PowerShell", false)] {
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
                });
            }
        }
        out
    }

    fn our_alias_file_rel(&self) -> &'static str {
        PS_ALIAS_FILE_REL
    }

    /// `if (Test-Path -LiteralPath '…') { . '…' }` —— `-LiteralPath` 让路径里的 `[` `]` 不被当通配符；
    /// 文件不在时整行什么都不做。
    fn source_line(&self, our_file: &str) -> String {
        let p = Self::word(our_file);
        format!("if (Test-Path -LiteralPath {p}) {{ . {p} }}")
    }

    /// 两种分隔符都认、大小写不敏感（Windows 路径）。
    fn sources_our_file(&self, text: &str) -> bool {
        let t = text.to_ascii_lowercase();
        t.contains(PS_ALIAS_FILE_REL) || t.contains(&PS_ALIAS_FILE_REL.replace('/', "\\"))
    }

    fn file_header(&self) -> String {
        copy_text("rsShellDialect.ps.header", &[])
    }

    /// 与 `scripts/cc.ps1.tpl` 里的 `function cc` 逐字同形（只多了预置参数，握手那一行带守卫）。
    fn render_alias(&self, name: &str, argv: &[String]) -> String {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        let (left, right) = split_last_end(argv);
        let mut call = format!("    & {word}");
        for w in left {
            call.push(' ');
            call.push_str(&Self::word(w));
        }
        call.push_str(PS_TAIL);
        if let Some(right) = right {
            call.push_str(PS_END);
            for w in right {
                call.push(' ');
                call.push_str(&Self::word(w));
            }
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

    /// 按块认：`function 名字 {` 起、`}` 止；块里只认 [`Self::render_alias`] 产出的那一形 ——
    /// 名字与参数取出来之后**原样再渲染一遍逐字比**，差一个字节就整块算认不出（不猜用户手改了什么）。
    fn parse_file(&self, text: &str) -> Vec<Parsed> {
        let mut out = Vec::new();
        let mut lines = text.lines();
        while let Some(raw) = lines.next() {
            let l = raw.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let Some(name) = l
                .strip_prefix("function ")
                .and_then(|r| r.strip_suffix(" {"))
            else {
                out.push(Err(copy_text(
                    "rsShellDialect.ps.outsideFn",
                    &[("line", &l.to_string())],
                )));
                continue;
            };
            let mut body: Vec<&str> = Vec::new();
            let mut closed = false;
            for b in lines.by_ref() {
                if b.trim_end() == "}" {
                    closed = true;
                    break;
                }
                body.push(b.trim_end());
            }
            if !closed {
                out.push(Err(copy_text(
                    "rsShellDialect.ps.noClose",
                    &[("line", &l.to_string())],
                )));
                continue;
            }
            let got = body
                .iter()
                .find(|b| b.trim_start().starts_with('&'))
                .ok_or_else(|| copy_text("rsShellDialect.ps.noCcmCall", &[]))
                .and_then(|call| Self::parse_call(call));
            match got {
                Ok(argv) => {
                    let again = self.render_alias(name, &argv);
                    let seen = std::iter::once(raw.trim_end().to_string())
                        .chain(body.iter().map(|b| b.to_string()))
                        .chain(std::iter::once("}".to_string()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if again == seen {
                        out.push(Ok((name.to_string(), argv)));
                    } else {
                        out.push(Err(copy_text(
                            "rsShellDialect.ps.handEdited",
                            &[("name", &name.to_string())],
                        )));
                    }
                }
                Err(why) => out.push(Err(format!("function {name}（{why}）"))),
            }
        }
        out
    }

    fn name_is_valid(&self, name: &str) -> bool {
        portable_name(name)
    }

    /// PowerShell 的函数名大小写不敏感：`Zcc` 与 `zcc` 是同一个函数，后定义的盖掉先定义的。
    fn same_name(&self, a: &str, b: &str) -> bool {
        a.eq_ignore_ascii_case(b)
    }

    /// ① 终端集成块（`scripts/cc.ps1.tpl`）里定义的函数（`__ccm_bind` ＋ 装了 wrapper 时的 `cc`）——
    ///    问的是那份模板本身（`profile_installer::render_cc_code` 渲染出来的那一份），不抄名单；
    /// ② `PATH` 上的同名程序（按 PowerShell 认的那几种扩展名）。函数的优先级高于外部程序 ⇒ 你这条会赢。
    ///
    /// ⚠ **够不着的一格**：PowerShell 的**内建别名**（`ls` / `cd` / `cat` …）优先级**高于**函数 ——
    /// 撞上它们的别名定义了也敲不到。要问得真准得起一个 PowerShell 跑 `Get-Command`，本机没有（W1 那一族）⇒
    /// 如实不查，不编一份内建别名清单出来。
    fn name_taken(&self, name: &str, look_on_path: bool) -> Option<String> {
        // 只问模板里定义了哪几个函数 —— 与数据目录无关，喂一个占位目录。
        let block =
            crate::profile_installer::render_cc_code("cc", true, std::path::Path::new("/_"));
        let ours = block.lines().any(|l| {
            l.trim_start()
                .strip_prefix("function ")
                .and_then(|r| r.split_whitespace().next())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
        if ours {
            return Some(copy_text(
                "rsShellDialect.ps.nameTakenIntegration",
                &[("name", &name.to_string())],
            ));
        }
        if !look_on_path {
            return None;
        }
        on_path(name, &["exe", "cmd", "bat", "ps1", "com"]).map(|cand| {
            copy_text(
                "rsShellDialect.ps.nameTakenPath",
                &[
                    ("name", &name.to_string()),
                    ("cand", &(cand.display()).to_string()),
                ],
            )
        })
    }

    /// PS 5.1 把参数交给原生程序（`ccm.exe`）时**不转义**：值里的 `"` 会被吞 / 劈开；
    /// 含空白的值被它包一对 `"`，结尾的 `\` 会把那个 `"` 转义掉；空串整个被丢掉。
    /// ⚠ 这三条是**文档读数**（PS 7.3 之前的 legacy 传参），没在真机上验（W1 买不到）。
    fn arg_is_passable(&self, word: &str) -> Result<(), String> {
        if word.is_empty() {
            return Err(copy_text("rsShellDialect.ps.emptyArg", &[]));
        }
        if word.contains('"') {
            return Err(copy_text("rsShellDialect.ps.doubleQuote", &[]));
        }
        if word.chars().any(char::is_whitespace) && word.ends_with('\\') {
            return Err(copy_text("rsShellDialect.ps.trailingBackslash", &[]));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/shell_dialect_tests.rs"]
mod tests;
