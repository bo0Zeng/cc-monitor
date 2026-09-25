//! 别名：**名字 ＋ 一组 ccm 参数**（`设计/71`）。一类，没有「账号别名」这一种。
//!
//! 〔用 2026-09-17〕逐字：「**不要有 account alias 这种东西。alias 应该独立吗？应该就是 ccm
//! 参数附加器。即生成别名，都是调用 ccm，ccm 本身就可以指定账号。**」
//!
//! # 来历（`K-R49`）
//!
//! 本模块起于 09-10 那一句「**添加账号后添加对应命令, 像是 alphacc, betacc 这种……还得手动去改**」：
//! 那时它按**账号表**整份重写一份文件，TS 侧的 `buildAliasLine`〔散文墓碑〕拼行、这里只管落盘
//! ＋ 一道「形状围栏」挡注入。〔AL1 · 2026-09-24〕两处都翻了：清单归**用户**（不再是账号表的投影，
//! `71 §8`），shell 文本由**本模块**渲染（前端递的是结构，不是代码 —— 审计 S-1 那条
//! 「后端拥有那段文本」终于对别名也成立了），那道形状围栏随之退役。
//!
//! # 🔴 落点：**我们自己那份文件**，不是用户的 `~/.bashrc`（这一条没变）
//!
//! 往 rc 里追加的那条路有三条病：
//! 1. **重复追加**：没有任何东西负责去重；
//! 2. **删不掉**：删了一条，那一行还在；
//! 3. **弄坏的代价是「shell 起不来」** —— 而那正是用户用来救火的东西。
//!
//! ⇒ 落点是 [`alias_file_in`]（`~/.cc-monitor/aliases.sh`，**monitor 自己的目录**），
//! **整份重写**：幂等、删一条当场消失、删掉整份文件也只是少几个命令，shell 照常起得来。
//! 〔RW1 · 第四波 09-24〕文件名从 `account-aliases.sh` 改成 `aliases.sh`（`71 §9.2` 那道迁移题按主会话裁结案：
//! **不留兼容** —— 不留转发件、不读旧名）。已经装过的 rc 那一段，下一次点「安装」就被改写成指向新名字的那一行
//! （同一对围栏，整块替换）；旧文件留在盘上，本模块不读、不写、不删。
//!
//! # 用户的 shell 配置里最多只多**一行** `source`，而且多数人连这一行都不用加
//!
//! `src/shared/ccm-aliases.sh`（别名块）自带一行 `if [ -r … ]; then . …; fi` 指向这份文件 ⇒
//! 装了别名块的人什么都不用做。没装的人可以让 [`install_in`] 把那一行写进**他自己指定**的那份 rc
//! —— 路径由界面上的人选，本模块**不猜**（`.bashrc` / `.zshrc` / fish 写法不同）。
//! 〔RW1 · 第四波 09-24〕两处写（那份文件 ＋ rc 里那一行）**都不在本进程落盘**：经本机后端的文件管理
//! 那一面（`user_files::edit` → `files-peek` / `files-put`），备份 · 原子替换 · 回读 · 回滚那一份规则住后端。

use std::path::{Path, PathBuf};

/// 生成文件在 home 下的相对路径。**monitor 自己的目录**，不是用户环境的一部分。
pub const ALIAS_FILE_REL: &str = ".cc-monitor/aliases.sh";

/// 写进用户 rc 的那一行所在的围栏。**刻意与 `profile_installer` 的
/// `# === cc-monitor BEGIN` 不同前缀** —— 后者装的是 PowerShell 的 `cc` 块，
/// 两者若共用标记，装一个就会把另一个整块替换掉。
/// ⚠ `K-R62` 起是 `pub(crate)`：同 `profile_installer::BEGIN_MARKER` 那条理由 ——
/// `fenced_block::FENCE_SHAPES` 指它，不抄它。
pub(crate) const RC_BEGIN: &str = "# === cc-monitor aliases BEGIN v1 ===";
pub(crate) const RC_END: &str = "# === cc-monitor aliases END ===";

/// 生成文件自己的围栏（整份重写，所以它只是给人看的边界）。
/// 〔AL1 · 2026-09-24〕v1 → v2：`71` 逐字「不要有 account alias 这种东西」—— 文件里只有**一类**别名。
/// 读回（[`read_in`]）不认这两行（注释行一律跳过），所以盘上那份 v1 照样读得回来。
const FILE_BEGIN: &str = "# === cc-monitor aliases BEGIN v2 ===";
const FILE_END: &str = "# === cc-monitor aliases END ===";

/// 界面上那个「要不要把这一行 `source` 加进去」的候选。**只列真实存在的那几份。**
///
/// ⚠ fish 的 `config.fish` **不在这里**，那不是遗漏：生成文件是 POSIX sh 的函数写法，
/// fish 根本 `source` 不了它。少列一个候选好过让人点一下之后 shell 报一屏语法错。
const RC_CANDIDATES: &[&str] = &[".bashrc", ".zshrc", ".bash_profile", ".profile"];

/// 一份候选 rc 的状态。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AccountAliasRc {
    /// 绝对路径。
    pub path: String,
    /// 这份 rc 今天已经把生成文件 `source` 进去了吗（装过 ccm 别名块的人这一格就是 true）。
    pub sourced: bool,
}

/// 生成文件的绝对路径。`home` 由调用方给 —— 测试拿临时目录当 home，**绝不碰真实家目录**。
pub fn alias_file_in(home: &Path) -> PathBuf {
    home.join(ALIAS_FILE_REL)
}

/// 要加进 rc 的那**一行**。
///
/// `[ -r … ]` 那道是承重的：文件还没生成 / 被用户删掉时它是个 no-op，
/// 而不是让用户每开一个终端就看见一行 `No such file or directory`。
///
/// ⚠ 写成 `if … then … fi` 而不是 `[ -r … ] && . …`，理由是**退出码**：
/// 后者在文件不存在时整行返回 1，而这一行在 `src/shared/ccm-aliases.sh` 里是**最后一行**
/// ⇒ `source` 那份片段会以非零收场。多数 rc 里无害，但「无害」不是理由 ——
/// `if` 那一形恒返回 0，而它一个字都不难读。
pub fn source_line(alias_path: &Path) -> String {
    let p = alias_path.display();
    format!("if [ -r \"{p}\" ]; then . \"{p}\"; fi")
}

/// 把中段按 POSIX 单引号规则切成词，顺便证明引号是配平的。
///
/// ⚠ 反斜杠**只**在 `'\''`（关引号 + 转义的引号 + 开引号）这一形里放行 ——
/// 它是 `shell_quote_core::posix_quote` 唯一会产出的反斜杠（从前 TS 那个 `q()` 也是这一形）；别处出现一律拒。
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
                    return Err("反斜杠只允许出现在 `'\\''` 这一形里".to_string());
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
        return Err("单引号没有配平".to_string());
    }
    if started {
        words.push(cur);
    }
    Ok(words)
}

/// 整份生成文件的内容。**没有时间戳** —— 有了就永远比不出「内容没变」，每次都要写一遍。
pub fn render_file(lines: &[String]) -> String {
    let mut out = String::new();
    out.push_str(FILE_BEGIN);
    out.push('\n');
    out.push_str(
        "# 这份文件由 cc-monitor 设置里「别名」那一块整份重写，别手改 —— 下一次写入会原样覆盖。\n\
         # 每一行是一条别名：名字 ＋ 一组 ccm 参数，调用时再给的参数接在后面。\n\
         # 写法是 POSIX sh 函数，bash / zsh 都 source 得了；fish 不行。\n",
    );
    if lines.is_empty() {
        out.push_str("# （当前一条别名都没有）\n");
    }
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(FILE_END);
    out.push('\n');
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔AL1 · 2026-09-24〕`设计/71`：**别名只有一类 —— 名字 ＋ 一组 ccm 参数**
// ═══════════════════════════════════════════════════════════════════════════
//
// 用户 2026-09-17 逐字：「不要有 account alias 这种东西……就是 ccm 参数附加器」。
// 账号（`--account`）只是参数里的一个维度。清单由**用户**拥有，不跟着账号表自动增删（`71 §8`）。
//
// 命令面是两跳（`71 §12.6`）：
//   ① [`render`] —— **纯**：清单 → 代码（＋ 每条的问题 ＋ 撞名提示）。预览、复制都只调这一跳；
//   ② [`install_in`] —— **唯一的副作用**：同一份渲染落进 `~/.cc-monitor/aliases.sh`（经本机后端），
//      可选地往用户选的 rc 里装一行 `source`。它收的是**清单**不是代码 —— 写进 shell 的文本
//      只由本模块产出（审计 S-1：绝不让前端注入可执行的 shell），而「写的就是预览的那一份」
//      由两跳调同一个 [`render`] 保证。
// 读回口：[`read_in`] 把盘上那份按行解析回清单（`70 §3.1` 那张「没有的」表第一条）。

/// 一条别名。`args` 是原样的 ccm argv（`["--tmux", "--account", "z"]`），渲染时逐个按需加引号。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct Alias {
    pub name: String,
    pub args: Vec<String>,
}

/// 一条别名的问题（进不了代码的那一条为什么进不了）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct AliasProblem {
    pub name: String,
    pub message: String,
}

/// ① 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasRender {
    /// 整份文件（写入那一跳原样落盘的就是它；手贴的人复制它或 `lines`）。
    pub code: String,
    /// 每条合格别名的那一行，按清单顺序。
    pub lines: Vec<String>,
    /// 不合格的那几条。**非空时 [`install_in`] 一个字节都不写**（fail-closed）。
    pub problems: Vec<AliasProblem>,
    /// 名字撞了的提示，一条一句。**只出声、不拦**（`cc` 在多数机器上是 C 编译器，盖不盖由人定）。
    pub collisions: Vec<String>,
}

/// 读回口的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasListing {
    pub alias_path: String,
    /// 那份文件在不在。不在 ≠ 读失败（读失败是 `Err`）。
    pub exists: bool,
    pub aliases: Vec<Alias>,
    /// 解析不回清单的那几行（原文 ＋ 原因）。**不静默丢**：写回去之前人得知道它们会没。
    pub unparsed: Vec<String>,
    /// 这台机器上找得到的 shell 配置候选（「那一行 source 加进哪份」）。
    pub rc_candidates: Vec<AccountAliasRc>,
}

/// ② 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasInstallReport {
    pub alias_path: String,
    pub wrote_alias_file: bool,
    pub wrote_rc: bool,
    /// 给人看的补充说明，一条一句。
    pub notes: Vec<String>,
}

/// 能进别名的 ccm 修饰：`(旗标, 要不要跟一个值)`。`71 §4` 第一、二档；
/// 第三档（`resume` / `attach` / `--ccm-sid` / `--print` / …）每次取值都不同，做成固定别名没意义 ⇒ 不收。
/// `--tmux=<名>` 是 `--tmux` 的内联形，另判；`--` 之后原样透传给 agent。
///
/// ⚠ 每一个旗标都得是后端 `ccm --help` 里真有的那个词 —— 判据
/// `account_aliases_tests.rs::every_alias_flag_is_a_real_ccm_flag` 去后端的用法文本里对（异源）。
pub(crate) const ALIAS_FLAGS: &[(&str, bool)] = &[
    ("--cwd", true),
    ("--account", true),
    ("--base", false),
    ("--tmux", false),
    ("--tmux-base", true),
    ("--agent", true),
    ("--model", true),
    ("--launcher", true),
    ("--tmux-size", true),
    ("--detach", false),
    ("--bus-register", false),
    ("--bus-note", true),
];

/// 名字的规则（`71 §5` V5 前半）：POSIX shell 函数名。
fn name_is_valid(name: &str) -> bool {
    let mut cs = name.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 一条别名合不合格。**这些是「判定的规则」**（`71 §12.3` 第 6 格），与哪种 shell 无关。
pub fn check_alias(a: &Alias) -> Result<(), String> {
    if !name_is_valid(&a.name) {
        return Err("名字只能用字母、数字、下划线，而且不能以数字开头".into());
    }
    let (mut account, mut base, mut tmux, mut tmux_named, mut tmux_base) =
        (false, false, false, false, false);
    let (mut size, mut detach, mut bus) = (false, false, false);
    let mut it = a.args.iter();
    while let Some(w) = it.next() {
        if w.chars().any(char::is_control) {
            return Err("参数里有换行或控制字符".into());
        }
        if w == "--" {
            if it.any(|x| x.chars().any(char::is_control)) {
                return Err("参数里有换行或控制字符".into());
            }
            break;
        }
        if let Some(n) = w.strip_prefix("--tmux=") {
            if n.is_empty() {
                return Err("`--tmux=` 后面缺会话名".into());
            }
            tmux = true;
            tmux_named = true;
            continue;
        }
        let Some((flag, takes)) = ALIAS_FLAGS.iter().find(|(f, _)| f == w) else {
            return Err(format!("`{w}` 不能放进别名（只收 ccm 的修饰）"));
        };
        if *takes {
            match it.next() {
                Some(v) if !v.is_empty() && !v.chars().any(char::is_control) => {}
                _ => return Err(format!("`{flag}` 后面缺一个值")),
            }
        }
        match *flag {
            "--account" => account = true,
            "--base" => base = true,
            "--tmux" => tmux = true,
            "--tmux-base" => {
                tmux = true;
                tmux_base = true;
            }
            "--tmux-size" => size = true,
            "--detach" => detach = true,
            "--bus-register" => bus = true,
            _ => {}
        }
    }
    // V1–V4（`71 §5`，依据是 `ccm --help` 逐字）。
    if account && base {
        return Err("`--account` 与 `--base` 只能选一个".into());
    }
    if tmux_named && tmux_base {
        return Err("`--tmux=<名>` 与 `--tmux-base` 只能选一个".into());
    }
    if bus && !detach {
        return Err("`--bus-register` 要和 `--detach` 一起用".into());
    }
    if (size || detach) && !tmux {
        return Err("`--tmux-size` 与 `--detach` 只在 tmux 里起的时候有意义".into());
    }
    Ok(())
}

/// 一个参数要不要加引号：只由「安全字符」组成的原样放，其余一律 POSIX 单引号
/// （实现只有一份：`shell_quote_core::posix_quote`）。
fn shell_word(w: &str) -> String {
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

/// 一条（合格的）别名在 POSIX shell 里那一行：`名字() { ccm <参数…> "$@"; }`。
/// `"$@"` 必须在最后 —— 那就是「参数附加器」的全部含义：调用时再给的参数接在后面、后者胜。
pub fn render_line(a: &Alias) -> String {
    let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
    let mut out = format!("{}() {{ {word}", a.name);
    for w in &a.args {
        out.push(' ');
        out.push_str(&shell_word(w));
    }
    out.push_str(" \"$@\"; }");
    out
}

/// ① **纯**：清单 → 代码。一个字节都不写、一个文件都不读（撞名检查读的是自带片段与 `PATH`）。
pub fn render(aliases: &[Alias]) -> AliasRender {
    let mut lines = Vec::new();
    let mut problems = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for a in aliases {
        if seen.contains(&a.name.as_str()) {
            problems.push(AliasProblem {
                name: a.name.clone(),
                message: "同一个名字出现了两次 —— 后一条会盖掉前一条".into(),
            });
            continue;
        }
        seen.push(&a.name);
        match check_alias(a) {
            Ok(()) => lines.push(render_line(a)),
            Err(message) => problems.push(AliasProblem {
                name: a.name.clone(),
                message,
            }),
        }
    }
    let collisions = aliases
        .iter()
        .filter(|a| name_is_valid(&a.name))
        .filter_map(|a| collision_note(&a.name))
        .collect();
    AliasRender {
        code: render_file(&lines),
        lines,
        problems,
        collisions,
    }
}

/// 把生成文件里的一行解析回一条别名。认两种调用词：裸 `ccm`，以及从前那种
/// `"${CCM:-<路径>}"`（`K-R69` 的 `ccmInvocation`〔散文墓碑〕吐过，那一格随 TS 生成器退役）—— 读回之后一律按裸 `ccm` 重写。
fn parse_line(line: &str) -> Result<Alias, String> {
    let rest = line
        .strip_suffix(" \"$@\"; }")
        .ok_or("结尾不是 `\"$@\"; }`")?;
    let (name, body) = rest
        .split_once("() { ")
        .ok_or("不是 `名字() { … }` 的形状")?;
    let mut words = split_words(body)?.into_iter();
    let head = words.next().unwrap_or_default();
    let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
    if head != word && !head.starts_with("\"${CCM:-") {
        return Err(format!("调的不是 {word}"));
    }
    let a = Alias {
        name: name.to_string(),
        args: words.collect(),
    };
    check_alias(&a)?;
    Ok(a)
}

/// **读回口**：盘上那份别名文件 → 清单。只读。
pub fn read_in(home: &Path) -> Result<AliasListing, String> {
    let path = alias_file_in(home);
    let (exists, text) = match std::fs::read_to_string(&path) {
        Ok(t) => (true, t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (false, String::new()),
        Err(e) => return Err(format!("读不了 {}：{e}", path.display())),
    };
    let mut aliases = Vec::new();
    let mut unparsed = Vec::new();
    for l in text.lines().map(str::trim) {
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        match parse_line(l) {
            Ok(a) => aliases.push(a),
            Err(why) => unparsed.push(format!("{l}（{why}）")),
        }
    }
    Ok(AliasListing {
        alias_path: path.display().to_string(),
        exists,
        aliases,
        unparsed,
        rc_candidates: rc_candidates_in(home),
    })
}

/// ② **唯一的副作用**：把 [`render`] 的产物整份写进别名文件，可选地把一行 `source` 装进 `rc`。
/// 有一条不合格 ⇒ **整批不写**（写一半的别名文件是最坏的结局：它 source 得进去，少了的没人发现）。
///
/// 〔RW1 · 第四波 09-24〕两处写都经 `door`（生产 = 本机后端的文件管理那一面）；home 也问它
/// （写落在后端认的那个 home 底下，两边的 home 不许各算各的）。
pub async fn install_in<D: crate::user_files::Door>(
    door: &D,
    aliases: &[Alias],
    rc: Option<&str>,
) -> Result<AliasInstallReport, String> {
    let r = render(aliases);
    if !r.problems.is_empty() {
        let why: Vec<String> = r
            .problems
            .iter()
            .map(|p| format!("{}：{}", p.name, p.message))
            .collect();
        return Err(format!(
            "有 {} 条别名不合格，一条都没写：{}",
            why.len(),
            why.join("；")
        ));
    }
    let home = door.home().await?;
    let path = alias_file_in(Path::new(&home));
    let wrote_alias_file = write_alias_file(door, &home, &r.code).await?;
    let mut notes = Vec::new();
    if !wrote_alias_file {
        notes.push("别名文件和盘上那份一模一样，没有重写。".to_string());
    }
    let mut wrote_rc = false;
    if let Some(rc_raw) = rc {
        if ensure_rc_source_line(door, &home, rc_raw, &source_line(&path)).await? {
            wrote_rc = true;
            notes.push(format!(
                "{rc_raw} 里加了一行 source（要撤就把 cc-monitor 那一小块整块删掉）。"
            ));
        } else {
            notes.push(format!("{rc_raw} 里已经接上了这份文件，没有再动它。"));
        }
    }
    notes.push(format!(
        "新开一个终端就能用；当前终端要先执行一次 . {}",
        path.display()
    ));
    Ok(AliasInstallReport {
        alias_path: path.display().to_string(),
        wrote_alias_file,
        wrote_rc,
        notes,
    })
}

/// 这个名字是不是已经被占了。**只出声、不拦** —— 见 `§0c 问三`。
///
/// 两条路各查一次，报出来的话里带住址，用户才知道自己在盖掉什么：
/// ① `src/shared/ccm-aliases.sh` 里自带的那几个（今天是 `cc` / `cct`；`K-R58` 删掉了 `cch`）——
///    **问的是那份文件本身**（`sftp::CCM_WRAPPER_SNIPPET` 就是它 `include_str!` 进来的），
///    不在这里抄一份名字清单；
/// ② `PATH` 上真有一个同名程序 —— 🔴 `cc` 在多数机器上是 C 编译器
///    （`/usr/bin/cc`），而自带那份别名只检查「有没有同名**函数**」、不检查程序。
///
/// ⚠ **诚实边界**：② 查的是 monitor 这个进程的 `PATH`，不是用户登录 shell 的 `PATH`，
/// 两者可以不同 ⇒ 它会漏报，不会误报成「有」。
pub fn collision_note(name: &str) -> Option<String> {
    if crate::sftp::CCM_WRAPPER_SNIPPET.contains(&format!("\n{name}()")) {
        return Some(format!(
            "`{name}`：cc-monitor 自带的别名块（src/shared/ccm-aliases.sh）里已经有同名函数 —— \
             那一份用 `declare -f` 让着你，所以你这条会赢；确认这就是你要的"
        ));
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(format!(
                "`{name}`：PATH 上已经有一个同名程序（{}）—— source 之后你在终端敲 `{name}` \
                 打到的是这条别名，不再是那个程序",
                cand.display()
            ));
        }
    }
    None
}

/// 候选 rc 的现状。**只列盘上真实存在的那几份**，不存在的不列（不许猜一个出来）。
pub fn rc_candidates_in(home: &Path) -> Vec<AccountAliasRc> {
    // 🔴 认 [`ALIAS_FILE_REL`] 而不是展开后的绝对路径 —— 理由与
    // [`ensure_rc_source_line`] 里那段红字是同一条：`src/shared/ccm-aliases.sh` 里那一行
    // 写的是 `$HOME/.cc-monitor/…`（没展开）。按绝对路径认，装了 ccm 别名块的人
    // 会被界面告知「还没 source 过」。
    let needle = ALIAS_FILE_REL;
    RC_CANDIDATES
        .iter()
        .map(|n| home.join(n))
        .filter(|p| p.is_file())
        .map(|p| {
            let sourced = std::fs::read_to_string(&p)
                .map(|s| s.contains(needle))
                .unwrap_or(false);
            AccountAliasRc {
                path: p.display().to_string(),
                sourced,
            }
        })
        .collect()
}

/// 把生成文件写下去。**内容一致就一个字节都不写。**
///
/// 〔RW1 · 第四波 09-24〕经 `door`（本机后端）写：生成文件是**我们自己**的东西 ⇒ 不留备份文件；
/// `~/.cc-monitor` 还不在就逐级补（`parents`）。回读 · 回滚那一份规则住后端。
async fn write_alias_file<D: crate::user_files::Door>(
    door: &D,
    home: &str,
    content: &str,
) -> Result<bool, String> {
    let done = crate::user_files::edit(door, home, ALIAS_FILE_REL, false, true, |_| {
        Ok(Some(content.to_string()))
    })
    .await?;
    Ok(matches!(done, crate::user_files::Edited::Written(_)))
}

/// 把那一行 `source` 装进用户指定的 rc。**已经有了就一个字节都不写。**
///
/// 🔴 三道，一道都不省：① 路径过 `profile_installer::fence_path_under`（只许落在 home 之内）；
/// ② 围栏损坏（有 BEGIN 没 END）**中止**，绝不用后面那个 END 去配对、吃掉中间的用户代码；
/// ③ 写之前先备份、写完回读逐字比对、不符回滚。② 是 `fenced_block::splice_in`；
/// ③ 〔RW1〕在后端（`files-put` 的 `backup: true`），与 rc 别名块、PowerShell profile、远端 rc 同一份规则。
async fn ensure_rc_source_line<D: crate::user_files::Door>(
    door: &D,
    home: &str,
    rc_raw: &str,
    line: &str,
) -> Result<bool, String> {
    // ⚠ 围栏是 `profile_installer` 那一份，**不在这里长第二道** —— `home` 当参数传进去，
    //   于是这条路测得了（临时目录当 home），而生产侧传的是后端答的 home。
    let path = crate::profile_installer::fence_path_under(Path::new(home), rc_raw)?;
    let rel = crate::user_files::rel_under(home, &path.display().to_string())?;
    let what = path.display().to_string();
    let block = format!("{RC_BEGIN}\n{line}\n{RC_END}\n");
    let done = crate::user_files::edit(door, home, &rel, true, false, |existing| {
        let Some(existing) = existing else {
            return Err(format!("读不到 {what}：文件不存在"));
        };
        // 🔴 认的是 [`ALIAS_FILE_REL`]，**不是那一整行**。
        //
        // 这一处栽过：`src/shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`（**没展开**），
        // 而这里手上的 `line` 带的是展开后的绝对路径 ⇒ 按整行比，
        // **一个已经装了 ccm 别名块的人会被判成「还没 source 过」，于是又被追加一行** ——
        // 那正是本件开头列的第一条病（重复追加）。
        // 按相对路径认，两种写法都认得出来。
        // ⚠ 〔RW1〕改名之后旧那一段（指着 `account-aliases.sh`）认不出来 ⇒ 走下面的整块替换，改写成新形状。
        if existing.contains(ALIAS_FILE_REL) {
            return Ok(None);
        }
        crate::fenced_block::splice_in(
            existing,
            RC_BEGIN,
            RC_END,
            &block,
            &what,
            crate::fenced_block::Layout::Posix,
        )
        .map(Some)
    })
    .await?;
    Ok(matches!(done, crate::user_files::Edited::Written(_)))
}

#[cfg(test)]
#[path = "../../../tests/bridge/account_aliases_tests.rs"]
mod tests;
