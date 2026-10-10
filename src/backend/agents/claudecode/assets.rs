//! Claude 的**资产布局**：skill 住哪、MCP 住哪、怎么从盘上认出它们。
//!
//! 只装布局知识（用户级 `skills/<名>/` 与项目里的 `.claude/skills/<名>/` · `SKILL.md` 头部的 `description:` ·
//! 用户级 `.claude.json` 顶层的 `mcpServers` · `<项目>/.mcp.json` 的 `mcpServers`），**不做摘要、不做合并、不记目录** ——
//! 那些是通用层的机器（`asset_catalog.rs`），经注册表 `agents::Adapter.assets` 那一格拿本层的知识（不增 `ADAPTER_CALL_SITES`）。
//! 扫哪几个项目由通用层交进来（这台上开过会话的项目目录），本层不另起一份项目清单。
//!
//! 读法宽容：缺 / 坏的那一份不让整次扫描失败，但**说出来**（[`Sightings::problems`]）—— 「这台没有」与
//! 「这台那份读不出来」不许合成一句。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::common::fs::read_regular_capped;

use super::accounts::{config_path_in, MAX_CONFIG_BYTES};
use super::paths::{resolve_home, CONFIG_DIR_ENV};
use crate::agents::{McpSeen, Sightings, SkillSeen};
use crate::common::said::IntoNote as _;

/// skill 目录名（配置根下）。
const SKILLS_DIR: &str = "skills";
/// 项目里装配置的那一层目录名（项目级 skill 住 `<项目>/.claude/skills`）。
const PROJECT_CONFIG_DIR: &str = ".claude";
/// 一个 skill 的说明文件。
const SKILL_DOC: &str = "SKILL.md";
/// 项目级 MCP 配置的文件名。
pub(crate) const PROJECT_MCP_FILE: &str = ".mcp.json";
/// `.mcp.json` / `.claude.json` 里装 server 表的键。
pub(crate) const SERVERS_KEY: &str = "mcpServers";
/// 一份项目 `.mcp.json` 的读取上限（手写的配置，远不到这个量级；超了说出来，不截断）。
pub(crate) const MAX_PROJECT_MCP_BYTES: u64 = 4 * 1024 * 1024;
/// `SKILL.md` 的读取上限（找头部 `description:` 用；手写的说明远不到这个量级）。超了 ⇒ 不取说明、说出来。
const SKILL_DOC_MAX_BYTES: u64 = 1024 * 1024;

/// 插件缓存（配置根下）：`plugins/cache/<市场>/<插件>/<版本>/` 每个版本一个插件根。
const PLUGIN_CACHE: [&str; 2] = ["plugins", "cache"];
/// 插件清单（相对插件根）。
const PLUGIN_MANIFEST: [&str; 2] = [".claude-plugin", "plugin.json"];
/// 读清单的上限（手写的几行 JSON，远不到这个量级）。
const PLUGIN_MANIFEST_MAX_BYTES: u64 = 64 * 1024;

/// 一个配置根底下装着的插件：插件根 ＋ 清单里的名字。插件根从两处来：`skills/<名>/`（用户自己放进来的，常是指向插件根的链接）
/// · 插件缓存里每个版本一个。没有清单 / 读不懂 / 没名字的目录不算插件；读不了的那一层当没有。
pub(crate) fn plugins(config_root: &Path) -> Vec<(PathBuf, String)> {
    fn subdirs(d: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
    let mut dirs = subdirs(&config_root.join(SKILLS_DIR));
    let cache = PLUGIN_CACHE
        .iter()
        .fold(config_root.to_path_buf(), |p, s| p.join(s));
    for market in subdirs(&cache) {
        for plugin in subdirs(&market) {
            dirs.extend(subdirs(&plugin));
        }
    }
    dirs.into_iter()
        .filter_map(|d| {
            let m = PLUGIN_MANIFEST.iter().fold(d.clone(), |p, s| p.join(s));
            if !m.exists() {
                return None; // 普通 skill 目录：不是插件，不用说
            }
            let bytes = match read_regular_capped(&m, PLUGIN_MANIFEST_MAX_BYTES) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        "[plugins] {} 读不出来，这个目录不算插件：{}",
                        m.display(),
                        e.said
                    );
                    return None;
                }
            };
            let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            let name = v.get("name")?.as_str()?.to_string();
            Some((d, name))
        })
        .collect()
}

/// 这台机器上 skill 的根：`<配置根>/skills`。
pub(crate) fn skills_root() -> Option<PathBuf> {
    Some(resolve_home().join(SKILLS_DIR))
}

/// 列项目的那份 `.claude.json`：设了 `CLAUDE_CONFIG_DIR` ⇒ 它下面那一份；否则 `$HOME/.claude.json`
/// （MCP 列表读法 `claudecode/mcp.rs` 也用这一份 —— monitor 那份三候选删了）。家目录都没有 ⇒ `None`。
pub(crate) fn claude_json() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV).filter(|d| !d.is_empty()) {
        return Some(config_path_in(Path::new(&dir)));
    }
    crate::platform::paths::home_dir().map(|h| config_path_in(&h))
}

/// 一个项目目录里 skill 的根：`<项目>/.claude/skills`。
pub(crate) fn project_skills_root(project: &Path) -> PathBuf {
    project.join(PROJECT_CONFIG_DIR).join(SKILLS_DIR)
}

/// 注册表那一格的实现：用户级 skill 的根 ＋ 交进来的那份用户级 MCP ＋ 每个项目各两处，现扫。
pub(crate) fn scan(projects: &[String], user_mcp: Option<&Path>) -> Sightings {
    scan_at(skills_root().as_deref(), user_mcp, projects)
}

/// [`scan`] 的本体：用户级两个根由调用方给（判据拿临时家目录喂）。
pub(crate) fn scan_at(
    skills: Option<&Path>,
    claude_json: Option<&Path>,
    projects: &[String],
) -> Sightings {
    let mut out = Sightings::default();
    if let Some(root) = skills {
        scan_skills_at(root, None, &mut out);
    }
    match claude_json {
        Some(cfg) => scan_user_mcp_at(cfg, &mut out),
        None => out
            .problems
            .push(copy_text("beClaudeAssets.scan.noHome", &[])),
    }
    for dir in projects {
        let p = Path::new(dir);
        scan_skills_at(&project_skills_root(p), Some(dir), &mut out);
        scan_project_mcp_at(p, &mut out);
    }
    out
}

/// skill：`<root>/<名>/` 每个目录一条（顶层链接跟到底；名字不是 UTF-8 的跳过并说出来）。`project` 原样记进每一条。
pub(crate) fn scan_skills_at(root: &Path, project: Option<&str>, out: &mut Sightings) {
    let rd = match std::fs::read_dir(root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems.push(
                crate::common::said::Said::with_raw(
                    copy_text(
                        "beClaudeAssets.scan.listFailed",
                        &[
                            ("path", &root.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
                .into_note(),
            );
            return;
        }
    };
    let mut found = Vec::new();
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
            out.problems.push(copy_text(
                "beClaudeAssets.scan.notUtf8",
                &[("path", &path.display().to_string())],
            ));
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let doc = path.join(SKILL_DOC);
        let description = match skill_description(&doc) {
            Ok(d) => d,
            Err(e) => {
                out.problems.push(
                    e.wrap(|why| {
                        copy_text(
                            "beClaudeAssets.skill.docFailed",
                            &[("path", &doc.display().to_string()), ("why", why)],
                        )
                    })
                    .into_note(),
                );
                None
            }
        };
        found.push(SkillSeen {
            project: project.map(str::to_string),
            name,
            dir: path,
            description,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    out.skills.extend(found);
}

/// `SKILL.md` 头部 front matter（三个 `-` 包着的那一段）里的 `description:`。没有这份文件 / 没有那一格 ⇒ `Ok(None)`；
/// 读不出来（超上限 / 不是常规文件 / 权限）⇒ `Err`（调用方说出来）。
fn skill_description(doc: &Path) -> Result<Option<String>, crate::common::said::Said> {
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

/// 用户级 MCP：`.claude.json` 顶层的 `mcpServers`。文件不在 ⇒ 这台没用过（不是问题）；在但读不出来 ⇒ 说出来。
pub(crate) fn scan_user_mcp_at(claude_json: &Path, out: &mut Sightings) {
    if !claude_json.exists() {
        return;
    }
    match crate::common::fs::read_json_capped(claude_json, MAX_CONFIG_BYTES) {
        Ok(v) => servers_into(&v, claude_json, None, out),
        // 这一条只是资产目录上的一句说明（没有复制详情那一格）：原话记日志。
        Err(e) => out.problems.push(
            e.wrap(|why| {
                copy_text(
                    "beClaudeAssets.mcp.configFailed",
                    &[("path", &claude_json.display().to_string()), ("why", why)],
                )
            })
            .into_note(),
        ),
    }
}

/// 项目级 MCP：`<dir>/.mcp.json` 的 `mcpServers`。没有这份 ⇒ 跳过（绝大多数项目都没有）；有但坏了 ⇒ 说出来，**不当成空表**。
pub(crate) fn scan_project_mcp_at(dir: &Path, out: &mut Sightings) {
    let file = dir.join(PROJECT_MCP_FILE);
    match std::fs::metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems.push(
                crate::common::said::Said::with_raw(
                    copy_text(
                        "beClaudeAssets.mcp.statFailed",
                        &[
                            ("path", &file.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
                .into_note(),
            );
            return;
        }
        Ok(_) => {}
    }
    match crate::common::fs::read_json_capped(&file, MAX_PROJECT_MCP_BYTES) {
        Ok(v) => servers_into(&v, &file, Some(&dir.display().to_string()), out),
        Err(e) => {
            let e = e.wrap(|why| {
                copy_text(
                    "beClaudeAssets.mcp.readFailed",
                    &[("path", &file.display().to_string()), ("why", why)],
                )
            });
            tracing::warn!("资产目录：{}", e.logged());
            out.problems.push(e.said);
        }
    }
}

/// 一份配置里 `mcpServers` 的每一条（没有这一格 ⇒ 什么都没有；不是对象 ⇒ 说出来）。
fn servers_into(v: &serde_json::Value, file: &Path, project: Option<&str>, out: &mut Sightings) {
    let Some(servers) = v.get(SERVERS_KEY) else {
        return;
    };
    let Some(servers) = servers.as_object() else {
        out.problems.push(copy_text(
            "beClaudeAssets.mcp.notObject",
            &[("path", &file.display().to_string())],
        ));
        return;
    };
    for (name, def) in servers {
        out.mcp.push(McpSeen {
            project: project.map(str::to_string),
            name: name.clone(),
            def: def.clone(),
            file: file.to_path_buf(),
        });
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/assets_tests.rs"]
mod tests;
