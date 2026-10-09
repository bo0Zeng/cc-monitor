//! skill「装到这台」—— 只读帧命令：读来源那台的 skill · 在要被写的那一台上判；另有 `skill-installs`（这台记着哪几个从别处装来的）·
//! `skill-uninstall-plan`（卸：判在被卸的那一台）—— 见文件末尾那一段。装要用户点，内容原样拷过去并标出可疑项（可执行文件 / 绝对路径 / 对面未必有的命令）。
//!
//! - `skill-read {name}`（来源那台跑）：`<skill 根>/<名>/` 下每个文件 `{path, text, bytes, exec, why}` ——
//!   `text` 是原文（不是文本 / 太大 / 读不出来 ⇒ `null` ＋ `why`：这一个装不过去，写口只收文本）。
//! - `skill-install-plan {name, source, take?, overwrite?}`（要被写的那一台跑，事实是那台的）：
//!   逐文件四态 · 可疑项 · 那台上现有那几份的原文（当 CAS 期望）· 给了 `take` 才答「写哪几个」。
//!
//! 差异四态与「不同的要显式说盖、不然整趟拒」那道闸用 `mcp_sync::{diff, plan}`（键 = 文件相对路径，值 = `{text, exec}`，JSON 值相等即相同）；
//! 这台机器的事实用 `mcp_sync::{Facts, Live, There, is_abs_any}`。本模块只多出 skill 自己的那几条可疑项规则（[`suspects_of`]）。
//!
//! 一个字节都不写（写经 monitor → 那台后端 `files-put`，带 `expect`）· 不改写任何一个文件的内容。

use copy_core::copy_text;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::assets::mcp_sync::{self, Facts, State};

/// skill 可疑项的种类（线上名）。**闭集**：界面文案表按它逐键给字。
pub const SUSPECT_KINDS: &[&str] = &["abs-path", "binary", "command-missing", "executable"];

// 上限与目录摘要同两个数（`asset_catalog::SKILL_MAX_FILES` / `SKILL_MAX_FILE_BYTES`，不另起一份）：
// 文件数超了整趟拒（装一半的 skill 比不装更坏）；单个文件超了那一个 `text = null` ＋ `why`（装不过去，说出来）。
use crate::assets::asset_catalog::{
    SKILL_MAX_FILES as MAX_FILES, SKILL_MAX_FILE_BYTES as MAX_FILE_BYTES,
};
/// 一个文件里最多标这么多条绝对路径（再多就是噪音；`abs-path` 那几条已经足够让人去看）。
const MAX_PATHS_PER_FILE: usize = 8;

/// 这一族的应答。
pub type Answer = Result<Value, (&'static str, String)>;

/// skill 名：一段目录名（不许带分隔符 / `..` / 点开头 / NUL）。装记录的写口也用它（`skill_ledger::record_at`）。
pub(crate) fn valid_name(name: &str) -> bool {
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
    let name = args.get("name").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `name` (skill directory name)"),
    ))?;
    if !valid_name(name) {
        return Err((
            "bad_args",
            copy_text("beSkillInstall.read.badName", &[("name", name)]),
        ));
    }
    Ok(name.to_string())
}

/// skill 的根与目录：`args.project` 给了 ⇒ 那个项目里的（项目目录须是这台上的绝对路径）；没给 ⇒ 用户级。
fn skill_dir(args: &Value, name: &str) -> Result<(PathBuf, PathBuf), (&'static str, String)> {
    let project = match args.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(p)) => Some(crate::assets::mcp_edit::project_root(p)?),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`project` must be a string or null"),
            ))
        }
    };
    let root = super::asset_kind()
        .and_then(|k| crate::agents::skill_root_at(k, project.as_deref().map(Path::new)))
        .ok_or(("io_failed", copy_text("beSkillInstall.read.noRoot", &[])))?;
    let dir = root.join(name);
    Ok((root, dir))
}

/// 读一个文件：`(原文, 读不出原文的原因)`，两者恰有一个。
fn read_text(p: &Path) -> (Option<String>, Option<String>) {
    match crate::common::fs::read_regular_capped(p, MAX_FILE_BYTES) {
        Ok(b) if b.contains(&0) => (None, Some(copy_text("beSkillInstall.text.binary", &[]))),
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => (Some(t), None),
            Err(_) => (None, Some(copy_text("beSkillInstall.text.notUtf8", &[]))),
        },
        Err(e) => (None, Some(e.said_logging_raw())),
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
        let ent = ent.map_err(|e| {
            (
                "io_failed",
                copy_text(
                    "beSkillInstall.read.walkFailed",
                    &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
                ),
            )
        })?;
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
                copy_text(
                    "beSkillInstall.read.tooMany",
                    &[
                        ("dir", &dir.display().to_string()),
                        ("max", &MAX_FILES.to_string()),
                    ],
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
        None => skill_dir(args, &name)?,
    };
    if !std::fs::metadata(&dir).is_ok_and(|m| m.is_dir()) {
        return Err((
            "not_found",
            copy_text(
                "beSkillInstall.read.notFound",
                &[("name", &name), ("dir", &dir.display().to_string())],
            ),
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
                // 读执行位是平台原语：借插件口那一份（`plugin::discover::is_executable`，非 unix 上恒 false）。
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
        crate::common::contract::malformed("missing `source` or it is not an array"),
    ))?;
    if arr.len() > MAX_FILES {
        return Err((
            "too_large",
            copy_text(
                "beSkillInstall.install.tooMany",
                &[("max", &MAX_FILES.to_string())],
            ),
        ));
    }
    let mut out = BTreeMap::new();
    for f in arr {
        let path = f
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| valid_rel(p))
            .ok_or((
                "bad_args",
                crate::common::contract::malformed(
                    "a file `path` is missing or not a relative path inside the skill",
                ),
            ))?;
        let text = match f.get("text") {
            Some(Value::String(t)) => Some(t.clone()),
            Some(Value::Null) => None,
            _ => {
                return Err((
                    "bad_args",
                    crate::common::contract::malformed(&format!(
                        "`text` of {path:?} must be a string or null"
                    )),
                ))
            }
        };
        let exec = f.get("exec").and_then(Value::as_bool).unwrap_or(false);
        if out
            .insert(path.to_string(), SourceFile { text, exec })
            .is_some()
        {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!("{path:?} given twice")),
            ));
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
        None => skill_dir(args, &name)?,
    };
    let source = source_arg(args)?;
    let take = mcp_sync::names_arg(args.get("take"), "take")?;
    let overwrite = mcp_sync::names_arg(args.get("overwrite"), "overwrite")?;
    if take.is_none() && overwrite.is_some() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`overwrite` given without `take`"),
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
            here.insert(
                rel.clone(),
                json!({ "unreadable": copy_text("beSkillInstall.plan.notRegular", &[]) }),
            );
            unwritable.insert(rel, copy_text("beSkillInstall.plan.notRegular", &[]));
        }
    }
    let there: Map<String, Value> = source
        .iter()
        .map(|(p, f)| match &f.text {
            Some(t) => (p.clone(), json!({ "text": t, "exec": f.exec })),
            None => (p.clone(), json!({ "unreadable": "来源那台读不出原文" })),
        })
        .collect();
    // 差异（键 = 相对路径；值相等才算相同）。
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
                    copy_text("beSkillInstall.install.sourceUnreadable", &[("p", p)]),
                ));
            }
            if let Some(p) = t.iter().find(|p| unwritable.contains_key(*p)) {
                return Err((
                    "bad_file",
                    copy_text("beSkillInstall.install.targetNotText", &[("p", p)]),
                ));
            }
            Some(mcp_sync::plan(&rows, &t, &overwrite.unwrap_or_default())?)
        }
    };
    // 真要写的那几个各自「装时写进去的那一份」的摘要 ＋ 装之前在不在（`new` ⇒ 新建；`differs` ⇒ 盖掉原有的）。
    //   monitor 写完把真写成了的那几个原样交回 `skill-install-record`（摘要与新旧是**这台**判的，不由 monitor 算）。
    let ledger = write.as_ref().map(|w| {
        let states: BTreeMap<&str, State> = rows.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        w.iter()
            .filter_map(|p| {
                let text = source.get(p)?.text.as_deref()?;
                Some((
                    p.clone(),
                    json!({
                        "digest": crate::assets::skill_ledger::digest_of(text),
                        "created": states.get(p.as_str()) == Some(&State::New),
                    }),
                ))
            })
            .collect::<Map<String, Value>>()
    });
    // 只交这一趟拷的那几个路径在这台上的原文（写的时候当 CAS 期望）；只在这台有的那几个不碰，也不回传。
    // 写的落点：`files-put` 的 `root` 必须已在 ⇒ skill 根在就用它，不在就用它的上一层（配置根）、让 `parents` 建出来。
    // 项目级那一形往上最多两层（`<项目>/.claude/skills` 的 `.claude` 也可能还没有）。
    let base = if root.is_dir() {
        root.clone()
    } else {
        root.ancestors()
            .skip(1)
            .take(2)
            .find(|p| p.is_dir())
            .map(Path::to_path_buf)
            .ok_or((
                "io_failed",
                copy_text(
                    "beSkillInstall.install.noRoot",
                    &[("dir", &root.display().to_string())],
                ),
            ))?
    };
    let prefix = dir
        .strip_prefix(&base)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| name.clone());
    let target: Vec<Value> = here_text
        .iter()
        .filter(|(p, _)| source.contains_key(*p))
        .map(|(p, t)| json!({ "path": p, "text": t }))
        .collect();
    Ok(json!({
        "root": root.display().to_string(),
        "dir": dir.display().to_string(),
        "base": base.display().to_string(),
        "prefix": prefix,
        "rows": rows_json,
        "target": target,
        "write": write,
        "ledger": ledger,
    }))
}

// ═══════════════════════ 卸 ═══════════════════════
//
// 卸只删装记录（`skill_ledger.rs`）里那几个文件；装完用户自己改过的先问。两条只读命令，判定都在**被卸的那一台**：
// `skill-installs`（这台记着哪几个从别处装来的 skill）· `skill-uninstall-plan`（逐文件四态 ＋ 要不要问 ＋ 给了 `take` 才答删哪几个）。
// 一个字节都不写：删经 monitor → 这台后端 `files-delete`（`expect` = 这里回的 `seen` 那一份）；摘记录经 `skill-install-record`。

/// 卸时一个记着的文件在盘上的样子（线上名）。**闭集**：界面文案表按它逐键给字。
// ⚠ 逐字写成字面量（不引用下面四个常量）：界面与 monitor 的判据按「这一行的引号」从源码现抠人群。
//   四个常量与这一行对不上由 `uninstall_judges_every_recorded_file_as_it_is_on_disk_now` 的闭集两向那一条逮。
#[cfg(test)]
pub const UNINSTALL_STATES: &[&str] = &["gone", "intact", "modified", "unreadable"];
/// 已经不在了（从记录里摘掉就行）。
pub const UNINSTALL_GONE: &str = "gone";
/// 在、是文本、摘要 == 装时写进去的那一份。
pub const UNINSTALL_INTACT: &str = "intact";
/// 在、摘要不同 —— 装完被改过（删之前要问）。
pub const UNINSTALL_MODIFIED: &str = "modified";
/// 不是普通文件 / 不是文本 / 读不出来 —— 没法按原文 CAS 删（不删）。
pub const UNINSTALL_UNREADABLE: &str = "unreadable";

/// 一个记着的文件现在的样子：`(state, 现有原文)`（原文只在 `intact` / `modified` 上有）。
fn on_disk_now(
    abs: &Path,
    recorded: &crate::assets::skill_ledger::Recorded,
) -> (&'static str, Option<String>) {
    // ⚠ `metadata` 跟链接（`symlink_metadata` 不在本模块能用的只读动词里，`readonly_guard` 白名单）：
    //   装完被换成指向一份同样文本的链接 ⇒ 这里判 `intact`，而后端 `files-delete` 带 `expect` 只删普通文件 ⇒ 那一下拒、什么都不删。
    //   悬空的链接 ⇒ `gone`（从记录里摘掉；链接本身不是装写进去的，不碰）。
    match std::fs::metadata(abs) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (UNINSTALL_GONE, None),
        Err(_) => (UNINSTALL_UNREADABLE, None),
        // 目录 / 特殊文件：装写进去的是一份普通文件，现在换了种类 ⇒ 不按原文删它。
        Ok(m) if !m.is_file() => (UNINSTALL_UNREADABLE, None),
        Ok(_) => match read_text(abs) {
            (Some(t), _) if crate::assets::skill_ledger::digest_of(&t) == recorded.digest => {
                (UNINSTALL_INTACT, Some(t))
            }
            (Some(t), _) => (UNINSTALL_MODIFIED, Some(t)),
            (None, _) => (UNINSTALL_UNREADABLE, None),
        },
    }
}

/// 卸的判定（被卸的那一台跑；`ext-uninstall-*` 经它判）：记录文件由调用方给。
pub fn answer_uninstall_plan_at(ledger: &Path, args: &Value) -> Answer {
    let dir = args.get("dir").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `dir`"),
    ))?;
    let take = mcp_sync::names_arg(args.get("take"), "take")?;
    let confirm = mcp_sync::names_arg(args.get("confirm"), "confirm")?;
    if take.is_none() && confirm.is_some() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`confirm` given without `take`"),
        ));
    }
    let l = crate::assets::skill_ledger::load_at(ledger)?;
    let install = l.installs.get(dir).ok_or((
        "not_found",
        copy_text(
            "beSkillInstall.uninstall.notRecorded",
            &[("dir", &dir.to_string())],
        ),
    ))?;
    let base = Path::new(dir);
    let mut rows = Vec::new();
    let mut seen = Vec::new();
    // (state, deletable, ask)，按路径。
    let mut judged: BTreeMap<&str, (&'static str, bool, bool)> = BTreeMap::new();
    for (path, rec) in &install.files {
        let (state, text) = on_disk_now(&base.join(path), rec);
        let deletable = state == UNINSTALL_INTACT || state == UNINSTALL_MODIFIED;
        // 要问：装完被改过的 · 装之前就在（装时盖掉了原有那一份，删了回不到装之前）。
        let ask = deletable && (state == UNINSTALL_MODIFIED || !rec.created);
        judged.insert(path, (state, deletable, ask));
        rows.push(json!({
            "path": path,
            "state": state,
            "created": rec.created,
            "deletable": deletable,
            "ask": ask,
        }));
        if let Some(t) = text {
            seen.push(json!({ "path": path, "text": t }));
        }
    }
    let (delete, forget) = match take {
        None => (Value::Null, Value::Null),
        Some(take) => {
            let confirm = confirm.unwrap_or_default();
            if let Some(p) = confirm.iter().find(|p| !take.contains(*p)) {
                return Err((
                    "bad_args",
                    crate::common::contract::malformed(&format!(
                        "{p:?} is in `confirm` but not in `take`"
                    )),
                ));
            }
            for p in &take {
                match judged.get(p.as_str()) {
                    None => {
                        return Err((
                            "bad_args",
                            copy_text("beSkillInstall.uninstall.notInLedger", &[("p", p)]),
                        ))
                    }
                    Some((state, false, _)) => {
                        return Err((
                            "bad_args",
                            copy_text(
                                "beSkillInstall.uninstall.badState",
                                &[("p", p), ("state", &state.to_string())],
                            ),
                        ))
                    }
                    Some((_, true, true)) if !confirm.contains(p) => {
                        return Err((
                            "needs_consent",
                            copy_text("beSkillInstall.uninstall.needsConsent", &[("p", p)]),
                        ))
                    }
                    Some(_) => {}
                }
            }
            let forget: Vec<&str> = judged
                .iter()
                .filter(|(_, (s, _, _))| *s == UNINSTALL_GONE)
                .map(|(p, _)| *p)
                .collect();
            (json!(take), json!(forget))
        }
    };
    Ok(json!({
        "dir": dir,
        "name": install.name,
        "rows": rows,
        "seen": seen,
        "delete": delete,
        "forget": forget,
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_install_tests.rs"]
mod tests;
