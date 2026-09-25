//! 〔AS2 · 第四波 4B〕`agents/claudecode/assets.rs` 的判据：Claude 的资产布局认得对。
//!
//! 守的要求（住址）：用户裁决 **V113** 逐字「本机后端在本机看见一个skill并记录下来」「mcp保持项目级别」
//! ＋ 题面「skill（`~/.claude/skills/*`）与 MCP 定义（项目 `.mcp.json` 里的条目）」。
//!
//! 买到：临时目录上真读 —— skill 逐个目录（链接跟到底、文件与点开头的跳过）· `description:` 从 front matter 取 ·
//! MCP 按 `.claude.json` 的 `projects` × `<项目>/.mcp.json` 逐条 · 坏的那一份**说出来**而不是当空表。
//! 买不到：真 `~/.claude` 的形状（夹具是按公开布局造的）；Windows 路径没跑过。

use super::*;
use crate::agents::Sightings;

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
    scan_skills_at(&root, &mut out);
    let got: Vec<(String, Option<String>)> = out
        .skills
        .iter()
        .map(|(n, _, dsc)| (n.clone(), dsc.clone()))
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
    scan_skills_at(&d.join("no-such"), &mut none);
    assert_eq!(
        none,
        Sightings::default(),
        "没有 skills 目录 ⇒ 什么都没有，也不算问题"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn project_mcp_is_projects_times_their_mcp_json_and_broken_ones_are_said() {
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
    let projects: serde_json::Value = serde_json::json!({
        "projects": {
            p1.to_string_lossy(): {},
            p2.to_string_lossy(): {},
            p3.to_string_lossy(): {},
        }
    });
    std::fs::write(&cfg, projects.to_string()).unwrap();

    let mut out = Sightings::default();
    scan_mcp_at(&cfg, &mut out);
    let got: Vec<(String, String)> = out
        .mcp
        .iter()
        .map(|(p, n, _)| (p.clone(), n.clone()))
        .collect();
    let p1s = p1.to_string_lossy().into_owned();
    assert_eq!(
        got,
        vec![(p1s.clone(), "fs".to_string()), (p1s, "gh".to_string())]
    );
    assert_eq!(
        out.mcp[0].2,
        serde_json::json!({"command": "/opt/fs"}),
        "定义原样"
    );
    assert_eq!(
        out.problems.len(),
        1,
        "坏的那一份要说出来：{:?}",
        out.problems
    );
    assert!(out.problems[0].contains("p3"), "{:?}", out.problems);

    let mut none = Sightings::default();
    scan_mcp_at(&d.join("no-such.json"), &mut none);
    assert_eq!(
        none,
        Sightings::default(),
        "没有 .claude.json ⇒ 这台没用过，不是问题"
    );
    let _ = std::fs::remove_dir_all(&d);
}
