//! 〔AS2 · 第四波 4B · V113〕Claude 的**资产布局**：skill 住哪、项目级 MCP 住哪、怎么从盘上认出它们。
//!
//! 只装布局知识（`skills/<名>/` · `SKILL.md` 头部的 `description:` · `.claude.json` 的 `projects` 键 ·
//! `<项目>/.mcp.json` 的 `mcpServers`），**不做摘要、不做合并、不记目录** —— 那些是通用层的机器
//! （`asset_catalog.rs`），经注册表 `agents::Adapter.assets` 那一格拿本层的知识（不增 `ADAPTER_CALL_SITES`）。
//!
//! 读法宽容：缺 / 坏的那一份不让整次扫描失败，但**说出来**（[`Sightings::problems`]）—— 「这台没有」与
//! 「这台那份读不出来」不许合成一句。

use std::path::{Path, PathBuf};

use crate::common::fs::read_regular_capped;

use super::accounts::{config_path_in, MAX_CONFIG_BYTES};
use super::paths::{resolve_home, CONFIG_DIR_ENV};
use crate::agents::Sightings;

/// skill 目录名（配置根下）。
const SKILLS_DIR: &str = "skills";
/// 一个 skill 的说明文件。
const SKILL_DOC: &str = "SKILL.md";
/// 项目级 MCP 配置的文件名。
pub(crate) const PROJECT_MCP_FILE: &str = ".mcp.json";
/// `.mcp.json` / `.claude.json` 里装 server 表的键。
const SERVERS_KEY: &str = "mcpServers";
/// 一份项目 `.mcp.json` 的读取上限（手写的配置，远不到这个量级；超了说出来，不截断）。
const MAX_PROJECT_MCP_BYTES: u64 = 4 * 1024 * 1024;
/// `SKILL.md` 的读取上限（找头部 `description:` 用；手写的说明远不到这个量级）。超了 ⇒ 不取说明、说出来。
const SKILL_DOC_MAX_BYTES: u64 = 1024 * 1024;

/// 这台机器上 skill 的根：`<配置根>/skills`。
pub(crate) fn skills_root() -> Option<PathBuf> {
    Some(resolve_home().join(SKILLS_DIR))
}

/// 列项目的那份 `.claude.json`：设了 `CLAUDE_CONFIG_DIR` ⇒ 它下面那一份；否则 `$HOME/.claude.json`
/// （与 monitor `mcp.rs::claude_json_candidates` 的两个主候选同序）。家目录都没有 ⇒ `None`。
fn claude_json() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV).filter(|d| !d.is_empty()) {
        return Some(config_path_in(Path::new(&dir)));
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(|h| config_path_in(Path::new(&h)))
}

/// 注册表那一格的实现：按这台机器的环境现解两个根，现扫。
pub(crate) fn scan() -> Sightings {
    let mut out = Sightings::default();
    if let Some(root) = skills_root() {
        scan_skills_at(&root, &mut out);
    }
    match claude_json() {
        Some(cfg) => scan_mcp_at(&cfg, &mut out),
        None => out
            .problems
            .push("家目录解析不出来，列不出项目（MCP 那一半没扫）".to_string()),
    }
    out
}

/// skill：`<root>/<名>/` 每个目录一条（顶层链接跟到底；名字不是 UTF-8 的跳过并说出来）。
pub(crate) fn scan_skills_at(root: &Path, out: &mut Sightings) {
    let rd = match std::fs::read_dir(root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems
                .push(format!("列 {} 失败：{e}", root.display()));
            return;
        }
    };
    for ent in rd.flatten() {
        let path = ent.path();
        // `metadata` 跟链接：`skills/foo -> ~/repo/foo` 是常见装法。
        if !std::fs::metadata(&path)
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(name) = ent.file_name().to_str().map(str::to_string) else {
            out.problems
                .push(format!("{} 的名字不是 UTF-8，没记", path.display()));
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let doc = path.join(SKILL_DOC);
        let description = match skill_description(&doc) {
            Ok(d) => d,
            Err(e) => {
                out.problems.push(format!(
                    "读 {} 失败：{e}（这个 skill 没取到说明）",
                    doc.display()
                ));
                None
            }
        };
        out.skills.push((name, path, description));
    }
    out.skills.sort_by(|a, b| a.0.cmp(&b.0));
}

/// `SKILL.md` 头部 front matter（三个 `-` 包着的那一段）里的 `description:`。没有这份文件 / 没有那一格 ⇒ `Ok(None)`；
/// 读不出来（超上限 / 不是常规文件 / 权限）⇒ `Err`（调用方说出来）。
fn skill_description(doc: &Path) -> Result<Option<String>, String> {
    if !doc.exists() {
        return Ok(None);
    }
    let bytes = read_regular_capped(doc, SKILL_DOC_MAX_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    // front matter 的围栏是一行三个 `-`（写成 `repeat` 是为了不让协议对拍把它认成一个 `--子命令` 字面量）。
    let fence = "-".repeat(3);
    if lines.next().map(str::trim) != Some(fence.as_str()) {
        return Ok(None);
    }
    for line in lines {
        if line.trim() == fence {
            break;
        }
        if let Some(v) = line.strip_prefix("description:") {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            return Ok((!v.is_empty()).then(|| v.to_string()));
        }
    }
    Ok(None)
}

/// 项目级 MCP：`.claude.json` 的 `projects` 键 × 各自的 `<dir>/.mcp.json`。
///
/// `.claude.json` 不在 ⇒ 这台没用过（不是问题）；在但读不出来 ⇒ 说出来。
/// 一个项目没有 `.mcp.json` ⇒ 跳过（绝大多数项目都没有）；有但坏了 ⇒ 说出来，**不当成空表**。
pub(crate) fn scan_mcp_at(claude_json: &Path, out: &mut Sightings) {
    if !claude_json.exists() {
        return;
    }
    let cfg: serde_json::Value = match read_regular_capped(claude_json, MAX_CONFIG_BYTES)
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            out.problems.push(format!(
                "读 {} 失败：{e}（MCP 那一半没扫）",
                claude_json.display()
            ));
            return;
        }
    };
    let mut dirs: Vec<String> = cfg
        .get("projects")
        .and_then(serde_json::Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let file = Path::new(&dir).join(PROJECT_MCP_FILE);
        match std::fs::metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                out.problems
                    .push(format!("看 {} 失败：{e}", file.display()));
                continue;
            }
            Ok(_) => {}
        }
        let parsed: Result<serde_json::Value, String> =
            read_regular_capped(&file, MAX_PROJECT_MCP_BYTES)
                .and_then(|b| serde_json::from_slice(strip_bom(&b)).map_err(|e| e.to_string()));
        let v = match parsed {
            Ok(v) => v,
            Err(e) => {
                let why = format!("读 {} 失败：{e}", file.display());
                tracing::warn!("资产目录：{why}");
                out.problems.push(why);
                continue;
            }
        };
        let Some(servers) = v.get(SERVERS_KEY) else {
            continue;
        };
        let Some(servers) = servers.as_object() else {
            out.problems.push(format!(
                "{} 里 `{SERVERS_KEY}` 不是对象，没记",
                file.display()
            ));
            continue;
        };
        for (name, def) in servers {
            out.mcp.push((dir.clone(), name.clone(), def.clone()));
        }
    }
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/assets_tests.rs"]
mod tests;
