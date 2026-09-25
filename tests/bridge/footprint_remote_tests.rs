//! 〔RM1a · 第四波〕`footprint_remote.rs` 的判据 —— 「足迹」的远端那一栏（monitor 半）。
//!
//! # 买到的
//!
//! - 🔴 两趟问法不漏：答题那一趟问到的路径 ⊆ 记账那一趟记下的路径（三种答案盘面各量一次：
//!   全不在 · 全是目录且带名字 · 全是文件）。漏了的那一条会被答成「不在」，与「查了、没有」同形 —— 这条挡它。
//! - 远端视角的人群：住 monitor 那台的那一族（`本机`）**一行都没有**；远端落点那一行**真查**
//!   （答在 ⇒ 「在」、答不在 ⇒ 「缺」，不再是「本页不连 SSH」）；「本机或远端」那一行答不在 ⇒ 「未确定」（不说「缺」）。
//!   两个方向都比同一份 `build_rows` 在本机视角下的产物（人群 = 本机视角去掉 `本机` 那一族，逐条相等）。
//! - 路径过线形：`\` 换 `/`；远端 `PATH` 按 `:` 切、按本机规矩合回去。
//! - 应答解析：`null` 不进表（探针答 `None`）· `entries: null` ⇒ 列不动（不是空目录）· 形状不对报错。
//! - 命令名与字段跨半边对拍（后端 `REGISTRY` 现抠）。
//!
//! # 买不到的
//!
//! - 真远端那一趟：后端那一半的行为判据住 `tests/backend/footprint_tests.rs`。
//! - 远端文件名里真带 `\` 的那一种会被问错（模块头注如实登记）。

use super::*;
use crate::config_surface::{SurfaceState, Vantage};

fn env() -> RemoteEnv {
    RemoteEnv {
        home: PathBuf::from("/r/home"),
        path: native_path_list("/usr/bin:/bin"),
        agent_home: PathBuf::from("/r/home/.claude"),
        agent_home_is_dir: true,
    }
}

/// 把记账那一趟记下的每一条路径都答成同一种事实。
fn answer_all(asked: &Asked, as_dir: Option<bool>) -> Answers {
    let mut a = Answers::default();
    if let Some(dir) = as_dir {
        for p in &asked.stat {
            a.meta.insert(p.clone(), (dir, if dir { 0 } else { 7 }));
            if dir {
                a.list.insert(p.clone(), vec!["cc-x".into(), "ccm".into()]);
            }
        }
        for p in &asked.hooks {
            a.hooks.insert(p.clone(), true);
        }
    }
    a
}

#[test]
fn the_second_pass_asks_nothing_the_first_pass_did_not_record() {
    let e = env();
    let first = ask(&e);
    assert!(
        !first.stat.is_empty(),
        "记账那一趟一条路径都没记 —— 下面的「⊆」是空真"
    );
    assert!(!first.hooks.is_empty(), "记账那一趟一份 settings 都没记");
    for as_dir in [None, Some(true), Some(false)] {
        let (_, second) = build(&e, &answer_all(&first, as_dir));
        let extra: Vec<&String> = second.stat.difference(&first.stat).collect();
        assert!(
            extra.is_empty(),
            "答题那一趟多问了没记过的路径（{as_dir:?}）：{extra:?}"
        );
        let extra: Vec<&String> = second.hooks.difference(&first.hooks).collect();
        assert!(
            extra.is_empty(),
            "答题那一趟多问了没记过的 settings（{as_dir:?}）：{extra:?}"
        );
    }
}

#[test]
fn the_remote_vantage_drops_the_monitor_machine_rows_and_really_probes_the_rest() {
    let e = env();
    let asked = ask(&e);
    let (absent, _) = build(&e, &Answers::default());
    let (present, _) = build(&e, &answer_all(&asked, Some(false)));

    // ① 人群：本机视角的产物去掉 `本机` 那一族，与远端视角逐条相等（按 (工具, 申报路径) 比）。
    let empty = FsProbe {
        meta: &|_| None,
        list: &|_| None,
    };
    let no_dir = |_: &Path| false;
    let monitor_rows = build_rows(&SurfaceEnv {
        home: &e.home,
        cfg_dir_env: None,
        is_dir: &no_dir,
        fs: &empty,
        path_env: None,
        vantage: Vantage::Monitor,
    });
    let client = crate::config_surface::host_label(crate::tool_registry::HostScope::Client);
    let want: Vec<(&str, &str)> = monitor_rows
        .iter()
        .filter(|r| r.host_label != client)
        .map(|r| (r.tool_id, r.path_declared))
        .collect();
    let got: Vec<(&str, &str)> = absent
        .rows
        .iter()
        .map(|r| (r.tool_id, r.path_declared))
        .collect();
    assert!(
        monitor_rows.iter().any(|r| r.host_label == client),
        "本机视角里没有 `本机` 那一族 —— 下面的「去掉」是空真"
    );
    assert_eq!(
        got, want,
        "远端视角的人群不等于「本机视角去掉住 monitor 那台的那一族」"
    );

    // ② 远端落点那一行真查：答在 ⇒ 在；答不在 ⇒ 缺（不再是「本页不连 SSH」）。
    let remote = crate::config_surface::host_label(crate::tool_registry::HostScope::Remote);
    let either = crate::config_surface::host_label(crate::tool_registry::HostScope::Either);
    let pick = |rows: &[crate::config_surface::SurfaceRow], label: &str, declared: &str| {
        rows.iter()
            .find(|r| r.host_label == label && r.path_declared == declared)
            .unwrap_or_else(|| panic!("找不到 {label} 那一行 {declared}"))
            .state
            .clone()
    };
    assert_eq!(
        pick(&absent.rows, remote, "~/.local/bin/ccm"),
        SurfaceState::Absent
    );
    assert!(matches!(
        pick(&present.rows, remote, "~/.local/bin/ccm"),
        SurfaceState::Present { .. }
    ));
    let ccm = absent
        .rows
        .iter()
        .find(|r| r.path_declared == "~/.local/bin/ccm")
        .unwrap();
    assert_eq!(
        ccm.path_resolved.as_deref(),
        Some("/r/home/.local/bin/ccm"),
        "解析用的不是那台的家目录"
    );
    // ③ 「本机或远端」答不在 ⇒ 未确定（也可能在另一台上），不说「缺」。
    assert!(
        matches!(
            pick(&absent.rows, either, "~/.claude/skills/cc-bus"),
            SurfaceState::Undetermined { .. }
        ),
        "「本机或远端」那一行在这台上没找到，被说成了「缺」"
    );
    // settings 那两格也按那台的 agent 家目录解、答案真被用上。
    assert_eq!(absent.claude_config_dir, "/r/home/.claude");
    assert_eq!(present.settings_scopes[0].has_cc_bus_hooks, Some(true));
    assert_eq!(
        absent.settings_scopes[0].has_cc_bus_hooks, None,
        "没答 ⇒ 不猜"
    );
}

#[test]
fn wire_paths_and_path_lists_are_posix_on_the_wire() {
    assert_eq!(
        wire_path(Path::new("/r/home\\.local/bin/ccm")),
        "/r/home/.local/bin/ccm"
    );
    let native = native_path_list("/usr/bin::/bin").expect("合得回去");
    let parts: Vec<PathBuf> = std::env::split_paths(&native).collect();
    assert_eq!(
        parts,
        vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")]
    );
    assert_eq!(native_path_list(""), None, "空 PATH ⇒ 查不动，不是「没有」");
}

#[test]
fn answers_are_parsed_without_guessing() {
    let d = json!({
        "stat": {
            "/a": { "kind": "file", "size": 3 },
            "/d": { "kind": "dir", "size": 0, "entries": ["x"] },
            "/big": { "kind": "dir", "size": 0, "entries": null },
            "/gone": null
        },
        "hooks": { "/s": true, "/t": null }
    });
    let a = answers_from_wire("h", &d).unwrap();
    assert_eq!(a.meta.get("/a"), Some(&(false, 3)));
    assert_eq!(a.list.get("/d"), Some(&vec!["x".to_string()]));
    assert!(
        a.meta.contains_key("/big") && !a.list.contains_key("/big"),
        "列不动那一格该是 None"
    );
    assert!(!a.meta.contains_key("/gone"));
    assert_eq!(a.hooks.get("/s"), Some(&true));
    assert!(!a.hooks.contains_key("/t"), "读不动那一格该是 None（不猜）");
    assert!(answers_from_wire("h", &json!({ "stat": [] , "hooks": {} })).is_err());
    assert!(answers_from_wire(
        "h",
        &json!({ "stat": { "/a": { "kind": "pipe" } }, "hooks": {} })
    )
    .is_err());
    // 环境：没有家目录 ⇒ 报错（解不了 `~/…`，不猜）。
    assert!(env_from_wire(
        "h",
        &json!({ "env": { "agentHome": "/x", "agentHomeIsDir": true } })
    )
    .is_err());
}

#[test]
fn the_command_and_its_fields_are_the_ones_the_backend_registers() {
    let src = include_str!("../../src/backend/inbound.rs");
    let at = src
        .find(&format!("name: \"{CMD}\","))
        .expect("后端没有这条命令");
    assert_eq!(src.matches(&format!("name: \"{CMD}\",")).count(), 1);
    let rest = &src[at..];
    let f = rest.find("fields: &[").unwrap() + "fields: &[".len();
    let end = rest[f..].find(']').unwrap() + f;
    let mut fields: Vec<&str> = rest[f..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
        .collect();
    fields.sort();
    // 本侧读的三格：`answers_from_wire` 读 stat / hooks，`env_from_wire` 读 env。
    // `notices` 本侧不读（为什么答 `null` 的那句话，界面今天没有它的格子；过线是为了能查）。
    assert_eq!(fields, vec!["env", "hooks", "notices", "stat"]);
}

#[test]
fn a_remote_without_a_channel_is_named() {
    let err = tauri::async_runtime::block_on(crate::config_surface::config_surface_report(
        crate::origin::Origin("rm1a-no-such-host-footprint".to_string()),
    ))
    .expect_err("没有通道还出了报告");
    assert!(err.contains("rm1a-no-such-host-footprint"), "实得：{err}");
}
