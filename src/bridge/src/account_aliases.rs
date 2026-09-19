//! `K-R49`：**加了账号，那条命令也该跟着有** —— 每个账号一条 shell 命令，而且真的落盘。
//!
//! 〔用 09-10〕逐字：「**比如添加账号后添加对应命令, 像是 zcc, bcc 这种, 我现在添加了一个
//! 账号但是没法直接添加命令, 还得手动去改**」。
//!
//! # 🔴 落点：**我们自己那份文件**，不是用户的 `~/.bashrc`
//!
//! 别名**本身**由 TS 侧的 `buildAliasLine` 生成（全仓唯一那份生成器，本模块**不再造第二份**）。
//! 本模块只回答「写到哪、怎么写、什么时候不写」三件事，而第一件的答案刻意**不是** `~/.bashrc`：
//!
//! 1. **重复追加**：加三个账号往 rc 里追三次，没有任何东西负责去重；
//! 2. **删不掉**：账号删了那一行还在，指向一个不存在的账号；
//! 3. **弄坏的代价是「shell 起不来」** —— 而那正是用户用来救火的东西。
//!
//! ⇒ 落点是 [`alias_file_in`]（`~/.cc-monitor/account-aliases.sh`，**monitor 自己的目录**，
//! 与 `local_backend` / `local_daemon` 用的是同一个），**每次按账号表整份重写**：
//! 幂等、删了账号那一行当场消失、删掉整份文件也只是少几个命令，shell 照常起得来。
//!
//! # 用户的 shell 配置里最多只多**一行** `source`，而且多数人连这一行都不用加
//!
//! `src/shared/ccm-aliases.sh`（装 ccm 别名块时写进 rc 的那一份）本轮起自带一行
//! `if [ -r … ]; then . …; fi` 指向这份生成文件 ⇒ **已经装了 ccm 别名块的人，加账号之后什么都不用做**。
//! 没装的人可以让本模块把那一行 `source` 写进**他自己指定**的那份 rc（[`apply`] 的 `rc`
//! 参数）—— 路径由界面上的人选，本模块**不猜**（`§0e`：`.bashrc` / `.zshrc` /
//! fish 的 `config.fish` 写法不同，猜一个写进去是最坏的那条路）。那一行走
//! BEGIN/END 围栏 + 备份 + 原子替换 + 写后回读校验 + 失败回滚，与 `profile_installer`
//! 同一套原语，且**幂等**（已经 source 过就一个字节都不写）。
//!
//! # 🔴 围栏：前端递过来的是**要被 shell 执行的代码**
//!
//! 那一侧 `sftp.rs` 早就写过这条账（审计 S-1：「写进 ~/.bashrc 的是被 shell **执行**的代码，
//! 绝不能让前端注入任意 bash」），它的解法是「后端拥有那段文本」。本模块拿不到那条路 ——
//! 别名的**内容**天然来自用户填的那几个格子 ⇒ 改成**形状围栏**：[`validate_alias_line`]
//! 只接受 `名字() { ccm <已知修饰...> "$@"; }` 这**一个**形状，命令分隔符 / 展开符
//! （`;` `&` `|` `$` 反引号 `(` `)` 换行 …）一个都不放行。
//!
//! ⚠ **它是围栏，不是第二个生成器** —— 它只判「合不合法」，一个字节的内容都不产出。
//! 这条分界是有意的：生成器有两份就会漂，围栏有两份只是更严。

use std::path::{Path, PathBuf};

/// 生成文件在 home 下的相对路径。**monitor 自己的目录**，不是用户环境的一部分。
pub const ALIAS_FILE_REL: &str = ".cc-monitor/account-aliases.sh";

/// 写进用户 rc 的那一行所在的围栏。**刻意与 `profile_installer` 的
/// `# === cc-monitor BEGIN` 不同前缀** —— 后者装的是 PowerShell 的 `cc` 块，
/// 两者若共用标记，装一个就会把另一个整块替换掉。
/// ⚠ `K-R62` 起是 `pub(crate)`：同 `profile_installer::BEGIN_MARKER` 那条理由 ——
/// `fenced_block::FENCE_SHAPES` 指它，不抄它。
pub(crate) const RC_BEGIN: &str = "# === cc-monitor aliases BEGIN v1 ===";
pub(crate) const RC_END: &str = "# === cc-monitor aliases END ===";

/// 生成文件自己的围栏（整份重写，所以它只是给人看的边界）。
const FILE_BEGIN: &str = "# === cc-monitor account aliases BEGIN v1 ===";
const FILE_END: &str = "# === cc-monitor account aliases END ===";

/// `buildAliasLine` 会拼出来的**全部**修饰。围栏按这张表认词，多一个就红。
///
/// ⚠ 它与 TS 那一侧 `buildAliasLine` 的 `flags` 是同一件事的两个住址，
/// 而这一侧是**收窄**方向：TS 多加一个修饰而这里忘了加 ⇒ 写不进去（报错，不是静默丢）。
const KNOWN_FLAGS: &[&str] = &[
    "--tmux",
    "--account",
    "--base",
    "--agent",
    "--model",
    "--launcher",
];

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

/// 一次生成的结果 —— **预览与真写走同一个返回形状**，差别只在 `wrote_*` 那两格。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AccountAliasReport {
    /// 生成文件的绝对路径。
    pub alias_path: String,
    /// 这一份文件里会有哪几条命令（按传进来的顺序）。
    pub names: Vec<String>,
    /// 名字撞了的那几条，每条一句人话。**只出声，不拦** —— 见模块头注。
    pub collisions: Vec<String>,
    /// 这台机器上找得到的 shell 配置候选。
    pub rc_candidates: Vec<AccountAliasRc>,
    /// 真写了生成文件吗（预览恒 false）。
    pub wrote_alias_file: bool,
    /// 内容与盘上那份逐字相同 ⇒ **一个字节都没写**。
    pub alias_file_unchanged: bool,
    /// 往用户 rc 里加了那一行 `source` 吗（已经有了 ⇒ false，且 `notes` 里说明）。
    pub wrote_rc: bool,
    /// 给人看的补充说明，一条一句。
    pub notes: Vec<String>,
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
/// 它是 `buildAliasLine` 的 `q()` 唯一会产出的反斜杠；别处出现一律拒。
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

/// 🔴 **形状围栏**：只放行 `名字() { ccm <已知修饰...> "$@"; }` 这一个形状。
///
/// 它挡的是**注入**，不是难看：放进用户 shell 会被执行的那一行，一个命令分隔符都不许有。
pub fn validate_alias_line(line: &str) -> Result<String, String> {
    let rest = line
        .strip_suffix(" \"$@\"; }")
        .ok_or_else(|| format!("形状不对（结尾必须是 ` \"$@\"; }}`）：{line}"))?;
    let (name, tail) = rest
        .split_once("() { ccm")
        .ok_or_else(|| format!("形状不对（缺 `() {{ ccm`）：{line}"))?;
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!(
            "命令名只能用字母/数字/下划线、且不能以数字开头（实得 {name:?}）"
        ));
    }
    let mid = match tail {
        "" => "",
        t => t
            .strip_prefix(' ')
            .ok_or_else(|| format!("`ccm` 后面缺一个空格：{line}"))?,
    };
    // 先按字符集拒一遍：命令分隔符 / 展开符 / 重定向 / 通配 一个都不放行。
    for c in mid.chars() {
        let ok = c.is_ascii_alphanumeric()
            || matches!(
                c,
                ' ' | '-' | '_' | '.' | ',' | ':' | '/' | '@' | '=' | '+' | '\'' | '\\'
            );
        if !ok {
            return Err(format!("修饰里出现了不许出现的字符 {c:?}：{line}"));
        }
    }
    for w in split_words(mid)? {
        if w.starts_with('-') && !KNOWN_FLAGS.contains(&w.as_str()) {
            return Err(format!("不认识的修饰 `{w}`：{line}"));
        }
        if w.is_empty() {
            return Err(format!("修饰里有一个空词：{line}"));
        }
    }
    Ok(name.to_string())
}

/// 整份生成文件的内容。**没有时间戳** —— 有了就永远比不出「内容没变」，每次都要写一遍。
pub fn render_file(lines: &[String]) -> String {
    let mut out = String::new();
    out.push_str(FILE_BEGIN);
    out.push('\n');
    out.push_str(
        "# 这份文件由 cc-monitor 按账号表**整份重写**，别手改 —— 下一次生成会原样覆盖。\n\
         # 删了某个账号、再生成一次，它那条命令就跟着没了（这正是它不住在你 rc 里的理由）。\n\
         # 写法是 POSIX sh 函数，bash / zsh 都 source 得了；fish 不行。\n",
    );
    if lines.is_empty() {
        out.push_str("# （当前一个账号命令都没有）\n");
    }
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(FILE_END);
    out.push('\n');
    out
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
/// 落盘走 `profile_installer::atomic_write_string`（临时文件 + 原子替换）——
/// 本仓已经有四份平台原语副本了，这里绝不添第五份。
fn write_alias_file(path: &Path, content: &str) -> Result<bool, String> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建不出 {}：{e}", parent.display()))?;
    }
    let owned = path.to_path_buf();
    crate::profile_installer::atomic_write_string(&owned, content)
        .map_err(|e| format!("写 {} 失败：{e}", path.display()))?;
    crate::verified_write::verify_and_rollback(
        content,
        || std::fs::read_to_string(path).map_err(|e| format!("{e}（{}）", path.display())),
        || {
            // 生成文件是**我们自己**的东西，没有「用户原内容」要回滚回去；
            // 写坏了就删掉 —— 一个半截的它比没有它更坏（source 时会报语法错）。
            let _ = std::fs::remove_file(path);
        },
    )?;
    Ok(true)
}

/// 把那一行 `source` 装进用户指定的 rc。**已经有了就一个字节都不写。**
///
/// 🔴 三道，一道都不省：① 路径过 `profile_installer::fence_profile_path`（只许落在 home 之内）；
/// ② 围栏损坏（有 BEGIN 没 END）**中止**，绝不用后面那个 END 去配对、吃掉中间的用户代码；
/// ③ 写之前先备份、写完回读逐字比对、不符回滚。
fn ensure_rc_source_line(home: &Path, rc_raw: &str, line: &str) -> Result<bool, String> {
    // ⚠ 围栏是 `profile_installer` 那一份，**不在这里长第二道** —— `home` 当参数传进去，
    //   于是这条路测得了（临时目录当 home），而生产侧传的是 `dirs::home_dir()`。
    let path = crate::profile_installer::fence_path_under(home, rc_raw)?;
    let existing =
        std::fs::read_to_string(&path).map_err(|e| format!("读不到 {}：{e}", path.display()))?;
    // 🔴 认的是 [`ALIAS_FILE_REL`]，**不是那一整行**。
    //
    // 这一处栽过：`src/shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`（**没展开**），
    // 而这里手上的 `line` 带的是展开后的绝对路径 ⇒ 按整行比，
    // **一个已经装了 ccm 别名块的人会被判成「还没 source 过」，于是又被追加一行** ——
    // 那正是本件开头列的第一条病（重复追加）。
    // 按相对路径认，两种写法都认得出来。
    if existing.contains(ALIAS_FILE_REL) {
        return Ok(false);
    }
    let what = path.display().to_string();
    let block = format!("{RC_BEGIN}\n{line}\n{RC_END}\n");
    let updated = match crate::fenced_block::find_pair(&existing, RC_BEGIN, RC_END, &what)? {
        Some((b, e)) => {
            let ls: Vec<&str> = existing.split_inclusive('\n').collect();
            let before: String = ls[..b].concat();
            let after: String = if e + 1 < ls.len() {
                ls[(e + 1)..].concat()
            } else {
                String::new()
            };
            format!("{before}{block}{after}")
        }
        None => {
            let mut out = existing.clone();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&block);
            out
        }
    };
    let backup = path.with_file_name(format!(
        "{}.ccm-aliases-backup-{}",
        path.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "rc".into()),
        std::process::id()
    ));
    std::fs::copy(&path, &backup).map_err(|e| format!("备份 {} 失败：{e}", path.display()))?;
    if let Err(e) = crate::profile_installer::atomic_write_string(&path, &updated) {
        let _ = std::fs::copy(&backup, &path);
        return Err(format!("写 {} 失败：{e}（已从备份恢复）", path.display()));
    }
    crate::verified_write::verify_and_rollback(
        &updated,
        || std::fs::read_to_string(&path).map_err(|e| format!("{e}（{what}）")),
        || {
            let _ = std::fs::copy(&backup, &path);
        },
    )?;
    Ok(true)
}

/// 正题：按账号表生成/重写那份别名文件，并（可选）把那一行 `source` 装进指定的 rc。
///
/// `dry_run = true` 时**一个字节都不写**，只把同一份报告算出来给界面预览。
pub fn apply(
    home: &Path,
    lines: &[String],
    rc: Option<&str>,
    dry_run: bool,
) -> Result<AccountAliasReport, String> {
    // 🔴 先整批过围栏：**有一条不合法就整批不写**（fail-closed）。
    // 写一半的别名文件是最坏的结局 —— 它 source 得进去，而少了的那几条没人会发现。
    let mut names: Vec<String> = Vec::new();
    for l in lines {
        names.push(validate_alias_line(l)?);
    }
    {
        let mut sorted = names.clone();
        sorted.sort();
        let before = sorted.len();
        sorted.dedup();
        if sorted.len() != before {
            return Err("同一个命令名出现了两次 —— 后一条会静默盖掉前一条，先把名字改开".into());
        }
    }

    let alias_path = alias_file_in(home);
    let content = render_file(lines);
    let line = source_line(&alias_path);
    let collisions: Vec<String> = names.iter().filter_map(|n| collision_note(n)).collect();
    let mut notes: Vec<String> = Vec::new();
    let mut wrote_alias_file = false;
    let mut alias_file_unchanged = false;
    let mut wrote_rc = false;

    if dry_run {
        notes.push("这是预览：盘上一个字节都没动。".to_string());
    } else {
        match write_alias_file(&alias_path, &content)? {
            true => wrote_alias_file = true,
            false => {
                alias_file_unchanged = true;
                notes.push("别名文件的内容和盘上那份逐字相同 —— 一个字节都没写。".to_string());
            }
        }
        if let Some(rc_raw) = rc {
            if ensure_rc_source_line(home, rc_raw, &line)? {
                wrote_rc = true;
                notes.push(format!(
                    "往 {rc_raw} 里加了一行 `source`（围栏 `{RC_BEGIN}`，要撤就整块删掉）。"
                ));
            } else {
                notes.push(format!("{rc_raw} 里已经 source 过这份文件了 —— 没再动它。"));
            }
        }
    }
    notes.push(format!(
        "生效方式：`. {}`，或者开一个新终端。",
        alias_path.display()
    ));
    Ok(AccountAliasReport {
        alias_path: alias_path.display().to_string(),
        names,
        collisions,
        rc_candidates: rc_candidates_in(home),
        wrote_alias_file,
        alias_file_unchanged,
        wrote_rc,
        notes,
    })
}

#[cfg(test)]
#[path = "../../../tests/bridge/account_aliases_tests.rs"]
mod tests;
