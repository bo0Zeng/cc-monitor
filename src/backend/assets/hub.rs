//! **两台之间「装」那一件的枢纽**：`ext-hub-preview` / `ext-hub-apply`，skill 与 MCP 同一对命令。
//!
//! 界面只 `call(<local>, …)` 一次（带 `kind` · `name` · `from` / `to` · `scope`），**本机常驻后端当枢纽**：向来源那台取、
//! 交被写那台判与写（被写那台照旧自己判 CAS、`stale` 就停）。种类在这里分派到原来那两条内层路：
//! skill = `skill-read` → `skill-install-plan` / `skill-install-apply`；MCP = `mcp-sync-source` → `mcp-sync-preview` / `mcp-sync-apply`；
//! cc-monitor 自带的那一个（cc-bus）不从别的机器拿：被写那台用它自己二进制里那一份（`cc-bus-install-state` / `cc-bus-install`）。
//! 落点先过 `ext::builtin_refused`（自带的只装全局），不行就拒、一跳都不发；用户级 MCP 能不能写由被写那台自己判（有账号库 ⇒ 写进各账号共用的那一份）。
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
//! 远端那一跳失败时码随原话一起交回（`remote_ask::Said`，别压成一个）：被写那台答 `stale` 就是 `stale`；
//! 读不出码的（拨号失败 · 链路断了 · 那台太旧）回 `unreachable` ＋ 原话。本机那一跳的码原样保留。

use copy_core::copy_text;
use serde_json::{json, Value};
use std::sync::Arc;

use super::ext::{
    builtin_refused, is_builtin, same_place, token_of, ExtBring, ExtCard, ExtDone, ExtKind,
    ExtList, ExtLoc, ExtSlot, ExtTarget, ExtTokens,
};
use crate::dial::remote_ask::{Remote, Table};

type Answer = Result<Value, (String, String)>;

/// 这台自己的那几条内层命令（生产 = `stream/inbound/doors.rs::LocalFrames`：本进程 `REGISTRY` 的 `run`，限那几条）。
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
            if !crate::dial::remote_ask::lock(table).contains_key(m) {
                return Err((
                    "unreachable".to_string(),
                    crate::dial::remote_ask::unreachable_message(m),
                ));
            }
            // 码随原话一起交回（`stale` / `refused` / …照那台的原码）；读不出码（拨号 / 链路坏了）⇒ `unreachable`。
            crate::dial::remote_ask::ask_json(m, cmd, &args, table, remote)
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

/// `scope` 的两头：只认形状。项目目录是不是绝对路径由它所属的那台判（来源 / 被写那台的内层命令各判各的），
/// 枢纽按自己这台的写法判会把别台的路径错拒（Windows 上的枢纽不认 `/home/…`）。
fn scope_of(args: &Value) -> Result<(ExtLoc, ExtLoc), (String, String)> {
    let s = args.get("scope");
    let own = |e: (&'static str, String)| (e.0.to_string(), e.1);
    Ok((
        ExtLoc::shape_of(s.and_then(|s| s.get("from")), "scope.from").map_err(own)?,
        ExtLoc::shape_of(s.and_then(|s| s.get("to")), "scope.to").map_err(own)?,
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
    if let Some(why) = builtin_refused(kind, name, &at_to) {
        return Err(("refused".to_string(), why));
    }
    if !is_builtin(kind, name) && same_place(from.as_deref(), &at_from, to.as_deref(), &at_to) {
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
    let mut suspects: Vec<String> = pre["suspects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| super::ext::suspect_said(s, &to))
        .collect();
    if a.at_to == ExtLoc::User {
        // 全局 = 这台各账号共用的那一份：卡上说清删除只有一条路、什么时候用上。
        suspects.push(copy_text("beExt.card.sharedDeleteHere", &[]));
        suspects.push(copy_text("beExt.card.sharedNewSessions", &[]));
    }
    ExtCard {
        kind: ExtKind::Mcp,
        name: a.name.to_string(),
        unchanged: pre["state"] == "same" && slots.is_empty(),
        writes: vec![path.clone()],
        path,
        suspects,
        stop: None,
        config: serde_json::to_string_pretty(&json!({ a.name: pre["def"] })).ok(),
        slots,
        tokens: ExtTokens {
            source: src["token"].as_str().unwrap_or_default().to_string(),
            target: pre["target"].as_str().map(str::to_string),
        },
    }
}

/// 自带的那一个（cc-bus）：不从别的机器拿 —— 被写那台用它自己二进制里那一份装（`cc-bus-install-state` / `cc-bus-install`）。
async fn builtin_look(
    here: &Arc<dyn Here>,
    a: &Ask<'_>,
    table: &Table,
    remote: &dyn Remote,
) -> Answer {
    ask_one(
        here,
        a.to.as_deref(),
        "cc-bus-install-state",
        json!({}),
        table,
        remote,
    )
    .await
}

/// 自带的那一个的确认卡：写哪几个（内容会变的）· 那台已有目录 ⇒ 装之前整个改名留作备份。
fn builtin_card(a: &Ask<'_>, st: &Value) -> ExtCard {
    let writes = strs(&st["writes"]);
    let mut suspects = Vec::new();
    if st["existing"] == true && !writes.is_empty() {
        suspects.push(copy_text("beExt.card.builtinBackup", &[]));
    }
    ExtCard {
        kind: a.kind,
        name: a.name.to_string(),
        path: st["dest"].as_str().unwrap_or_default().to_string(),
        unchanged: writes.is_empty(),
        writes,
        suspects,
        stop: None,
        config: None,
        slots: Vec::new(),
        tokens: ExtTokens {
            source: st["version"].as_str().unwrap_or_default().to_string(),
            target: Some(token_of(st)),
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
    if is_builtin(a.kind, a.name) {
        let st = builtin_look(here, &a, table, remote).await?;
        return card_value(builtin_card(&a, &st));
    }
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
        _ if is_builtin(a.kind, a.name) => {
            let st = builtin_look(here, &a, table, remote).await?;
            if builtin_card(&a, &st).tokens != tokens {
                return Err(changed_since());
            }
            let out = ask_one(
                here,
                a.to.as_deref(),
                "cc-bus-install",
                json!({}),
                table,
                remote,
            )
            .await?;
            let note: Vec<String> = [
                out["backup"]
                    .as_str()
                    .map(|b| copy_text("beExt.done.builtinBackup", &[("path", b)])),
                out["recordFailed"].as_str().map(str::to_string),
            ]
            .into_iter()
            .flatten()
            .collect();
            ExtDone {
                path: out["dest"].as_str().unwrap_or_default().to_string(),
                changed: strs(&out["written"]),
                note: (!note.is_empty()).then(|| note.join(" ")),
            }
        }
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

// ═══════════════════════════════════════════════════════════════════════════
// 一次装到几台（`ext-hub-preview` / `ext-hub-apply` 收一组机器）：落点取交集 · 要填格并成一份 · 各台各自结局
// ═══════════════════════════════════════════════════════════════════════════

/// 勾上的一台：那一格的「装到…」（表里那台那一格；没有 ⇒ 那台装不了，逐台说）。
#[derive(Debug, Clone)]
pub(crate) struct Pick {
    pub to: Option<String>,
    pub name: String,
    pub bring: Option<ExtBring>,
}

/// 那一组的落点与各台。
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    /// 各台能装的各处并起来：每一处只有勾上的每台都能装才可选（不行的照列、说第一台为什么不行）。
    pub places: Vec<ExtTarget>,
    /// 共用的那一处：要的那一处对每台都行就用它；否则第一台建议的那一处（对每台都行时）；再否则第一处对每台都行的；都没有 ⇒ `None`。
    pub place: Option<ExtLoc>,
    pub picks: Vec<Pick>,
}

/// 那张表（`ext-list` 同一份）⇒ 那一组的落点与各台。**纯函数**，判定只在这里。
pub(crate) fn plan_many(
    list: &ExtList,
    kind: ExtKind,
    name: &str,
    to: &[Option<String>],
    want: Option<ExtLoc>,
) -> Result<Plan, (String, String)> {
    if to.is_empty() {
        return Err(bad("`to` must list at least one machine"));
    }
    let row = list
        .rows
        .iter()
        .find(|r| r.kind == kind && r.name == name)
        .ok_or_else(|| {
            (
                "missing".to_string(),
                copy_text("beExt.many.noRow", &[("name", name)]),
            )
        })?;
    let picks: Vec<Pick> = to
        .iter()
        .map(|t| {
            let col = list.machines.iter().position(|m| match t {
                None => m.here,
                Some(k) => m.key.as_deref() == Some(k.as_str()),
            });
            let name = col
                .map(|i| {
                    let m = &list.machines[i];
                    if m.here {
                        copy_text("beExt.machine.here", &[])
                    } else {
                        m.name.clone()
                    }
                })
                .unwrap_or_else(|| t.clone().unwrap_or_default());
            Pick {
                to: t.clone(),
                name,
                bring: col
                    .and_then(|i| row.cells.get(i))
                    .and_then(|c| c.bring.clone()),
            }
        })
        .collect();
    let mut places: Vec<ExtTarget> = Vec::new();
    for p in &picks {
        for t in p.bring.iter().flat_map(|b| b.targets.iter()) {
            if !places.iter().any(|x| x.at == t.at) {
                places.push(ExtTarget {
                    at: t.at.clone(),
                    ok: true,
                    note: None,
                });
            }
        }
    }
    for x in &mut places {
        for p in &picks {
            let hit = p
                .bring
                .as_ref()
                .and_then(|b| b.targets.iter().find(|t| t.at == x.at));
            let why = match hit {
                Some(t) if t.ok => None,
                Some(t) => Some(t.note.clone().unwrap_or_default()),
                None => Some(copy_text("beExt.many.notThere", &[("machine", &p.name)])),
            };
            if let Some(w) = why {
                x.ok = false;
                if x.note.is_none() {
                    x.note = Some(w);
                }
            }
        }
    }
    let ok_all = |at: &ExtLoc| places.iter().any(|x| x.ok && x.at == *at);
    let place = want
        .filter(|w| ok_all(w))
        .or_else(|| {
            picks
                .iter()
                .find_map(|p| p.bring.as_ref())
                .map(|b| b.scope.to.clone())
                .filter(|w| ok_all(w))
        })
        .or_else(|| places.iter().find(|x| x.ok).map(|x| x.at.clone()));
    Ok(Plan {
        places,
        place,
        picks,
    })
}

/// 几张卡的要填格并成一份：每格一次，`kept` ＝ 哪几台那一格已经有值（不填就沿用）。**纯函数**。
pub(crate) fn merge_slots(cards: &[(String, &ExtCard)]) -> Value {
    let mut out: Vec<(String, String, Vec<String>)> = Vec::new();
    for (name, c) in cards {
        for s in &c.slots {
            let i = match out
                .iter()
                .position(|(f, k, _)| *f == s.field && *k == s.key)
            {
                Some(i) => i,
                None => {
                    out.push((s.field.clone(), s.key.clone(), Vec::new()));
                    out.len() - 1
                }
            };
            if s.kept {
                out[i].2.push(name.clone());
            }
        }
    }
    Value::Array(
        out.into_iter()
            .map(|(field, key, kept)| json!({"field": field, "key": key, "kept": kept}))
            .collect(),
    )
}

/// 会写的文件那一行（给人看）：MCP ＝ 那份配置文件 ＋ 那一个键；skill ＝ 那个目录 ＋ 目录里要写的几个。**纯函数**。
pub(crate) fn files_of(c: &ExtCard) -> Vec<String> {
    match c.kind {
        ExtKind::Mcp => vec![format!("{} · mcpServers", c.path)],
        ExtKind::Skill => vec![format!("{} · {}", c.path, c.writes.join(", "))],
    }
}

/// 一台那一跳的参数（与单台那一对同形）。
fn ask_for(kind: ExtKind, name: &str, p: &Pick, b: &ExtBring, place: &ExtLoc) -> Value {
    json!({
        "kind": kind,
        "name": name,
        "from": b.from,
        "to": p.to,
        "scope": {"from": b.scope.from, "to": place},
    })
}

fn many_args(
    args: &Value,
) -> Result<(ExtKind, &str, Vec<Option<String>>, Option<ExtLoc>), (String, String)> {
    let kind = ExtKind::from_arg(args).map_err(|e| (e.0.to_string(), e.1))?;
    let name = str_arg(args, "name")?;
    let to = args
        .get("to")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("`to` must be a list of machine keys (null = this backend)"))?
        .iter()
        .map(|v| match v {
            Value::Null => Ok(None),
            Value::String(s) if !s.is_empty() => Ok(Some(s.clone())),
            _ => Err(bad("`to` entries must be a machine key or null")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let place = match args.get("place") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|e| bad(&format!("`place` has the wrong shape: {e}")))?,
        ),
    };
    Ok((kind, name, to, place))
}

/// 目录那一份的读口（生产 = `asset_catalog::answer_current`，由命令表那扇门递进来）；函数指针 ⇒ 跨线程送得出去。
pub(crate) type CurrentFn =
    fn(bool) -> Result<(super::asset_catalog::Catalog, Vec<String>), (&'static str, String)>;

/// 那张表现拼一次（`ext-list` 同一份目录 ＋ 可达表）；目录那一份由命令表那扇门递进来（同 `ext-list`）。
fn current_list(table: &Table, current: CurrentFn) -> Result<ExtList, (String, String)> {
    let (cat, _) = current(false).map_err(|(c, m)| (c.to_string(), m))?;
    Ok(super::ext::table(&cat, &super::ext::reach_of(table)))
}

/// `ext-hub-preview {kind, name, to: [机器…], place?}` → `{places, place, slots, machines: [{to, name, card, files, error}]}`。只读。
pub(crate) async fn ext_preview_many(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
    current: CurrentFn,
) -> Answer {
    let (kind, name, to, want) = many_args(args)?;
    let plan = plan_many(&current_list(table, current)?, kind, name, &to, want)?;
    let mut machines = Vec::new();
    let mut cards: Vec<(String, ExtCard)> = Vec::new();
    for p in &plan.picks {
        let got = match (&p.bring, &plan.place) {
            (None, _) => Err((
                "refused".to_string(),
                copy_text("beExt.many.noBring", &[("machine", &p.name)]),
            )),
            (Some(_), None) => Err(("refused".to_string(), copy_text("beExt.many.noPlace", &[]))),
            (Some(b), Some(place)) => {
                ext_preview(here, &ask_for(kind, name, p, b, place), table, remote)
                    .await
                    .and_then(|v| {
                        serde_json::from_value::<ExtCard>(v)
                            .map_err(|e| ("io_failed".to_string(), e.to_string()))
                    })
            }
        };
        match got {
            Ok(card) => {
                machines.push(json!({"to": p.to, "name": p.name, "card": card, "files": files_of(&card), "error": null}));
                cards.push((p.name.clone(), card));
            }
            Err((_, m)) => machines
                .push(json!({"to": p.to, "name": p.name, "card": null, "files": [], "error": m})),
        }
    }
    let refs: Vec<(String, &ExtCard)> = cards.iter().map(|(n, c)| (n.clone(), c)).collect();
    Ok(json!({
        "places": plan.places,
        "place": plan.place,
        "slots": merge_slots(&refs),
        "machines": machines,
    }))
}

/// `ext-hub-apply {kind, name, to: [机器…], place, tokens: {机器: 记号}, fill}` → `{machines: [{to, name, done, error}]}`。
/// 各台照它那张卡装（值取共用那一份里那张卡要的几格；没填的不交 ⇒ 沿用那台已有的）；一台没成不挡别台。
pub(crate) async fn ext_apply_many(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
    current: CurrentFn,
) -> Answer {
    let (kind, name, to, want) = many_args(args)?;
    let plan = plan_many(
        &current_list(table, current)?,
        kind,
        name,
        &to,
        want.clone(),
    )?;
    let fill = args.get("fill").cloned().unwrap_or_else(|| json!({}));
    let tokens = args.get("tokens").cloned().unwrap_or_else(|| json!({}));
    let mut machines = Vec::new();
    for p in &plan.picks {
        let key = p.to.clone().unwrap_or_default();
        let done = match (&p.bring, &plan.place) {
            (Some(b), Some(place)) if want.as_ref() == Some(place) => {
                let ask = ask_for(kind, name, p, b, place);
                async {
                    let card: ExtCard =
                        serde_json::from_value(ext_preview(here, &ask, table, remote).await?)
                            .map_err(|e| ("io_failed".to_string(), e.to_string()))?;
                    let mut mine = serde_json::Map::new();
                    for s in &card.slots {
                        if let Some(v) = fill
                            .get(&s.field)
                            .and_then(|f| f.get(&s.key))
                            .and_then(Value::as_str)
                            .filter(|v| !v.is_empty())
                        {
                            mine.entry(s.field.clone()).or_insert_with(|| json!({}))[&s.key] =
                                json!(v);
                        }
                    }
                    let mut a = ask.clone();
                    a["tokens"] = tokens.get(&key).cloned().unwrap_or(Value::Null);
                    a["fill"] = Value::Object(mine);
                    ext_apply(here, &a, table, remote).await
                }
                .await
            }
            (None, _) => Err((
                "refused".to_string(),
                copy_text("beExt.many.noBring", &[("machine", &p.name)]),
            )),
            _ => Err(changed_since()),
        };
        machines.push(match done {
            Ok(d) => json!({"to": p.to, "name": p.name, "done": d, "error": null}),
            Err((_, m)) => json!({"to": p.to, "name": p.name, "done": null, "error": m}),
        });
    }
    Ok(json!({ "machines": machines }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/hub_tests.rs"]
mod tests;
