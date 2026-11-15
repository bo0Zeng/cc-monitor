//! Codex 的**资产布局**：skill 住哪、插件装在哪、怎么从盘上认出它们（注册表 `Adapter.assets` 那一格的本体）。
//!
//! 形照 codex-rs rust-v0.167.0：
//! - skill（`codex_skills_extension::host_roots` · `codex_skills_extension::loader::discovery`）：用户级住 `$CODEX_HOME/skills`（旧住处，Codex 照读）与
//!   `~/.agents/skills`；项目里住项目配置层的 `.codex/skills`，与项目根到工作目录之间每一层的 `.agents/skills`
//!   （项目根是往上第一个带 `.git` 的那一层，没有 ⇒ 只算工作目录本身）。一个目录带 `SKILL.md` 才算一个 skill，
//!   往下找至多 [`MAX_SCAN_DEPTH`] 层、点开头的跳过（`$CODEX_HOME/skills/.system` 是 Codex 自带的那几个）；
//!   名字先看头部 `name:`，没写才用目录名。
//! - 插件（`codex_core_plugin_common::installed` · `codex_core_plugins::store`）：`$CODEX_HOME/plugins/cache/<市场>/<插件>/<版本>/`，
//!   清单 `.codex-plugin/plugin.json` 的 `name` · `version` · `description`；用的那一版：有 `local` 就是它，否则取版本最大的；
//!   开关记在 `config.toml` 的 `[plugins."<名>@<市场>"] enabled`，没写 ⇒ 开着。装 · 删 · 列交 Codex 自己的命令（[`PLUGIN_CELLS`]）。
//! - Codex 0.167.0 没有住文件里的斜杠命令（自定义 prompts 已删）⇒ 会话那一份只有 skill。
//!
//! 读法宽容：缺的那一份不让整次扫描失败；在而读不出的**说出来**（[`Sightings::problems`]）。

use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::agents::{
    McpSeen, PluginFrom, PluginParts, PluginSeen, SessionAssets, Sightings, SkillSeen,
};
use crate::common::said::IntoNote as _;

/// skill 目录名（配置根 · `.agents` · `.codex` 底下都叫它）。
pub const SKILLS_DIR: &str = "skills";
/// 一个 skill 的说明文件。
pub const SKILL_DOC: &str = "SKILL.md";
/// 跨 agent 共用的那一层目录名（家目录与项目里都有：`~/.agents/skills` · `<目录>/.agents/skills`）。
const AGENTS_DIR: &str = ".agents";
/// 项目里装 Codex 配置的那一层目录名（项目配置层，skill 住 `<目录>/.codex/skills`）。
pub(crate) const PROJECT_CONFIG_DIR: &str = ".codex";
/// 认项目根的标记（Codex 缺省的项目根标记就这一个）。
const PROJECT_ROOT_MARKER: &str = ".git";
/// skill 往下找的层数（同 Codex `MAX_SCAN_DEPTH`）。
const MAX_SCAN_DEPTH: usize = 6;
/// `SKILL.md` 的读取上限（找头部用；手写的说明远不到这个量级）。超了 ⇒ 不取说明、说出来。
const SKILL_DOC_MAX_BYTES: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::HardError,
};

/// 插件缓存（配置根下）：`plugins/cache/<市场>/<插件>/<版本>/`。
const PLUGIN_CACHE: [&str; 2] = ["plugins", "cache"];
/// 插件清单（相对插件根）。
const PLUGIN_MANIFEST: [&str; 2] = [".codex-plugin", "plugin.json"];
/// 清单没写 `version` 时装进去的那一版的名字（同 `codex_core_plugin_common::installed::DEFAULT_PLUGIN_VERSION`）。
pub const LOCAL_VERSION: &str = "local";
/// 插件自带 MCP 的那份文件（相对插件根）。
const PLUGIN_MCP: &str = ".mcp.json";
/// 读清单 · 插件 MCP 的上限（手写的几行 JSON）。
const PLUGIN_JSON_MAX_BYTES: Cap = Cap {
    bytes: 256 * 1024,
    over: Over::SkipAndSay,
};
/// 插件那一格（注册表 `AssetFace.plugin_cells`）：装 · 删 · 列交 Codex 自己的命令（codex 0.167，本机
/// `codex plugin --help` 核过）—— `codex plugin add <名>@<市场> --json` · `codex plugin remove <名>@<市场> --json` ·
/// `codex plugin list --json`。`add` · `remove` 收 `PLUGIN[@MARKETPLACE]`，光名字又不带 `--marketplace` ⇒ 它报错退出，
/// 所以交进来的是这一家自己的写法 `<名>@<市场>`（即 `pluginId`，扫目录那一份的 [`PluginSeen::key`]），原样接上。
/// 命令只在默认那个家跑，插件只到用户级（`CODEX_HOME`）。
pub(crate) const PLUGIN_CELLS: crate::agents::PluginCells = crate::agents::PluginCells {
    install: |id| plugin_argv(&["add", id, "--json"]),
    remove: |id| plugin_argv(&["remove", id, "--json"]),
    list: || plugin_argv(&["list", "--json"]),
    read_list: read_plugin_list,
};

fn plugin_argv(rest: &[&str]) -> Vec<String> {
    [super::resume::DEFAULT_COMMAND, "plugin"]
        .iter()
        .chain(rest)
        .map(|s| s.to_string())
        .collect()
}

/// `codex plugin list --json` 的输出（`JsonPluginListOutput`：`installed` · `available` 两张，每条 camelCase）⇒ 装着的那几个：
/// `key` 取 `pluginId`（装 · 删交回去的就是它），`name` 取光名字 `name`，版本 `version`（没有 ⇒ 缺），开不开照 `enabled`；
/// `available` 那张不算。不是那一形（不是对象 · 没有 `installed` 数组 · 一条缺 `pluginId` / `name` / `enabled`）⇒ `None`（不猜）。
fn read_plugin_list(stdout: &str) -> Option<Vec<crate::agents::PluginListed>> {
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    v.get("installed")?
        .as_array()?
        .iter()
        .map(|p| {
            Some(crate::agents::PluginListed {
                key: p.get("pluginId")?.as_str()?.to_string(),
                name: p.get("name")?.as_str()?.to_string(),
                version: p
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                on: p.get("enabled")?.as_bool()?,
            })
        })
        .collect()
}

// ───────────────────────── skill ─────────────────────────

/// 「装到这台」的用户级 skill 落点：`~/.agents/skills`（Codex 现在的用户级住处；`$CODEX_HOME/skills` 是旧的，只读不写）。
pub(crate) fn skills_root() -> Option<PathBuf> {
    crate::platform::paths::home_dir().map(|h| skills_root_in(&h))
}

/// [`skills_root`] 的本体：家目录是参数。
pub fn skills_root_in(home: &Path) -> PathBuf {
    home.join(AGENTS_DIR).join(SKILLS_DIR)
}

/// 一个项目目录里 skill 的落点：`<项目>/.agents/skills`（不要项目被信任过就认；`.codex/skills` 要）。
pub fn project_skills_root(project: &Path) -> PathBuf {
    project.join(AGENTS_DIR).join(SKILLS_DIR)
}

/// `root` 底下的 skill：带 `SKILL.md` 的目录各一条（找到就不再往它里面找）；`project` 原样记进每一条。根不在 ⇒ 什么都不加。
pub(crate) fn scan_skills_at(root: &Path, project: Option<&str>, out: &mut Sightings) {
    let mut found = Vec::new();
    let mut seen = Vec::new();
    walk_skills(root, 0, project, &mut seen, &mut found, &mut out.problems);
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.dir.cmp(&b.dir)));
    out.skills.extend(found);
}

fn walk_skills(
    dir: &Path,
    depth: usize,
    project: Option<&str>,
    seen: &mut Vec<PathBuf>,
    found: &mut Vec<SkillSeen>,
    problems: &mut Vec<String>,
) {
    // 链接跟到底；绕回来的那一层不再走。
    if let Ok(real) = Ro::canonicalize(dir) {
        if seen.contains(&real) {
            return;
        }
        seen.push(real);
    }
    let rd = match Ro::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            problems.push(read_failed(dir, &e));
            return;
        }
    };
    let mut subs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| Ro::metadata(p).map(|m| m.is_dir()).unwrap_or(false))
        .collect();
    subs.sort();
    for sub in subs {
        let doc = sub.join(SKILL_DOC);
        if Ro::metadata(&doc).map(|m| m.is_file()).unwrap_or(false) {
            let dir_name = sub
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (name, description) = match front_matter(&doc) {
                Ok(f) => (f.name.unwrap_or(dir_name), f.description),
                Err(e) => {
                    problems.push(
                        e.wrap(|why| {
                            copy_text(
                                "beAssets.skill.docFailed",
                                &[("path", &doc.display().to_string()), ("why", why)],
                            )
                        })
                        .into_note(),
                    );
                    (dir_name, None)
                }
            };
            found.push(SkillSeen {
                project: project.map(str::to_string),
                name,
                dir: sub,
                description,
            });
        } else if depth + 1 < MAX_SCAN_DEPTH {
            walk_skills(&sub, depth + 1, project, seen, found, problems);
        }
    }
}

/// `SKILL.md` 头部（三个 `-` 包着的那一段）里的 `name:` 与 `description:`。
#[derive(Default)]
struct Front {
    name: Option<String>,
    description: Option<String>,
}

fn front_matter(doc: &Path) -> Result<Front, crate::common::said::Said> {
    let bytes = Ro::read_capped(doc, SKILL_DOC_MAX_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    let fence = "-".repeat(3);
    let mut out = Front::default();
    if lines.next().map(str::trim) != Some(fence.as_str()) {
        return Ok(out);
    }
    let value = |v: &str| {
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        (!v.is_empty()).then(|| v.to_string())
    };
    for line in lines {
        if line.trim() == fence {
            break;
        }
        if let Some(v) = line.strip_prefix("name:") {
            out.name = out.name.or_else(|| value(v));
        } else if let Some(v) = line.strip_prefix("description:") {
            out.description = out.description.or_else(|| value(v));
        }
    }
    Ok(out)
}

/// 一个工作目录往上到项目根的每一层（工作目录在前）：项目根是往上第一个带 `.git` 的那一层；一路都没有 ⇒ 只有工作目录。
pub(crate) fn project_layers(cwd: &Path) -> Vec<PathBuf> {
    let mut up = Vec::new();
    for d in cwd.ancestors() {
        up.push(d.to_path_buf());
        if Ro::metadata(d.join(PROJECT_ROOT_MARKER)).is_ok() {
            return up;
        }
    }
    vec![cwd.to_path_buf()]
}

/// 一个项目（工作目录）能用的项目 skill：它往上到项目根的每一层各两处（`.codex/skills` · `.agents/skills`），都记成这个项目的。
fn scan_project(cwd: &Path, label: &str, out: &mut Sightings) {
    for dir in project_layers(cwd) {
        scan_skills_at(
            &dir.join(PROJECT_CONFIG_DIR).join(SKILLS_DIR),
            Some(label),
            out,
        );
        scan_skills_at(&project_skills_root(&dir), Some(label), out);
    }
}

// ───────────────────────── 扫描（注册表那几格） ─────────────────────────

/// 注册表 `AssetFace.scan`：用户级两处 skill ＋ 交进来的那份用户级 MCP ＋ 每个项目两处 ＋ 这个家里装的插件，现扫。
pub(crate) fn scan(projects: &[String], user_mcp: Option<&Path>) -> Sightings {
    let codex_home = super::home();
    let home = crate::platform::paths::home_dir();
    let mut out = scan_at(codex_home.as_deref(), home.as_deref(), user_mcp, projects);
    if let Some(root) = codex_home.as_deref() {
        scan_plugins_at(root, &mut out);
    }
    out
}

/// [`scan`] 除插件外的那几样：Codex 的家 · 家目录 · 用户级 MCP 那份 `config.toml` 由调用方给（判据拿临时目录喂）。
/// MCP 读法与 MCP 读面同一份（[`super::mcp::read_at`]）：用户级那份 ＋ 每个项目从它往上到项目根的 `.codex/config.toml`。
pub fn scan_at(
    codex_home: Option<&Path>,
    home: Option<&Path>,
    user_mcp: Option<&Path>,
    projects: &[String],
) -> Sightings {
    let mut out = Sightings::default();
    if let Some(root) = codex_home {
        scan_skills_at(&root.join(SKILLS_DIR), None, &mut out);
    }
    if let Some(h) = home {
        scan_skills_at(&skills_root_in(h), None, &mut out);
    }
    if user_mcp.is_some() {
        mcp_into(
            super::mcp::read_at(user_mcp, None, &Default::default()),
            None,
            &mut out,
        );
    }
    for dir in projects {
        scan_project(Path::new(dir), dir, &mut out);
        let read = super::mcp::read_at(None, Some(Path::new(dir)), &Default::default());
        mcp_into(read, Some(dir), &mut out);
    }
    out
}

/// MCP 读面读出来的条目 ⇒ 资产目录那一形（`project` 原样记进每一条）。
fn mcp_into(read: crate::agents::McpRead, project: Option<&str>, out: &mut Sightings) {
    out.problems.extend(read.problems);
    out.mcp.extend(read.entries.into_iter().map(|e| McpSeen {
        project: project.map(str::to_string),
        name: e.name,
        def: e.server,
        file: PathBuf::from(e.source),
    }));
}

/// 注册表 `AssetFace.session_scan`：那个号的 `$CODEX_HOME/skills` ＋ `~/.agents/skills` ＋ 工作目录到项目根每一层的两处。
pub(crate) fn session_scan(project: Option<&Path>, config_root: &Path) -> SessionAssets {
    session_scan_at(
        project,
        config_root,
        crate::platform::paths::home_dir().as_deref(),
    )
}

/// [`session_scan`] 的本体：家目录是参数。
pub fn session_scan_at(
    project: Option<&Path>,
    config_root: &Path,
    home: Option<&Path>,
) -> SessionAssets {
    let mut seen = Sightings::default();
    if let Some(cwd) = project {
        scan_project(cwd, &cwd.display().to_string(), &mut seen);
    }
    scan_skills_at(&config_root.join(SKILLS_DIR), None, &mut seen);
    if let Some(h) = home {
        scan_skills_at(&skills_root_in(h), None, &mut seen);
    }
    SessionAssets {
        commands: Vec::new(),
        skills: seen.skills,
        problems: seen.problems,
    }
}

// ───────────────────────── 插件 ─────────────────────────

/// 一个插件装着的一版：插件根 ＋ 版本目录的名字。
struct Version {
    root: PathBuf,
    dir_name: String,
}

fn subdirs(d: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Ro::read_dir(d)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| Ro::metadata(p).map(|m| m.is_dir()).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 缓存里 `(市场, 插件目录, 那几版)`。
fn installs_at(config_root: &Path) -> Vec<(String, Vec<Version>)> {
    let cache = PLUGIN_CACHE
        .iter()
        .fold(config_root.to_path_buf(), |p, s| p.join(s));
    let mut out = Vec::new();
    for market in subdirs(&cache) {
        for plugin in subdirs(&market) {
            let versions = subdirs(&plugin)
                .into_iter()
                .map(|root| Version {
                    dir_name: name_of(&root),
                    root,
                })
                .collect();
            out.push((name_of(&market), versions));
        }
    }
    out
}

/// 版本号比大小：都能拆成点分的数 ⇒ 按数比，否则按字（同 Codex：semver 解得开按它，解不开按字）。
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let nums = |s: &str| -> Option<Vec<u64>> { s.split('.').map(|p| p.parse().ok()).collect() };
    match (nums(a), nums(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

/// 用的那一版：有 `local` ⇒ 它；否则版本最大的。
fn active(versions: Vec<Version>) -> Option<Version> {
    if let Some(i) = versions.iter().position(|v| v.dir_name == LOCAL_VERSION) {
        return versions.into_iter().nth(i);
    }
    versions
        .into_iter()
        .max_by(|a, b| version_cmp(&a.dir_name, &b.dir_name))
}

/// 读一份 JSON：不在 ⇒ `Ok(None)`；在而读不出 / 不是 JSON ⇒ `Err(一句)`。
fn read_json(file: &Path) -> Result<Option<serde_json::Value>, String> {
    match Ro::metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(read_failed(file, &e)),
        Ok(_) => {}
    }
    Ro::read_json(file, PLUGIN_JSON_MAX_BYTES)
        .map(Some)
        .map_err(|e| {
            e.wrap(|why| {
                copy_text(
                    "reason.fs.readFailed",
                    &[("path", &file.display().to_string()), ("why", why)],
                )
            })
            .into_note()
        })
}

fn manifest_path(root: &Path) -> PathBuf {
    PLUGIN_MANIFEST
        .iter()
        .fold(root.to_path_buf(), |p, s| p.join(s))
}

/// 注册表 `AssetFace.plugins`：一个配置根底下装着的插件（每一版一个插件根 ＋ 清单里的名字）。没有清单 / 读不懂 / 没名字的不算。
pub(crate) fn plugins(config_root: &Path) -> Vec<(PathBuf, String)> {
    installs_at(config_root)
        .into_iter()
        .flat_map(|(_, versions)| versions)
        .filter_map(|v| {
            let m = read_json(&manifest_path(&v.root)).ok()??;
            let name = m.get("name")?.as_str()?.to_string();
            Some((v.root, name))
        })
        .collect()
}

/// 一个插件带了什么：`skills/` 底下的 skill · `.mcp.json` 里的服务器名。
fn parts_of(root: &Path, problems: &mut Vec<String>) -> PluginParts {
    let mut seen = Sightings::default();
    scan_skills_at(&root.join(SKILLS_DIR), None, &mut seen);
    problems.extend(seen.problems);
    let mut skills: Vec<String> = seen.skills.into_iter().map(|s| s.name).collect();
    skills.sort();
    let mut mcp: Vec<String> = match read_json(&root.join(PLUGIN_MCP)) {
        Ok(v) => v
            .as_ref()
            .and_then(|v| v.get("mcpServers"))
            .and_then(serde_json::Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default(),
        Err(why) => {
            problems.push(why);
            Vec::new()
        }
    };
    mcp.sort();
    PluginParts {
        skills,
        mcp,
        ..PluginParts::default()
    }
}

/// `config.toml` 里 `[plugins."<id>"] enabled` 那一格（没写 · 不是布尔 · 读不懂 ⇒ `None`）。
fn enabled_in(config: Option<&str>, id: &str) -> Option<bool> {
    let doc = super::accounts::config_doc(config?).ok()?;
    super::accounts::item_at(&doc, &["plugins", id, "enabled"])?.as_bool()
}

/// 这个家里装的插件逐个读出来（用的那一版）：名字 · 版本（清单没写 ⇒ `local`）· 说明 · 带了什么 · 开没开。
pub(crate) fn scan_plugins_at(config_root: &Path, out: &mut Sightings) {
    let installs = installs_at(config_root);
    if installs.is_empty() {
        return;
    }
    let cfg_file = config_root.join(super::accounts::CONFIG_FILE);
    // 开关那几格读不出来（那份在而读不动 · 解不开）⇒ 说一句，插件照「没写」当开着。
    let config = match Ro::metadata(&cfg_file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        _ => match Ro::read_capped(&cfg_file, super::mcp::CONFIG_CAP) {
            Ok(b) => Some(String::from_utf8_lossy(&b).into_owned()),
            Err(e) => {
                out.problems.push(
                    e.wrap(|why| {
                        copy_text(
                            "reason.fs.readFailed",
                            &[("path", &cfg_file.display().to_string()), ("why", why)],
                        )
                    })
                    .into_note(),
                );
                None
            }
        },
    };
    if config
        .as_deref()
        .is_some_and(|t| super::accounts::config_doc(t).is_err())
    {
        out.problems.push(unparsable(&cfg_file));
    }
    let mut found = Vec::new();
    for (market, versions) in installs {
        let Some(v) = active(versions) else {
            continue;
        };
        let manifest = match read_json(&manifest_path(&v.root)) {
            Ok(Some(m)) => m,
            // 没有清单的目录不是插件，不用说。
            Ok(None) => continue,
            Err(why) => {
                out.problems.push(why);
                continue;
            }
        };
        let text = |k: &str| {
            manifest
                .get(k)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let Some(name) = text("name") else {
            out.problems.push(unparsable(&manifest_path(&v.root)));
            continue;
        };
        let id = format!("{name}@{market}");
        let set = enabled_in(config.as_deref(), &id);
        found.push(PluginSeen {
            project: None,
            version: Some(text("version").unwrap_or_else(|| LOCAL_VERSION.to_string())),
            description: text("description"),
            parts: parts_of(&v.root, &mut out.problems),
            on: set.unwrap_or(true),
            from: if set.is_some() {
                PluginFrom::User
            } else {
                PluginFrom::Manifest
            },
            name,
            key: id,
            market,
            root: v.root,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.market.cmp(&b.market)));
    out.plugins.extend(found);
}

/// 那一份在、但内容解不开那一句（只带路径）。
fn unparsable(path: &Path) -> String {
    copy_text(
        "reason.fs.readFailed",
        &[
            ("path", &path.display().to_string()),
            ("why", &copy_text("reason.content.unparsable", &[])),
        ],
    )
}

/// 读不动那一句（只带路径与原因词，原话进日志）。
fn read_failed(path: &Path, e: &std::io::Error) -> String {
    crate::common::said::Said::with_raw(
        copy_text(
            "reason.fs.readFailed",
            &[
                ("path", &path.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
    .into_note()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/assets_tests.rs"]
mod tests;
