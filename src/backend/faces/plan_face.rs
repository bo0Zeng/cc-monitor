//! 计划读面（`plan-*`）的帧面宿主：把这台的事实拼成 [`crate::plan`] 要的几个口 ——
//! pb 装在哪（各配置根底下的插件目录，问注册表）· 会话在哪些目录里（会话表）· 一个 id 是哪个会话（会话表 ＋ 记录树）。
//!
//! 命令：
//! - `plan-list {dirs?, fresh?}`：这台的工作区与片。目录 ＝ 此刻活着的会话的工作目录 ∪ `dirs`；每个目录让 pb 自己往上找工作区
//!   （认过的目录不再问，`fresh: true` 全部重问）。
//! - `plan-read {workspace}`：一个工作区的成品（[`crate::plan::product`]，顶上带 `rev` · `readAt` · `stale`）。
//! - `plan-cell-view {workspace, slice, id}`：一格的 `agent_view`（上一次读好的那一份里的）。
//! - `plan-command {workspace, cmd: continue|pause|view}`：以人的身份代敲 pb 的用户命令（写盘的是 pb）。
//! - `plan-files {dir}`：文件窗口反查（[`crate::plan::owners`]）：这个目录落在哪一片的仓库里、每份文件归哪一格。
//!
//! 读过的工作区就开始盯（[`crate::plan::watch`]），变了推 `changed {plan}`。

use crate::plan::{book, locate, watch, wire, Live, Whose};
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
    crate::common::time::now()
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

/// 此刻的认可与退回记录（读不懂 ⇒ 当什么都没记）。
fn review_now() -> crate::plan::review::Review {
    crate::plan::review::current(crate::plan::review::review_path().as_deref())
}

/// 给本子出的成品标上认可 · 要你看的数 · 退回的状态（每次答之前现读那份记录，本子里存的是没标的）。不写时刻的字（[`annotated`] 写）。
pub(crate) fn marked(mut doc: Value) -> Value {
    review_now().annotate(&mut doc);
    doc
}

/// [`marked`] ＋ 时刻写成给人看的字（按请求带来的看的那一台的时区）：要交出去的那一份走它。
pub(crate) fn annotated(doc: Value, tz: &crate::Tz) -> Value {
    let mut doc = marked(doc);
    crate::plan::product::with_time_texts(
        &mut doc,
        i64::try_from(now_ms()).unwrap_or(i64::MAX),
        tz,
    );
    doc
}

/// 读一个目录（并进本子）；读到了就开始盯它的工作区。回的是**没标**认可的那一份（[`annotated`] 再标）。
fn read_dir(entry: &Path, dir: &Path) -> Result<Value, book::Miss> {
    let who = who_port();
    let review = review_now();
    let prior = |ws: &str, slice: &str| review.prior(ws, slice);
    let mut out = book::book().read(entry, dir, &who, now_ms(), &prior)?;
    if let Some(ws) = out
        .get("workspace")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    {
        // pb 的输出随当前目录变（agent_view 里的路径是相对当前目录的）⇒ 一律以工作区根为准再读一次，
        // 摘要才与盯盘那一路（当前目录 ＝ 工作区根）对得上，不会凭空推一帧。
        if ws.as_path() != dir {
            out = book::book().read(entry, &ws, &who, now_ms(), &prior)?;
        }
        arm(entry, &ws);
    }
    Ok(out)
}

fn arm(entry: &Path, ws: &Path) {
    let e = entry.to_path_buf();
    let reread: watch::Reread = std::sync::Arc::new(move |p: &Path| {
        let who = who_port();
        let review = review_now();
        let prior = |ws: &str, slice: &str| review.prior(ws, slice);
        let mut v = book::book().read(&e, p, &who, now_ms(), &prior).ok()?;
        review.annotate(&mut v);
        seen_of(&v)
    });
    let last = book::book()
        .last(&ws.to_string_lossy())
        .map(marked)
        .as_ref()
        .and_then(seen_of);
    watch::arm(ws, last, reread);
}

/// 一份标过的成品此刻的样子（`changed {plan}` 比的那两样）。
pub(crate) fn seen_of(v: &Value) -> Option<watch::Seen> {
    Some((
        v.get("rev")?.as_str()?.to_string(),
        v.get("needCount").and_then(Value::as_u64).unwrap_or(0),
    ))
}

/// 现读一个工作区（起 pb）并标好；计划审面送退回之前用它拿此刻的接手与状态。
pub(crate) fn read_fresh(ws: &str, tz: &crate::Tz) -> Result<Value, Fail> {
    let entry = entry()?;
    read_dir(&entry, Path::new(ws))
        .map(|d| annotated(d, tz))
        .map_err(miss)
}

/// 出口过一遍线上类型（[`crate::plan::wire::checked`]）；对不上是拼的那一侧的错 ⇒ `failed`。
pub(crate) fn wired<T: serde::Serialize + serde::de::DeserializeOwned>(v: Value) -> Answer {
    wire::checked::<T>(v)
        .map_err(|e| Fail::new("failed", copy_text("bePlan.face.shape", &[])).with_raw(Some(&e)))
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
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：成品里的时刻字按它写。
pub(crate) fn answer(cmd: &str, args: &Value, tz: &crate::Tz) -> Answer {
    match cmd {
        "plan-list" => list(args, tz),
        "plan-read" => {
            let ws = args
                .get("workspace")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("missing `workspace` (a string)"))?;
            read_fresh(ws, tz).and_then(wired::<wire::PlanRead>)
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
        "plan-command" => command(args),
        "plan-files" => files(args, tz),
        other => Err(Fail::new(
            "bad_args",
            crate::common::contract::malformed(&format!("unknown command `{other}`")),
        )),
    }
}

/// `plan-command {workspace, cmd}`：以人的身份代敲 pb 的一条用户命令（[`crate::plan::command`]）。
/// 写盘的是 pb；`continue` / `pause` 改了工作区 `.env` ⇒ 盯盘那一路照常推 `changed {plan}`（`auto` 在成品里）。
fn command(args: &Value) -> Answer {
    let ws = args
        .get("workspace")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `workspace` (a string)"))?;
    let cmd = args
        .get("cmd")
        .and_then(Value::as_str)
        .filter(|c| crate::plan::command::VERBS.contains(c))
        .ok_or_else(|| bad("`cmd` must be one of continue · pause · view"))?;
    let entry = entry()?;
    crate::plan::command::run(&entry, Path::new(ws), cmd)
        .map_err(|(code, said, raw)| Fail::new(code, said).with_raw(raw.as_deref()))
        .and_then(wired::<wire::PlanCmdReply>)
}

/// `plan-files {dir}`：文件窗口反查（稿 06）。只看读好过的工作区（主窗口与需手动的账起来就问过 `plan-list`）——
/// 不为文件窗口走到的每个目录起一次 pb；目录在最深的那个工作区里才算。不在任何一片的仓库里 ⇒ `slice: null`（文件窗口什么都不多）。
fn files(args: &Value, tz: &crate::Tz) -> Answer {
    let dir = args
        .get("dir")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `dir` (a string)"))?;
    let b = book::book();
    let ws = b
        .workspaces()
        .into_iter()
        .filter(|w| {
            let w = w.trim_end_matches('/');
            dir.strip_prefix(w)
                .is_some_and(|r| r.is_empty() || r.starts_with('/'))
        })
        .max_by_key(String::len);
    let doc = ws
        .and_then(|w| b.last(w.as_str()))
        .map(|d| annotated(d, tz));
    let out = match doc {
        Some(d) => crate::plan::owners::of_dir(&d, dir),
        None => {
            json!({"workspace": null, "slice": null, "unreadable": null, "entries": [], "unowned": null})
        }
    };
    wired::<wire::PlanFiles>(out)
}

fn list(args: &Value, tz: &crate::Tz) -> Answer {
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
            return wired::<wire::PlanList>(
                json!({ "pb": { "state": "missing", "said": f.message }, "workspaces": [] }),
            )
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
        match got.map(|d| annotated(d, tz)) {
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
    let workspaces: Vec<Value> = seen.values().map(crate::plan::product::list_row).collect();
    if let Some(v) = seen.values().next() {
        pb["version"] = v["pb"].clone();
    }
    wired::<wire::PlanList>(json!({ "pb": pb, "workspaces": workspaces }))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_face_tests.rs"]
mod tests;
