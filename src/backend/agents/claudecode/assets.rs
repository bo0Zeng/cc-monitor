//! Claude 的**资产布局**：skill 住哪、MCP 住哪、怎么从盘上认出它们。
//!
//! 只装布局知识（用户级 `skills/<名>/` 与项目里的 `.claude/skills/<名>/` · `SKILL.md` 头部的 `description:` ·
//! 用户级 `.claude.json` 顶层的 `mcpServers` · `<项目>/.mcp.json` 的 `mcpServers`），**不做摘要、不做合并、不记目录** ——
//! 那些是通用层的机器（`asset_catalog.rs`），经注册表 `agents::Adapter.assets` 那一格拿本层的知识（不增 `ADAPTER_CALL_SITES`）。
//! 扫哪几个项目由通用层交进来（这台上开过会话的项目目录），本层不另起一份项目清单。
//!
//! 读法宽容：缺 / 坏的那一份不让整次扫描失败，但**说出来**（[`Sightings::problems`]）—— 「这台没有」与
//! 「这台那份读不出来」不许合成一句。

use crate::platform::fs::Ro;
use copy_core::copy_text;
use relay_route_core::{Cap, Over};
use std::path::{Path, PathBuf};

use super::accounts::{config_path_in, CONFIG_CAP};
use super::paths::{resolve_home, CONFIG_DIR_ENV};
use crate::agents::{McpSeen, Sightings, SkillSeen};
use crate::common::said::IntoNote as _;

/// skill 目录名（配置根下）。
pub const SKILLS_DIR: &str = "skills";
/// 项目里装配置的那一层目录名（项目级 skill 住 `<项目>/.claude/skills`）。
const PROJECT_CONFIG_DIR: &str = ".claude";
/// 一个 skill 的说明文件。
pub const SKILL_DOC: &str = "SKILL.md";
/// 项目级 MCP 配置的文件名。
pub const PROJECT_MCP_FILE: &str = ".mcp.json";
/// `.mcp.json` / `.claude.json` 里装 server 表的键。
pub(crate) const SERVERS_KEY: &str = "mcpServers";
/// 一份项目 `.mcp.json` 的读取上限（手写的配置，远不到这个量级）：超了那一份跳过、记进 `problems`，不截断。
pub(crate) const PROJECT_MCP_CAP: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::SkipAndSay,
};
/// `SKILL.md` 的读取上限（找头部 `description:` 用；手写的说明远不到这个量级）。超了 ⇒ 错交给调用方，不取说明、记进 `problems`。
const SKILL_DOC_MAX_BYTES: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::HardError,
};

/// 插件缓存（配置根下）：`plugins/cache/<市场>/<插件>/<版本>/` 每个版本一个插件根。
const PLUGIN_CACHE: [&str; 2] = ["plugins", "cache"];
/// 插件清单（相对插件根）。
const PLUGIN_MANIFEST: [&str; 2] = [".claude-plugin", "plugin.json"];
/// 读清单的上限（手写的几行 JSON，远不到这个量级）：超了那一份跳过、记日志。
const PLUGIN_MANIFEST_MAX_BYTES: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::SkipAndSay,
};

/// 一个配置根底下装着的插件：插件根 ＋ 清单里的名字。插件根从两处来：`skills/<名>/`（用户自己放进来的，常是指向插件根的链接）
/// · 插件缓存里每个版本一个。没有清单 / 读不懂 / 没名字的目录不算插件；读不了的那一层当没有。
pub(crate) fn plugins(config_root: &Path) -> Vec<(PathBuf, String)> {
    fn subdirs(d: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = Ro::read_dir(d)
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
            let bytes = match Ro::read_capped(&m, PLUGIN_MANIFEST_MAX_BYTES) {
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
pub fn project_skills_root(project: &Path) -> PathBuf {
    project.join(PROJECT_CONFIG_DIR).join(SKILLS_DIR)
}

/// 注册表那一格的实现：用户级 skill 的根 ＋ 交进来的那份用户级 MCP ＋ 每个项目各两处 ＋ 这台的插件，现扫。
pub(crate) fn scan(projects: &[String], user_mcp: Option<&Path>) -> Sightings {
    let mut out = scan_at(skills_root().as_deref(), user_mcp, projects);
    scan_plugins_at(
        &resolve_home(),
        &super::asks::policy_files(),
        projects,
        &mut out,
    );
    out
}

/// [`scan`] 的本体：用户级两个根由调用方给（判据拿临时家目录喂）。
pub fn scan_at(
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
    let rd = match Ro::read_dir(root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems.push(
                crate::common::said::Said::with_raw(
                    copy_text(
                        "reason.fs.readFailed",
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
        if !Ro::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) {
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
        if project.is_none() && is_plugin_root(&path) {
            continue; // 带清单的是插件（[`scan_plugins_at`]），它带的 skill 不另成一行
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
    let bytes = Ro::read_capped(doc, SKILL_DOC_MAX_BYTES)?;
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
pub fn scan_user_mcp_at(claude_json: &Path, out: &mut Sightings) {
    if !claude_json.exists() {
        return;
    }
    match Ro::read_json(claude_json, CONFIG_CAP) {
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
    match Ro::metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems.push(
                crate::common::said::Said::with_raw(
                    copy_text(
                        "reason.fs.readFailed",
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
    match Ro::read_json(&file, PROJECT_MCP_CAP) {
        Ok(v) => servers_into(&v, &file, Some(&dir.display().to_string()), out),
        Err(e) => {
            let e = e.wrap(|why| {
                copy_text(
                    "reason.fs.readFailed",
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

// ───────────────────────── 插件 ─────────────────────────

/// 插件安装记录（配置根下）：`{"version": 2, "plugins": {"<名>@<市场>": [{scope, installPath, version, projectPath}]}}`。
const PLUGIN_RECORD: [&str; 2] = ["plugins", "installed_plugins.json"];
/// 认得的安装记录格式版本。
const PLUGIN_RECORD_V: u64 = 2;
/// 安装记录的读取上限（几百条也远不到）：超了当没有、记日志 ＋ 记进 `problems`。
const PLUGIN_RECORD_MAX_BYTES: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::SkipAndSay,
};
/// 放在 skills 目录里的插件属的那个市场名（`enabledPlugins` 里写 `<名>@skills-dir`）。
const SKILLS_DIR_MARKET: &str = "skills-dir";
/// 设置里写开关的那一格。
const ENABLED_KEY: &str = "enabledPlugins";
/// 远端下发的托管设置（配置根下；与策略文件同一级）。
const REMOTE_SETTINGS: &str = "remote-settings.json";
/// 一份设置的读取上限：超了当没写、记日志 ＋ 记进 `problems`。
const SETTINGS_MAX_BYTES: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::SkipAndSay,
};
/// 插件里装 hook 的那份（相对插件根）。
const PLUGIN_HOOKS: [&str; 2] = ["hooks", "hooks.json"];

/// 这个目录是插件根（底下有清单）。
fn is_plugin_root(dir: &Path) -> bool {
    PLUGIN_MANIFEST
        .iter()
        .fold(dir.to_path_buf(), |p, s| p.join(s))
        .is_file()
}

/// 盘上看到的一处安装：`<名>@<市场>` · 插件根 · 装在哪个项目里 · 安装记录里写的版本。
struct Install {
    name: String,
    market: String,
    root: PathBuf,
    project: Option<String>,
    version: Option<String>,
}

/// 找插件时读的几种 JSON（各自的读取上限）。
#[derive(Clone, Copy)]
enum PluginJson {
    /// 安装记录。
    Record,
    /// 一份设置（只为取 `enabledPlugins`）· 插件根里的 `hooks/hooks.json`（只为数事件名）。
    Settings,
    /// 插件清单。
    Manifest,
    /// 插件根里的 `.mcp.json`（只为数 server 名）。
    Mcp,
}

/// 读一份 JSON：不在 ⇒ `None`（不是问题）；在却读不出 ⇒ 记日志 ＋ 说出来、`None`（那一份当没有）。
fn read_json_said(
    file: &Path,
    what: PluginJson,
    problems: &mut Vec<String>,
) -> Option<serde_json::Value> {
    if matches!(Ro::metadata(file), Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
        return None;
    }
    let cap = match what {
        PluginJson::Record => PLUGIN_RECORD_MAX_BYTES,
        PluginJson::Settings => SETTINGS_MAX_BYTES,
        PluginJson::Manifest => PLUGIN_MANIFEST_MAX_BYTES,
        PluginJson::Mcp => PROJECT_MCP_CAP,
    };
    match Ro::read_json(file, cap) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("[plugins] {} 读不出来，当没有：{}", file.display(), e.said);
            problems.push(
                e.wrap(|why| {
                    copy_text(
                        "reason.fs.readFailed",
                        &[("path", &file.display().to_string()), ("why", why)],
                    )
                })
                .into_note(),
            );
            None
        }
    }
}

/// 一份设置里的 `enabledPlugins`（`<名>@<市场>` → 开 / 关）；没有这一格 ⇒ 空。
fn enabled_in(file: &Path, problems: &mut Vec<String>) -> Vec<(String, bool)> {
    read_json_said(file, PluginJson::Settings, problems)
        .and_then(|v| v.get(ENABLED_KEY).and_then(|e| e.as_object()).cloned())
        .map(|m| {
            m.into_iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k, b)))
                .collect()
        })
        .unwrap_or_default()
}

/// 几份设置按优先级从低到高叠：后写的压过先写的。
fn layered(
    files: &[PathBuf],
    problems: &mut Vec<String>,
) -> std::collections::BTreeMap<String, bool> {
    files.iter().flat_map(|f| enabled_in(f, problems)).collect()
}

/// 目录底下的子目录名 / 某种扩展名的文件名（去扩展名），排好序；读不了 ⇒ 空。
fn names_in(dir: &Path, ext: Option<&str>, want_dir: bool) -> Vec<String> {
    let mut v: Vec<String> = Ro::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    let is_dir = Ro::metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
                    if is_dir != want_dir {
                        return None;
                    }
                    match ext {
                        Some(x) => (p.extension().and_then(|s| s.to_str()) == Some(x))
                            .then(|| p.file_stem()?.to_str().map(str::to_string))
                            .flatten(),
                        None => e.file_name().to_str().map(str::to_string),
                    }
                })
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// 一个对象的键（排好序）；不是对象 ⇒ 空。
fn keys_of(v: Option<&serde_json::Value>) -> Vec<String> {
    let mut k: Vec<String> = v
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    k.sort();
    k
}

/// 插件根里带了什么：skills（带 `SKILL.md` 的子目录）· commands / agents（`.md`）· hooks（事件名）· MCP（server 名）。
/// 清单里内联写的 hooks / mcpServers 一并算。
fn parts_of(
    root: &Path,
    manifest: Option<&serde_json::Value>,
    problems: &mut Vec<String>,
) -> crate::agents::PluginParts {
    let skills = names_in(&root.join(SKILLS_DIR), None, true)
        .into_iter()
        .filter(|n| root.join(SKILLS_DIR).join(n).join(SKILL_DOC).is_file())
        .collect();
    let hooks_file = PLUGIN_HOOKS
        .iter()
        .fold(root.to_path_buf(), |p, s| p.join(s));
    let mut hooks = keys_of(
        read_json_said(&hooks_file, PluginJson::Settings, problems)
            .as_ref()
            .and_then(|v| v.get("hooks")),
    );
    hooks.extend(keys_of(manifest.and_then(|m| m.get("hooks"))));
    hooks.sort();
    hooks.dedup();
    let mut mcp = keys_of(
        read_json_said(&root.join(PROJECT_MCP_FILE), PluginJson::Mcp, problems)
            .as_ref()
            .and_then(|v| v.get(SERVERS_KEY)),
    );
    mcp.extend(keys_of(manifest.and_then(|m| m.get(SERVERS_KEY))));
    mcp.sort();
    mcp.dedup();
    crate::agents::PluginParts {
        skills,
        commands: names_in(&root.join(COMMANDS_DIR), Some("md"), false),
        agents: names_in(&root.join("agents"), Some("md"), false),
        hooks,
        mcp,
    }
}

/// 盘上的各处安装：安装记录（第 [`PLUGIN_RECORD_V`] 版）记的 ＋ `skills/<名>/` 里带清单的。
fn installs_at(config_root: &Path, problems: &mut Vec<String>) -> Vec<Install> {
    let mut out = Vec::new();
    let record = PLUGIN_RECORD
        .iter()
        .fold(config_root.to_path_buf(), |p, s| p.join(s));
    if let Some(v) = read_json_said(&record, PluginJson::Record, problems) {
        match v.get("plugins").and_then(|p| p.as_object()) {
            Some(plugins) if v.get("version").and_then(|x| x.as_u64()) == Some(PLUGIN_RECORD_V) => {
                for (id, entries) in plugins {
                    let Some((name, market)) = id.rsplit_once('@') else {
                        continue;
                    };
                    for e in entries.as_array().into_iter().flatten() {
                        let Some(root) = e.get("installPath").and_then(|p| p.as_str()) else {
                            continue;
                        };
                        let in_project = matches!(
                            e.get("scope").and_then(|s| s.as_str()),
                            Some("project" | "local")
                        );
                        out.push(Install {
                            name: name.to_string(),
                            market: market.to_string(),
                            root: PathBuf::from(root),
                            project: in_project
                                .then(|| e.get("projectPath").and_then(|p| p.as_str()))
                                .flatten()
                                .map(str::to_string),
                            version: e
                                .get("version")
                                .and_then(|x| x.as_str())
                                .map(str::to_string),
                        });
                    }
                }
            }
            _ => problems.push(copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &record.display().to_string()),
                    ("why", &copy_text("reason.content.unparsable", &[])),
                ],
            )),
        }
    }
    let skills = config_root.join(SKILLS_DIR);
    for dir in names_in(&skills, None, true) {
        let root = skills.join(&dir);
        if !is_plugin_root(&root) {
            continue;
        }
        let name = manifest_at(&root, &mut Vec::new())
            .and_then(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .unwrap_or(dir);
        out.push(Install {
            name,
            market: SKILLS_DIR_MARKET.to_string(),
            root,
            project: None,
            version: None,
        });
    }
    out
}

fn manifest_at(root: &Path, problems: &mut Vec<String>) -> Option<serde_json::Value> {
    let m = PLUGIN_MANIFEST
        .iter()
        .fold(root.to_path_buf(), |p, s| p.join(s));
    read_json_said(&m, PluginJson::Manifest, problems)
}

/// 插件：两处读盘（[`installs_at`]），开不开照六处 `enabledPlugins` 合并 ——
/// 托管（策略文件 ＋ 配置根下 [`REMOTE_SETTINGS`]）＞ 本机项目设置 ＞ 项目设置 ＞ 用户设置（`settings.json` · `settings.local.json`）＞ 清单 `defaultEnabled`（没写 ⇒ 关）。
/// 用户级装着的出一条用户级的，再给设置里写了它的每个项目各出一条；装在项目里的只出那个项目的。不读 `plugins/synced/`，不读凭据。
pub(crate) fn scan_plugins_at(
    config_root: &Path,
    policy: &[PathBuf],
    projects: &[String],
    out: &mut Sightings,
) {
    use crate::agents::{PluginFrom, PluginSeen};
    let mut problems = Vec::new();
    let installs = installs_at(config_root, &mut problems);
    if installs.is_empty() {
        out.problems.extend(problems);
        return;
    }
    let mut managed_files = policy.to_vec();
    managed_files.push(config_root.join(REMOTE_SETTINGS));
    let managed = layered(&managed_files, &mut problems);
    let user = layered(
        &super::footprint::user_settings_files(config_root),
        &mut problems,
    );
    let mut by_project: std::collections::BTreeMap<
        String,
        [std::collections::BTreeMap<String, bool>; 2],
    > = Default::default();
    let mut project_layers = |dir: &str, problems: &mut Vec<String>| {
        by_project
            .entry(dir.to_string())
            .or_insert_with(|| {
                let [shared, local] = super::footprint::project_settings_files(Path::new(dir));
                [layered(&[shared], problems), layered(&[local], problems)]
            })
            .clone()
    };
    let mut seen: std::collections::BTreeSet<(String, Option<String>)> = Default::default();
    for i in installs {
        let id = format!("{}@{}", i.name, i.market);
        let manifest = manifest_at(&i.root, &mut problems);
        let text = |k: &str| {
            manifest
                .as_ref()
                .and_then(|m| m.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        let default_on = manifest
            .as_ref()
            .and_then(|m| m.get("defaultEnabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let parts = parts_of(&i.root, manifest.as_ref(), &mut problems);
        let decide = |layers: Option<&[std::collections::BTreeMap<String, bool>; 2]>| {
            if let Some(b) = managed.get(&id) {
                return (*b, PluginFrom::Managed);
            }
            if let Some([shared, local]) = layers {
                if let Some(b) = local.get(&id) {
                    return (*b, PluginFrom::Local);
                }
                if let Some(b) = shared.get(&id) {
                    return (*b, PluginFrom::Project);
                }
            }
            match user.get(&id) {
                Some(b) => (*b, PluginFrom::User),
                None => (default_on, PluginFrom::Manifest),
            }
        };
        let mut emit = |project: Option<String>, (on, from): (bool, PluginFrom)| {
            if !seen.insert((i.name.clone(), project.clone())) {
                return;
            }
            out.plugins.push(PluginSeen {
                project,
                name: i.name.clone(),
                market: i.market.clone(),
                root: i.root.clone(),
                version: text("version").or_else(|| i.version.clone()),
                description: text("description"),
                parts: parts.clone(),
                on,
                from,
            });
        };
        match &i.project {
            Some(dir) => {
                let layers = project_layers(dir, &mut problems);
                emit(Some(dir.clone()), decide(Some(&layers)));
            }
            None => {
                emit(None, decide(None));
                for dir in projects {
                    let layers = project_layers(dir, &mut problems);
                    if layers.iter().any(|l| l.contains_key(&id)) {
                        emit(Some(dir.clone()), decide(Some(&layers)));
                    }
                }
            }
        }
    }
    problems.dedup();
    out.problems.extend(problems);
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/assets_tests.rs"]
mod tests;

/// 斜杠命令住的目录名（`<项目>/.claude/commands` · `<配置根>/commands`）。
const COMMANDS_DIR: &str = "commands";

/// 一份命令文件最多读多少（只取 front matter 与第一行）：超了那一份跳过、记进 `problems`，不列这一条。
const COMMAND_DOC_MAX_BYTES: Cap = Cap {
    bytes: 256 * 1024,
    over: Over::SkipAndSay,
};

/// Claude Code 内置、手机 ［+］ 面板会用到的几条（`/clear` 丢掉对话 ⇒ 先确认；`/model` 不带名字会弹选择框 ⇒ 要参数）。
pub(crate) const BUILTIN_COMMANDS: &[crate::agents::BuiltinCommand] = &[
    crate::agents::BuiltinCommand {
        name: "clear",
        text: || copy_text("beSessionCommands.builtin.clear", &[]),
        needs_args: false,
        confirm: true,
    },
    crate::agents::BuiltinCommand {
        name: "compact",
        text: || copy_text("mobPlus.row.compact", &[]),
        needs_args: false,
        confirm: false,
    },
    crate::agents::BuiltinCommand {
        name: "init",
        text: || copy_text("beSessionCommands.builtin.init", &[]),
        needs_args: false,
        confirm: false,
    },
    crate::agents::BuiltinCommand {
        name: "model",
        text: || copy_text("beSessionCommands.builtin.model", &[]),
        needs_args: true,
        confirm: false,
    },
];

/// 一条会话能用的命令与 skill：本项目 `.claude/commands` · `.claude/skills` ＋ 号的配置根 `commands` · `skills`。
pub(crate) fn session_scan(
    project: Option<&Path>,
    config_root: &Path,
) -> crate::agents::SessionAssets {
    let mut out = crate::agents::SessionAssets::default();
    let mut seen = Sightings::default();
    if let Some(p) = project {
        scan_commands_at(
            &p.join(PROJECT_CONFIG_DIR).join(COMMANDS_DIR),
            true,
            &mut out,
        );
        scan_skills_at(
            &project_skills_root(p),
            Some(&p.display().to_string()),
            &mut seen,
        );
    }
    scan_commands_at(&config_root.join(COMMANDS_DIR), false, &mut out);
    scan_skills_at(&config_root.join(SKILLS_DIR), None, &mut seen);
    out.skills = seen.skills;
    out.problems.extend(seen.problems);
    out
}

/// 一处命令目录：顶层的 `*.md` ＋ 下一层子目录里的 `*.md`；同一处按名字排。目录不在 ⇒ 什么都不加。
fn scan_commands_at(root: &Path, project: bool, out: &mut crate::agents::SessionAssets) {
    let mut found = Vec::new();
    let mut files = Vec::new();
    list_md(root, 0, &mut files, out);
    for path in files {
        let Some(name) = path
            .file_stem()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let bytes = match Ro::read_capped(&path, COMMAND_DOC_MAX_BYTES) {
            Err(e) => {
                tracing::warn!("一条会话能用的命令：跳过读不出的 {}", path.display());
                out.problems.push(
                    e.wrap(|why| {
                        copy_text(
                            "reason.fs.readFailed",
                            &[("path", &path.display().to_string()), ("why", why)],
                        )
                    })
                    .into_note(),
                );
                continue;
            }
            Ok(b) => b,
        };
        let (description, needs_args) = command_doc(&String::from_utf8_lossy(&bytes));
        found.push(crate::agents::CommandSeen {
            name,
            project,
            description,
            needs_args,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    out.commands.extend(found);
}

/// 列一个目录里的 `*.md`（`depth` 0 时再下一层子目录）。不在 ⇒ 不说；读不出 ⇒ 说出来。
fn list_md(
    dir: &Path,
    depth: u8,
    files: &mut Vec<PathBuf>,
    out: &mut crate::agents::SessionAssets,
) {
    let rd = match Ro::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.problems.push(
                crate::common::said::Said::with_raw(
                    copy_text(
                        "reason.fs.readFailed",
                        &[
                            ("path", &dir.display().to_string()),
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
    for ent in rd.flatten() {
        let path = ent.path();
        let hidden = ent.file_name().to_str().is_none_or(|n| n.starts_with('.'));
        if hidden {
            continue;
        }
        let meta = Ro::metadata(&path);
        if meta.as_ref().is_ok_and(|m| m.is_dir()) {
            if depth == 0 {
                list_md(&path, 1, files, out);
            }
        } else if path.extension().and_then(|x| x.to_str()) == Some("md") {
            files.push(path);
        }
    }
}

/// 一份命令文件 ⇒（简介, 要不要参数）。简介：front matter 的 `description`，没有 ⇒ 正文第一行非空（去掉标题井号）。
/// 要参数：front matter 有 `argument-hint`，或正文里有 `$ARGUMENTS` / `$1`。
pub(crate) fn command_doc(text: &str) -> (Option<String>, bool) {
    let fence = "-".repeat(3);
    let all: Vec<&str> = text.lines().collect();
    let mut description = None;
    let mut hint = false;
    let mut start = 0;
    if all.first().map(|l| l.trim()) == Some(fence.as_str()) {
        start = all.len();
        for (i, line) in all.iter().enumerate().skip(1) {
            if line.trim() == fence {
                start = i + 1;
                break;
            }
            if let Some(v) = line.strip_prefix("description:") {
                let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
                description = (!v.is_empty()).then(|| v.to_string());
            }
            if line.starts_with("argument-hint:") {
                hint = true;
            }
        }
    }
    let body = &all[start..];
    let description = description.or_else(|| {
        body.iter()
            .map(|l| l.trim().trim_start_matches('#').trim())
            .find(|l| !l.is_empty())
            .map(str::to_string)
    });
    let args = hint
        || body
            .iter()
            .any(|l| l.contains("$ARGUMENTS") || l.contains("$1"));
    (description, args)
}
