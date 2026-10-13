//! 足迹那两条大命令（`footprint-report` · `data-report`）是界面 / 别的台读的线上形：换形之前用旧代码跑出回包字节、钉住；
//! 换形之后逐字节比（经 `serde_json::Value` 那一跳，同帧面：键按字典序）。临时目录换成 `<D>`。
//! 没有重写开关：对不上就是线上形变了。
//!
//! - `footprint-report` 远端那一栏有跨语言金样（`face_tests::the_report_wire_matches_the_cross_language_golden`）；
//!   这里钉本机那一栏（带 `client`）。
//! - 钉住的字节都住 `tests/__fixtures__/footprint-wire.pin.txt`（一份一行：`名字<TAB>帧面那一份`；整份太长，不写进源码）。
//! - `data-report`：成品那一层（[`super::data::shape`]）· 「待办」各件（[`super::chores::chores`]，各分支都走一遍）·
//!   自己家里那几样（[`super::data::own_rows`]）分开钉（整份那一问要问这台的 tmux 与登录 shell，判据不碰）。

use super::chores::{chores, Clash, DeadLines, Facts, Hooks, McpLogin, Relay, SelfPaste, StaleCcm};
use super::registry::EnvTier;
use super::rows::{ConfigSurfaceReport, SurfaceRow, SurfaceState};
use serde_json::json;

/// 帧面那一跳的写法（键按字典序）。
fn wire(v: &impl serde::Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()
}

/// 钉住的那一份：`tests/__fixtures__/footprint-wire.pin.txt` 里 `名字<TAB>字节` 那一行（一份一行）。
fn pinned(name: &str) -> String {
    let path = crate::guard_support::repo_root().join("tests/__fixtures__/footprint-wire.pin.txt");
    let all = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}：{e}"));
    all.lines()
        .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
        .unwrap_or_else(|| panic!("{path:?} 里没有 `{name}` 那一行"))
        .to_string()
}

fn check(name: &str, got: String) {
    assert_eq!(got, pinned(name), "`{name}` 的线上形变了");
}

fn env_of(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
    move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
}

#[cfg(not(windows))]
#[test]
fn footprint_report_local_column_keeps_its_wire_bytes() {
    let home = "/nonexistent-footprint-pin";
    let got = super::answer_with(
        &env_of(vec![("HOME", home.to_string())]),
        &|| None,
        &std::path::Path::new(home).join(".claude"),
        &json!({"client": {"home": "/nonexistent-footprint-pin-client", "path": "/nonexistent-footprint-pin-client/bin"}}),
    )
    .unwrap();
    check("footprint-report.local", wire(&got));
}

fn row(
    tool_id: &'static str,
    tier: EnvTier,
    path: &'static str,
    state: SurfaceState,
) -> SurfaceRow {
    SurfaceRow {
        tool_id,
        tool_name: format!("{tool_id}-name"),
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

fn report() -> ConfigSurfaceReport {
    let present = || SurfaceState::Present { detail: "d".into() };
    ConfigSurfaceReport {
        rows: vec![
            row("ccm", EnvTier::AppInstalls, "~/.local/bin/ccm", present()),
            row(
                "cc-bus",
                EnvTier::AppInstalls,
                "~/.claude/skills/cc-bus",
                present(),
            ),
            row("other", EnvTier::AppInstalls, "~/.other", present()),
            row("own", EnvTier::AppInstalls, "~/.cc-monitor/x", present()),
            row(
                "claude-cli",
                EnvTier::UserInstallsWePrompt,
                "claude",
                SurfaceState::Absent,
            ),
            row(
                "login-shell",
                EnvTier::UserInstallsWePrompt,
                "bash",
                SurfaceState::Absent,
            ),
            row(
                "tmux",
                EnvTier::UserInstallsWePrompt,
                "tmux",
                SurfaceState::Absent,
            ),
            row(
                "odd",
                EnvTier::UserInstallsWePrompt,
                "odd",
                SurfaceState::Undetermined { why: "w".into() },
            ),
        ],
        settings_scopes: vec![],
        agent_home: "/h/.claude".into(),
        home: "/h".into(),
    }
}

fn base() -> Facts {
    Facts {
        windows: false,
        needs_install: vec![],
        stale_ccm: None,
        self_paste: None,
        clashes: vec![],
        dead: vec![],
        relay: vec![],
        hooks: None,
        mcp_login: vec![],
        declined: vec![],
    }
}

fn relay(state: &str, path: &str, text: Option<&str>, url: Option<&str>) -> Relay {
    let face = crate::agents::claudecode::paths::SETTINGS_ENV;
    Relay {
        agent: "claude-code".into(),
        name: "Claude Code".into(),
        slot: face.slot,
        merge: face.merge,
        state: state.into(),
        path: path.into(),
        text: text.map(Into::into),
        url: url.map(Into::into),
        secret: url.map(|_| "k".into()),
    }
}

/// 各分支都走一遍的那一份事实（同一份文件里两件那一形另一份）。
fn every_branch() -> Facts {
    let mut f = base();
    f.needs_install = super::data::needs_install(&report());
    f.stale_ccm = Some(StaleCcm {
        path: "/usr/local/bin/ccm".into(),
        needs_root: true,
    });
    f.self_paste = Some(SelfPaste {
        rc: "/h/.bashrc".into(),
        lines: vec!["# BEGIN".into(), "alias x=y".into()],
        after_line: 12,
        present: false,
    });
    f.clashes = ["yours", "list", "unclear"]
        .iter()
        .enumerate()
        .map(|(i, w)| Clash {
            name: format!("c{i}"),
            path: "/h/.bashrc".into(),
            line: 3 + i,
            wins: (*w).into(),
        })
        .collect();
    f.dead = vec![DeadLines {
        path: "/h/.zshrc".into(),
        lines: vec![(4, "old 1".into()), (9, "old 2".into())],
    }];
    f.relay = vec![
        relay("installed", "/h/a.json", Some("{}\n"), Some("http://u")),
        relay("absent", "/h/b.json", None, None),
        relay(
            "stale",
            "/h/c.json",
            Some("{\n  \"env\": {\n    \"ANTHROPIC_BASE_URL\": \"old\"\n  }\n}\n"),
            Some("http://u"),
        ),
        relay(
            "absent",
            "/h/d.json",
            Some("{\n  \"model\": \"opus\"\n}\n"),
            Some("http://u"),
        ),
    ];
    f.hooks = Some(Hooks {
        ccbus: false,
        installed: false,
        path: "/h/.claude/settings.json".into(),
        text: None,
        items: vec![],
    });
    f.mcp_login = vec![McpLogin {
        name: "m".into(),
        who: vec!["a".into(), "b".into()],
        command: "/mcp",
    }];
    f.declined = vec!["dead:/h/.zshrc".into()];
    f
}

fn same_file() -> Facts {
    let mut f = base();
    let text = "{\n  \"model\": \"opus\"\n}\n".to_string();
    f.relay = vec![relay("absent", "/h/s.json", Some(&text), Some("http://u"))];
    f.hooks = Some(Hooks {
        ccbus: true,
        installed: false,
        path: "/h/s.json".into(),
        text: Some(text),
        items: vec![("Stop".into(), json!({"hooks": []}))],
    });
    f
}

#[test]
fn data_report_chores_keep_their_wire_bytes() {
    check(
        "data-report.todo.every-branch",
        wire(&chores(&every_branch())),
    );
    check("data-report.todo.same-file", wire(&chores(&same_file())));
    let mut installed = same_file();
    installed.hooks.as_mut().unwrap().installed = true;
    installed.relay.clear();
    check(
        "data-report.todo.hooks-installed",
        wire(&chores(&installed)),
    );
}

#[test]
fn data_report_shape_keeps_its_wire_bytes() {
    let todo = chores(&every_branch());
    let own = vec![];
    for (tmux, name) in [
        (Some(true), "data-report.shape.tmux"),
        (None, "data-report.shape.tmux-unknown"),
    ] {
        check(
            name,
            wire(&super::data::shape(
                &report(),
                todo.clone(),
                tmux,
                own.clone(),
            )),
        );
    }
}

#[test]
fn data_report_own_rows_keep_their_wire_bytes() {
    let d = std::env::temp_dir().join(format!("fp-own-pin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".cc-monitor/bin")).unwrap();
    std::fs::write(d.join(".cc-monitor/chores.json"), "{}").unwrap();
    std::fs::write(d.join(".cc-monitor/last-seen.json"), "[1,2]").unwrap();
    let got = wire(&super::data::own_rows(&d));
    let _ = std::fs::remove_dir_all(&d);
    check("data-report.own", got);
}
