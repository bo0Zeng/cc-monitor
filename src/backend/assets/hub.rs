//! 〔MIG-3a · `设计/01 §3.5` · 主会话 09-28 裁〕**两台之间那几件的枢纽**：MCP 推 / 拉 · skill 装到这台。
//!
//! # 裁决（主会话 09-28，DOCS-C 查出）
//!
//! 前半合进去的形状是「界面先问来源那台拿原文（`mcp-sync-source` / `skill-read`），再把原文递给被写那台」——
//! 那是**经前端中继**，撞 `01 §3.5`「不是经前端中继」。改成设计那形：界面只 `call(<local>, …)` 一次（带 `from` / `to`），
//! **本机常驻后端当枢纽**：向来源那台取、向被写那台写（被写那台照旧自己判 CAS、`stale` 就停）。
//! 本机那一跳就是「不走 ssh 的远端」—— 同一条内层命令，本机经 [`Here`]（本进程 `REGISTRY` 的 `run`），远端经
//! `remote_ask`（池里那条 SSH 上多开一个 capture，跑那台 CLI 面的同名子命令）。
//!
//! ```text
//!  界面 ── call(<local>, mcp-sync-hub-preview {from, fromDir, to, toDir}) ──▶ 本机后端（枢纽）
//!        ① from: mcp-sync-source {projectDir}          （本机 / 远端各走各的那一跳）
//!        ② to:   mcp-sync-preview {projectDir, source…} ──▶ 成品原样交回界面
//!  界面 ── call(<local>, mcp-sync-hub-apply {…, expectSource, target, take, overwrite})
//!        ① from 再取一次：与看差异时那份不同 ⇒ `stale`（一个字节不写）
//!        ② to:   mcp-sync-apply（被写那台判 CAS：`target` 是看差异时那份）
//! ```
//!
//! # `from` / `to` 的线上形
//!
//! 可达表的键（monitor 交来的那台的名字，本后端只当不透明的键），**`null` = 这台自己**。后端不认「本机」这个名字
//! （`<local>` 是 monitor 那一侧的具名 origin，界面在发之前换成 `null`）—— 于是这里没有第二份「本机叫什么」。
//!
//! # 诚实边界
//!
//! 远端那一跳失败时，那台 CLI 面信封里的 `code` 在 capture 那一层没保住（`remote_ask::pull_over` 只交原话）⇒ 枢纽回
//! `refused` ＋ 那台的原话（含被写那台的 `stale` 那句）。本机那一跳的码原样保留。界面对这几件只显示原话，不按码分流。

use copy_core::copy_text;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::remote_ask::{Remote, Table};

type Answer = Result<Value, (String, String)>;

/// 这台自己的那几条内层命令（生产 = `inbound.rs::LocalFrames`：本进程 `REGISTRY` 的 `run`，限那几条）。
pub(crate) trait Here: Send + Sync {
    fn ask(&self, cmd: &str, args: Value) -> Answer;
}

/// 枢纽要问的一台：`None` = 这台自己。
fn machine_of(args: &Value, k: &str) -> Result<Option<String>, (String, String)> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        _ => Err(bad(&format!(
            "`{k}` must be a machine key or null (= this backend)"
        ))),
    }
}

fn bad(msg: &str) -> (String, String) {
    (
        "bad_args".to_string(),
        crate::common::contract::malformed(msg),
    )
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (String, String)> {
    args.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing `{k}`")))
}

/// 问一台跑一条内层命令：这台自己 ⇒ [`Here`]（阻塞档，挪到阻塞线程上跑）；别的台 ⇒ 可达表 ＋ capture。
async fn ask_one(
    here: &Arc<dyn Here>,
    machine: Option<&str>,
    cmd: &'static str,
    args: Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    match machine {
        None => {
            let h = Arc::clone(here);
            tokio::task::spawn_blocking(move || h.ask(cmd, args))
                .await
                .map_err(|e| ("io_failed".to_string(), e.to_string()))?
        }
        Some(m) => {
            if !crate::remote_ask::lock(table).contains_key(m) {
                return Err((
                    "unreachable".to_string(),
                    crate::remote_ask::unreachable_message(m),
                ));
            }
            crate::remote_ask::ask_json(m, cmd, &args, table, remote)
                .await
                .map_err(|said| ("refused".to_string(), said))
        }
    }
}

/// `mcp-sync-hub-preview {from, fromDir, to, toDir}` → 被写那台 `mcp-sync-preview` 的成品（原样）。
pub(crate) async fn mcp_preview(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let (from, to) = (machine_of(args, "from")?, machine_of(args, "to")?);
    let (from_dir, to_dir) = (str_arg(args, "fromDir")?, str_arg(args, "toDir")?);
    let src = ask_one(
        here,
        from.as_deref(),
        "mcp-sync-source",
        json!({ "projectDir": from_dir }),
        table,
        remote,
    )
    .await?;
    ask_one(
        here,
        to.as_deref(),
        "mcp-sync-preview",
        json!({
            "projectDir": to_dir,
            "source": src.get("text").cloned().unwrap_or(Value::Null),
            "sourcePath": src.get("path").cloned().unwrap_or(Value::Null),
            "sameMachine": from == to,
        }),
        table,
        remote,
    )
    .await
}

/// `mcp-sync-hub-apply {from, fromDir, to, toDir, expectSource, target, take, overwrite}` → 被写那台 `mcp-sync-apply` 的成品。
/// 来源那份再取一次、与看差异时那份（`expectSource`）逐字比：不同 ⇒ `stale`、一个字节不写。
pub(crate) async fn mcp_apply(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let (from, to) = (machine_of(args, "from")?, machine_of(args, "to")?);
    let (from_dir, to_dir) = (str_arg(args, "fromDir")?, str_arg(args, "toDir")?);
    let expect = str_arg(args, "expectSource")?;
    let src = ask_one(
        here,
        from.as_deref(),
        "mcp-sync-source",
        json!({ "projectDir": from_dir }),
        table,
        remote,
    )
    .await?;
    if src.get("text").and_then(Value::as_str) != Some(expect) {
        return Err((
            "stale".to_string(),
            copy_text("beAssetsHub.source.changed", &[]),
        ));
    }
    ask_one(
        here,
        to.as_deref(),
        "mcp-sync-apply",
        json!({
            "projectDir": to_dir,
            "source": expect,
            "target": args.get("target").cloned().unwrap_or(Value::Null),
            "take": args.get("take").cloned().unwrap_or(Value::Null),
            "overwrite": args.get("overwrite").cloned().unwrap_or(Value::Null),
        }),
        table,
        remote,
    )
    .await
}

/// 来源那台 `skill-read` 的 `files` → 装那一跳要的三格（`path` · `text` · `exec`），按路径排好（比较用同一个形）。
fn source_files(read: &Value) -> Vec<Value> {
    let mut v: Vec<Value> = read
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| json!({ "path": f.get("path"), "text": f.get("text"), "exec": f.get("exec") }))
        .collect();
    v.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    v
}

/// `skill-install-hub-preview {from, to, name}` → `{dir, rows, target, source}`：被写那台判（`skill-install-plan`），
/// `source` 是来源那台读出来的那几份（界面照它显示、写的时候当期望原样送回）。
pub(crate) async fn skill_preview(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let (from, to) = (machine_of(args, "from")?, machine_of(args, "to")?);
    if from == to {
        return Err((
            "refused".to_string(),
            copy_text("beAssetsHub.skill.sameMachine", &[]),
        ));
    }
    let name = str_arg(args, "name")?;
    let read = ask_one(
        here,
        from.as_deref(),
        "skill-read",
        json!({ "name": name }),
        table,
        remote,
    )
    .await?;
    let files: Vec<Value> = read
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let source = source_files(&read);
    let plan = ask_one(
        here,
        to.as_deref(),
        "skill-install-plan",
        json!({ "name": name, "source": source }),
        table,
        remote,
    )
    .await?;
    Ok(json!({
        "dir": plan.get("dir").cloned().unwrap_or(Value::Null),
        "rows": plan.get("rows").cloned().unwrap_or(Value::Null),
        "target": plan.get("target").cloned().unwrap_or(Value::Null),
        "source": files,
    }))
}

/// `skill-install-hub-apply {from, to, name, expectSource, target, take, overwrite}` → 被写那台 `skill-install-apply` 的成品。
/// 来源那几份再读一次、与看差异时那几份（`expectSource`，`path` · `text` · `exec`）比：不同 ⇒ `stale`、一个字节不写。
pub(crate) async fn skill_apply(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let (from, to) = (machine_of(args, "from")?, machine_of(args, "to")?);
    let name = str_arg(args, "name")?;
    let read = ask_one(
        here,
        from.as_deref(),
        "skill-read",
        json!({ "name": name }),
        table,
        remote,
    )
    .await?;
    let fresh = source_files(&read);
    let expect =
        source_files(&json!({ "files": args.get("expectSource").cloned().unwrap_or(Value::Null) }));
    if fresh != expect {
        return Err((
            "stale".to_string(),
            copy_text("beAssetsHub.source.changed", &[]),
        ));
    }
    ask_one(
        here,
        to.as_deref(),
        "skill-install-apply",
        json!({
            "name": name,
            "source": fresh,
            "target": args.get("target").cloned().unwrap_or(Value::Null),
            "take": args.get("take").cloned().unwrap_or(Value::Null),
            "overwrite": args.get("overwrite").cloned().unwrap_or(Value::Null),
        }),
        table,
        remote,
    )
    .await
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/hub_tests.rs"]
mod tests;
