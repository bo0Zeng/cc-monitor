//! F87（#50+#51）：MCP 管理（项目级）。**SS-14 读写分界**（doc/../plan SS-14）：
//! - **读**：跨 scope 展示——用户 scope（`~/.claude.json` 顶层 `mcpServers`）+ local scope
//!   （`~/.claude.json` `projects[<dir>].mcpServers`）+ 项目 scope（`<dir>/.mcp.json`）。**宽容读**
//!   （INVARIANTS §18）：文件缺 / 坏 JSON / 字段缺一律跳过（空），不崩；server 值**原样保留**。
//! - **写**：**只** `<dir>/.mcp.json`（增/改/删）。**绝不写 `~/.claude.json` / `settings.json`**——
//!   写函数经 `mcp_json_path` 硬编码只拼 `.mcp.json`（同 SFTP 编辑：写用户自己项目的文件，铁律正交）。
//! - **运行时控制 / 活状态 / 跨机 / managed**：首刀不做（记账，见 feature 计划）。
//!
//! `~/.claude.json` 路径有变体（`CLAUDE_CONFIG_DIR` vs `$HOME`），故 `claude_json_candidates` 取多候选、
//! 读第一个存在的——防御式，schema 真机可能变，不硬假设完整。

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 一条 MCP server 展示项。`server` 原样保留（宽容，未知字段不丢）。camelCase 上 wire。
#[derive(serde::Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    /// `"user"` | `"local"` | `"project"`
    pub scope: String,
    pub name: String,
    /// server 配置原样（`{command,args,env}` 或 `{type:"sse"|"http",url,...}`）。
    pub server: Value,
    /// 来源文件绝对路径（展示 / 诊断用）。
    pub source_path: String,
}

/// `~/.claude.json` 候选路径（防御多变体：`CLAUDE_CONFIG_DIR` / `$HOME`）。取第一个存在的。
fn claude_json_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(cfg) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(&cfg);
        v.push(p.join(".claude.json")); // $CLAUDE_CONFIG_DIR/.claude.json
        if let Some(parent) = p.parent() {
            v.push(parent.join(".claude.json"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".claude.json")); // 经典位置
    }
    v
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
}

/// 从一个 `mcpServers` 对象抽条目（宽容：非对象 → 不加）。**纯**，供单测。
fn push_servers(servers: Option<&Value>, scope: &str, source: &str, out: &mut Vec<McpServerEntry>) {
    if let Some(obj) = servers.and_then(Value::as_object) {
        for (name, server) in obj {
            out.push(McpServerEntry {
                scope: scope.to_string(),
                name: name.clone(),
                server: server.clone(),
                source_path: source.to_string(),
            });
        }
    }
}

/// **纯核心**（供单测，不碰文件系统）：给定已解析的 `~/.claude.json`、项目 `.mcp.json` 两个可选
/// Value + 项目目录，合并出三 scope 条目。缺失传 None → 该 scope 空。
fn collect_entries(
    claude_json: Option<&Value>,
    claude_json_src: &str,
    project_mcp: Option<&Value>,
    project_mcp_src: &str,
    project_dir: Option<&str>,
) -> Vec<McpServerEntry> {
    let mut out = Vec::new();
    if let Some(cj) = claude_json {
        // 用户 scope：顶层 mcpServers
        push_servers(cj.get("mcpServers"), "user", claude_json_src, &mut out);
        // local scope：projects[<dir>].mcpServers（用 get 链，dir 作精确 key，免 JSON pointer 转义）
        if let Some(dir) = project_dir {
            let local = cj
                .get("projects")
                .and_then(|p| p.get(dir))
                .and_then(|proj| proj.get("mcpServers"));
            push_servers(local, "local", claude_json_src, &mut out);
        }
    }
    // 项目 scope：<dir>/.mcp.json 顶层 mcpServers
    push_servers(
        project_mcp.and_then(|m| m.get("mcpServers")),
        "project",
        project_mcp_src,
        &mut out,
    );
    out
}

/// 宽容读一个 JSON 文件为 Value（缺 / 坏 → None，不报错）。
fn read_json_lenient(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// F87 读命令：跨 scope 展示当前（活跃会话所在）项目的 MCP servers。宽容——缺/坏文件返回空段。
#[tauri::command]
pub fn read_mcp_servers(project_dir: Option<String>) -> Result<Vec<McpServerEntry>, String> {
    let claude_path = first_existing(&claude_json_candidates());
    let claude_json = claude_path.as_deref().and_then(read_json_lenient);
    let claude_src = claude_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let (project_mcp, project_src) = match project_dir.as_deref() {
        Some(dir) if !dir.trim().is_empty() => {
            let mcp = Path::new(dir).join(".mcp.json");
            let src = mcp.to_string_lossy().into_owned();
            (read_json_lenient(&mcp), src)
        }
        _ => (None, String::new()),
    };

    Ok(collect_entries(
        claude_json.as_ref(),
        &claude_src,
        project_mcp.as_ref(),
        &project_src,
        project_dir.as_deref(),
    ))
}

/// **只**返回 `<dir>/.mcp.json` 路径——写侧唯一出口，硬编码 `.mcp.json`，杜绝误写
/// `~/.claude.json` / `settings.json`（SS-14 铁律；grep 门禁：本文件写路径只此一处）。
fn mcp_json_path(project_dir: &str) -> Result<PathBuf, String> {
    let d = project_dir.trim();
    if d.is_empty() {
        return Err("project_dir 为空，拒绝写".into());
    }
    Ok(Path::new(d).join(".mcp.json"))
}

/// 读 `.mcp.json`（存在则解析，不存在给骨架 `{"mcpServers":{}}`）。
fn read_or_skeleton(mcp: &Path) -> Result<Value, String> {
    if mcp.is_file() {
        let raw =
            std::fs::read_to_string(mcp).map_err(|e| format!("read {}: {e}", mcp.display()))?;
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", mcp.display()))
    } else {
        Ok(serde_json::json!({ "mcpServers": {} }))
    }
}

/// 原子写 JSON（tmp + config::atomic_replace，Windows 安全）。同 INVARIANTS §4 定性。
fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    crate::config::atomic_replace(&tmp, path)
        .map_err(|e| format!("replace → {}: {e}", path.display()))
}

/// F87 写命令：增 / 改项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**
#[tauri::command]
pub fn write_project_mcp_server(
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("server 名为空".into());
    }
    let mcp = mcp_json_path(&project_dir)?;
    let mut root = read_or_skeleton(&mcp)?;
    let obj = root
        .as_object_mut()
        .ok_or_else(|| ".mcp.json 根不是对象".to_string())?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let smap = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers 不是对象".to_string())?;
    smap.insert(name, server);
    write_json_atomic(&mcp, &root)
}

/// F87 写命令：删项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**
#[tauri::command]
pub fn remove_project_mcp_server(project_dir: String, name: String) -> Result<(), String> {
    let mcp = mcp_json_path(&project_dir)?;
    if !mcp.is_file() {
        return Ok(()); // 无文件即无条目
    }
    let mut root = read_or_skeleton(&mcp)?;
    if let Some(smap) = root.get_mut("mcpServers").and_then(|m| m.as_object_mut()) {
        smap.remove(&name);
    }
    write_json_atomic(&mcp, &root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collect_merges_three_scopes() {
        let claude = json!({
            "mcpServers": { "user-srv": { "command": "u" } },
            "projects": { "/proj": { "mcpServers": { "local-srv": { "command": "l" } } } }
        });
        let proj = json!({ "mcpServers": { "proj-srv": { "type": "http", "url": "x" } } });
        let out = collect_entries(Some(&claude), "cj", Some(&proj), "mcp", Some("/proj"));
        assert_eq!(out.len(), 3);
        let scopes: Vec<&str> = out.iter().map(|e| e.scope.as_str()).collect();
        assert!(
            scopes.contains(&"user") && scopes.contains(&"local") && scopes.contains(&"project")
        );
        // server 原样保留
        let proj_e = out.iter().find(|e| e.scope == "project").unwrap();
        assert_eq!(proj_e.server["url"], json!("x"));
    }

    #[test]
    fn collect_tolerant_missing_and_bad_shapes() {
        // 全 None → 空
        assert!(collect_entries(None, "", None, "", None).is_empty());
        // mcpServers 非对象 → 跳过不崩
        let claude = json!({ "mcpServers": "not-an-object" });
        assert!(collect_entries(Some(&claude), "cj", None, "", Some("/p")).is_empty());
        // 有 user 无 local（projects 缺该 dir）
        let claude2 = json!({ "mcpServers": { "u": {} }, "projects": {} });
        let out = collect_entries(Some(&claude2), "cj", None, "", Some("/p"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].scope, "user");
    }

    #[test]
    fn write_and_remove_only_touch_mcp_json() {
        let tmp = std::env::temp_dir().join(format!("ccm-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let dir = tmp.to_string_lossy().into_owned();
        // 放一个假 .claude.json 在临时目录，断言写后它不变
        let fake_claude = tmp.join(".claude.json");
        std::fs::write(&fake_claude, "{\"mcpServers\":{\"keep\":{}}}").unwrap();

        // 写：建骨架 + 加条目
        write_project_mcp_server(dir.clone(), "srv".into(), json!({ "command": "c" })).unwrap();
        let mcp = tmp.join(".mcp.json");
        assert!(mcp.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["srv"]["command"], json!("c"));
        // .claude.json 一字未动
        assert_eq!(
            std::fs::read_to_string(&fake_claude).unwrap(),
            "{\"mcpServers\":{\"keep\":{}}}"
        );

        // 删
        remove_project_mcp_server(dir.clone(), "srv".into()).unwrap();
        let v2: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
        assert!(v2["mcpServers"].get("srv").is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_rejects_empty_project_dir() {
        assert!(write_project_mcp_server("  ".into(), "n".into(), json!({})).is_err());
        assert!(mcp_json_path("").is_err());
    }
}
