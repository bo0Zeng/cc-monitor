//! 〔D 组〕项目 `.mcp.json` 的增 / 改 / 删：帧面 `mcp-server-put` / `mcp-server-remove`。
//!
//! 从 monitor `mcp.rs` 原样搬来（D 组「monitor 算好、后端写」改成那台后端自己算、自己写）。
//! 写面**只** `<项目目录>/.mcp.json`（SS-14 · `INVARIANTS §1` 例外 5）：`~/.claude.json` / `settings.json` 一个字节不碰。
//! 「原文怎么变成新原文」只有 [`plan_project_mcp`] 一处（推 / 拉那一趟也用它）。

use super::door::{self, Door, Edited};
use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::path::Path;

/// 项目 `.mcp.json` 的文件名（写面只此一个落点）。
pub(crate) const MCP_JSON: &str = ".mcp.json";

type Answer = Result<Value, (&'static str, String)>;

/// 项目目录必须是这台机器上的绝对路径（相对路径在后端这个进程里指的是别处）。
pub(crate) fn project_root(project_dir: &str) -> Result<String, (&'static str, String)> {
    let d = project_dir.trim();
    if d.is_empty() {
        return Err(("bad_args", copy_text("beMcpEdit.path.emptyDir", &[])));
    }
    if !Path::new(d).is_absolute() || d.split(['/', '\\']).any(|s| s == "..") {
        return Err((
            "bad_path",
            copy_text("beMcpEdit.path.notAbsolute", &[("dir", &format!("{d:?}"))]),
        ));
    }
    Ok(d.trim_end_matches(['/', '\\']).to_string())
}

/// 把一条 server upsert 进 `.mcp.json` Value（名空拒）。
pub(crate) fn upsert_mcp_server_value(
    root: &mut Value,
    name: String,
    server: Value,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(copy_text("beMcpEdit.upsert.emptyName", &[]));
    }
    let obj = root
        .as_object_mut()
        .ok_or_else(|| copy_text("beMcpEdit.mcpJson.rootNotObject", &[]))?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let smap = servers
        .as_object_mut()
        .ok_or_else(|| copy_text("beMcpEdit.mcpJson.serversNotObject", &[]))?;
    smap.insert(name, server);
    Ok(())
}

/// 从 `.mcp.json` Value 删一条 server，回是否真删。
pub(crate) fn remove_mcp_server_value(root: &mut Value, name: &str) -> Result<bool, String> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| copy_text("beMcpEdit.mcpJson.rootNotObject", &[]))?;
    Ok(obj
        .get_mut("mcpServers")
        .and_then(|m| m.as_object_mut())
        .map(|smap| smap.remove(name).is_some())
        .unwrap_or(false))
}

/// 🔴 `.mcp.json` 的原文怎么变成新原文 —— 只有这一处（纯）：不存在 ⇒ 从骨架算起；已存在但解析失败 ⇒ **拒绝覆盖**；
/// `change` 回 `false` ⇒ 没事可做（`None`）。
pub(crate) fn plan_project_mcp(
    what: &str,
    existing: Option<&str>,
    change: &mut impl FnMut(&mut Value) -> Result<bool, String>,
) -> Result<Option<String>, String> {
    let mut v = match existing {
        None => json!({ "mcpServers": {} }),
        Some(t) => serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| {
            copy_text(
                "beMcpEdit.plan.parseFailed",
                &[("what", what), ("e", &e.to_string())],
            )
        })?,
    };
    if !change(&mut v)? {
        return Ok(None);
    }
    serde_json::to_string_pretty(&v)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{k}` (a string)")),
    ))
}

/// 读改写一趟：不留备份（`.mcp.json` 住用户的仓里）、不建父目录；回 `{path, changed}`。
fn edit_at(
    d: &dyn Door,
    project_dir: &str,
    mut change: impl FnMut(&mut Value) -> Result<bool, String>,
) -> Answer {
    let root = project_root(project_dir)?;
    let target = door::join_under(&root, MCP_JSON);
    match door::edit(d, &root, MCP_JSON, false, false, |existing| {
        plan_project_mcp(&target, existing, &mut change)
    }) {
        Ok(Edited::Written(l)) => Ok(json!({ "path": l.path, "changed": true })),
        Ok(Edited::Unchanged) => Ok(json!({ "path": target, "changed": false })),
        Err(m) => Err(("refused", m)),
    }
}

/// `mcp-server-put {projectDir, name, server}`：增 / 改一条。
pub(crate) fn answer_put(d: &dyn Door, args: &Value) -> Answer {
    let dir = str_arg(args, "projectDir")?;
    let name = str_arg(args, "name")?.to_string();
    let server = args.get("server").cloned().ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `server`"),
    ))?;
    edit_at(d, dir, |v| {
        upsert_mcp_server_value(v, name.clone(), server.clone()).map(|()| true)
    })
}

/// `mcp-server-remove {projectDir, name}`：删一条；文件不在或条目不在 ⇒ 一个字节不写、不建文件。
pub(crate) fn answer_remove(d: &dyn Door, args: &Value) -> Answer {
    let dir = str_arg(args, "projectDir")?;
    let name = str_arg(args, "name")?.to_string();
    edit_at(d, dir, |v| remove_mcp_server_value(v, &name))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/mcp_edit_tests.rs"]
mod tests;
