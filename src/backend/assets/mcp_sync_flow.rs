//! 〔MIG-3a · D 组〕MCP 推 / 拉的 I/O 那一半：帧面 `mcp-sync-source` · `mcp-sync-preview` · `mcp-sync-apply`。
//!
//! 从 monitor `mcp_sync.rs` 搬来（从前 monitor 经两台后端 `files-peek` 读、请对面 `mcp-sync-plan` 判、再 `files-put` 写）。
//! 今天每一问都只在**一台**上：来源那台答「那份原文」（`mcp-sync-source`）；要被写的那一台自己读自己那份、判、写
//! （`mcp-sync-preview` / `-apply`，判定原样是 `mcp_sync::answer_with`）。界面只把用户看过的那份原文原样递过去（看差异时拿到、
//! 写的时候原样交回 —— 与从前 monitor 那一趟同一份原文）。
//! 🔴 写**不重读重算**：用户确认的是他看到的那份差异；对面在他看过之后变了 ⇒ `stale` 就停（`96 §3.5`）。

use super::door::{self, Door, Refused};
use super::mcp_edit::{plan_project_mcp, project_root, upsert_mcp_server_value, MCP_JSON};
use crate::assets::mcp_sync::Facts;
use copy_core::copy_text;
use serde_json::{json, Map, Value};

type Answer = Result<Value, (&'static str, String)>;

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{k}` (a string)")),
    ))
}

/// 原文里的 `mcpServers`（只取值：合不合法已由 `mcp_sync::answer_with` 判过）。
fn servers_in(text: Option<&str>) -> Map<String, Value> {
    text.and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.get("mcpServers").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

/// `mcp-sync-source {projectDir}`：来源那份原文 `{path, text}`（不存在 ⇒ `missing`，没东西可拷）。
pub(crate) fn answer_source(d: &dyn Door, args: &Value) -> Answer {
    let root = project_root(str_arg(args, "projectDir")?)?;
    let got = door::peek(d, &root, MCP_JSON).map_err(|m| ("refused", m))?;
    let text = got.text.ok_or((
        "missing",
        copy_text("beMcpSyncFlow.source.missing", &[("path", &got.path)]),
    ))?;
    Ok(json!({ "path": got.path, "text": text }))
}

/// `mcp-sync-preview {projectDir, source, sourcePath, sameMachine}`：这台是要被写的那一台 —— 读自己那份、判差异与可疑项。
pub(crate) fn answer_preview(d: &dyn Door, facts: &dyn Facts, args: &Value) -> Answer {
    let root = project_root(str_arg(args, "projectDir")?)?;
    let source = str_arg(args, "source")?;
    let source_path = str_arg(args, "sourcePath")?;
    let tgt = door::peek(d, &root, MCP_JSON).map_err(|m| ("refused", m))?;
    if args.get("sameMachine").and_then(Value::as_bool) == Some(true) && tgt.path == source_path {
        return Err(("bad_args", copy_text("beMcpSyncFlow.preview.sameFile", &[])));
    }
    let plan = crate::assets::mcp_sync::answer_with(
        facts,
        &json!({ "source": source, "target": tgt.text }),
    )?;
    let (sv, tv) = (servers_in(Some(source)), servers_in(tgt.text.as_deref()));
    let rows: Vec<Value> = plan["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut r| {
            let name = r["name"].as_str().unwrap_or_default().to_string();
            r["source"] = sv.get(&name).cloned().unwrap_or(Value::Null);
            r["target"] = tv.get(&name).cloned().unwrap_or(Value::Null);
            r
        })
        .collect();
    Ok(json!({
        "sourcePath": source_path,
        "targetPath": tgt.path,
        "sourceText": source,
        "targetText": tgt.text,
        "rows": rows,
    }))
}

/// `mcp-sync-apply {projectDir, source, target, take, overwrite}`：把勾的那几条原样合进这台那份，CAS 期望 = 看差异时的 `target`。
pub(crate) fn answer_apply(d: &dyn Door, facts: &dyn Facts, args: &Value) -> Answer {
    let root = project_root(str_arg(args, "projectDir")?)?;
    let source = str_arg(args, "source")?;
    let target = match args.get("target") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Null) => None,
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`target` must be a string or `null`"),
            ))
        }
    };
    let plan = crate::assets::mcp_sync::answer_with(
        facts,
        &json!({ "source": source, "target": target, "take": args.get("take"), "overwrite": args.get("overwrite") }),
    )?;
    let names: Vec<String> = plan["write"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let at = door::join_under(&root, MCP_JSON);
    let unchanged = |names: Vec<String>| json!({ "path": at, "written": false, "names": names });
    if names.is_empty() {
        return Ok(unchanged(Vec::new()));
    }
    let src = servers_in(Some(source));
    let next = plan_project_mcp(&at, target, &mut |v: &mut Value| {
        for n in &names {
            let server = src
                .get(n)
                .cloned()
                .ok_or_else(|| copy_text("beMcpSyncFlow.apply.notInCopy", &[("name", n)]))?;
            upsert_mcp_server_value(v, n.clone(), server)?;
        }
        Ok(true)
    })
    .map_err(|m| ("refused", m))?;
    let Some(next) = next.filter(|c| Some(c.as_str()) != target) else {
        return Ok(unchanged(names));
    };
    match door::put(d, &root, MCP_JSON, &next, target, false, false) {
        Ok(l) => Ok(json!({ "path": l.path, "written": l.changed, "names": names })),
        Err(Refused::Stale(_)) => Err(("stale", copy_text("beMcpSyncFlow.apply.stale", &[]))),
        Err(e) => Err(("refused", e.said())),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/mcp_sync_flow_tests.rs"]
mod tests;
