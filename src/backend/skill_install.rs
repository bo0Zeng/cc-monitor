//! 〔AS2 · 第四波 4B · V113〕**skill「装到这台」** —— 两条只读帧命令：读来源那台的 skill · 在要被写的那一台上判。
//!
//! # 用户裁决（逐字）
//!
//! V113「目录自动同步，**装要你点**」· 「这样远端后端也能在远端装skill或者mcp」；V112「内容，原样拷过去并标出可疑项」
//! —— 不替用户改写。题面补一句：「skill 可能带脚本 / 二进制：可疑项规则要覆盖『可执行文件 / 绝对路径 / 对面未必有的命令』」。
//!
//! # 两条命令
//!
//! - `skill-read {name}`（**来源那台**跑）：`<skill 根>/<名>/` 下每个文件 `{path, text, bytes, exec, why}` ——
//!   `text` 是原文（不是文本 / 太大 / 读不出来 ⇒ `null` ＋ `why`：这一个装不过去，今天的写口只收文本）。
//! - `skill-install-plan {name, source, take?, overwrite?}`（**要被写的那一台**跑，事实是那台的）：
//!   逐文件四态 · 可疑项 · 那台上现有那几份的原文（当 CAS 期望）· 给了 `take` 才答「写哪几个」。
//!
//! # 复用 AS1，不写第二份（`AS2.md §1.3`）
//!
//! 差异四态与「不同的要显式说盖、不然整趟拒」那道闸**原样用** `mcp_sync::{diff, plan}`（键 = 文件相对路径，
//! 值 = `{text, exec}`，JSON 值相等即相同）；这台机器的事实（有没有这条路径 · `PATH` 上找不找得到这个名字）
//! 原样用 `mcp_sync::{Facts, Live, There, is_abs_any}`。本模块只多出 skill 自己的那几条可疑项规则（[`suspects_of`]）。
//!
//! # 它**不**做什么
//!
//! 一个字节都不写（写经 monitor → 那台后端 `files-put`，带 `expect`）· 不改写任何一个文件的内容。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::mcp_sync::{self, Facts, State};

/// skill 可疑项的种类（线上名）。**闭集**：界面文案表按它逐键给字。
pub const SUSPECT_KINDS: &[&str] = &["abs-path", "binary", "command-missing", "executable"];

// 上限与目录摘要同两个数（`asset_catalog::SKILL_MAX_FILES` / `SKILL_MAX_FILE_BYTES`，不另起一份）：
// 文件数超了整趟拒（装一半的 skill 比不装更坏）；单个文件超了那一个 `text = null` ＋ `why`（装不过去，说出来）。
use crate::asset_catalog::{SKILL_MAX_FILES as MAX_FILES, SKILL_MAX_FILE_BYTES as MAX_FILE_BYTES};
/// 一个文件里最多标这么多条绝对路径（再多就是噪音；`abs-path` 那几条已经足够让人去看）。
const MAX_PATHS_PER_FILE: usize = 8;

/// 这一族的应答。
pub type Answer = Result<Value, (&'static str, String)>;

/// skill 名：一段目录名（不许带分隔符 / `..` / 点开头 / NUL）。
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0', ':'])
}

/// skill 里的相对路径：`/` 分段、每段非空、不许 `.` / `..`、不许绝对 / 盘符 / 反斜杠 / NUL。
pub fn valid_rel(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 512
        && !p.starts_with('/')
        && !p.contains(['\\', '\0', ':'])
        && p.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

fn name_arg(args: &Value) -> Result<String, (&'static str, String)> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or(("bad_args", "少了 `name`（skill 的目录名）".to_string()))?;
    if !valid_name(name) {
        return Err((
            "bad_args",
            format!("「{name}」不是一个能当 skill 目录名的名字"),
        ));
    }
    Ok(name.to_string())
}

fn skill_dir(name: &str) -> Result<(PathBuf, PathBuf), (&'static str, String)> {
    let root = crate::agents::skills_root().ok_or((
        "io_failed",
        "这台机器说不出 skill 的根在哪 —— 不猜一个路径".to_string(),
    ))?;
    let dir = root.join(name);
    Ok((root, dir))
}

/// 读一个文件：`(原文, 读不出原文的原因)`，两者恰有一个。
fn read_text(p: &Path) -> (Option<String>, Option<String>) {
    match crate::common::fs::read_regular_capped(p, MAX_FILE_BYTES) {
        Ok(b) if b.contains(&0) => (None, Some("不是文本文件".into())),
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => (Some(t), None),
            Err(_) => (None, Some("不是 UTF-8 文本".into())),
        },
        Err(e) => (None, Some(e)),
    }
}

/// 走一个 skill 目录：每个普通文件（文件链接跟到底）一条 `(相对路径, 绝对路径)`。
/// 指向目录的链接 / 特殊文件不下去，进第二个返回值（说出来）。超 [`MAX_FILES`] ⇒ `Err`。
/// 走一个目录的结果：`(每个普通文件的（相对路径, 绝对路径）, 没下去的那几处)`。
type Walked = (Vec<(String, PathBuf)>, Vec<String>);

fn walk(dir: &Path) -> Result<Walked, (&'static str, String)> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for ent in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .sort_by_file_name()
    {
        let ent =
            ent.map_err(|e| ("io_failed", format!("走 {} 时读不出来：{e}", dir.display())))?;
        if ent.file_type().is_dir() {
            continue;
        }
        let rel = ent
            .path()
            .strip_prefix(dir)
            .unwrap_or(ent.path())
            .to_string_lossy()
            .replace('\\', "/");
        if !std::fs::metadata(ent.path()).is_ok_and(|m| m.is_file()) {
            skipped.push(rel);
            continue;
        }
        if files.len() >= MAX_FILES {
            return Err((
                "too_large",
                format!(
                    "{} 里的文件超过 {MAX_FILES} 个 —— 装一半比不装更坏，这一趟不读",
                    dir.display()
                ),
            ));
        }
        files.push((rel, ent.path().to_path_buf()));
    }
    Ok((files, skipped))
}

/// `skill-read`：来源那台上这个 skill 的全部文件。
pub fn answer_read(args: &Value) -> Answer {
    answer_read_at(None, args)
}

/// [`answer_read`] 的本体：skill 根可喂（判据拿临时目录喂）。
pub fn answer_read_at(root: Option<&Path>, args: &Value) -> Answer {
    let name = name_arg(args)?;
    let (root, dir) = match root {
        Some(r) => (r.to_path_buf(), r.join(&name)),
        None => skill_dir(&name)?,
    };
    if !std::fs::metadata(&dir).is_ok_and(|m| m.is_dir()) {
        return Err((
            "not_found",
            format!("这台机器上没有 skill「{name}」（{}）", dir.display()),
        ));
    }
    let (paths, skipped) = walk(&dir)?;
    let files: Vec<Value> = paths
        .iter()
        .map(|(rel, abs)| {
            let bytes = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
            let (text, why) = read_text(abs);
            json!({
                "path": rel,
                "text": text,
                "bytes": bytes,
                // 读执行位是平台原语：借插件口那一份（`plugin::discover::is_executable`，非 unix 上恒 false —— 如实登记）。
                "exec": crate::plugin::discover::is_executable(abs),
                "why": why,
            })
        })
        .collect();
    Ok(json!({
        "root": root.display().to_string(),
        "dir": dir.display().to_string(),
        "files": files,
        "skipped": skipped,
    }))
}

/// 一段文本里像绝对路径的词（两种系统的写法都认；引号 / 括号 / 标点当分隔）。去重、保序、最多 [`MAX_PATHS_PER_FILE`] 条。**纯**。
pub fn abs_paths_in(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\''
                    | '`'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | ';'
                    | '<'
                    | '>'
                    | '='
                    | '#'
                    | '!'
            )
    }) {
        let tok = raw.trim_end_matches(['.', ':']);
        if tok.len() > 1 && mcp_sync::is_abs_any(tok) && seen.insert(tok.to_string()) {
            out.push(tok.to_string());
            if out.len() >= MAX_PATHS_PER_FILE {
                break;
            }
        }
    }
    out
}

/// 第一行 `#!` 里要靠 `PATH` 找的那个名字（`#!/usr/bin/env python3` ⇒ `python3`）。解释器本身是绝对路径的那一形归 `abs-path`。**纯**。
pub fn shebang_command(text: &str) -> Option<String> {
    let first = text.lines().next()?.strip_prefix("#!")?.trim();
    let mut words = first.split_whitespace();
    let interp = words.next()?;
    let tail = interp.rsplit(['/', '\\']).next().unwrap_or(interp);
    if tail == "env" {
        return words.find(|w| !w.starts_with('-')).map(str::to_string);
    }
    None
}

/// 一个要拷过去的文件的可疑项（`kind`, `value`, 这台上的事实）。**纯**（事实是参数）。
pub(crate) fn suspects_of(text: Option<&str>, exec: bool, facts: &dyn Facts) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(text) = text else {
        out.push(json!({ "kind": "binary", "value": "", "there": null }));
        return out;
    };
    if exec || text.starts_with("#!") {
        out.push(json!({ "kind": "executable", "value": "", "there": null }));
    }
    // `#!/usr/bin/env X` 里的 `env` 是「靠 PATH 找 X」那个启动器本身 —— 它要的 X 归 `command-missing`，它自己不标。
    let launcher = shebang_command(text)
        .and(text.lines().next())
        .and_then(|l| {
            l.trim_start_matches("#!")
                .split_whitespace()
                .next()
                .map(str::to_string)
        });
    for p in abs_paths_in(text) {
        if launcher.as_deref() == Some(p.as_str()) {
            continue;
        }
        let there = facts.path(&p).wire();
        out.push(json!({ "kind": "abs-path", "value": p, "there": there }));
    }
    if let Some(cmd) = shebang_command(text) {
        let there = match facts.command(&cmd) {
            Some(true) => None,
            Some(false) => Some("absent"),
            None => Some("unknown"),
        };
        if let Some(there) = there {
            out.push(json!({ "kind": "command-missing", "value": cmd, "there": there }));
        }
    }
    out
}

/// 入参 `source`：`[{path, text, exec}]`（`text` 可为 `null` = 装不过去的那一个）。
struct SourceFile {
    text: Option<String>,
    exec: bool,
}

fn source_arg(args: &Value) -> Result<BTreeMap<String, SourceFile>, (&'static str, String)> {
    let arr = args.get("source").and_then(Value::as_array).ok_or((
        "bad_args",
        "少了 `source`（来源那台读到的文件），或它不是数组".to_string(),
    ))?;
    if arr.len() > MAX_FILES {
        return Err(("too_large", format!("文件超过 {MAX_FILES} 个")));
    }
    let mut out = BTreeMap::new();
    for f in arr {
        let path = f
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| valid_rel(p))
            .ok_or((
                "bad_args",
                "有一个文件的 `path` 缺了 / 不是 skill 里的相对路径".to_string(),
            ))?;
        let text = match f.get("text") {
            Some(Value::String(t)) => Some(t.clone()),
            Some(Value::Null) => None,
            _ => {
                return Err((
                    "bad_args",
                    format!("「{path}」的 `text` 只收字符串或 `null`"),
                ))
            }
        };
        let exec = f.get("exec").and_then(Value::as_bool).unwrap_or(false);
        if out
            .insert(path.to_string(), SourceFile { text, exec })
            .is_some()
        {
            return Err(("bad_args", format!("「{path}」给了两次")));
        }
    }
    Ok(out)
}

/// 帧面入口。
pub fn answer_plan(args: &Value) -> Answer {
    answer_plan_with(&mcp_sync::Live::from_env(), None, args)
}

/// [`answer_plan`] 的本体：事实与 skill 根都可喂（判据拿临时目录喂，不改进程环境）。
pub(crate) fn answer_plan_with(facts: &dyn Facts, root: Option<&Path>, args: &Value) -> Answer {
    let name = name_arg(args)?;
    let (root, dir) = match root {
        Some(r) => (r.to_path_buf(), r.join(&name)),
        None => skill_dir(&name)?,
    };
    let source = source_arg(args)?;
    let take = mcp_sync::names_arg(args.get("take"), "take")?;
    let overwrite = mcp_sync::names_arg(args.get("overwrite"), "overwrite")?;
    if take.is_none() && overwrite.is_some() {
        return Err((
            "bad_args",
            "给了 `overwrite` 没给 `take` —— 两张单子对不上".to_string(),
        ));
    }
    // 这台上现有的那一份（没有这个目录 ⇒ 空）。读不出原文的那几个记下来：它们盖不了（CAS 要原文）。
    let mut here: Map<String, Value> = Map::new();
    let mut here_text: BTreeMap<String, String> = BTreeMap::new();
    let mut unwritable: BTreeMap<String, String> = BTreeMap::new();
    if std::fs::metadata(&dir).is_ok_and(|m| m.is_dir()) {
        let (paths, skipped) = walk(&dir)?;
        for (rel, abs) in paths {
            let exec = crate::plugin::discover::is_executable(&abs);
            match read_text(&abs) {
                (Some(t), _) => {
                    here.insert(rel.clone(), json!({ "text": t, "exec": exec }));
                    here_text.insert(rel, t);
                }
                (None, why) => {
                    let why = why.unwrap_or_default();
                    here.insert(rel.clone(), json!({ "unreadable": why }));
                    unwritable.insert(rel, why);
                }
            }
        }
        for rel in skipped {
            here.insert(rel.clone(), json!({ "unreadable": "不是普通文件" }));
            unwritable.insert(rel, "不是普通文件".into());
        }
    }
    let there: Map<String, Value> = source
        .iter()
        .map(|(p, f)| match &f.text {
            Some(t) => (p.clone(), json!({ "text": t, "exec": f.exec })),
            None => (p.clone(), json!({ "unreadable": "来源那台读不出原文" })),
        })
        .collect();
    // AS1 那一份差异（键 = 相对路径；值相等才算相同）。
    let rows = mcp_sync::diff(&there, &here);
    let rows_json: Vec<Value> = rows
        .iter()
        .map(|(path, state)| {
            let suspects = match (state, source.get(path)) {
                (State::New | State::Differs, Some(f)) => {
                    suspects_of(f.text.as_deref(), f.exec, facts)
                }
                _ => Vec::new(),
            };
            json!({
                "path": path,
                "state": state.wire(),
                "suspects": suspects,
                "blocked": unwritable.get(path),
            })
        })
        .collect();
    let write = match take {
        None => None,
        Some(t) => {
            // 装不过去的那几个（来源没原文 / 这台那一份盖不了）先拒：不许勾，勾了整趟拒。
            if let Some(p) = t
                .iter()
                .find(|p| source.get(*p).is_some_and(|f| f.text.is_none()))
            {
                return Err((
                    "bad_args",
                    format!("「{p}」在来源那台读不出原文，今天装不过去 —— 这一趟一个都没写"),
                ));
            }
            if let Some(p) = t.iter().find(|p| unwritable.contains_key(*p)) {
                return Err((
                    "bad_file",
                    format!("这台上的「{p}」不是能按原文比对的文本，盖不了它 —— 这一趟一个都没写"),
                ));
            }
            Some(mcp_sync::plan(&rows, &t, &overwrite.unwrap_or_default())?)
        }
    };
    // 只交这一趟拷的那几个路径在这台上的原文（写的时候当 CAS 期望）；只在这台有的那几个不碰，也不回传。
    let target: Vec<Value> = here_text
        .iter()
        .filter(|(p, _)| source.contains_key(*p))
        .map(|(p, t)| json!({ "path": p, "text": t }))
        .collect();
    Ok(json!({
        "root": root.display().to_string(),
        "dir": dir.display().to_string(),
        "rows": rows_json,
        "target": target,
        "write": write,
    }))
}

#[cfg(test)]
#[path = "../../tests/backend/skill_install_tests.rs"]
mod tests;
