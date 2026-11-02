//! agent 适配层判据里经资产目录的那几条：适配层是独立的包（`backend-agents`）、够不到资产这一层 ⇒ 挂在 `assets` 底下，
//! 这一层拆成包时跟着走。夹具是按公开布局造的，只采结构。

use crate::agents::claudecode::assets::{
    project_skills_root, scan_at, PROJECT_MCP_FILE, SKILLS_DIR, SKILL_DOC,
};
use std::path::Path;

/// ★ 覆盖面：一个临时家目录里 skill × 用户级 / 项目级、MCP × 用户级 / 项目级四格各放一个 —— 项目取自这台的会话清单
/// （会话记录里的 `cwd`，只采结构），目录四格各认出一个（两向相等）。
#[test]
fn the_catalog_sees_one_of_each_kind_at_each_level() {
    let d = std::env::temp_dir().join(format!("ccm-ccassets-four-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建临时目录");
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
