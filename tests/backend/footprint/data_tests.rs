//! 「文件与数据」那一份成品：同一份足迹按三样重排。

use super::*;
use crate::footprint::rows::SettingsScope;

fn row(
    tool_id: &'static str,
    tier: EnvTier,
    path: &'static str,
    state: SurfaceState,
) -> SurfaceRow {
    SurfaceRow {
        tool_id,
        tool_name: format!("{tool_id} 的名字"),
        tier,
        source_label: String::new(),
        path_declared: path,
        path_resolved: None,
        note: None,
        host_label: String::new(),
        effect_label: String::new(),
        state,
        installable: true,
        uninstallable: true,
    }
}

fn present() -> SurfaceState {
    SurfaceState::Present {
        detail: "在".into(),
    }
}

fn report(rows: Vec<SurfaceRow>) -> ConfigSurfaceReport {
    let scopes: Vec<SettingsScope> = Vec::new();
    ConfigSurfaceReport {
        rows,
        settings_scopes: scopes,
        claude_config_dir: "/h/.claude".into(),
        home: "/h".into(),
    }
}

#[test]
fn 改过你的文件只算装口放的且今天在的且不在自己家里的() {
    let got = shape(&report(vec![
        row("ccm", EnvTier::AppInstalls, "~/.bashrc", present()),
        row(
            "ccm",
            EnvTier::AppInstalls,
            "~/.cc-monitor/bin/ccm",
            present(),
        ),
        row(
            "skill-install",
            EnvTier::AppInstalls,
            "~/.claude/skills",
            SurfaceState::Absent,
        ),
        row(
            "cc-bus",
            EnvTier::AppInstalls,
            "~/.claude/skills/cc-bus",
            present(),
        ),
        row("project-mcp", EnvTier::AppInstalls, ".mcp.json", present()),
    ]));
    let paths: Vec<&str> = got["changedFiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["~/.bashrc", "~/.claude/skills/cc-bus"]);
    assert_eq!(
        got["changedFiles"][0]["undo"],
        json!({"page": "machine", "tab": "config", "anchor": "connect-terminal"}),
        "接上终端那几行撤回在别名与配置文件那一栏"
    );
    assert_eq!(got["changedFiles"][1]["undo"], json!({"page": "ext"}));
}

#[test]
fn 要装只报确实缺的_查不动的不算缺_缺了起不了会话的才进角标() {
    let got = shape(&report(vec![
        row(
            "claude-cli",
            EnvTier::UserInstallsWePrompt,
            "claude",
            SurfaceState::Absent,
        ),
        row(
            "tmux",
            EnvTier::UserInstallsWePrompt,
            "tmux",
            SurfaceState::Absent,
        ),
        row(
            "login-shell",
            EnvTier::UserInstallsWePrompt,
            "bash",
            SurfaceState::Undetermined {
                why: "查不动".into(),
            },
        ),
        row(
            "mcp-server",
            EnvTier::UserInstallsWePrompt,
            "$MCP_COMMAND",
            present(),
        ),
    ]));
    let ids: Vec<&str> = got["needsInstall"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["claude-cli"],
        "查不动的说成缺 ＝ 对能用的环境报假警报；tmux 一律可选，缺了不进「要装」，只走 tmux 那一格"
    );
    assert_eq!(got["needsInstall"][0]["required"], json!(true));
    assert_eq!(got["chores"], json!(1), "进角标的只算缺了起不了会话的");
    assert_eq!(got["tmux"], json!(false));
}

#[test]
fn 有没有_tmux_三态() {
    let t = |s| {
        shape(&report(vec![row(
            "tmux",
            EnvTier::UserInstallsWePrompt,
            "tmux",
            s,
        )]))["tmux"]
            .clone()
    };
    assert_eq!(t(present()), json!(true));
    assert_eq!(t(SurfaceState::Absent), json!(false));
    assert_eq!(
        t(SurfaceState::Undetermined { why: "x".into() }),
        json!(null),
        "查不动不许说成没有"
    );
}
