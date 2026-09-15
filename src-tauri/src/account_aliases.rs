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
//! `shared/ccm-aliases.sh`（装 ccm 别名块时写进 rc 的那一份）本轮起自带一行
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
/// 后者在文件不存在时整行返回 1，而这一行在 `shared/ccm-aliases.sh` 里是**最后一行**
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
/// ① `shared/ccm-aliases.sh` 里自带的那几个（今天是 `cc` / `cct`；`K-R58` 删掉了 `cch`）——
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
            "`{name}`：cc-monitor 自带的别名块（shared/ccm-aliases.sh）里已经有同名函数 —— \
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
    // [`ensure_rc_source_line`] 里那段红字是同一条：`shared/ccm-aliases.sh` 里那一行
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
    // 这一处栽过：`shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`（**没展开**），
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
mod tests {
    use super::*;

    /// 「这份文本里**有整整一行**逐字等于它」。
    ///
    /// ⚠ 刻意不是 `hay.contains(needle)`：那种比法的**匹配单位（子串）比事实（一整行）小**
    /// —— 别名那一行被截断、或被加了个没人要的修饰，子串比法照样绿。
    /// `needle_anchor_registry` 里那条递减棘轮数的正是这一族，本模块一处都不往上加。
    fn pinned(hay: &str, needle: &str) -> bool {
        hay.lines().any(|l| l == needle)
    }

    /// panic 也要清干净（`profile_installer` 那条纪律的同一份）。
    struct TmpHome(PathBuf);
    impl Drop for TmpHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tmp_home(tag: &str) -> TmpHome {
        let d = std::env::temp_dir().join(format!(
            "ccm-alias-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("建 tempdir");
        TmpHome(d)
    }

    /// ★★ 围栏：**放进用户 shell 会被执行的那一行，一个命令分隔符都不许有**。
    ///
    /// 正例那一半不是凑数：围栏收太紧会把 `buildAliasLine` 真正产出的形状挡在外面
    /// （六个修饰 + 带空格的模型名那种要引号的值），那是把一个洞换成一个回归。
    #[test]
    fn the_alias_fence_only_lets_the_generated_shape_through() {
        for ok in [
            "zcc() { ccm --account 'z' \"$@\"; }",
            "bcc() { ccm \"$@\"; }",
            "zcct() { ccm --tmux --account 'z' --agent codex --model 'opus' \"$@\"; }",
            "basecc() { ccm --base --launcher '/usr/bin/claude' \"$@\"; }",
            "m() { ccm --model 'claude-opus-4-1' \"$@\"; }",
            // `q()` 对含引号的值产出的那一形 —— 反斜杠唯一合法的出场
            "q1() { ccm --model 'a'\\''b' \"$@\"; }",
        ] {
            assert!(
                validate_alias_line(ok).is_ok(),
                "围栏拒了一个 `buildAliasLine` 真会产出的形状：{ok:?} —— \
                 收太紧会让用户在界面上点了「写入」却永远写不成"
            );
        }
        for bad in [
            // 注入：分号起第二条命令
            "zcc() { ccm --account 'z'; rm -rf ~ \"$@\"; }",
            // 注入：命令替换
            "zcc() { ccm --account $(id -u) \"$@\"; }",
            "zcc() { ccm --account `id -u` \"$@\"; }",
            // 注入：后台 / 管道 / 重定向
            "zcc() { ccm --account z & \"$@\"; }",
            "zcc() { ccm --account z | sh \"$@\"; }",
            "zcc() { ccm --account z > ~/.bashrc \"$@\"; }",
            // 换行：整条第二行都是自由的
            "zcc() { ccm\nrm -rf ~ \"$@\"; }",
            // 名字非法：带空格粘进 rc 会当场弄坏 shell 配置（Phase D 审计逼出来的那条）
            "my alias() { ccm \"$@\"; }",
            "2cc() { ccm \"$@\"; }",
            // 修饰不在闭集里
            "zcc() { ccm --dangerously-skip-permissions \"$@\"; }",
            // 形状：不转发 "$@"
            "zcc() { ccm --account 'z'; }",
            // 引号不配平
            "zcc() { ccm --model 'op \"$@\"; }",
            // 干脆不是我们生成的东西
            "rm -rf ~",
            "",
        ] {
            assert!(
                validate_alias_line(bad).is_err(),
                "围栏放行了 {bad:?} —— 这一行会被写进一份用户 shell 会 source 的文件"
            );
        }
    }

    /// ★★ **整份重写**：删了账号，它那条命令当场没了。
    ///
    /// 这一条买的正是「为什么不往 `~/.bashrc` 追加」——追加那条路上，
    /// 下面第二次调用只会让文件里**同时**有 `zcc` 和 `bcc`。
    #[test]
    fn regenerating_drops_the_account_you_deleted() {
        let h = tmp_home("regen");
        let two = vec![
            "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
            "bcc() { ccm --account 'b' \"$@\"; }".to_string(),
        ];
        let r = apply(&h.0, &two, None, false).expect("第一次生成");
        assert!(r.wrote_alias_file, "第一次必须真写：{r:?}");
        let p = alias_file_in(&h.0);
        let after = std::fs::read_to_string(&p).expect("读回生成文件");
        // ⚠ 比的是**整行相等**，而且拿的是喂进去的那一行本身 ——
        //   `contains("zcc()")` 这种子串比法的匹配单位比事实小：那一行被截断 / 被改了修饰，
        //   它照样绿（`needle_anchor_registry` 那条递减棘轮数的正是这一族）。
        assert!(
            pinned(&after, &two[0]) && pinned(&after, &two[1]),
            "{after}"
        );

        // 删掉 b 这个账号之后再生成一次
        let one = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
        apply(&h.0, &one, None, false).expect("第二次生成");
        let after = std::fs::read_to_string(&p).expect("读回生成文件");
        assert!(pinned(&after, &one[0]), "留下来的那条没了：{after}");
        assert!(
            !pinned(&after, &two[1]),
            "★ 删了账号，它那条命令还在 —— 那正是「往 rc 里追加」这条路的病：{after}"
        );
    }

    /// 内容一致 ⇒ **一个字节都不写**（不是「写了一遍一样的」）。
    #[test]
    fn identical_content_is_not_rewritten() {
        let h = tmp_home("idem");
        let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
        assert!(apply(&h.0, &lines, None, false).unwrap().wrote_alias_file);
        let r = apply(&h.0, &lines, None, false).unwrap();
        assert!(
            !r.wrote_alias_file && r.alias_file_unchanged,
            "第二次不该再写：{r:?}"
        );
    }

    /// 预览**一个字节都不写** —— 「看一眼会发生什么」不该有副作用。
    #[test]
    fn dry_run_touches_nothing() {
        let h = tmp_home("dry");
        let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
        let r = apply(&h.0, &lines, None, true).expect("预览");
        assert_eq!(r.names, vec!["zcc".to_string()]);
        assert!(!r.wrote_alias_file);
        assert!(
            !alias_file_in(&h.0).exists(),
            "预览把文件写出来了 —— 那它就不是预览"
        );
    }

    /// 一条不合法 ⇒ **整批不写**。半份别名文件比没有更坏。
    #[test]
    fn one_bad_line_aborts_the_whole_batch() {
        let h = tmp_home("batch");
        let lines = vec![
            "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
            "evil() { ccm --account 'z'; curl x|sh \"$@\"; }".to_string(),
        ];
        assert!(apply(&h.0, &lines, None, false).is_err());
        assert!(
            !alias_file_in(&h.0).exists(),
            "整批该被拒，可文件已经建出来了"
        );
    }

    /// 同名两条 ⇒ 拒。后一条会静默盖掉前一条，而用户只会看见「少了一个命令」。
    #[test]
    fn duplicate_names_are_refused() {
        let h = tmp_home("dup");
        let lines = vec![
            "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
            "zcc() { ccm --account 'b' \"$@\"; }".to_string(),
        ];
        assert!(apply(&h.0, &lines, None, false).is_err());
    }

    /// ★★ rc 那一行：**加一次，第二次一个字节都不写**，而且用户自己的内容一行不动。
    #[test]
    fn the_rc_source_line_is_added_once_and_keeps_user_content() {
        let h = tmp_home("rc");
        let rc = h.0.join(".bashrc");
        std::fs::write(&rc, "export PATH=$PATH:/opt/bin\nalias ll='ls -l'\n").expect("写 rc");
        let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
        let line = source_line(&alias_file_in(&h.0));

        let added =
            ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("第一次装");
        assert!(added, "第一次该真写");
        let after = std::fs::read_to_string(&rc).expect("读回");
        assert!(
            pinned(&after, "alias ll='ls -l'"),
            "用户内容被动了：{after}"
        );
        assert!(after.contains(&line), "那一行没进去：{after}");
        assert!(
            after.contains(RC_BEGIN) && after.contains(RC_END),
            "{after}"
        );

        let again = ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("第二次");
        assert!(!again, "★ 第二次又写了一遍 —— 那正是「重复追加」那条病");
        let twice = std::fs::read_to_string(&rc).expect("读回");
        assert_eq!(twice.matches(&line).count(), 1, "那一行出现了两次：{twice}");

        // 顺带：候选表能认出「已经 source 过了」
        let r = apply(&h.0, &lines, None, true).expect("预览");
        let me = r
            .rc_candidates
            .iter()
            .find(|c| c.path == rc.display().to_string())
            .expect("候选里该有 .bashrc");
        assert!(me.sourced, "装完了还说没 source 过：{me:?}");
    }

    /// 🔴 **装了 ccm 别名块的人，不许再被追加一行。**
    ///
    /// `shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`（**没展开**），
    /// 而 [`source_line`] 手上是展开后的绝对路径。按整行比，这个人会被判成
    /// 「还没 source 过」，于是又被追加一行 —— **那正是本件开头列的第一条病**。
    /// 本条把那个形状原样喂进去。
    #[test]
    fn an_rc_that_already_sources_it_via_home_var_is_recognized() {
        let h = tmp_home("homevar");
        let rc = h.0.join(".bashrc");
        // 逐字取自 `shared/ccm-aliases.sh` 的最后一行（`$HOME` 没展开）。
        let ccm_block = "if [ -r \"$HOME/.cc-monitor/account-aliases.sh\" ]; then . \"$HOME/.cc-monitor/account-aliases.sh\"; fi\n";
        std::fs::write(&rc, format!("# mine\n{ccm_block}")).expect("写 rc");
        let before = std::fs::read(&rc).expect("读原文");

        let line = source_line(&alias_file_in(&h.0));
        let added =
            ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("不该报错");
        assert!(
            !added,
            "★ 已经 source 过了还要再加一行 —— 那正是「重复追加」那条病"
        );
        assert_eq!(std::fs::read(&rc).expect("读回"), before, "rc 必须逐字未变");

        // 候选表也要认得出来（界面上那一项会显「已经 source 过了」）。
        let me = rc_candidates_in(&h.0)
            .into_iter()
            .find(|c| c.path == rc.display().to_string())
            .expect("候选里该有 .bashrc");
        assert!(me.sourced, "候选表没认出 `$HOME` 那一形：{me:?}");
    }

    /// 那一行在文件不存在时**必须返回 0** —— 它是 `ccm-aliases.sh` 的最后一行。
    #[test]
    fn the_source_line_is_a_no_op_when_the_file_is_absent() {
        let line = source_line(Path::new("/nowhere/account-aliases.sh"));
        assert!(
            line.starts_with("if ") && line.ends_with("; fi"),
            "写成了 `&&` 那一形 ⇒ 文件不存在时整行返回 1，而它是那份片段的最后一行：{line}"
        );
        assert!(
            line.contains(ALIAS_FILE_REL.rsplit('/').next().unwrap()),
            "{line}"
        );
    }

    /// 围栏损坏（有 BEGIN 没 END）⇒ **中止**，文件逐字未变。
    ///
    /// 这条与 `profile_installer::damaged_fence_leaves_the_file_byte_identical` 同形：
    /// 用后面那个 END 去配对，会把中间的用户代码整段吃掉。
    #[test]
    fn a_damaged_fence_leaves_the_rc_byte_identical() {
        let h = tmp_home("bad");
        let rc = h.0.join(".bashrc");
        let original = format!("# mine\n{RC_BEGIN}\nalias ll='ls -l'\n");
        std::fs::write(&rc, &original).expect("写 rc");
        let before = std::fs::read(&rc).expect("读原文");
        let line = source_line(&alias_file_in(&h.0));
        let e = ensure_rc_source_line(&h.0, &rc.display().to_string(), &line)
            .expect_err("围栏损坏该中止");
        assert!(e.contains("找不到配对的 END"), "理由要说得清：{e}");
        assert_eq!(
            std::fs::read(&rc).expect("读回"),
            before,
            "围栏损坏时文件必须逐字未变"
        );
    }

    /// rc 路径过 home 围栏 —— 跑出 home 的一律拒（复用 `profile_installer` 那道，不另立一份）。
    ///
    /// ⚠ 正例那一半不是凑数：围栏若收成「谁都不许」，上面那条「装一行 source」会当场变成
    /// 一个永远写不成的按钮，而本条**只看反例时读起来一样绿**。
    #[test]
    fn the_rc_path_cannot_escape_home() {
        let h = tmp_home("fence");
        for bad in ["/etc/profile", "/tmp/x.rc", ".bashrc"] {
            let e = ensure_rc_source_line(&h.0, bad, "# x").expect_err("该被围栏拒");
            assert!(e.starts_with("refuse profile path"), "{bad}：{e}");
        }
        // 正例：home 之内那一份真的过得去（文件不存在 ⇒ 停在「读不到」，而不是停在围栏上）。
        let e = ensure_rc_source_line(&h.0, &h.0.join(".zshrc").display().to_string(), "# x")
            .expect_err("文件还不存在，这一步该停在读那一步");
        assert!(
            !e.starts_with("refuse profile path"),
            "围栏把 home 之内的路径也拒了 —— 那会让那个按钮永远写不成：{e}"
        );
    }

    /// 🔴 `§0c 问三`：`cc` 在多数机器上是 C 编译器 —— 撞了要**出声**。
    ///
    /// ⚠ 这一条断的是「自带别名块里那几个名字会被认出来」，人群取自
    /// `sftp::CCM_WRAPPER_SNIPPET`（= `shared/ccm-aliases.sh` 本身），**不抄第二份名单** ——
    /// `KR58D1` 起这句话**真的兑现了**：人群由 `sftp::builtin_alias_names()` 现算，
    /// 上一版这里手写着 `["cc", "cch", "cct"]`，那就是第二个住址。
    #[test]
    fn a_name_that_is_already_taken_gets_a_note() {
        let taken = crate::sftp::builtin_alias_names();
        assert!(
            !taken.is_empty(),
            "自带别名块里一个函数都没解析出来 —— 这一条会变成空真，先修人群"
        );
        for t in &taken {
            let note =
                collision_note(t).unwrap_or_else(|| panic!("`{t}` 在自带别名块里就有，却一声不吭"));
            assert!(note.contains(t), "{note}");
        }
        assert!(
            collision_note("zzz_no_such_command_anywhere").is_none(),
            "没撞的名字不该报警 —— 一句假警报会让人把所有警报都当噪音"
        );
    }

    /// 🔴 `KR58D1` 的**正面**：`cch` 从自带块里删掉之后，它变成**用户可以自己用的名字**。
    ///
    /// 〔用@09-11 逐字〕「`cch` = 不让它猜目录，就在你当前这个目录起。**这个不要。删掉。**」
    /// `K37` 第一条的判据：把 `cch` 去掉，用户自己写一行就有了 ⇒ **偏好，该出去**。
    ///
    /// ⚠ **这不是回归，是多一格自由** —— 所以这里断的是「不再报『自带块里已经有』」，
    /// 而**不是**「`collision_note` 返回 `None`」：`PATH` 上真有个叫 `cch` 的程序时它**该**出声，
    /// 那条支路一个字都没动（断成 `is_none()` 会让这条判据在那种机器上假红）。
    ///
    /// **失效方向**（DoD 逐字）：只删了 sh 文件那两行、而别处还把它当「已被占用」
    /// ⇒ 用户仍然用不了这个名字。
    #[test]
    fn cch_is_gone_and_the_name_is_free_for_the_user() {
        assert!(
            !crate::sftp::builtin_alias_names().contains(&"cch"),
            "`cch` 还定义在 shared/ccm-aliases.sh 里 —— 用户逐字说的是「这个不要。删掉。」"
        );
        if let Some(note) = collision_note("cch") {
            assert!(
                !note.contains("ccm-aliases.sh"),
                "`cch` 已经不在自带别名块里了，却仍被报成「自带块占了」：{note}"
            );
        }
    }

    /// 生成文件里**没有时间戳** —— 有了就永远比不出「内容没变」。
    #[test]
    fn the_generated_file_is_byte_stable() {
        let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
        assert_eq!(render_file(&lines), render_file(&lines));
        assert!(render_file(&[]).contains("一个账号命令都没有"));
    }
}
