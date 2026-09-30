//! **两台之间「装」那一件的枢纽**：`ext-hub-preview` / `ext-hub-apply`，skill 与 MCP 同一对命令。
//!
//! 界面只 `call(<local>, …)` 一次（带 `kind` · `name` · `from` / `to` · `scope`），**本机常驻后端当枢纽**：向来源那台取、
//! 交被写那台判与写（被写那台照旧自己判 CAS、`stale` 就停）。种类在这里分派到原来那两条内层路：
//! skill = `skill-read` → `skill-install-plan` / `skill-install-apply`；MCP = `mcp-sync-source` → `mcp-sync-preview` / `mcp-sync-apply`。
//! 本机那一跳就是「不走 ssh 的远端」—— 同一条内层命令，本机经 [`Here`]（本进程 `REGISTRY` 的 `run`），远端经
//! `remote_ask`（池里那条 SSH 上多开一个 capture，跑那台 CLI 面的同名子命令）。
//!
//! ```text
//!  界面 ── call(<local>, ext-hub-preview {kind, name, from, to, scope:{from, to}}) ──▶ 本机后端（枢纽）
//!        ① from: 读来源那一份（MCP：那一条，密钥值在来源那台就换成空位）
//!        ② to:   被写那台判 ──▶ 枢纽拼成确认卡（带两个记号：来源那一份 · 被写那台那一份）
//!  界面 ── call(<local>, ext-hub-apply {…同上, tokens, fill?})
//!        ① from 再取一次：记号对不上 ⇒ `stale`（一个字节不写）
//!        ② to:   再判一次、记号对不上 ⇒ `stale`；对得上才交被写那台写（它自己再 CAS 一次）
//! ```
//!
//! 记号只在后端比：界面拿到什么原样交回，不解读。
//!
//! # `from` / `to` 的线上形
//!
//! 可达表的键（monitor 交来的那台的名字，本后端只当不透明的键），**`null` = 这台自己**。后端不认「本机」这个名字
//! （`<local>` 是 monitor 那一侧的具名 origin，界面在发之前换成 `null`）—— 于是这里没有第二份「本机叫什么」。
//!
//! # 诚实边界
//!
//! 远端那一跳失败时码随原话一起交回（`remote_ask::Said`，〔主会话 09-28 裁〕别压成一个）：被写那台答 `stale` 就是 `stale`；
//! 读不出码的（拨号失败 · 链路断了 · 那台太旧）回 `unreachable` ＋ 原话。本机那一跳的码原样保留。

use copy_core::copy_text;
use serde_json::{json, Value};
use std::sync::Arc;

use super::ext::{token_of, ExtCard, ExtDone, ExtKind, ExtLoc, ExtSlot, ExtTokens};
use crate::stream::remote_ask::{Remote, Table};

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
            if !crate::stream::remote_ask::lock(table).contains_key(m) {
                return Err((
                    "unreachable".to_string(),
                    crate::stream::remote_ask::unreachable_message(m),
                ));
            }
            // 〔主会话 09-28 裁〕码随原话一起交回（`stale` / `refused` / …照那台的原码）；读不出码（拨号 / 链路坏了）⇒ `unreachable`。
            crate::stream::remote_ask::ask_json(m, cmd, &args, table, remote)
                .await
                .map_err(|s| {
                    (
                        s.code.unwrap_or_else(|| "unreachable".to_string()),
                        s.message,
                    )
                })
        }
    }
}

/// 给人看的那台的名字（`null` = 本机后端这一台）。
fn machine_name(m: Option<&str>) -> String {
    m.map(str::to_string)
        .unwrap_or_else(|| copy_text("beExt.machine.here", &[]))
}

/// `scope` 的两头。
fn scope_of(args: &Value) -> Result<(ExtLoc, ExtLoc), (String, String)> {
    let s = args.get("scope");
    let own = |e: (&'static str, String)| (e.0.to_string(), e.1);
    Ok((
        ExtLoc::from_arg(s.and_then(|s| s.get("from")), "scope.from").map_err(own)?,
        ExtLoc::from_arg(s.and_then(|s| s.get("to")), "scope.to").map_err(own)?,
    ))
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

/// 一趟「装」要的全部：两头 · 名字 · 种类。
struct Ask<'a> {
    kind: ExtKind,
    name: &'a str,
    from: Option<String>,
    to: Option<String>,
    at_from: ExtLoc,
    at_to: ExtLoc,
}

fn ask_of(args: &Value) -> Result<Ask<'_>, (String, String)> {
    let own = |e: (&'static str, String)| (e.0.to_string(), e.1);
    let kind = ExtKind::from_arg(args).map_err(own)?;
    let (from, to) = (machine_of(args, "from")?, machine_of(args, "to")?);
    let name = str_arg(args, "name")?;
    let (at_from, at_to) = scope_of(args)?;
    if from == to && at_from == at_to {
        return Err((
            "refused".to_string(),
            copy_text("beExt.card.sameMachine", &[]),
        ));
    }
    Ok(Ask {
        kind,
        name,
        from,
        to,
        at_from,
        at_to,
    })
}

fn project_arg(at: &ExtLoc) -> Value {
    at.project().map_or(Value::Null, |d| json!(d))
}

/// skill 两跳：来源读一趟、被写那台判一趟。回（来源那几份, 被写那台的判定）。
async fn skill_look(
    here: &Arc<dyn Here>,
    a: &Ask<'_>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<(Vec<Value>, Value), (String, String)> {
    let read = ask_one(
        here,
        a.from.as_deref(),
        "skill-read",
        json!({ "name": a.name, "project": project_arg(&a.at_from) }),
        table,
        remote,
    )
    .await?;
    let source = source_files(&read);
    let plan = ask_one(
        here,
        a.to.as_deref(),
        "skill-install-plan",
        json!({ "name": a.name, "source": source, "project": project_arg(&a.at_to) }),
        table,
        remote,
    )
    .await?;
    Ok((source, plan))
}

/// MCP 两跳：来源交那一条（空位）、被写那台判。
async fn mcp_look(
    here: &Arc<dyn Here>,
    a: &Ask<'_>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<(Value, Value), (String, String)> {
    let src = ask_one(
        here,
        a.from.as_deref(),
        "mcp-sync-source",
        json!({ "name": a.name, "at": a.at_from }),
        table,
        remote,
    )
    .await?;
    let pre = ask_one(
        here,
        a.to.as_deref(),
        "mcp-sync-preview",
        json!({ "name": a.name, "at": a.at_to, "def": src["def"] }),
        table,
        remote,
    )
    .await?;
    Ok((src, pre))
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// skill 的确认卡：写哪几个 · 可疑项 · 装不了的原因（来源读不出原文 · 这台那一份盖不了 · 非文本）。
fn skill_card(a: &Ask<'_>, source: &[Value], plan: &Value) -> ExtCard {
    let rows = plan["rows"].as_array().cloned().unwrap_or_default();
    let to = machine_name(a.to.as_deref());
    let writes: Vec<String> = rows
        .iter()
        .filter(|r| r["state"] == "new" || r["state"] == "differs")
        .filter_map(|r| r["path"].as_str().map(str::to_string))
        .collect();
    let mut suspects = Vec::new();
    let mut stop = None;
    for r in &rows {
        for s in r["suspects"].as_array().into_iter().flatten() {
            suspects.push(format!(
                "{}{}",
                copy_text(
                    "beExt.card.fileLead",
                    &[("path", r["path"].as_str().unwrap_or_default())]
                ),
                super::ext::suspect_said(s, &to)
            ));
            if s["kind"] == "binary" && stop.is_none() {
                stop = Some(copy_text(
                    "beExt.card.binary",
                    &[("path", r["path"].as_str().unwrap_or_default())],
                ));
            }
        }
        if let Some(why) = r["blocked"].as_str() {
            stop.get_or_insert_with(|| {
                copy_text(
                    "beExt.card.blocked",
                    &[
                        ("path", r["path"].as_str().unwrap_or_default()),
                        ("why", why),
                    ],
                )
            });
        }
    }
    if let Some(p) = source.iter().find(|f| f["text"].is_null()) {
        stop.get_or_insert_with(|| {
            copy_text(
                "beExt.card.binary",
                &[("path", p["path"].as_str().unwrap_or_default())],
            )
        });
    }
    ExtCard {
        kind: ExtKind::Skill,
        name: a.name.to_string(),
        path: plan["dir"].as_str().unwrap_or_default().to_string(),
        unchanged: writes.is_empty(),
        writes,
        suspects,
        stop,
        config: None,
        slots: Vec::new(),
        tokens: ExtTokens {
            source: token_of(&json!(source)),
            target: Some(token_of(&plan["target"])),
        },
    }
}

fn mcp_card(a: &Ask<'_>, src: &Value, pre: &Value) -> ExtCard {
    let to = machine_name(a.to.as_deref());
    let slots: Vec<ExtSlot> = pre["slots"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| ExtSlot {
            field: s["field"].as_str().unwrap_or_default().to_string(),
            key: s["key"].as_str().unwrap_or_default().to_string(),
            kept: s["kept"] == true,
        })
        .collect();
    let path = pre["path"].as_str().unwrap_or_default().to_string();
    ExtCard {
        kind: ExtKind::Mcp,
        name: a.name.to_string(),
        unchanged: pre["state"] == "same" && slots.is_empty(),
        writes: vec![path.clone()],
        path,
        suspects: pre["suspects"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| super::ext::suspect_said(s, &to))
            .collect(),
        stop: None,
        config: serde_json::to_string_pretty(&json!({ a.name: pre["def"] })).ok(),
        slots,
        tokens: ExtTokens {
            source: src["token"].as_str().unwrap_or_default().to_string(),
            target: pre["target"].as_str().map(str::to_string),
        },
    }
}

fn card_value(card: ExtCard) -> Answer {
    serde_json::to_value(card).map_err(|e| ("io_failed".to_string(), e.to_string()))
}

/// `ext-hub-preview {kind, name, from, to, scope}` → 确认卡（[`ExtCard`]）。只读。
pub(crate) async fn ext_preview(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let a = ask_of(args)?;
    match a.kind {
        ExtKind::Skill => {
            let (source, plan) = skill_look(here, &a, table, remote).await?;
            card_value(skill_card(&a, &source, &plan))
        }
        ExtKind::Mcp => {
            let (src, pre) = mcp_look(here, &a, table, remote).await?;
            card_value(mcp_card(&a, &src, &pre))
        }
    }
}

fn changed_since() -> (String, String) {
    ("stale".to_string(), copy_text("beExt.apply.changed", &[]))
}

/// `ext-hub-apply {kind, name, from, to, scope, tokens, fill?}` → [`ExtDone`]。两头都再看一次：记号对不上 ⇒ `stale`、一个字节不写。
pub(crate) async fn ext_apply(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    let a = ask_of(args)?;
    let tokens: ExtTokens = serde_json::from_value(
        args.get("tokens").cloned().unwrap_or(Value::Null),
    )
    .map_err(|e| {
        (
            "bad_args".to_string(),
            crate::common::contract::malformed(&format!("`tokens` has the wrong shape: {e}")),
        )
    })?;
    let done = match a.kind {
        ExtKind::Skill => {
            let (source, plan) = skill_look(here, &a, table, remote).await?;
            let card = skill_card(&a, &source, &plan);
            if card.tokens != tokens {
                return Err(changed_since());
            }
            if let Some(why) = card.stop {
                return Err(("refused".to_string(), why));
            }
            let overwrite: Vec<Value> = plan["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["state"] == "differs")
                .map(|r| r["path"].clone())
                .collect();
            let out = ask_one(
                here,
                a.to.as_deref(),
                "skill-install-apply",
                json!({
                    "name": a.name,
                    "project": project_arg(&a.at_to),
                    "source": source,
                    "target": plan["target"],
                    "take": card.writes,
                    "overwrite": overwrite,
                }),
                table,
                remote,
            )
            .await?;
            let failed = strs(&out["chmodFailed"]);
            let mut note: Vec<String> = Vec::new();
            if !failed.is_empty() {
                note.push(copy_text(
                    "beExt.done.chmodFailed",
                    &[("list", &failed.join(", "))],
                ));
            }
            note.extend(out["recordFailed"].as_str().map(str::to_string));
            ExtDone {
                path: out["dir"].as_str().unwrap_or_default().to_string(),
                changed: strs(&out["written"]),
                note: (!note.is_empty()).then(|| note.join(" ")),
            }
        }
        ExtKind::Mcp => {
            let (src, pre) = mcp_look(here, &a, table, remote).await?;
            let card = mcp_card(&a, &src, &pre);
            if card.tokens != tokens {
                return Err(changed_since());
            }
            let out = ask_one(
                here,
                a.to.as_deref(),
                "mcp-sync-apply",
                json!({
                    "name": a.name,
                    "at": a.at_to,
                    "def": pre["def"],
                    "fill": args.get("fill").cloned().unwrap_or(Value::Null),
                    "target": pre["target"],
                }),
                table,
                remote,
            )
            .await?;
            ExtDone {
                path: out["path"].as_str().unwrap_or_default().to_string(),
                changed: if out["written"] == true {
                    vec![a.name.to_string()]
                } else {
                    Vec::new()
                },
                note: out["recordFailed"].as_str().map(str::to_string),
            }
        }
    };
    serde_json::to_value(done).map_err(|e| ("io_failed".to_string(), e.to_string()))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/hub_tests.rs"]
mod tests;
