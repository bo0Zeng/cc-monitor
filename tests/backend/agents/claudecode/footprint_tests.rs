//! Claude 足迹面：申报路径里哪一段算 Claude 家底下（`under_agent_home`）· 用户级 / 项目级 settings 各是哪两份、谁压谁 ·
//! 注册表里这一家的足迹面接的就是这几样。申报表逐条（工具 · 路径 · 哪台）钉在 `tests/backend/footprint/rows_tests.rs` 的全表那一条，这里不另钉。

use super::*;
use std::path::Path;

/// ★ 只有 `.claude/` 那一段算 Claude 家底下，回家底下那一段；名字带 `.claude` 前缀的别的目录 · 光一个 `.claude` · 别的目录都不算。
#[test]
fn only_paths_under_the_claude_dir_count_as_under_the_agent_home() {
    assert_eq!(
        under_agent_home(".claude/settings.json"),
        Some("settings.json")
    );
    assert_eq!(
        under_agent_home(".claude/skills/cc-bus"),
        Some("skills/cc-bus")
    );
    for rest in [".claude-alt/y", ".claude", ".config/x", "claude/x", ""] {
        assert_eq!(under_agent_home(rest), None, "{rest}");
    }
}

/// ★ 用户级两份：`settings.json` 在前、`settings.local.json` 在后（后者压前者）；项目级同形、住项目的 `.claude/` 底下。
#[test]
fn settings_scopes_are_shared_then_local_and_projects_use_their_claude_dir() {
    let home = Path::new("/h/.claude");
    assert_eq!(
        user_settings_files(home),
        [home.join("settings.json"), home.join("settings.local.json")]
    );
    let proj = Path::new("/w/app");
    assert_eq!(
        project_settings_files(proj),
        [
            proj.join(".claude/settings.json"),
            proj.join(".claude/settings.local.json")
        ]
    );
}

/// ★ 注册表里这一家的足迹面接的就是这份表与这几样判法；Claude 的收信钩子不要信任。
#[test]
fn the_face_hands_this_familys_table_and_layout() {
    assert!(std::ptr::eq(FACE.tools, TOOLS));
    assert!(std::ptr::eq(FACE.env, UNMANAGED_ENV));
    let home = Path::new("/h/.claude");
    assert_eq!((FACE.user_settings)(home), user_settings_files(home));
    assert_eq!(
        (FACE.under_agent_home)(".claude/projects/"),
        Some("projects/")
    );
    assert_eq!((FACE.under_agent_home)(".codex/sessions"), None);
    assert!(FACE.hook_trust.is_none());
}

/// ★ 申报路径的两形（带 `~/` · 家目录相对）说的是同一处；落在 Claude 布局里的那几条都在家底下。
#[test]
fn declared_paths_agree_between_their_two_forms() {
    for (declared, rel) in [
        (CC_BUS_SKILL_DIR, CC_BUS_SKILL_DIR_REL),
        (SKILLS_DIR, SKILLS_DIR_REL),
        (ACCOUNTS_ROOT, ACCOUNTS_ROOT_REL),
    ] {
        assert_eq!(declared.strip_prefix("~/"), Some(rel), "{declared}");
    }
    for p in [CC_BUS_SKILL_DIR, SKILLS_DIR, USER_SETTINGS, PROJECTS_DIR] {
        let rest = p.strip_prefix("~/").expect("带 ~/");
        assert!(under_agent_home(rest).is_some(), "{p} 不在 Claude 家底下");
    }
    assert!(
        under_agent_home(ACCOUNTS_ROOT_REL).is_none(),
        "账号库住后端的家，不在 Claude 家底下"
    );
}
