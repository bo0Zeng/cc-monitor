//! 「文件与数据」那一份成品：同一份足迹按三样重排。

use super::*;
use crate::footprint::chores::{Chore, ChoreAction, ChoreKind, ChoreState};
use crate::footprint::rows::SettingsScope;
use serde_json::{json, Value};

/// 自己家里那几样写成线上那一份（下面几条按格读）。
fn own_rows(home: &std::path::Path) -> Vec<Value> {
    super::own_rows(home)
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect()
}

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
        agent_home: "/h/.claude".into(),
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
    let ids: Vec<&str> = got.iter().map(|n| n.id).collect();
    assert_eq!(
        ids,
        vec!["claude-cli"],
        "查不动的说成缺 ＝ 对能用的环境报假警报；tmux 一律可选，缺了不进「要装」，只走 tmux 那一格"
    );
    assert!(got[0].required);
}

#[test]
fn 成品五格_tmux_原样带_角标数照各件算() {
    let chore = |kind, state| {
        let mut c = Chore::new("x", kind, ChoreAction::How);
        c.state = state;
        c
    };
    let todo = vec![
        chore(ChoreKind::Must, ChoreState::Todo),
        chore(ChoreKind::Optional, ChoreState::Todo),
        chore(ChoreKind::Decide, ChoreState::Done),
    ];
    for tmux in [Some(true), Some(false), None] {
        let got = serde_json::to_value(shape(&report(vec![]), todo.clone(), tmux, vec![])).unwrap();
        let mut keys: Vec<&str> = got
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "changedFiles",
                "chores",
                "home",
                "open",
                "own",
                "rank",
                "tmux",
                "todo"
            ]
        );
        assert_eq!(got["tmux"], json!(tmux), "查不动不许说成没有");
        assert_eq!(got["chores"], json!(1));
        assert_eq!(got["open"], json!(2), "没做完 ＝ 不是已做也不是不用了");
    }
}

#[test]
fn 先后键_有进角标的件在前_只剩别的没做完的次之_没事的最后() {
    let chore = |kind, state| {
        let mut c = Chore::new("x", kind, ChoreAction::How);
        c.state = state;
        c
    };
    let rank = |todo: Vec<Chore>| {
        serde_json::to_value(shape(&report(vec![]), todo, None, vec![])).unwrap()["rank"].clone()
    };
    assert_eq!(
        rank(vec![chore(ChoreKind::Install, ChoreState::Expired)]),
        json!(0)
    );
    assert_eq!(
        rank(vec![chore(ChoreKind::Optional, ChoreState::Blocked)]),
        json!(1)
    );
    assert_eq!(
        rank(vec![
            chore(ChoreKind::Must, ChoreState::Done),
            chore(ChoreKind::Optional, ChoreState::Declined),
        ]),
        json!(2)
    );
    assert_eq!(rank(vec![]), json!(2));
}

/// 改过你的文件那一格（旧测试只看它）。
fn shape_rows(r: &ConfigSurfaceReport, todo: Vec<Chore>, tmux: Option<bool>) -> Value {
    serde_json::to_value(shape(r, todo, tmux, vec![])).unwrap()
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
        assert_eq!(
            keys,
            vec!["class", "dir", "exists", "id", "path", "size", "what"]
        );
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

/// 自家每一样带写好的那一句（界面照抄，不再按 id 取字）：契约里每个 id 都认得（没有一行落到「未识别」），答题插件那一行是它那一句。
#[test]
fn 自己家里的每一样带写好的那一句_界面照抄() {
    let h = Home::new("what");
    let rows = own_rows(&h.0);
    assert_eq!(rows.len(), relay_route_core::OWN_HOME_ENTRIES.len());
    for r in &rows {
        let id = r["id"].as_str().unwrap();
        let what = r["what"]
            .as_str()
            .unwrap_or_else(|| panic!("{id} 没带 what"));
        assert_eq!(
            what,
            copy_core::copy_text(&format!("rsDataPaths.backend.{id}"), &[]),
            "{id}"
        );
    }
    let plugin = rows
        .iter()
        .find(|r| r["id"] == "answerPlugin")
        .expect("有答题插件那一行");
    assert_eq!(
        plugin["what"],
        json!(copy_core::copy_text(
            "rsDataPaths.backend.answerPlugin",
            &[]
        ))
    );
}

/// 〔`INVARIANTS §2.1`，异源〕自家每一样「删了会丢 / 能重建」== 那张散文表里它那一行的类（名字 ＝ 去掉家那一段、目录带尾 `/`，同界面上那一格）。
/// 那张表还列着 monitor 数据目录里界面自己那几份（壳 `data_paths.rs` 对拍那一半）⇒ 这里只比自家这几样，一样都不许缺、类不许错。
#[test]
fn 自己家里每一样的类与_invariants_那张表一致() {
    let md = include_str!("../../../src/doc/INVARIANTS.md");
    let start = md
        .find("### 2.1 真相 vs 缓存")
        .expect("INVARIANTS.md 里找不到 §2.1 —— 标题改了就来改这条");
    let sec = &md[start..];
    let end = sec[4..].find("\n## ").map_or(sec.len(), |i| i + 4);
    let mut table: std::collections::BTreeMap<String, &str> = Default::default();
    for line in sec[..end].lines() {
        let cols: Vec<&str> = line.split('|').map(str::trim).collect();
        if cols.len() < 4 || !cols[1].starts_with('`') {
            continue;
        }
        let class = if cols[2].contains("真相") || cols[2].contains("混") {
            "truth"
        } else if cols[2].contains("缓存") {
            "cache"
        } else {
            panic!("§2.1 那一行的类认不出来：{line}")
        };
        for name in cols[1].split('`').skip(1).step_by(2) {
            table.insert(name.to_string(), class);
        }
    }
    let h = Home::new("invariants");
    let rows = own_rows(&h.0);
    assert!(
        rows.len() >= 10,
        "自家那几样只有 {} 样 —— 下面的逐样比是空真",
        rows.len()
    );
    let mut off = Vec::new();
    for r in &rows {
        let tail = r["path"]
            .as_str()
            .unwrap()
            .trim_start_matches("~/.cc-monitor/");
        let name = if r["dir"] == json!(true) {
            format!("{tail}/")
        } else {
            tail.to_string()
        };
        let got = r["class"].as_str().unwrap();
        match table.get(&name) {
            Some(&want) if want == got => {}
            want => off.push(format!("{name}：成品说 {got}，§2.1 说 {want:?}")),
        }
    }
    assert_eq!(
        off,
        Vec::<String>::new(),
        "自家那几样与 §2.1 那张表对不上 ⇒ 新加一样要同拍 `OWN_HOME_ENTRIES` 与 §2.1 那张表"
    );
}
