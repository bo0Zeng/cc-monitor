//! 〔AS2 · 第四波 4B〕`agents/claudecode/assets.rs` 的判据：Claude 的资产布局认得对。
//!
//! 守的要求（住址）：用户裁决 **V113** 逐字「本机后端在本机看见一个skill并记录下来」「mcp保持项目级别」
//! ＋ 题面「skill（`~/.claude/skills/*`）与 MCP 定义（项目 `.mcp.json` 里的条目）」。
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
    let got: std::collections::BTreeSet<(String, String, bool)> = assets
        .iter()
        .map(|a| (a.kind.clone(), a.name.clone(), a.project.is_some()))
        .collect();
    let want: std::collections::BTreeSet<(String, String, bool)> = [
        ("skill", "su", false),
        ("skill", "sp", true),
        ("mcp", "mu", false),
        ("mcp", "mp", true),
    ]
    .iter()
    .map(|(k, n, p)| (k.to_string(), n.to_string(), *p))
    .collect();
    assert_eq!(got, want);
    let _ = std::fs::remove_dir_all(&d);
}
