//! 计划读面（`plan-*`）的帧面宿主：把这台的事实拼成 [`crate::plan`] 要的几个口 ——
//! pb 装在哪（各配置根底下的插件目录，问注册表）· 会话在哪些目录里（会话表）· 一个 id 是哪个会话（会话表 ＋ 记录树）。
//!
//! 命令：
//! - `plan-list {dirs?, fresh?}`：这台的工作区与片。目录 ＝ 此刻活着的会话的工作目录 ∪ `dirs`；每个目录让 pb 自己往上找工作区
//!   （认过的目录不再问，`fresh: true` 全部重问）。
//! - `plan-read {workspace}`：一个工作区的成品（[`crate::plan::product`]，顶上带 `rev` · `readAt` · `stale`）。
//! - `plan-cell-view {workspace, slice, id}`：一格的 `agent_view`（上一次读好的那一份里的）。
//!
//! 读过的工作区就开始盯（[`crate::plan::watch`]），变了推 `plan_changed`。

use crate::plan::{book, locate, watch, Live, Whose};
use crate::stream::inbound::spec::Fail;
use copy_core::copy_text;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Answer = Result<Value, Fail>;

fn bad(what: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(what))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 这台的配置根：默认那一个 ＋ 账号库里每个号的配置目录。
fn config_roots() -> Vec<PathBuf> {
    let mut roots = vec![crate::observe::history_query::agent_home()];
    if let Some(home) = crate::platform::paths::home_dir() {
        roots.extend(
            crate::faces::rotation_face::library_at(&home)
                .accounts
                .into_iter()
                .filter_map(|a| a.dir),
        );
    }
    roots.dedup();
    roots
}

/// pb 的入口脚本；没装 ⇒ `no_pb`。
fn entry() -> Result<PathBuf, Fail> {
    locate::entry_among(&crate::agents::plugins(&config_roots()))
        .ok_or_else(|| Fail::new("no_pb", copy_text("bePlan.face.noPb", &[])))
}

/// 生产的「这个 id 是谁」：活会话表（判活与在干什么同主窗口标签页）· 记录树里有它的记录 ⇒ 会话；
/// 记录树里有它的子 agent 记录 ⇒ 挂在父会话底下（活不活看父会话）；都不中 ⇒ 认不出。
fn who_port() -> impl Fn(&str) -> Whose {
    let home = crate::observe::history_query::agent_home();
    let root = crate::agents::records_root(&home);
    let doing = crate::observe::accounts_query::live_doing(&home);
    let live = move |sid: &str| {
        doing.get(sid).map(|d| Live {
            activity: d.activity,
            needs: d.needs,
        })
    };
    let memo = std::sync::Mutex::new(BTreeMap::<String, Whose>::new());
    move |id: &str| {
        if let Some(w) = memo.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            return w.clone();
        }
        let w = if let Some(l) = live(id) {
            Whose::Session {
                sid: id.to_string(),
                live: Some(l),
            }
        } else if let Some(r) = root.as_deref() {
            if crate::agents::find_session_file(r, id).is_ok() {
                Whose::Session {
                    sid: id.to_string(),
                    live: None,
                }
            } else if let Some(parent) = crate::agents::child_parent_sid(r, id) {
                let l = live(&parent);
                Whose::Subagent { parent, live: l }
            } else {
                Whose::Unknown
            }
        } else {
            Whose::Unknown
        };
        memo.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), w.clone());
        w
    }
}

/// 读一个目录（并进本子）；读到了就开始盯它的工作区。
fn read_dir(entry: &Path, dir: &Path) -> Result<Value, book::Miss> {
    let who = who_port();
    let mut out = book::book().read(entry, dir, &who, now_ms())?;
    if let Some(ws) = out
        .get("workspace")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    {
        // pb 的输出随当前目录变（agent_view 里的路径是相对当前目录的）⇒ 一律以工作区根为准再读一次，
        // 摘要才与盯盘那一路（当前目录 ＝ 工作区根）对得上，不会凭空推一帧。
        if ws.as_path() != dir {
            out = book::book().read(entry, &ws, &who, now_ms())?;
        }
        arm(entry, &ws);
    }
    Ok(out)
}

fn arm(entry: &Path, ws: &Path) {
    let e = entry.to_path_buf();
    let reread: watch::Reread = std::sync::Arc::new(move |p: &Path| {
        let who = who_port();
        book::book()
            .read(&e, p, &who, now_ms())
            .ok()
            .and_then(|v| v.get("rev").and_then(Value::as_str).map(str::to_string))
    });
    watch::arm(ws, book::book().rev(&ws.to_string_lossy()), reread);
}

fn miss(m: book::Miss) -> Fail {
    match m {
        book::Miss::NotWorkspace(said) => {
            Fail::new("not_workspace", copy_text("bePlan.face.notWorkspace", &[]))
                .with_raw(Some(&said))
        }
        book::Miss::Unsupported(said) => Fail::new("pb_unsupported", said),
        book::Miss::Failed { said, raw } => Fail::new("failed", said).with_raw(raw.as_deref()),
    }
}

/// 帧面入口：命令名从 `r.cmd` 来。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    match cmd {
        "plan-list" => list(args),
        "plan-read" => {
            let ws = args
                .get("workspace")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("missing `workspace` (a string)"))?;
            let entry = entry()?;
            read_dir(&entry, Path::new(ws)).map_err(miss)
        }
        "plan-cell-view" => {
            let get = |k: &str| {
                args.get(k)
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad(&format!("missing `{k}` (a string)")))
            };
            let (ws, slice, id) = (get("workspace")?, get("slice")?, get("id")?);
            book::book()
                .view(ws, slice, id)
                .map(|v| json!({ "view": v }))
                .ok_or_else(|| Fail::new("no_view", copy_text("bePlan.face.noView", &[])))
        }
        other => Err(Fail::new(
            "bad_args",
            crate::common::contract::malformed(&format!("unknown command `{other}`")),
        )),
    }
}

fn list(args: &Value) -> Answer {
    let fresh = args.get("fresh").and_then(Value::as_bool).unwrap_or(false);
    let mut dirs: Vec<String> =
        crate::observe::accounts_query::live_cwds(&crate::observe::history_query::agent_home());
    if let Some(a) = args.get("dirs") {
        let a = a
            .as_array()
            .ok_or_else(|| bad("`dirs` must be an array of strings"))?;
        for d in a {
            dirs.push(
                d.as_str()
                    .ok_or_else(|| bad("`dirs` must be an array of strings"))?
                    .to_string(),
            );
        }
    }
    dirs.sort();
    dirs.dedup();
    let entry = match entry() {
        Ok(e) => e,
        Err(f) => {
            return Ok(json!({ "pb": { "state": "missing", "said": f.message }, "workspaces": [] }))
        }
    };
    let b = book::book();
    let mut seen: BTreeMap<String, Value> = BTreeMap::new();
    let mut pb = json!({ "state": "ok", "said": null });
    for d in &dirs {
        let dir = Path::new(d);
        let cached = match b.known_dir(dir) {
            Some(None) if !fresh => continue,
            Some(Some(ws)) if !fresh => {
                if seen.contains_key(&ws) {
                    continue;
                }
                b.last(&ws)
            }
            _ => None,
        };
        let got = match cached {
            Some(v) => Ok(v),
            None => read_dir(&entry, dir),
        };
        match got {
            Ok(v) => {
                let ws = v
                    .get("workspace")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                seen.entry(ws).or_insert(v);
            }
            Err(book::Miss::NotWorkspace(_)) => {}
            Err(book::Miss::Unsupported(said)) => {
                pb = json!({ "state": "unsupported", "said": said })
            }
            Err(book::Miss::Failed { .. }) => {}
        }
    }
    let workspaces: Vec<Value> = seen
        .values()
        .map(|v| {
            json!({
                "workspace": v["workspace"],
                "repo": v["repo"],
                "auto": v["auto"],
                "rev": v["rev"],
                "stale": v["stale"],
                "slices": v["slices"].as_array().map(|a| a.iter().map(crate::plan::product::slice_summary).collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect();
    if let Some(v) = seen.values().next() {
        pb["version"] = v["pb"].clone();
    }
    Ok(json!({ "pb": pb, "workspaces": workspaces }))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_face_tests.rs"]
mod tests;
