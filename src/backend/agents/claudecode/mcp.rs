//! Claude 的 **MCP 布局**：`.claude.json` 顶层 `mcpServers`（user）· `projects[<目录>].mcpServers`（local）·
//! `<目录>/.mcp.json` 的 `mcpServers`（project）。读法宽容：缺 ⇒ 那一段空；坏 ⇒ 那一段空并说出来（`problems`）。
//! `.claude.json` 找哪一份与资产目录同一处（`assets::claude_json`）。

use std::path::Path;

use serde_json::Value;

use super::accounts::MAX_CONFIG_BYTES;
use super::assets::{MAX_PROJECT_MCP_BYTES, PROJECT_MCP_FILE};
use crate::agents::{McpEntry, McpRead};
use crate::common::fs::read_regular_capped;

/// 注册表那一格的实现：按这台机器的环境现解 `.claude.json`。
pub(crate) fn read(project_dir: Option<&Path>) -> McpRead {
    read_at(super::assets::claude_json().as_deref(), project_dir)
}

/// [`read`] 的本体，`.claude.json` 的位置是参数（判据拿夹具喂它）。
pub(crate) fn read_at(claude_json: Option<&Path>, project_dir: Option<&Path>) -> McpRead {
    let mut out = McpRead::default();
    if let Some(path) = claude_json {
        if let Some(cj) = read_json(path, MAX_CONFIG_BYTES, &mut out.problems) {
            let src = path.display().to_string();
            push(&mut out.entries, cj.get("mcpServers"), "user", &src);
            if let Some(dir) = project_dir {
                let local = cj
                    .get("projects")
                    .and_then(|p| p.get(dir.to_string_lossy().as_ref()))
                    .and_then(|p| p.get("mcpServers"));
                push(&mut out.entries, local, "local", &src);
            }
            let mut dirs: Vec<String> = cj
                .get("projects")
                .and_then(Value::as_object)
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            dirs.sort();
            out.dirs = dirs;
        }
    }
    if let Some(dir) = project_dir {
        let file = dir.join(PROJECT_MCP_FILE);
        if let Some(v) = read_json(&file, MAX_PROJECT_MCP_BYTES, &mut out.problems) {
            push(
                &mut out.entries,
                v.get("mcpServers"),
                "project",
                &file.display().to_string(),
            );
        }
    }
    out
}

fn push(out: &mut Vec<McpEntry>, servers: Option<&Value>, scope: &'static str, source: &str) {
    for (name, server) in servers.and_then(Value::as_object).into_iter().flatten() {
        out.push(McpEntry {
            scope,
            name: name.clone(),
            server: server.clone(),
            source: source.to_string(),
        });
    }
}

/// 不在 ⇒ `None`、不出声；在而读不出 / 不是 JSON ⇒ `None` 并记一句（「这台没有」与「那份坏了」不合成一句）。
fn read_json(path: &Path, cap: u64, problems: &mut Vec<String>) -> Option<Value> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            problems.push(copy_core::copy_text(
                "beMcp.read.statFailed",
                &[("path", &path.display().to_string()), ("e", &e.to_string())],
            ));
            return None;
        }
        Ok(_) => {}
    }
    let parsed = read_regular_capped(path, cap).and_then(|b| {
        serde_json::from_slice::<Value>(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b))
            .map_err(|e| e.to_string())
    });
    match parsed {
        Ok(v) => Some(v),
        Err(e) => {
            problems.push(copy_core::copy_text(
                "beMcp.read.readFailed",
                &[("path", &path.display().to_string()), ("e", &e)],
            ));
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/mcp_tests.rs"]
mod tests;
