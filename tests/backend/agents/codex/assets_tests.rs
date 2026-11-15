//! `agents/codex/assets.rs` 的判据：Codex 的 skill 与插件布局认得对。
//!
//! 夹具手写，形照 codex-rs rust-v0.167.0：skill 住处（`codex_skills_extension::host_roots`：`$CODEX_HOME/skills` ·
//! `~/.agents/skills` · 项目配置层 `.codex/skills` · 项目根到工作目录之间每一层的 `.agents/skills`；一个目录带 `SKILL.md`
//! 才算，往下找、点开头的跳过，名字先看头部 `name:`）· 插件缓存（`codex_core_plugin_common::installed`：
//! `plugins/cache/<市场>/<插件>/<版本>/`，有 `local` 那一版就用它、否则取最大的）· 清单 `.codex-plugin/plugin.json`。
//! 全在临时目录上读，不碰真的 `~/.codex`。

use super::*;
use std::path::Path;

fn skill(dir: &Path, front: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(SKILL_DOC), format!("---\n{front}\n---\n# body\n")).unwrap();
}

fn names(v: &[crate::agents::SkillSeen]) -> Vec<(Option<String>, String, Option<String>)> {
    v.iter()
        .map(|s| (s.project.clone(), s.name.clone(), s.description.clone()))
        .collect()
}

#[test]
fn a_skill_is_a_directory_with_skill_md_found_below_the_root() {
    let d = test_home::scratch("codex-skills");
    let root = d.join("skills");
    skill(
        &root.join("alpha"),
        "name: alpha\ndescription: \"Does alpha\"",
    );
    // 名字取头部的 `name:`，不是目录名。
    skill(&root.join("dir-b"), "name: beta\ndescription: b");
    // 往下一层也认（Codex 递归找）。
    skill(&root.join("group").join("gamma"), "description: g");
    // 没有 `SKILL.md` 的目录不算；点开头的（`.system` 那一类）跳过。
    std::fs::create_dir_all(root.join("empty")).unwrap();
    skill(&root.join(".system").join("hidden"), "description: h");

    let mut out = crate::agents::Sightings::default();
    scan_skills_at(&root, None, &mut out);
    assert_eq!(
        names(&out.skills),
        vec![
            (None, "alpha".into(), Some("Does alpha".into())),
            (None, "beta".into(), Some("b".into())),
            (None, "gamma".into(), Some("g".into())),
        ]
    );
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn the_machine_scan_lists_all_four_homes_of_skills() {
    let d = test_home::scratch("codex-skill-homes");
    let codex_home = d.join("codex");
    let home = d.join("home");
    let proj = d.join("proj");
    skill(&codex_home.join("skills").join("one"), "description: 1");
    skill(
        &home.join(".agents").join("skills").join("two"),
        "description: 2",
    );
    skill(
        &proj.join(".codex").join("skills").join("three"),
        "description: 3",
    );
    skill(
        &proj.join(".agents").join("skills").join("four"),
        "description: 4",
    );
    let p = proj.display().to_string();

    let out = scan_at(
        Some(&codex_home),
        Some(&home),
        None,
        std::slice::from_ref(&p),
    );
    let mut got = names(&out.skills);
    got.sort();
    assert_eq!(
        got,
        vec![
            (None, "one".into(), Some("1".into())),
            (None, "two".into(), Some("2".into())),
            (Some(p.clone()), "four".into(), Some("4".into())),
            (Some(p.clone()), "three".into(), Some("3".into())),
        ]
    );
}

#[test]
fn a_session_sees_its_account_skills_and_the_project_up_to_the_repo_root() {
    let d = test_home::scratch("codex-session-skills");
    let account = d.join("acct");
    let home = d.join("home");
    let repo = d.join("repo");
    let cwd = repo.join("sub");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    skill(&account.join("skills").join("mine"), "description: m");
    skill(
        &home.join(".agents").join("skills").join("shared"),
        "description: s",
    );
    skill(
        &repo.join(".agents").join("skills").join("root-level"),
        "description: r",
    );
    skill(
        &cwd.join(".codex").join("skills").join("here"),
        "description: h",
    );
    // 仓外那一层不算（项目根是带 `.git` 的那一层）。
    skill(
        &d.join(".agents").join("skills").join("outside"),
        "description: o",
    );

    let out = session_scan_at(Some(&cwd), &account, Some(&home));
    let mut got: Vec<String> = out.skills.iter().map(|s| s.name.clone()).collect();
    got.sort();
    assert_eq!(got, ["here", "mine", "root-level", "shared"]);
    assert!(out.commands.is_empty());
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn without_a_repo_root_only_the_working_directory_counts() {
    let d = test_home::scratch("codex-session-norepo");
    let cwd = d.join("a").join("b");
    std::fs::create_dir_all(&cwd).unwrap();
    skill(
        &d.join("a").join(".agents").join("skills").join("up"),
        "description: u",
    );
    skill(
        &cwd.join(".agents").join("skills").join("at"),
        "description: a",
    );
    let out = session_scan_at(Some(&cwd), &d.join("acct"), None);
    let got: Vec<String> = out.skills.iter().map(|s| s.name.clone()).collect();
    assert_eq!(got, ["at"]);
}

fn plugin(root: &Path, manifest: &str) {
    let m = root.join(".codex-plugin");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("plugin.json"), manifest).unwrap();
}

#[test]
fn plugins_read_name_and_version_from_the_manifest() {
    let d = test_home::scratch("codex-plugins");
    let cache = d.join("plugins").join("cache");
    plugin(
        &cache.join("mk").join("tool").join("1.2.0"),
        r#"{"name":"tool","version":"1.2.0","description":"A tool"}"#,
    );
    skill(
        &cache
            .join("mk")
            .join("tool")
            .join("1.2.0")
            .join("skills")
            .join("s1"),
        "description: s",
    );
    // 没写 `version` ⇒ `local`（同 Codex）。
    plugin(
        &cache.join("mk").join("bare").join("local"),
        r#"{"name":"bare"}"#,
    );

    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    let got: Vec<(String, String, Option<String>, Option<String>)> = out
        .plugins
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.market.clone(),
                p.version.clone(),
                p.description.clone(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("bare".into(), "mk".into(), Some("local".into()), None),
            (
                "tool".into(),
                "mk".into(),
                Some("1.2.0".into()),
                Some("A tool".into())
            ),
        ]
    );
    let tool = out.plugins.iter().find(|p| p.name == "tool").unwrap();
    assert_eq!(tool.parts.skills, ["s1"]);
    assert!(tool.on);
    assert!(out.problems.is_empty(), "{:?}", out.problems);

    let listed: Vec<String> = plugins(&d).into_iter().map(|(_, n)| n).collect();
    assert_eq!(listed, ["bare", "tool"]);
}

#[test]
fn a_broken_manifest_is_said_not_silently_dropped() {
    let d = test_home::scratch("codex-plugin-bad");
    plugin(
        &d.join("plugins")
            .join("cache")
            .join("mk")
            .join("bad")
            .join("local"),
        "{not json",
    );
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert!(out.plugins.is_empty());
    assert_eq!(out.problems.len(), 1, "{:?}", out.problems);
    assert!(plugins(&d).is_empty());
}

#[test]
fn the_active_version_is_local_when_present_else_the_highest() {
    let d = test_home::scratch("codex-plugin-versions");
    let base = d.join("plugins").join("cache").join("mk").join("p");
    plugin(&base.join("1.9.0"), r#"{"name":"p","version":"1.9.0"}"#);
    plugin(&base.join("1.10.0"), r#"{"name":"p","version":"1.10.0"}"#);
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert_eq!(out.plugins.len(), 1);
    assert_eq!(out.plugins[0].version.as_deref(), Some("1.10.0"));

    plugin(&base.join("local"), r#"{"name":"p"}"#);
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert_eq!(out.plugins[0].version.as_deref(), Some("local"));
}

#[test]
fn a_plugin_turned_off_in_config_toml_reads_off_from_user() {
    let d = test_home::scratch("codex-plugin-off");
    plugin(
        &d.join("plugins")
            .join("cache")
            .join("mk")
            .join("tool")
            .join("local"),
        r#"{"name":"tool"}"#,
    );
    std::fs::write(
        d.join("config.toml"),
        "model = \"x\"\n\n[plugins.\"tool@mk\"]\nenabled = false # off\n",
    )
    .unwrap();
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert!(!out.plugins[0].on);
    assert_eq!(out.plugins[0].from, crate::agents::PluginFrom::User);
}

/// 插件那一格的三行（codex 0.167 `codex plugin --help` 本机核过：`add` · `remove` 收 `PLUGIN[@MARKETPLACE]`，光名字又不给
/// `--marketplace` ⇒ 它报错退出；`list --json`）。交进来的就是这一家自己的写法（`<名>@<市场>`，即 `pluginId`），原样接在后面。
#[test]
fn plugin_cells_speak_codex_plugin_commands() {
    assert_eq!(
        (PLUGIN_CELLS.install)("tool@mk"),
        ["codex", "plugin", "add", "tool@mk", "--json"]
    );
    assert_eq!(
        (PLUGIN_CELLS.remove)("tool@mk"),
        ["codex", "plugin", "remove", "tool@mk", "--json"]
    );
    assert_eq!((PLUGIN_CELLS.list)(), ["codex", "plugin", "list", "--json"]);
}

/// `codex plugin list --json` 的输出（`JsonPluginListOutput`：`installed` · `available` 两张，每条 camelCase）⇒ 装着的那几个：
/// `key` 是这一家自己的写法 `pluginId`（装 · 删交回去的就是它），`name` 是光名字（同扫目录那一份），版本没说 ⇒ 缺，开不开照 `enabled`；`available` 不算。
/// 不是那一形（不是对象 · 没有 `installed` · 一条缺 `pluginId` / `name` / `enabled`）⇒ 读不懂。
#[test]
fn the_plugin_list_reads_the_installed_ones() {
    let out = serde_json::json!({
        "installed": [
            {"pluginId": "tool@mk", "name": "tool", "marketplaceName": "mk", "version": "1.2.0", "installed": true, "enabled": true},
            {"pluginId": "off@mk", "name": "off", "marketplaceName": "mk", "version": null, "installed": true, "enabled": false}
        ],
        "available": [
            {"pluginId": "new@mk", "name": "new", "marketplaceName": "mk", "installed": false, "enabled": false}
        ]
    })
    .to_string();
    let got = (PLUGIN_CELLS.read_list)(&out).expect("读得懂");
    assert_eq!(
        got,
        [
            crate::agents::PluginListed {
                key: "tool@mk".into(),
                name: "tool".into(),
                version: Some("1.2.0".into()),
                on: true
            },
            crate::agents::PluginListed {
                key: "off@mk".into(),
                name: "off".into(),
                version: None,
                on: false
            },
        ]
    );
    assert!((PLUGIN_CELLS.read_list)("[]").is_none());
    assert!((PLUGIN_CELLS.read_list)(r#"{"available":[]}"#).is_none());
    assert!((PLUGIN_CELLS.read_list)(r#"{"installed":[{"name":"x","enabled":true}]}"#).is_none());
    assert!(
        (PLUGIN_CELLS.read_list)(r#"{"installed":[{"pluginId":"x@m","enabled":true}]}"#).is_none(),
        "缺 name"
    );
    assert!((PLUGIN_CELLS.read_list)("not json").is_none());
}

#[test]
fn install_targets_are_the_agents_skills_dirs() {
    let p = Path::new("/w/proj");
    assert_eq!(project_skills_root(p), p.join(".agents").join("skills"));
    assert_eq!(
        skills_root_in(Path::new("/h")),
        Path::new("/h").join(".agents").join("skills")
    );
}

#[test]
fn the_machine_scan_lists_user_and_project_mcp_from_config_toml() {
    let d = test_home::scratch("codex-asset-mcp");
    let user = d.join("config.toml");
    std::fs::write(&user, "[mcp_servers.u]\ncommand = \"a\"\n").unwrap();
    let proj = d.join("proj");
    std::fs::create_dir_all(proj.join(".codex")).unwrap();
    std::fs::write(
        proj.join(".codex").join("config.toml"),
        "[mcp_servers.p]\nurl = \"https://p.example\"\n",
    )
    .unwrap();
    let p = proj.display().to_string();
    let out = scan_at(None, None, Some(&user), std::slice::from_ref(&p));
    let got: Vec<(Option<String>, String, serde_json::Value)> = out
        .mcp
        .iter()
        .map(|m| (m.project.clone(), m.name.clone(), m.def.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (None, "u".into(), serde_json::json!({"command": "a"})),
            (
                Some(p.clone()),
                "p".into(),
                serde_json::json!({"url": "https://p.example"})
            ),
        ]
    );
    assert_eq!(out.mcp[0].file, user);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn an_unreadable_config_toml_is_said_and_plugins_read_as_on() {
    let d = test_home::scratch("codex-plugin-badcfg");
    plugin(
        &d.join("plugins")
            .join("cache")
            .join("mk")
            .join("tool")
            .join("local"),
        r#"{"name":"tool"}"#,
    );
    std::fs::write(
        d.join("config.toml"),
        "[plugins.\"tool@mk\"\nenabled = false\n",
    )
    .unwrap();
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert!(out.plugins[0].on);
    assert_eq!(out.problems.len(), 1, "{:?}", out.problems);
}

#[test]
fn the_machine_scan_walks_a_project_up_to_its_repo_root_like_a_session() {
    let d = test_home::scratch("codex-scan-layers");
    let repo = d.join("repo");
    let cwd = repo.join("sub");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    skill(
        &repo.join(".agents").join("skills").join("top"),
        "description: t",
    );
    std::fs::create_dir_all(&cwd).unwrap();
    let p = cwd.display().to_string();
    let out = scan_at(None, None, None, std::slice::from_ref(&p));
    assert_eq!(
        names(&out.skills),
        vec![(Some(p), "top".into(), Some("t".into()))]
    );
}

/// 注册表 Codex 那一行声明了插件那一格，就是这一份；扫目录那一份的 `key` 是 `<名>@<市场>`，跟列出来的 `key` 照同一个写法比。
#[test]
fn the_registry_declares_codex_plugin_cells_keyed_like_the_scan() {
    let cells = super::super::ADAPTER
        .assets
        .and_then(|a| a.plugin_cells)
        .expect("Codex 声明了插件那一格");
    assert_eq!((cells.list)(), (PLUGIN_CELLS.list)());
    assert_eq!(
        (cells.install)("tool@mk"),
        (PLUGIN_CELLS.install)("tool@mk")
    );
    let d = test_home::scratch("codex-plugin-key");
    plugin(
        &d.join("plugins")
            .join("cache")
            .join("mk")
            .join("tool")
            .join("1.0.0"),
        r#"{"name":"tool"}"#,
    );
    let mut out = crate::agents::Sightings::default();
    scan_plugins_at(&d, &mut out);
    assert_eq!(
        (out.plugins[0].key.as_str(), out.plugins[0].name.as_str()),
        ("tool@mk", "tool")
    );
    let _ = std::fs::remove_dir_all(&d);
}
