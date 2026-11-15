//! 假 agent 的资产与 MCP 面（`McpFace`）。

use super::FAKE_FILE_CAP;
use crate::platform::fs::Ro;
use std::path::Path;

/// 假 agent 的 MCP 读面（夹具家那一份布局）：只认 `<项目>/.fake-mcp.json` 的 `servers` 表，一律记成 project 段。
/// 它要证的是「通用层经注册表那一格读 MCP、不认识任何一家的文件名」—— 判据 `fake_tests.rs::the_fake_agents_mcp_face_is_read_through_the_generic_layer`。
pub const MCP: crate::agents::McpFace = crate::agents::McpFace {
    read: read_mcp,
    login: Some(mcp_login_argv),
    sign_in: None,
    needs_login_file: None,
};

/// 同一份 MCP 读面，登录那一格不声明（判「没声明 ⇒ 那一项不出」用）。
pub const MCP_NO_LOGIN: crate::agents::McpFace = crate::agents::McpFace { login: None, ..MCP };

/// 假 agent 登录一个 MCP 服务器：`fakeagent mcp-auth <名>`（与 Claude 的斜杠命令、Codex 的 `mcp login` 都不同形）。
pub fn mcp_login_argv(name: &str) -> Vec<String> {
    [super::launch::DEFAULT_COMMAND, "mcp-auth", name]
        .map(str::to_string)
        .to_vec()
}

/// 假 agent 的资产面：skill 住 `<项目>/.fake-agent/abilities/<名>/`（与 Claude 的 `.claude/skills`、Codex 的都不同名）；
/// 没有用户级 skill 根（同它的家：没有默认）；用户级 MCP 只认交进来的那一份，格式同各号那一份（[`super::MCP_CELLS`]，行式）。
pub const ASSETS: crate::agents::AssetFace = crate::agents::AssetFace {
    scan,
    skills_root: || None,
    project_skills_root,
    user_mcp_file: || None,
    project_mcp_file: ".fake-mcp.json",
    servers_key: "servers",
    plugins: |_| Vec::new(),
    plugin_cells: Some(PLUGINS),
    session_scan: |_, _| crate::agents::SessionAssets::default(),
    builtin_commands: &[],
};

/// [`ASSETS_USER_SKILLS`] 的用户级 skill 根：一个测试进程只设一次（没设 ⇒ 说不出）。判据收尾会删它，所以只许一条判据用这一面。
pub static USER_SKILLS: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// 同 [`ASSETS`]，只是说得出用户级 skill 根（[`USER_SKILLS`]）：判「用户级 skill 根照注入的注册表那一家取」用。
pub const ASSETS_USER_SKILLS: crate::agents::AssetFace = crate::agents::AssetFace {
    skills_root: || USER_SKILLS.get().cloned(),
    ..ASSETS
};

/// 假 agent 的插件装删那一格（与 Claude 的 `plugin install`、Codex 的 `plugin add … --json` 都不同形）：
/// `fakeagent gadget fit <写法>` · `fakeagent gadget drop <写法>` · `fakeagent gadget roster`；
/// 它的写法是 `<市场>/<名>`（与光名字不同，也与 Claude 的 `<名>@<市场>` 不同形）；
/// 列的输出一行一个 `<市场>/<名> <版本|-> <on|off>`，别的样子（写法里没有 `/` 也算）一律读不懂。
pub const PLUGINS: crate::agents::PluginCells = crate::agents::PluginCells {
    install: |key| gadget(&["fit", key]),
    remove: |key| gadget(&["drop", key]),
    list: || gadget(&["roster"]),
    read_list: read_roster,
};

fn gadget(rest: &[&str]) -> Vec<String> {
    [super::launch::DEFAULT_COMMAND, "gadget"]
        .iter()
        .chain(rest)
        .map(|s| s.to_string())
        .collect()
}

fn read_roster(stdout: &str) -> Option<Vec<crate::agents::PluginListed>> {
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [key, version, on @ ("on" | "off")] => Some(crate::agents::PluginListed {
                key: key.to_string(),
                name: key.rsplit_once('/')?.1.to_string(),
                version: (version != "-").then(|| version.to_string()),
                on: on == "on",
            }),
            _ => None,
        })
        .collect()
}

/// 项目里 skill 的根。
pub fn project_skills_root(project: &Path) -> std::path::PathBuf {
    project.join(".fake-agent").join("abilities")
}

/// 第二家替身（`fake2`）自己的资产面：项目级 skill 住 `<项目>/.fake-b/skills/<名>/`、项目级 MCP 住 `fake2-mcp.conf`
/// （都与 [`ASSETS`]、与注册表里各家不同名）；用户级的根与 MCP 一律说不出（不读真环境）。
pub const ASSETS_B: crate::agents::AssetFace = crate::agents::AssetFace {
    scan: scan_b,
    project_skills_root: second_project_skills_root,
    project_mcp_file: "fake2-mcp.conf",
    plugin_cells: None,
    ..ASSETS
};

/// 第二家替身项目里 skill 的根。
pub fn second_project_skills_root(project: &Path) -> std::path::PathBuf {
    project.join(".fake-b").join("skills")
}

/// 两个替身家：`fake`（默认起的那一家，[`ASSETS`]）· `fake2`（换了家名，资产面是 [`ASSETS_B`]）。判两家的判据都拿它。
pub fn two_families() -> &'static [crate::agents::Adapter] {
    static TWO: std::sync::OnceLock<Vec<crate::agents::Adapter>> = std::sync::OnceLock::new();
    TWO.get_or_init(|| {
        let second = crate::agents::Adapter {
            assets: Some(ASSETS_B),
            ..super::family_row("fake2", "CCM_FAKE_B", "fakeb", false, || None)
        };
        vec![
            super::family_row("fake", "CCM_FAKE_A", "fakea", true, || None),
            second,
        ]
    })
}

fn scan(projects: &[String], user_mcp: Option<&Path>) -> crate::agents::Sightings {
    scan_under(project_skills_root, projects, user_mcp)
}

fn scan_b(projects: &[String], user_mcp: Option<&Path>) -> crate::agents::Sightings {
    scan_under(second_project_skills_root, projects, user_mcp)
}

fn scan_under(
    skills_root: fn(&Path) -> std::path::PathBuf,
    projects: &[String],
    user_mcp: Option<&Path>,
) -> crate::agents::Sightings {
    let mut out = crate::agents::Sightings::default();
    for p in projects {
        let root = skills_root(Path::new(p));
        let Ok(dir) = Ro::read_dir(&root) else {
            continue;
        };
        let mut names: Vec<_> = dir
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            out.skills.push(crate::agents::SkillSeen {
                project: Some(p.clone()),
                dir: root.join(&name),
                name,
                description: None,
            });
        }
    }
    if let Some(file) = user_mcp {
        match Ro::read_string_capped(file, FAKE_FILE_CAP).map_err(|e| e.to_string()) {
            Ok(text) => {
                match (super::MCP_CELLS.read)(&text) {
                    Ok(servers) => out.mcp.extend(servers.into_iter().map(|(name, def)| {
                        crate::agents::McpSeen {
                            project: None,
                            name,
                            def,
                            file: file.to_path_buf(),
                        }
                    })),
                    Err(e) => out.problems.push(e),
                }
            }
            Err(e) => out
                .problems
                .push(format!("读 {} 失败：{e}", file.display())),
        }
    }
    out
}

fn read_mcp(project_dir: Option<&Path>, _look: &crate::agents::McpLook) -> crate::agents::McpRead {
    let mut out = crate::agents::McpRead::default();
    let Some(dir) = project_dir else { return out };
    let file = dir.join(".fake-mcp.json");
    let Ok(text) = Ro::read_string_capped(&file, FAKE_FILE_CAP) else {
        return out;
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => {
            for (name, server) in v
                .get("servers")
                .and_then(|s| s.as_object())
                .into_iter()
                .flatten()
            {
                // 定义里写了 `"needsLogin": true` ⇒ 记成要登录（没设账号那一位）；判「要登录的 MCP」那几条用。
                let needs = server
                    .get("needsLogin")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
                out.entries.push(crate::agents::McpEntry {
                    scope: "project",
                    name: name.clone(),
                    server: server.clone(),
                    source: file.display().to_string(),
                    status: if needs {
                        crate::agents::McpStatus::NeedsLogin
                    } else {
                        Default::default()
                    },
                    login_in: if needs { vec![None] } else { Vec::new() },
                    seen_ms: None,
                });
            }
        }
        Err(e) => out
            .problems
            .push(format!("读 {} 失败：{e}", file.display())),
    }
    out
}
