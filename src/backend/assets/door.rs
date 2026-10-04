//! 资产域够用户文件的那一扇门：**这台后端自己的文件管理面**。
//!
//! 从前这一层住 monitor（`user_files.rs`：读 → 算 → 经那台后端 `files-peek` / `files-put` 交写）。
//! D 组的计算进了后端之后，算的与写的是同一台后端 ⇒ 门就是本进程里那几条 `files-*` 帧命令本身
//! （实现住 `stream/inbound/doors.rs::LocalFiles`：`readonly_guard` 第三层只许 `inbound.rs` 够得着写面）。
//! 写的规则（CAS · 相同不写 · 备份 · 暂存旁名换名上位 · 回读比对 · 回滚）仍只有 `files_write::put_text` 那一份。

use copy_core::copy_text;
use serde_json::{json, Value};

/// 一次 `files-peek` 读回来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peeked {
    /// 读的是哪一份（解过链接的那一个，给人看）。
    pub path: String,
    /// `None` = 确定不存在（「读不出来」是 `Err`）。
    pub text: Option<String>,
}

/// 一次 `files-put` 真写了之后的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Landed {
    pub path: String,
    pub changed: bool,
    pub created: bool,
    pub backup: Option<String>,
}

/// 一次写没成：`stale` 与别的分得开（前者该重读重算，后者原话交给用户）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refused {
    Stale(String),
    Peer { code: String, said: String },
}

impl Refused {
    pub(crate) fn said(self) -> String {
        match self {
            Refused::Stale(s) | Refused::Peer { said: s, .. } => s,
        }
    }
}

/// 这台后端的文件管理面：一条 `files-*` 帧命令 → 它的应答。生产 = `stream/inbound/doors.rs::LocalFiles`；判据用临时目录上的同一份。
pub(crate) trait Door {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)>;
}

fn refused((code, said): (String, String)) -> Refused {
    if code == "stale" {
        Refused::Stale(said)
    } else {
        Refused::Peer { code, said }
    }
}

/// 应答里的路径（字符串或 `{"b16": …}`）→ 给人看的一行。
fn path_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other
            .get("b16")
            .and_then(Value::as_str)
            .and_then(|h| {
                (0..h.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                    .collect::<Option<Vec<u8>>>()
            })
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default(),
    }
}

fn path_of(v: &Value) -> String {
    path_text(v.get("path").unwrap_or(&Value::Null))
}

pub(crate) fn home(d: &dyn Door) -> Result<String, String> {
    let v = d
        .ask("files-home", json!({}))
        .map_err(|e| refused(e).said())?;
    let p = path_of(&v);
    if p.is_empty() {
        return Err(copy_text("beAssets.door.homeUnknown", &[]));
    }
    Ok(p)
}

pub(crate) fn peek(d: &dyn Door, root: &str, rel: &str) -> Result<Peeked, String> {
    let v = d
        .ask("files-peek", json!({ "root": root, "rel": rel }))
        .map_err(|e| refused(e).said())?;
    Ok(Peeked {
        path: path_of(&v),
        text: v.get("text").and_then(Value::as_str).map(str::to_string),
    })
}

pub(crate) fn put(
    d: &dyn Door,
    root: &str,
    rel: &str,
    content: &str,
    expect: Option<&str>,
    backup: bool,
    parents: bool,
) -> Result<Landed, Refused> {
    let v = d
        .ask(
            "files-put",
            json!({ "root": root, "rel": rel, "content": content, "expect": expect, "backup": backup, "parents": parents }),
        )
        .map_err(refused)?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    Ok(Landed {
        path: path_of(&v),
        changed: flag("changed"),
        created: flag("created"),
        backup: v.get("backup").filter(|b| !b.is_null()).map(path_text),
    })
}

/// 删一个文件，带 CAS（`expect` = 读到的那一份）；不等 / 已经不在 ⇒ `Stale`。
pub(crate) fn delete(d: &dyn Door, root: &str, rel: &str, expect: &str) -> Result<(), Refused> {
    d.ask(
        "files-delete",
        json!({ "root": root, "rel": rel, "expect": expect }),
    )
    .map(|_| ())
    .map_err(refused)
}

/// 只删一个空目录（`expect: {"empty_dir": true}`）；不空 / 已经不在 ⇒ `Stale`。
pub(crate) fn delete_empty_dir(d: &dyn Door, root: &str, rel: &str) -> Result<(), Refused> {
    d.ask(
        "files-delete",
        json!({ "root": root, "rel": rel, "expect": { "empty_dir": true } }),
    )
    .map(|_| ())
    .map_err(refused)
}

/// 删一项（链接删链接本身；`recursive` ⇒ 整棵目录，逐条目过路径解析）。
pub(crate) fn remove(d: &dyn Door, root: &str, rel: &str, recursive: bool) -> Result<(), String> {
    d.ask(
        "files-delete",
        json!({ "root": root, "rel": rel, "recursive": recursive }),
    )
    .map(|_| ())
    .map_err(|e| refused(e).said())
}

/// 同根内复制一份（`files-copy`：目标已在 ⇒ 拒；`recursive` ⇒ 整棵目录，链接照原样复制成链接；权限位从源抄）。
pub(crate) fn copy(
    d: &dyn Door,
    root: &str,
    from: &str,
    to: &str,
    recursive: bool,
) -> Result<(), String> {
    d.ask(
        "files-copy",
        json!({ "root": root, "from": from, "to": to, "recursive": recursive }),
    )
    .map(|_| ())
    .map_err(|e| refused(e).said())
}

/// 建一层目录（`files-mkdir`，父目录要在）。
pub(crate) fn mkdir(d: &dyn Door, root: &str, rel: &str) -> Result<(), String> {
    d.ask("files-mkdir", json!({ "root": root, "rel": rel }))
        .map(|_| ())
        .map_err(|e| refused(e).said())
}

/// 建一条链接（`files-link`：目标文本原样，链接那条路径过根底下的解析；已在 ⇒ 拒）。
pub(crate) fn link(d: &dyn Door, root: &str, rel: &str, target: &str) -> Result<(), String> {
    d.ask(
        "files-link",
        json!({ "root": root, "rel": rel, "target": target }),
    )
    .map(|_| ())
    .map_err(|e| refused(e).said())
}

/// 改名（同一个根底下，`files-rename`）。
pub(crate) fn rename(d: &dyn Door, root: &str, from: &str, to: &str) -> Result<(), String> {
    d.ask(
        "files-rename",
        json!({ "root": root, "from": from, "to": to }),
    )
    .map(|_| ())
    .map_err(|e| refused(e).said())
}

pub(crate) fn chmod(d: &dyn Door, root: &str, rel: &str, mode: u32) -> Result<(), String> {
    d.ask(
        "files-chmod",
        json!({ "root": root, "rel": rel, "mode": mode }),
    )
    .map(|_| ())
    .map_err(|e| refused(e).said())
}

/// 一个路径在不在：在 ⇒ `Some(kind)`；答「读不到」⇒ `None`（权限不够也是 `None`，只给「在不在场」那种展示用）。
pub(crate) fn stat_kind(d: &dyn Door, path: &str) -> Result<Option<String>, String> {
    match d.ask("files-stat", json!({ "path": path })) {
        Ok(v) => Ok(Some(
            v.get("kind")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        )),
        Err((code, _)) if code == "unreadable" => Ok(None),
        Err(e) => Err(refused(e).said()),
    }
}

/// 读改写一次最多重来几趟（`stale` 才重来：读与写之间盘上那份被别人改了）。
pub(crate) const EDIT_ATTEMPTS: usize = 3;

/// 一次 [`edit`] 的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Edited {
    /// 规划说「没事可做」，或算出来与盘上逐字相同 ⇒ 一个字节没写。
    Unchanged,
    Written(Landed),
}

/// 读 → 算 → 交。`plan` 拿到读到的全文（`None` = 不存在），回 `Some(新全文)` 或 `None`（没事可做）；
/// `stale` ⇒ 重读重算，最多 [`EDIT_ATTEMPTS`] 趟。
pub(crate) fn edit(
    d: &dyn Door,
    root: &str,
    rel: &str,
    backup: bool,
    parents: bool,
    mut plan: impl FnMut(Option<&str>) -> Result<Option<String>, String>,
) -> Result<Edited, String> {
    let mut last = String::new();
    for _ in 0..EDIT_ATTEMPTS {
        let got = match peek(d, root, rel) {
            Ok(p) => p,
            // `files-peek` 先解父目录：父目录还不在时它报错 —— 而那一形就是「这份文件不在」。
            //   要逐级补目录（`parents`）的那一种写，这时按「不存在」算（`$PROFILE` 所在目录常常要装的时候才建）；
            //   父目录在、却读不了 ⇒ 原话交出去。
            Err(_) if parents && parent_absent(d, root, rel)? => Peeked {
                path: join_under(root, rel),
                text: None,
            },
            Err(said) => return Err(said),
        };
        let Some(next) = plan(got.text.as_deref())? else {
            return Ok(Edited::Unchanged);
        };
        if got.text.as_deref() == Some(next.as_str()) {
            return Ok(Edited::Unchanged);
        }
        match put(d, root, rel, &next, got.text.as_deref(), backup, parents) {
            Ok(landed) if landed.changed => return Ok(Edited::Written(landed)),
            Ok(_) => return Ok(Edited::Unchanged),
            Err(Refused::Stale(s)) => last = s,
            Err(e) => return Err(e.said()),
        }
    }
    Err(copy_text(
        "beAssets.door.editGaveUp",
        &[("last", &last), ("attempts", &EDIT_ATTEMPTS.to_string())],
    ))
}

/// `root/rel` 的父目录是不是确定不在（`files-stat` 答「读不到」）。
fn parent_absent(d: &dyn Door, root: &str, rel: &str) -> Result<bool, String> {
    let rel = rel.replace('\\', "/");
    let parent = match rel.rsplit_once('/') {
        Some((p, _)) => join_under(root, p),
        None => root.to_string(),
    };
    Ok(stat_kind(d, &parent)?.is_none())
}

/// `abs` 在 `home` 底下的那一段（字符串算法，分隔符两种都认）。不在 home 底下 ⇒ 拒。
pub(crate) fn rel_under(home: &str, abs: &str) -> Result<String, String> {
    let norm = |s: &str| s.replace('\\', "/");
    let (h, a) = (norm(home), norm(abs));
    let h = h.trim_end_matches('/');
    a.strip_prefix(h)
        .and_then(|r| r.strip_prefix('/'))
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .ok_or_else(|| copy_text("beAssets.door.outsideHome", &[("abs", abs), ("home", home)]))
}

/// [`rel_under`] 的反方向。住址下沉到 `platform::paths`（别名方言列 rc 候选也要它，适配层不往上依赖）。
pub(crate) use crate::platform::paths::join_under;
