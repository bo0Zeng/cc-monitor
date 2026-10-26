//! `agents/claudecode/assets.rs` 的判据：Claude 的资产布局认得对。
//!
//! 守的要求：「本机后端在本机看见一个skill并记录下来」「mcp保持项目级别」；
//! 资产 = skill（`~/.claude/skills/*`）与 MCP 定义（项目 `.mcp.json` 里的条目）。
//!
//! 买到：临时目录上真读 —— skill 逐个目录（链接跟到底、文件与点开头的跳过）· `description:` 从 front matter 取 ·
//! MCP 用户级（`.claude.json` 顶层）与交进来的每个项目的 `.mcp.json` 逐条 · 坏的那一份**说出来**而不是当空表。
//! 买不到：真 `~/.claude` 的形状（夹具是按公开布局造的）；Windows 路径没跑过。

use super::*;
use crate::agents::{McpSeen, Sightings};

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-ccassets-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

#[test]
fn skills_are_the_directories_under_the_root_with_their_description() {
    let d = temp_dir("skills");
    let root = d.join("skills");
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::write(
        root.join("alpha").join(SKILL_DOC),
        "---\nname: alpha\ndescription: \"Does alpha things\"\n---\n# body\ndescription: not this\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("beta")).unwrap();
    std::fs::write(
        root.join("beta").join(SKILL_DOC),
        "no front matter\ndescription: nope\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::write(root.join("loose-file.md"), "x").unwrap();
    std::fs::create_dir_all(d.join("elsewhere").join("gamma")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(d.join("elsewhere").join("gamma"), root.join("gamma")).unwrap();

    let mut out = Sightings::default();
    scan_skills_at(&root, None, &mut out);
    let got: Vec<(String, Option<String>)> = out
        .skills
        .iter()
        .map(|k| (k.name.clone(), k.description.clone()))
        .collect();
    let mut want = vec![
        ("alpha".to_string(), Some("Does alpha things".to_string())),
        ("beta".to_string(), None),
    ];
    #[cfg(unix)]
    want.push(("gamma".to_string(), None));
    assert_eq!(got, want);
    assert!(out.problems.is_empty(), "{:?}", out.problems);

    let mut none = Sightings::default();
    scan_skills_at(&d.join("no-such"), None, &mut none);
    assert_eq!(
        none,
        Sightings::default(),
        "没有 skills 目录 ⇒ 什么都没有，也不算问题"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn mcp_is_the_user_level_table_plus_each_given_projects_mcp_json_and_broken_ones_are_said() {
    let d = temp_dir("mcp");
    let (p1, p2, p3) = (d.join("p1"), d.join("p2"), d.join("p3"));
    for p in [&p1, &p2, &p3] {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(
        p1.join(PROJECT_MCP_FILE),
        "\u{feff}{\"mcpServers\":{\"fs\":{\"command\":\"/opt/fs\"},\"gh\":{\"url\":\"https://x\"}}}",
    )
    .unwrap();
    std::fs::write(p3.join(PROJECT_MCP_FILE), "{broken").unwrap();
    let cfg = d.join(".claude.json");
    std::fs::write(
        &cfg,
        serde_json::json!({ "mcpServers": { "u": { "command": "u-cmd" } }, "projects": {} })
            .to_string(),
    )
    .unwrap();
    let projects: Vec<String> = [&p1, &p2, &p3]
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let out = scan_at(None, Some(&cfg), &projects);
    let got: Vec<(Option<String>, String)> = out
        .mcp
        .iter()
        .map(|m| (m.project.clone(), m.name.clone()))
        .collect();
    let p1s = p1.to_string_lossy().into_owned();
    assert_eq!(
        got,
        vec![
            (None, "u".to_string()),
            (Some(p1s.clone()), "fs".to_string()),
            (Some(p1s), "gh".to_string())
        ]
    );
    assert_eq!(
        out.mcp[1],
        McpSeen {
            project: Some(p1.to_string_lossy().into_owned()),
            name: "fs".into(),
            def: serde_json::json!({"command": "/opt/fs"}),
            file: p1.join(PROJECT_MCP_FILE),
        },
        "定义原样、住哪一份也记下"
    );
    assert_eq!(
        out.problems.len(),
        1,
        "坏的那一份要说出来：{:?}",
        out.problems
    );
    assert!(out.problems[0].contains("p3"), "{:?}", out.problems);

    let none = scan_at(None, Some(&d.join("no-such.json")), &[]);
    assert_eq!(
        none,
        Sightings::default(),
        "没有 .claude.json ⇒ 这台没用过，不是问题"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 覆盖面：一个临时家目录里 skill × 用户级 / 项目级、MCP × 用户级 / 项目级四格各放一个 —— 项目取自这台的会话清单
/// （会话记录里的 `cwd`，只采结构），目录四格各认出一个（两向相等）。
#[test]
fn the_catalog_sees_one_of_each_kind_at_each_level() {
    let d = temp_dir("four");
    let agent_home = d.join(".claude");
    let proj = d.join("proj");
    let skill = |root: &Path, name: &str| {
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(
            root.join(name).join(SKILL_DOC),
            "---\ndescription: x\n---\n",
        )
        .unwrap();
    };
    skill(&agent_home.join(SKILLS_DIR), "su");
    skill(&project_skills_root(&proj), "sp");
    std::fs::write(
        d.join(".claude.json"),
        serde_json::json!({ "mcpServers": { "mu": { "command": "u" } } }).to_string(),
    )
    .unwrap();
    std::fs::write(
        proj.join(PROJECT_MCP_FILE),
        serde_json::json!({ "mcpServers": { "mp": { "command": "p" } } }).to_string(),
    )
    .unwrap();
    let sessions = agent_home.join("projects").join("-proj");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("00000000-0000-0000-0000-000000000001.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({ "type": "user", "cwd": proj.display().to_string() })
        ),
    )
    .unwrap();

    let (projects, why) = crate::assets::asset_catalog::session_projects_at(&agent_home);
    assert_eq!(
        (projects.clone(), why),
        (vec![proj.display().to_string()], None)
    );
    let seen = scan_at(
        Some(&agent_home.join(SKILLS_DIR)),
        Some(&d.join(".claude.json")),
        &projects,
    );
    let (assets, problems) = crate::assets::asset_catalog::assets_from(&[seen]);
    assert!(problems.is_empty(), "{problems:?}");
    use crate::assets::asset_catalog::ExtKind;
    let got: std::collections::BTreeSet<(ExtKind, String, bool)> = assets
        .iter()
        .map(|a| (a.kind, a.name.clone(), a.project.is_some()))
        .collect();
    let want: std::collections::BTreeSet<(ExtKind, String, bool)> = [
        (ExtKind::Skill, "su", false),
        (ExtKind::Skill, "sp", true),
        (ExtKind::Mcp, "mu", false),
        (ExtKind::Mcp, "mp", true),
    ]
    .iter()
    .map(|(k, n, p)| (*k, n.to_string(), *p))
    .collect();
    assert_eq!(got, want);
    let _ = std::fs::remove_dir_all(&d);
}

/// 装着的插件：`skills/<名>/`（含指向别处的链接）与插件缓存里每个版本各一个，名字取清单；没清单的目录不算。
#[test]
fn plugins_come_from_skills_and_the_plugin_cache_named_by_their_manifest() {
    let root = temp_dir("plugins");
    let manifest = |d: &Path, name: &str| {
        std::fs::create_dir_all(d.join(".claude-plugin")).unwrap();
        std::fs::write(
            d.join(".claude-plugin/plugin.json"),
            format!(r#"{{"name":"{name}"}}"#),
        )
        .unwrap();
    };
    let elsewhere = root.join("elsewhere/pb-src");
    manifest(&elsewhere, "alpha-plugin");
    std::fs::create_dir_all(root.join("skills")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&elsewhere, root.join("skills/alpha")).unwrap();
    std::fs::create_dir_all(root.join("skills/plain-skill")).unwrap();
    let cached = root.join("plugins/cache/market/beta/1.0.0");
    manifest(&cached, "beta-plugin");
    let got = plugins(&root);
    let names: Vec<&str> = got.iter().map(|(_, n)| n.as_str()).collect();
    #[cfg(unix)]
    assert_eq!(names, vec!["alpha-plugin", "beta-plugin"]);
    assert!(got.iter().any(|(d, n)| d == &cached && n == "beta-plugin"));
    assert!(!got.iter().any(|(d, _)| d.ends_with("plain-skill")));
}

/// 沙箱里造一个文件（连同上层目录）。
fn put(root: &std::path::Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// ★ 一条会话能用的命令：本项目 `.claude/commands` ＋ 号的配置目录 `commands`（各下一层子目录也算），名字 ＝ 文件名去 `.md`；
/// 简介先取 front matter 的 `description`，没有 ⇒ 正文第一行（去掉标题井号）；要参数 ＝ front matter 有 `argument-hint`
/// 或正文有 `$ARGUMENTS` / `$1`。Skill 同扫两处。不是 `.md` 的不算。
#[test]
fn a_session_sees_its_project_and_account_commands_and_skills() {
    let d = std::env::temp_dir().join(format!("ccm-session-assets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let proj = d.join("proj");
    let cfg = d.join("acct");
    put(
        &proj,
        ".claude/commands/ship.md",
        "---\ndescription: 发版前核一遍\nargument-hint: <版本>\n---\n照做\n",
    );
    put(
        &proj,
        ".claude/commands/ops/tidy.md",
        "# 收拾工作树\n\n删掉 $1 之外的\n",
    );
    put(&proj, ".claude/commands/notes.txt", "不是命令");
    put(&cfg, "commands/standup.md", "\n\n总结昨天的提交\n");
    put(&cfg, "commands/ask.md", "回答：$ARGUMENTS\n");
    put(
        &proj,
        ".claude/skills/lint/SKILL.md",
        "---\ndescription: 查风格\n---\n",
    );
    put(
        &cfg,
        "skills/plot/SKILL.md",
        "---\ndescription: 画图\n---\n",
    );
    let got = super::session_scan(Some(&proj), &cfg);
    let cmds: Vec<(&str, bool, Option<&str>, bool)> = got
        .commands
        .iter()
        .map(|c| {
            (
                c.name.as_str(),
                c.project,
                c.description.as_deref(),
                c.needs_args,
            )
        })
        .collect();
    assert_eq!(
        cmds,
        vec![
            ("ship", true, Some("发版前核一遍"), true),
            ("tidy", true, Some("收拾工作树"), true),
            ("ask", false, Some("回答：$ARGUMENTS"), true),
            ("standup", false, Some("总结昨天的提交"), false),
        ]
    );
    let skills: Vec<(&str, Option<&str>)> = got
        .skills
        .iter()
        .map(|s| (s.name.as_str(), s.description.as_deref()))
        .collect();
    assert_eq!(
        skills,
        vec![("lint", Some("查风格")), ("plot", Some("画图"))]
    );
    assert!(got.problems.is_empty(), "{:?}", got.problems);
    // 目录都不在 ⇒ 空，不是问题；没有项目目录 ⇒ 只扫号的那一处。
    let none = super::session_scan(Some(&d.join("nope")), &d.join("nope2"));
    assert!(none.commands.is_empty() && none.skills.is_empty() && none.problems.is_empty());
    assert_eq!(super::session_scan(None, &cfg).commands.len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 插件两处读盘：安装记录 `plugins/installed_plugins.json`（第 2 版）记的各处 ＋ `skills/<名>/.claude-plugin/`（市场 `skills-dir`）。
/// 清单给名字 · 版本 · 说明；组成按插件根里的 skills · commands · agents · hooks · `.mcp.json` 数名字。
/// 开不开照六处 `enabledPlugins` 合并：托管 ＞ 本机项目设置 ＞ 项目设置 ＞ 用户设置两份 ＞ 清单默认；只给出处的码。
/// 项目设置写了它的那个项目另出一条；装在项目里的那一条住那个项目。带清单的 skill 目录不再算 skill。
#[test]
fn plugins_are_read_from_the_install_record_and_the_skills_dir_with_six_settings_merged() {
    use crate::agents::{PluginFrom, PluginParts, PluginSeen};
    let d = temp_dir("plugin-scan");
    let cfg = d.join(".claude");
    let proj = d.join("proj");
    let proj2 = d.join("proj2");
    // 市场插件：用户级装着，组成齐全。
    let gh = cfg.join("plugins/cache/mkt/github/1.4.0");
    put(
        &gh,
        ".claude-plugin/plugin.json",
        r#"{"name":"github","version":"1.4.0","description":"GitHub 议题"}"#,
    );
    put(&gh, "skills/triage/SKILL.md", "---\ndescription: x\n---\n");
    put(&gh, "commands/pr.md", "x");
    put(&gh, "commands/issue.md", "x");
    put(&gh, "agents/helper.md", "x");
    put(
        &gh,
        "hooks/hooks.json",
        r#"{"hooks":{"Stop":[],"PreToolUse":[]}}"#,
    );
    put(
        &gh,
        ".mcp.json",
        r#"{"mcpServers":{"github":{"type":"http"}}}"#,
    );
    // 装在 proj2 里的那一个。
    let fmt = cfg.join("plugins/cache/team/formatter/0.1.0");
    put(
        &fmt,
        ".claude-plugin/plugin.json",
        r#"{"name":"formatter","version":"0.1.0"}"#,
    );
    let record = serde_json::json!({
        "version": 2,
        "plugins": {
            "github@mkt": [{ "scope": "user", "installPath": gh.display().to_string(), "version": "1.4.0" }],
            "formatter@team": [{ "scope": "project", "installPath": fmt.display().to_string(),
                                 "projectPath": proj2.display().to_string() }],
        }
    });
    put(&cfg, "plugins/installed_plugins.json", &record.to_string());
    // skills 目录里的插件：清单默认开，用户设置里关掉。
    put(
        &cfg,
        "skills/pb/.claude-plugin/plugin.json",
        r#"{"name":"planned-build","version":"0.5.4","defaultEnabled":true}"#,
    );
    put(
        &cfg,
        "skills/pb/skills/planned-build/SKILL.md",
        "---\ndescription: x\n---\n",
    );
    put(
        &cfg,
        "skills/plain/SKILL.md",
        "---\ndescription: plain\n---\n",
    );
    // 设置：用户级开 github · 关 planned-build（local 压过 settings.json）；proj 的 local 关 github；托管开 formatter。
    put(
        &cfg,
        "settings.json",
        r#"{"enabledPlugins":{"github@mkt":true,"planned-build@skills-dir":true}}"#,
    );
    put(
        &cfg,
        "settings.local.json",
        r#"{"enabledPlugins":{"planned-build@skills-dir":false}}"#,
    );
    put(
        &proj,
        ".claude/settings.json",
        r#"{"enabledPlugins":{"github@mkt":true}}"#,
    );
    put(
        &proj,
        ".claude/settings.local.json",
        r#"{"enabledPlugins":{"github@mkt":false}}"#,
    );
    let policy = d.join("managed-settings.json");
    put(
        &d,
        "managed-settings.json",
        r#"{"enabledPlugins":{"formatter@team":true}}"#,
    );

    let projects = vec![proj.display().to_string(), proj2.display().to_string()];
    let mut out = Sightings::default();
    scan_plugins_at(&cfg, &[policy], &projects, &mut out);
    scan_skills_at(&cfg.join(SKILLS_DIR), None, &mut out);
    assert!(out.problems.is_empty(), "{:?}", out.problems);

    let gh_parts = PluginParts {
        skills: vec!["triage".into()],
        commands: vec!["issue".into(), "pr".into()],
        agents: vec!["helper".into()],
        hooks: vec!["PreToolUse".into(), "Stop".into()],
        mcp: vec!["github".into()],
    };
    let seen = |project: Option<&Path>,
                name: &str,
                market: &str,
                root: &Path,
                version: &str,
                description: Option<&str>,
                parts: PluginParts,
                on: bool,
                from: PluginFrom| PluginSeen {
        project: project.map(|p| p.display().to_string()),
        name: name.into(),
        market: market.into(),
        root: root.to_path_buf(),
        version: Some(version.into()),
        description: description.map(str::to_string),
        parts,
        on,
        from,
    };
    let pb_parts = PluginParts {
        skills: vec!["planned-build".into()],
        ..Default::default()
    };
    let mut want = vec![
        seen(
            None,
            "github",
            "mkt",
            &gh,
            "1.4.0",
            Some("GitHub 议题"),
            gh_parts.clone(),
            true,
            PluginFrom::User,
        ),
        seen(
            Some(&proj),
            "github",
            "mkt",
            &gh,
            "1.4.0",
            Some("GitHub 议题"),
            gh_parts,
            false,
            PluginFrom::Local,
        ),
        seen(
            Some(&proj2),
            "formatter",
            "team",
            &fmt,
            "0.1.0",
            None,
            PluginParts::default(),
            true,
            PluginFrom::Managed,
        ),
        seen(
            None,
            "planned-build",
            "skills-dir",
            &cfg.join("skills/pb"),
            "0.5.4",
            None,
            pb_parts,
            false,
            PluginFrom::User,
        ),
    ];
    let key = |p: &PluginSeen| (p.name.clone(), p.project.clone());
    want.sort_by_key(key);
    let mut got = out.plugins.clone();
    got.sort_by_key(key);
    assert_eq!(got, want);
    let skills: Vec<&str> = out.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        skills,
        vec!["plain"],
        "带清单的 skill 目录算插件，不另成 skill"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 哪份设置都没写 ⇒ 照清单默认（没写默认 ⇒ 关着）；安装记录格式不认得 ⇒ 说出来，不当成没装。
#[test]
fn an_unmentioned_plugin_follows_its_manifest_and_an_unknown_record_is_said() {
    use crate::agents::PluginFrom;
    let d = temp_dir("plugin-default");
    let cfg = d.join(".claude");
    put(
        &cfg,
        "skills/a/.claude-plugin/plugin.json",
        r#"{"name":"a","defaultEnabled":true}"#,
    );
    put(
        &cfg,
        "skills/b/.claude-plugin/plugin.json",
        r#"{"name":"b"}"#,
    );
    put(
        &cfg,
        "plugins/installed_plugins.json",
        r#"{"version":9,"plugins":{}}"#,
    );
    let mut out = Sightings::default();
    scan_plugins_at(&cfg, &[], &[], &mut out);
    let got: Vec<(&str, bool, PluginFrom)> = out
        .plugins
        .iter()
        .map(|p| (p.name.as_str(), p.on, p.from))
        .collect();
    assert_eq!(
        got,
        vec![
            ("a", true, PluginFrom::Manifest),
            ("b", false, PluginFrom::Manifest)
        ]
    );
    assert_eq!(out.problems.len(), 1, "{:?}", out.problems);
    assert!(
        out.problems[0].contains("installed_plugins.json"),
        "{:?}",
        out.problems
    );
    let _ = std::fs::remove_dir_all(&d);
}
