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
    let got = shape_rows(
        &report(vec![
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
        ]),
        vec![],
        None,
    );
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
    let got = needs_install(&report(vec![
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
    let ids: Vec<&str> = got.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        vec!["claude-cli"],
        "查不动的说成缺 ＝ 对能用的环境报假警报；tmux 一律可选，缺了不进「要装」，只走 tmux 那一格"
    );
    assert_eq!(got[0]["required"], json!(true));
}

#[test]
fn 成品五格_tmux_原样带_角标数照各件算() {
    let todo = vec![
        json!({"kind": "must", "state": "todo"}),
        json!({"kind": "optional", "state": "todo"}),
        json!({"kind": "decide", "state": "done"}),
    ];
    for tmux in [Some(true), Some(false), None] {
        let got = shape(&report(vec![]), todo.clone(), tmux, vec![]);
        let mut keys: Vec<&str> = got
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["changedFiles", "chores", "home", "own", "tmux", "todo"]
        );
        assert_eq!(got["tmux"], json!(tmux), "查不动不许说成没有");
        assert_eq!(got["chores"], json!(1));
    }
}

/// 改过你的文件那一格（旧测试只看它）。
fn shape_rows(r: &ConfigSurfaceReport, todo: Vec<Value>, tmux: Option<bool>) -> Value {
    shape(r, todo, tmux, vec![])
}

/// 临时家目录（测完删）。
struct Home(std::path::PathBuf);
impl Home {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ccm-own-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Home(p)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn 自己家里的每一样一行_文件给大小_目录不给_不在的照实说不在() {
    let h = Home::new("rows");
    let own = h.0.join(".cc-monitor");
    std::fs::create_dir_all(own.join("accounts")).unwrap();
    std::fs::write(own.join("profiles.toml"), b"12345").unwrap();
    let rows = own_rows(&h.0);
    let by = |id: &str| {
        rows.iter()
            .find(|r| r["id"] == json!(id))
            .cloned()
            .unwrap_or_else(|| panic!("没有 {id}：{rows:?}"))
    };
    let p = by("profiles");
    assert_eq!(p["path"], json!("~/.cc-monitor/profiles.toml"));
    assert_eq!(
        (p["exists"].clone(), p["size"].clone(), p["dir"].clone()),
        (json!(true), json!(5), json!(false))
    );
    assert_eq!(p["class"], json!("truth"));
    let a = by("accounts");
    assert_eq!(
        (a["exists"].clone(), a["size"].clone(), a["dir"].clone()),
        (json!(true), Value::Null, json!(true))
    );
    let b = by("bin");
    assert_eq!(
        (b["exists"].clone(), b["class"].clone()),
        (json!(false), json!("cache"))
    );
    for r in &rows {
        let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["class", "dir", "exists", "id", "path", "size"]);
    }
}

/// 〔家里的都进这一份，判据两向，异源〕契约 crate 里 `~/.cc-monitor/` 下的每一个相对路径常量 == 这一份覆盖的路径
/// （后端落点由它所在的 `bin/` 那一行覆盖；只住 monitor 那一侧的进程记录与 API key 表不在这台后端的家里算）。
#[test]
fn 契约里家里的每个路径都有一行() {
    let src = include_str!("../../../src/common/relay-route-core/src/lib.rs");
    let macro_lit = |name: &str| -> Option<String> {
        let body = &src[src.find(&format!("macro_rules! {name} {{"))?..];
        let q = body.find('"')? + 1;
        Some(body[q..q + body[q..].find('"')?].to_string())
    };
    let mut declared: std::collections::BTreeSet<String> = Default::default();
    for line in src.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("pub const ") {
            if let Some((_, v)) = rest.split_once("&str = ") {
                let v = v.trim_end_matches(';');
                let lit = match v.strip_prefix('"') {
                    Some(q) => Some(q.trim_end_matches('"').to_string()),
                    None => v.strip_suffix("!()").and_then(macro_lit),
                };
                if let Some(v) = lit.filter(|v| v.starts_with(".cc-monitor/")) {
                    declared.insert(v);
                }
            }
        }
    }
    assert!(declared.len() >= 10, "抽取器坏了：{declared:?}");
    let h = Home::new("contract");
    let covered: std::collections::BTreeSet<String> = own_rows(&h.0)
        .iter()
        .map(|r| {
            r["path"]
                .as_str()
                .unwrap()
                .trim_start_matches("~/")
                .to_string()
        })
        .collect();
    let mut want = declared;
    want.remove(relay_route_core::BACKEND_LANDING_REL);
    want.insert(".cc-monitor/bin".to_string());
    assert_eq!(covered, want);
}
