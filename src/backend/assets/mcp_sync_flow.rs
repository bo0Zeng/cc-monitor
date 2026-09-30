//! 〔MIG-3a · D 组〕MCP 装到一台的 I/O 那一半：帧面 `mcp-sync-source` · `mcp-sync-preview` · `mcp-sync-apply`，只看被点的那一条。
//!
//! 每一问都只在**一台**上：来源那台交那一条（`mcp-sync-source`）；要被写的那一台自己读自己那份、判、写（`-preview` / `-apply`）。
//! 🔴 **密钥不出来源机**：`env` / `headers` 的值（与资产目录只记键名的是同一张表，`asset_catalog::MCP_KEYS_ONLY_FIELDS`）
//! 在来源那台就换成空位（`null`），交出去的只有键名与一个记号；被写那台补空位时只取用户在确认卡上填的、或它自己那一条原有的值，
//! 两样都没有 ⇒ 拒（`needs_input`），不拿骨架凑。
//! 🔴 写**不重读重算**：用户确认的是他看到的那一份；被写那台在他看过之后变了 ⇒ `stale` 就停。
//! 用户级 MCP 住 agent 自己的热状态文件 ⇒ 只当来源读，这里不往它里面写。

use super::door::{self, Door, Refused};
use super::ext::ExtLoc;
use super::mcp_edit::{plan_project_mcp, upsert_mcp_server_value, MCP_JSON};
use super::skill_flow::Record;
use crate::assets::asset_catalog::MCP_KEYS_ONLY_FIELDS;
use crate::assets::mcp_sync::{candidates, judge, servers_of, Facts, There};
use copy_core::copy_text;
use serde_json::{json, Map, Value};

type Answer = Result<Value, (&'static str, String)>;

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{k}` (a string)")),
    ))
}

/// 一条定义里装密钥的那几格换成空位：交回（换好的那一条, [(字段, 键)]）。**纯**；不是对象的原样交回、没有空位。
pub(crate) fn redact(def: &Value) -> (Value, Vec<(String, String)>) {
    let mut out = def.clone();
    let mut slots = Vec::new();
    if let Some(o) = out.as_object_mut() {
        for field in MCP_KEYS_ONLY_FIELDS {
            if let Some(Value::Object(m)) = o.get_mut(*field) {
                for (k, v) in m.iter_mut() {
                    *v = Value::Null;
                    slots.push((field.to_string(), k.clone()));
                }
            }
        }
    }
    (out, slots)
}

/// 这一级的 MCP 配置文件在这台上的（根, 相对段）：项目 = `<项目>/.mcp.json`；用户级 = agent 的那份（只读）。
fn file_of(
    at: &ExtLoc,
    user_mcp: Option<&std::path::Path>,
) -> Result<(String, String), (&'static str, String)> {
    match at {
        ExtLoc::Project { dir } => Ok((dir.clone(), MCP_JSON.to_string())),
        ExtLoc::User => {
            let f = user_mcp.ok_or(("io_failed", copy_text("beExt.uninstall.noHome", &[])))?;
            let (Some(dir), Some(name)) = (f.parent(), f.file_name()) else {
                return Err(("io_failed", copy_text("beExt.uninstall.noHome", &[])));
            };
            Ok((
                dir.display().to_string(),
                name.to_string_lossy().into_owned(),
            ))
        }
    }
}

fn read_only() -> (&'static str, String) {
    ("refused", copy_text("beExt.note.userMcpReadOnly", &[]))
}

/// `mcp-sync-source {name, at}`（来源那台）：那一条，密钥值换成空位 `{path, def, slots, token}`。
/// `token` 按**原样那一条**（含密钥值）算：它变了（连只改了一个密钥值也算）⇒ 应用时判 `stale`。
pub(crate) fn answer_source(
    d: &dyn Door,
    user_mcp: Option<&std::path::Path>,
    args: &Value,
) -> Answer {
    let name = str_arg(args, "name")?;
    let at = ExtLoc::from_arg(args.get("at"), "at")?;
    let (root, rel) = file_of(&at, user_mcp)?;
    let got = door::peek(d, &root, &rel).map_err(|m| ("refused", m))?;
    let text = got.text.ok_or((
        "missing",
        copy_text("beMcpSyncFlow.source.missing", &[("path", &got.path)]),
    ))?;
    let servers = servers_of(
        Some(&text),
        &copy_text("beMcpSync.answerWith.sourceSide", &[]),
    )?;
    let def = servers.get(name).ok_or((
        "missing",
        copy_text(
            "beExt.uninstall.noEntry",
            &[("name", name), ("path", &got.path)],
        ),
    ))?;
    let (open, slots) = redact(def);
    let slots: Vec<Value> = slots
        .into_iter()
        .map(|(field, key)| json!({ "field": field, "key": key }))
        .collect();
    Ok(json!({
        "path": got.path,
        "def": open,
        "slots": slots,
        "token": crate::assets::ext::token_of(def),
    }))
}

/// 被写那台：`args.def` 必须是一条对象、且装密钥的那几格一个值都没有（来源交出来就该是空位）。
fn def_arg(args: &Value) -> Result<Value, (&'static str, String)> {
    let def = args.get("def").cloned().filter(Value::is_object).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `def` (an object)"),
    ))?;
    if redact(&def).0 != def {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`def` carries values in env / headers"),
        ));
    }
    Ok(def)
}

type Target = (String, door::Peeked, Map<String, Value>);

fn target_of(d: &dyn Door, at: &ExtLoc) -> Result<Target, (&'static str, String)> {
    let dir = at.project().ok_or_else(read_only)?;
    let tgt = door::peek(d, dir, MCP_JSON).map_err(|m| ("refused", m))?;
    let servers = servers_of(
        tgt.text.as_deref(),
        &copy_text("beMcpSync.answerWith.targetSide", &[]),
    )?;
    Ok((dir.to_string(), tgt, servers))
}

fn has_value(existing: Option<&Value>, field: &str, key: &str) -> Option<String> {
    existing?.get(field)?.get(key)?.as_str().map(str::to_string)
}

/// `mcp-sync-preview {name, at, def}`（要被写的那一台）：读自己那份、判「新 / 一样 / 不同」、可疑项、每个空位这台有没有原值。
pub(crate) fn answer_preview(d: &dyn Door, facts: &dyn Facts, args: &Value) -> Answer {
    let name = str_arg(args, "name")?;
    let at = ExtLoc::from_arg(args.get("at"), "at")?;
    let def = def_arg(args)?;
    let (_, tgt, servers) = target_of(d, &at)?;
    let existing = servers.get(name);
    let state = match existing.map(|e| redact(e).0) {
        None => "new",
        Some(e) if e == def => "same",
        Some(_) => "differs",
    };
    let slots: Vec<Value> = redact(&def)
        .1
        .into_iter()
        .map(|(field, key)| {
            let kept = has_value(existing, &field, &key).is_some();
            json!({ "field": field, "key": key, "kept": kept })
        })
        .collect();
    let suspects: Vec<Value> = judge(&candidates(&def), facts)
        .into_iter()
        .map(|s| json!({ "kind": s.kind, "field": s.field, "value": s.value, "there": s.there.map(There::wire) }))
        .collect();
    Ok(json!({
        "path": tgt.path,
        "state": state,
        "def": def,
        "slots": slots,
        "suspects": suspects,
        "target": tgt.text.as_deref().map(crate::assets::skill_ledger::digest_of),
    }))
}

/// `mcp-sync-apply {name, at, def, fill, target}`：`target` = 看卡时这台那份的记号（不存在 = `null`）；`fill` = `{env: {键: 值}, headers: {…}}`
/// （用户填的；没填的键沿用这台原有的值）。写完记进装记录（写口由 `inbound.rs` 递进来）。回 `{path, written, recordFailed}`。
pub(crate) fn answer_apply(d: &dyn Door, record: Record, args: &Value) -> Answer {
    let name = str_arg(args, "name")?;
    let at = ExtLoc::from_arg(args.get("at"), "at")?;
    let def = def_arg(args)?;
    let (root, tgt, servers) = target_of(d, &at)?;
    let seen = match args.get("target") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Null) | None => None,
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`target` must be a string or `null`"),
            ))
        }
    };
    let now = tgt
        .text
        .as_deref()
        .map(crate::assets::skill_ledger::digest_of);
    if now.as_deref() != seen {
        return Err(("stale", copy_text("beMcpSyncFlow.apply.stale", &[])));
    }
    let existing = servers.get(name);
    let fill = args.get("fill").cloned().unwrap_or(Value::Null);
    let mut full = def.clone();
    for (field, key) in redact(&def).1 {
        let typed = fill
            .get(&field)
            .and_then(|m| m.get(&key))
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let value = typed.or_else(|| has_value(existing, &field, &key)).ok_or((
            "needs_input",
            copy_text(
                "beExt.apply.needsValue",
                &[("field", &field), ("key", &key)],
            ),
        ))?;
        full[field.as_str()][key.as_str()] = Value::String(value);
    }
    let at_path = door::join_under(&root, MCP_JSON);
    if existing == Some(&full) {
        return Ok(json!({ "path": at_path, "written": false, "recordFailed": null }));
    }
    let next = plan_project_mcp(&at_path, tgt.text.as_deref(), &mut |v: &mut Value| {
        upsert_mcp_server_value(v, name.to_string(), full.clone())?;
        Ok(true)
    })
    .map_err(|m| ("refused", m))?
    .unwrap_or_default();
    let landed = match door::put(d, &root, MCP_JSON, &next, tgt.text.as_deref(), false, false) {
        Ok(l) => l,
        Err(Refused::Stale(_)) => {
            return Err(("stale", copy_text("beMcpSyncFlow.apply.stale", &[])))
        }
        Err(e) => return Err(("refused", e.said())),
    };
    let record_failed = record(&json!({
        "op": "mcp-add",
        "file": landed.path,
        "name": name,
        "digest": crate::assets::skill_ledger::mcp_digest(&full),
    }))
    .err()
    .map(|(_, e)| copy_text("beExt.apply.recordFailed", &[("e", &e)]));
    Ok(json!({ "path": landed.path, "written": landed.changed, "recordFailed": record_failed }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/mcp_sync_flow_tests.rs"]
mod tests;
