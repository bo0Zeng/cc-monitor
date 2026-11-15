//! 「文件与数据」那一份成品：同一份足迹按三样重排。

use super::*;
use crate::footprint::chores::{Chore, ChoreAction, ChoreKind, ChoreState};
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
        path_declared: std::borrow::Cow::Borrowed(path),
        state,
    }
}

fn present() -> SurfaceState {
    SurfaceState::Present
}

fn report(rows: Vec<SurfaceRow>) -> ConfigSurfaceReport {
    ConfigSurfaceReport {
        rows,
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
    let got = needs_install(
        &report(vec![
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
                SurfaceState::Undetermined,
            ),
            row(
                "mcp-server",
                EnvTier::UserInstallsWePrompt,
                "$MCP_COMMAND",
                present(),
            ),
        ]),
        crate::agents::REGISTRY,
    );
    let ids: Vec<&str> = got.iter().map(|n| n.id).collect();
    assert_eq!(
        ids,
        vec!["claude-cli"],
        "查不动的说成缺 ＝ 对能用的环境报假警报；tmux 一律可选，缺了不进「要装」，只走 tmux 那一格"
    );
    assert!(got[0].required);
}

#[test]
fn 成品五格_tmux_原样带_各类件数照各件算() {
    let chore = |kind, state| {
        let mut c = Chore::new("x", kind, ChoreAction::How);
        c.state = state;
        c
    };
    let todo = vec![
        chore(ChoreKind::Must, ChoreState::Todo),
        chore(ChoreKind::Must, ChoreState::Expired),
        chore(ChoreKind::Install, ChoreState::Todo),
        chore(ChoreKind::InstallOptional, ChoreState::Todo),
        chore(ChoreKind::Optional, ChoreState::Todo),
        chore(ChoreKind::Optional, ChoreState::Blocked),
        chore(ChoreKind::Optional, ChoreState::Declined),
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
            vec!["changedFiles", "counts", "own", "rank", "tmux", "todo"]
        );
        assert_eq!(got["tmux"], json!(tmux), "查不动不许说成没有");
        assert_eq!(
            got["counts"],
            json!({ "badge": 3, "open": 6, "must": 2, "decide": 0, "install": 1, "optional": 3, "done": 1 }),
            "角标 ＝ 要做 ＋ 待定 ＋ 要装；没做完 ＝ 不是已做也不是不用了；可选含要装 · 可选与先决没满足的"
        );
    }
}

#[test]
fn 跨台合计_各类逐格相加_一台没有就是全零() {
    use crate::footprint::chores::{merge_counts, ChoreCounts};
    let one = |badge, open, must, done| ChoreCounts {
        badge,
        open,
        must,
        decide: 0,
        install: badge - must,
        optional: open - badge,
        done,
    };
    assert_eq!(
        serde_json::to_value(merge_counts(vec![one(1, 3, 1, 0), one(2, 2, 0, 4)])).unwrap(),
        json!({ "badge": 3, "open": 5, "must": 1, "decide": 0, "install": 2, "optional": 2, "done": 4 })
    );
    assert_eq!(merge_counts(vec![]), ChoreCounts::default());
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
        (
            p["exists"].clone(),
            p["size_text"].clone(),
            p["dir"].clone()
        ),
        (json!(true), json!(copy_core::size_text(5)), json!(false))
    );
    assert_eq!(p["class"], json!("truth"));
    let a = by("accounts");
    assert_eq!(
        (
            a["exists"].clone(),
            a["size_text"].clone(),
            a["dir"].clone()
        ),
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
            vec![
                "abs",
                "absent_said",
                "class",
                "dir",
                "exists",
                "id",
                "label",
                "path",
                "size_text",
                "what"
            ]
        );
    }
}

/// 显示名与绝对路径由后端写好：显示名是家里 `.cc-monitor/` 底下那一段（目录带尾 `/`）；
/// 绝对路径照那台家目录的分隔符拼（Windows 家 ⇒ 全反斜杠，不混 `/`）。
#[test]
fn 每一行带显示名与照那台分隔符拼好的绝对路径() {
    let rows = own_rows(std::path::Path::new(r"C:\Users\x"));
    let by = |id: &str| rows.iter().find(|r| r["id"] == json!(id)).cloned().unwrap();
    assert_eq!(by("profiles")["label"], json!("profiles.toml"));
    assert_eq!(
        by("profiles")["abs"],
        json!(r"C:\Users\x\.cc-monitor\profiles.toml")
    );
    assert_eq!(by("accounts")["label"], json!("accounts/"));
    assert_eq!(
        by("accounts")["abs"],
        json!(r"C:\Users\x\.cc-monitor\accounts")
    );
    for r in &rows {
        let abs = r["abs"].as_str().unwrap();
        assert!(abs.starts_with(r"C:\Users\x\.cc-monitor\"), "{abs}");
        assert!(!abs.contains('/'), "Windows 家拼出混合分隔符：{abs}");
        assert!(!r["label"].as_str().unwrap().starts_with('~'), "{r}");
    }
    let posix = own_rows(std::path::Path::new("/home/u"));
    let p = posix.iter().find(|r| r["id"] == json!("profiles")).unwrap();
    assert_eq!(p["abs"], json!("/home/u/.cc-monitor/profiles.toml"));
}

/// 不在盘上的那一样带后端写好的那一句（界面照抄，不按 `exists` 自己取字）；在的那一样这一格是 `null`。
#[test]
fn 不在盘上的那一样带写好的原因句_在的不带() {
    let h = Home::new("absent");
    let own = h.0.join(".cc-monitor");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("profiles.toml"), b"x").unwrap();
    let rows = own_rows(&h.0);
    let by = |id: &str| rows.iter().find(|r| r["id"] == json!(id)).cloned().unwrap();
    assert_eq!(by("profiles")["absent_said"], Value::Null);
    let bin = by("bin");
    assert_eq!(bin["exists"], json!(false));
    assert_eq!(
        bin["absent_said"],
        json!(copy_core::copy_text("data.item.notCreated", &[])),
        "不在盘上的那一行要带后端写好的那一句"
    );
}

/// 本机那一问（monitor 交了自己的事实）多出 monitor 数据目录里界面自己那几份 ＋ 网页引擎那一行；远端那一问（没有 `client`）一行都不多。
/// 期望逐条手写（名字 · 落点 · 是不是目录），不从被测函数派生。
#[test]
fn 本机那一问多出界面自己那几份_远端那一问没有() {
    let h = Home::new("monitor-rows");
    let data = h.0.join(".cc-monitor");
    std::fs::create_dir_all(data.join("logs").join("backend")).unwrap();
    std::fs::write(data.join("config.json"), b"{}").unwrap();
    let web = h.0.join("web").join("EBWebView");
    let got: Vec<(String, String, bool, bool)> =
        super::monitor_rows(&h.0, &data, Some(web.as_path()))
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_string(),
                    r["path"].as_str().unwrap().to_string(),
                    r["dir"].as_bool().unwrap(),
                    r["exists"].as_bool().unwrap(),
                )
            })
            .collect();
    let row = |id: &str, p: &str, dir: bool, ex: bool| (id.to_string(), p.to_string(), dir, ex);
    assert_eq!(
        got,
        vec![
            row("config", "~/.cc-monitor/config.json", false, true),
            row("autoLaunch", "~/.cc-monitor/auto-launch.json", false, false),
            row(
                "filewinView",
                "~/.cc-monitor/filewin-view.json",
                false,
                false
            ),
            row("psAwait", "~/.cc-monitor/ps-await", true, false),
            row("psRegistry", "~/.cc-monitor/ps-registry", true, false),
            row("logs", "~/.cc-monitor/logs", true, true),
            row("backendLogs", "~/.cc-monitor/logs/backend", true, true),
            row("webview", "~/web/EBWebView", true, false),
        ]
    );
    // 每一行都带写好的那一句（契约里的 id 都认得，没有一行落到「未识别」）。
    for r in super::monitor_rows(&h.0, &data, None) {
        assert_ne!(
            r.what,
            copy_core::copy_text("dataPage.own.unknownItem", &[("id", &r.id)]),
            "{}",
            r.id
        );
    }
    // 那一问带不带 monitor 的事实：带 ⇒ 自家那几样之后接界面自己那几份（网页引擎那一行照它交的落点）；不带（远端那一问）⇒ 只有自家那几样。
    let ids = |rows: Vec<OwnItem>| {
        rows.into_iter()
            .map(|r| r.id.into_owned())
            .collect::<Vec<_>>()
    };
    let own_ids = ids(super::own_rows(&h.0));
    assert_eq!(ids(super::rows_for(&h.0, Some(&data), None)), own_ids);
    let client = crate::footprint::FootprintClient {
        home: h.0.display().to_string(),
        path: None,
        webview: Some(web.display().to_string()),
    };
    let local = ids(super::rows_for(&h.0, Some(&data), Some(&client)));
    assert_eq!(&local[..own_ids.len()], &own_ids[..]);
    assert_eq!(
        &local[own_ids.len()..],
        &[
            "config",
            "autoLaunch",
            "filewinView",
            "psAwait",
            "psRegistry",
            "logs",
            "backendLogs",
            "webview"
        ]
    );
}

/// 界面自己那几份的说明里不说内部词（`sid` / `HWND`）。带正控。
#[test]
fn 界面自己那几份的说明不说内部词() {
    let hits = |s: &str| {
        let low = s.to_lowercase();
        low.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| w == "sid" || w == "hwnd")
    };
    assert!(
        hits("cc 集成的 sid → 终端 HWND 持久绑定"),
        "正控没逮到 —— 尺子瞎了"
    );
    let h = Home::new("r1");
    let bad: Vec<String> = super::monitor_rows(&h.0, &h.0.join(".cc-monitor"), Some(&h.0))
        .into_iter()
        .filter(|r| hits(&r.what))
        .map(|r| format!("{}：{}", r.id, r.what))
        .collect();
    assert_eq!(bad, Vec::<String>::new());
}

/// 〔家里的都进这一份，判据两向，异源〕契约 crate 里 `~/.cc-monitor/` 下的每一个相对路径常量 == 这一份覆盖的路径
/// （后端落点由它所在的 `bin/` 那一行覆盖；只住 monitor 那一侧的设置 `CONFIG_REL`、进程记录不在这台后端的家里算）。
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
    want.remove(relay_route_core::CONFIG_REL);
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
    // 自家那几样 ＋ 本机那一问里界面自己那几份（网页引擎那一行不住数据目录、不在那张表里）。
    let client = crate::footprint::FootprintClient {
        home: h.0.display().to_string(),
        path: None,
        webview: None,
    };
    let rows: Vec<Value> = super::rows_for(&h.0, Some(&h.0.join(".cc-monitor")), Some(&client))
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert!(
        rows.len() >= 10,
        "自家那几样只有 {} 样 —— 下面的逐样比是空真",
        rows.len()
    );
    let mut off = Vec::new();
    let mut names = std::collections::BTreeSet::new();
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
        names.insert(name);
    }
    assert_eq!(
        off,
        Vec::<String>::new(),
        "数据位置那几行与 §2.1 那张表对不上 ⇒ 新加一样要同拍 `OWN_HOME_ENTRIES` / `MONITOR_DATA_ENTRIES` 与 §2.1 那张表"
    );
    // 两向：那张表里的每一样都有一行。
    let unlisted: Vec<&String> = table.keys().filter(|k| !names.contains(*k)).collect();
    assert_eq!(
        unlisted,
        Vec::<&String>::new(),
        "§2.1 那张表列着、数据位置里却没有这一行"
    );
}

/// 用户级 `PATH` 里那一段（`ccm-user-path` 加的）也是改过你的东西：今天在 ⇒ 列一行，路径写它在注册表里的位置，撤回指到机器设置「配置」栏那个开关。
#[test]
fn 用户级_path_那一段算改过你的_撤回在机器设置配置栏() {
    let token = crate::agents::footprint_decl::USER_PATH_TOKEN;
    let got = shape_rows(
        &report(vec![
            row("ccm-user-path", EnvTier::AppInstalls, token, present()),
            row(
                "ccm-user-path",
                EnvTier::AppInstalls,
                token,
                SurfaceState::Absent,
            ),
        ]),
        vec![],
        None,
    );
    let changed = got["changedFiles"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "只算今天在的那一份：{changed:?}");
    assert_eq!(changed[0]["path"], json!(token));
    assert_eq!(
        changed[0]["undo"],
        json!({"page": "machine", "tab": "config"}),
        "撤回在机器设置「配置」栏的用户级 PATH 开关"
    );
}

/// cc-monitor 改过的项目级 MCP 配置（装记录 `touched` 那一格）：一份一行接在足迹那几行后面，路径是那份的绝对路径；没有 ⇒ 不多。
#[test]
fn 改过的项目_mcp_配置一份一行_接在足迹那几行后面() {
    let base = report(vec![row(
        "ccm",
        EnvTier::AppInstalls,
        "~/.bashrc",
        present(),
    )]);
    let none = with_touched(shape(&base, vec![], None, vec![]), &Default::default());
    assert_eq!(none.changed_files.len(), 1, "正控：足迹那一行");
    let t = |names: &[&str]| crate::assets::skill_ledger::Touched {
        before: None,
        after: "0000000000000001".into(),
        names: names.iter().map(|n| n.to_string()).collect(),
        installed: Default::default(),
    };
    let touched = [
        ("/p/a/.mcp.json".to_string(), t(&["x"])),
        ("/p/b/.mcp.json".to_string(), t(&["y", "z"])),
    ]
    .into_iter()
    .collect();
    let got = with_touched(shape(&base, vec![], None, vec![]), &touched);
    let paths: Vec<&str> = got.changed_files.iter().map(|c| &*c.path).collect();
    assert_eq!(paths, ["~/.bashrc", "/p/a/.mcp.json", "/p/b/.mcp.json"]);
    // 撤回在那一行上（帧命令 `mcp-project-revert {file: path}`）：项目 MCP 那几行带，足迹那一行不带、线上也不出那一格。
    let wire = serde_json::to_value(&got).unwrap();
    let rows = wire["changedFiles"].as_array().unwrap();
    assert_eq!(rows[0].get("revert"), None, "{wire}");
    assert_eq!(rows[1]["revert"], json!(true));
    assert_eq!(rows[2]["revert"], json!(true));
}

/// 落盘声明表里故意不在「文件与数据」那一页列的：`(声明的落点, 为什么)`。
const NOT_LISTED: &[(&str, &str)] = &[(
    "<account-config-dir>/<credentials-file>",
    "住每个号自己的配置目录（落点随号）· 不在 cc-monitor 的家里 · 号由账号页管",
)];

/// 落盘声明表（`relay_route_core` 的 `OWN_FILES`，只在那个 crate 的测试档里有）每一行的落点：读那份源码原文，
/// 照 `OWN_FILES` 列的名字找各自的 `Decl` 声明，取第一个参数（字面量，或 `&str` 常量 ⇒ 再取那个常量的字面量）。
fn declared_rels(src: &str) -> Vec<String> {
    // 常量写成字面量，或由布局宏给（`pub const X: &str = x_rel!();` ⇒ 到 `macro_rules! x_rel` 里取那一个字面量）。
    let first_lit = |s: &str| -> Option<String> {
        let q = s.find('"')? + 1;
        Some(s[q..q + s[q..].find('"')?].to_string())
    };
    let lit_of = |name: &str| -> Option<String> {
        let head = format!("pub const {name}: &str = ");
        let rhs = &src[src.find(&head)? + head.len()..];
        if rhs.starts_with('"') {
            return first_lit(rhs);
        }
        let mac = rhs.split("!()").next()?.trim();
        first_lit(&src[src.find(&format!("macro_rules! {mac} {{"))?..])
    };
    let start = src
        .find("const OWN_FILES: &[Row] = &[")
        .expect("声明表 `OWN_FILES` 不在了 —— 改了名就来改这条");
    let body = &src[start..start + src[start..].find("];").unwrap()];
    body.lines()
        .filter_map(|l| l.trim().strip_suffix(".row,"))
        .map(|name| {
            let at = src
                .find(&format!("pub const {name}: Decl<"))
                .unwrap_or_else(|| panic!("{name}：找不到它的 Decl 声明"));
            let rest = &src[at..];
            // 头一个构造口（`::json(` · `::foreign(` 里先出现的那个，免得跨到下一份声明）。
            let open = ["::json(", "::foreign("]
                .iter()
                .filter_map(|k| rest.find(k).map(|i| i + k.len()))
                .min();
            let open = open.unwrap_or_else(|| panic!("{name}：认不出 Decl 的写法"));
            let arg = rest[open..rest[open..].find(',').unwrap() + open].trim();
            match arg.strip_prefix('"') {
                Some(q) => q.trim_end_matches('"').to_string(),
                None => {
                    lit_of(arg).unwrap_or_else(|| panic!("{name}：落点常量 {arg} 取不到字面量"))
                }
            }
        })
        .collect()
}

/// ★★ 「文件与数据」那一页列全落盘声明表（`relay_route_core` 的 `OWN_FILES`）：每一份声明要么在本机那一问的行里
/// （自家那几样 ＋ 界面自己那几样；同名，或落在列着的那个目录底下），要么在 [`NOT_LISTED`] 里写明为什么。
/// 名单里的每一行都得真在声明表里、又真没被列（死名单红）。
#[test]
fn 每一份落盘声明都在本机那一问的行里_或写明为什么不列() {
    const HOME: &str = ".cc-monitor/";
    let own_files = declared_rels(include_str!(
        "../../../src/common/relay-route-core/src/lib.rs"
    ));
    let h = Home::new("declared");
    let client = crate::footprint::FootprintClient {
        home: h.0.display().to_string(),
        path: None,
        webview: None,
    };
    let listed: Vec<String> = super::rows_for(&h.0, Some(&h.0.join(".cc-monitor")), Some(&client))
        .iter()
        .map(|r| {
            let tail = r.path.trim_start_matches("~/").trim_start_matches(HOME);
            if r.dir {
                format!("{tail}/")
            } else {
                tail.to_string()
            }
        })
        .collect();
    let covered = |rel: &str| {
        rel.strip_prefix(HOME).is_some_and(|tail| {
            listed
                .iter()
                .any(|n| n == tail || (n.ends_with('/') && tail.starts_with(n.as_str())))
        })
    };
    assert!(
        own_files.len() >= 20 && own_files.iter().all(|r| !r.is_empty()),
        "声明表只抽出 {} 份：抽取器坏了，下面的比较是空真",
        own_files.len()
    );
    let missing: Vec<&str> = own_files
        .iter()
        .map(String::as_str)
        .filter(|rel| !covered(rel) && !NOT_LISTED.iter().any(|(n, _)| n == rel))
        .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "这几份声明了却不在「文件与数据」那一页 ⇒ 列进 `OWN_HOME_ENTRIES` 或 `MONITOR_DATA_ENTRIES`，或写进 NOT_LISTED 并写明为什么"
    );
    for (rel, why) in NOT_LISTED {
        assert!(!why.is_empty(), "{rel}：名单里没写为什么");
        assert!(
            own_files.iter().any(|r| r == rel),
            "{rel}：名单里这一行不在声明表里（死名单）"
        );
        assert!(!covered(rel), "{rel}：已经列着了，名单里这一行是死的");
    }
}
