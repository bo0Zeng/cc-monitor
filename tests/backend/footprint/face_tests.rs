//! # 要求：「`tool_registry` ＋ `build_rows` 进后端，`HostScope::Client` 那几行照旧由 monitor 答」
//!
//! 核原文：同一裁逐字「一处后端」压过「tool_registry 只住 monitor」」；红线「查不了的显示成
//! 「未确定 ＋ 为什么」，绝不显示成「缺失」」·「远端也有真栏」。本族在临时目录上真 stat 真读，判整份足迹（`data-report` 的原料）的
//! 两种问法，各一问：远端那一栏（住 monitor 那台的那一族不进人群）· 本机那一栏（monitor 只交它自己进程的几条事实，四拍收成两拍）。
//!
//! # 买不到的
//!
//! - 🔴 真远端：环境是那台**后端进程**的（agent 命令行那一行按起会话那个 shell 的 `PATH` 判，判据里那一份是夹具，真问登录 shell 那一跳在 `platform/shell_tests.rs`）。
//! - 真 monitor 那一趟（`footprint_client_facts`）：monitor 侧 `tests/frontend/shell/footprint_client_tests.rs`。

use super::*;
use crate::agents::footprint_decl::HostScope;
use crate::footprint::registry::{environment_among, EnvBacking};
use crate::footprint::rows::{SurfaceRow, SurfaceState};
use serde_json::json;

// 线上那一份入参经 typed 那一处收（同分派），再交本体。
fn answer_with(
    get: &dyn Fn(&str) -> Option<String>,
    session_path: &dyn Fn() -> Option<String>,
    agent_home: &Path,
    args: &serde_json::Value,
) -> Result<ConfigSurfaceReport, crate::common::said::Said> {
    let req: FootprintReq = crate::wire::typed::take(args).map_err(|f| f.coded("bad_args"))?;
    // 用户级 `PATH` 那一格只有 Windows 那一台问得出；这里的判据不问它 ⇒ 问不出。
    super::answer_with(
        crate::agents::REGISTRY,
        get,
        session_path,
        agent_home,
        &|| None,
        &req,
    )
}

fn env_of(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
    move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
}

/// 起会话那个 shell 的 `PATH` 问不出来（判据默认：不碰这台的登录环境）。
fn no_session() -> Option<String> {
    None
}

fn key(r: &SurfaceRow) -> (String, String) {
    (r.tool_id.to_string(), r.path_declared.to_string())
}

fn kind(s: &SurfaceState) -> &'static str {
    match s {
        SurfaceState::Present => "present",
        SurfaceState::Absent => "absent",
        SurfaceState::Undetermined => "undetermined",
    }
}

/// 那一行（工具 · 申报路径）的状态。
fn kind_of(rows: &[SurfaceRow], tool: &str, declared: &str) -> &'static str {
    kind(
        &rows
            .iter()
            .find(|r| r.tool_id == tool && r.path_declared.as_ref() == declared)
            .unwrap_or_else(|| panic!("找不到 {tool} 那一行 {declared}"))
            .state,
    )
}

/// 申报表里住 monitor 那台（`HostScope::Client`）的那几行，按 (工具, 申报路径)。
fn client_keys() -> Vec<(String, String)> {
    environment_among(crate::agents::REGISTRY, cfg!(windows))
        .iter()
        .flat_map(|e| match e.backing {
            EnvBacking::Managed(t) => t
                .touches()
                .filter(|f| f.host == HostScope::Client)
                .map(|f| (e.id.to_string(), f.path.to_string()))
                .collect::<Vec<_>>(),
            EnvBacking::Named { named, host, .. } => (host == HostScope::Client)
                .then(|| (e.id.to_string(), named.to_string()))
                .into_iter()
                .collect(),
        })
        .collect()
}

/// 同一份 `build_rows` 在「monitor 那台」视角下的人群（`with_client` = 含住 monitor 那台的那一族），按 (工具, 申报路径) 比。
fn monitor_vantage_population(home: &Path, with_client: bool) -> Vec<(String, String)> {
    let empty = FsProbe {
        meta: &|_| None,
        list: &|_| None,
        user_path: &|| None,
    };
    let agent = home.join(".claude");
    let env = SurfaceEnv {
        home,
        agent_homes: &crate::agents::IntoHomes::into_homes(&agent),
        fs: &empty,
        path_env: None,
        session_path: None,
        system_root: None,
        windows: false,
        vantage: Vantage::Monitor,
        registry: crate::agents::REGISTRY,
        accounts: &[],
    };
    let client = client_keys();
    build_rows(&env, with_client.then_some(&env))
        .iter()
        .map(key)
        .filter(|k| with_client || !client.contains(k))
        .collect()
}

/// ★ 远端那一栏：一问即得；住 monitor 那台的那一族一行都没有；这台的落点**真查**（在 ⇒ 在，不在 ⇒ 缺）；
/// 「本机或远端」那一行没找到 ⇒ 未确定（不说「缺」）。
#[test]
fn the_remote_column_really_probes_this_machine_and_drops_the_monitor_rows() {
    let h = test_home::scratch("remote");
    std::fs::create_dir_all(h.join(".cc-monitor/bin")).unwrap();
    std::fs::write(h.join(".cc-monitor/bin/ccm"), "x").unwrap();
    let agent = h.join(".claude");
    let ask = || {
        answer_with(
            &env_of(vec![("HOME", h.display().to_string())]),
            &no_session,
            &agent,
            &json!({}),
        )
        .unwrap()
        .rows
    };
    let rows = ask();
    assert!(
        !client_keys().is_empty(),
        "住 monitor 那台的一行都没有 —— 下面是空真"
    );
    assert_eq!(
        rows.iter().map(key).collect::<Vec<_>>(),
        monitor_vantage_population(&h, false),
        "远端视角的人群不等于「monitor 那台视角去掉住 monitor 那台的那一族」"
    );
    assert_eq!(kind_of(&rows, "ccm", "~/.cc-monitor/bin/ccm"), "present");
    // 同一个落点拿走之后再问一次：这台上真不在 ⇒ 缺（不是「本页不连 SSH」）。
    std::fs::remove_file(h.join(".cc-monitor/bin/ccm")).unwrap();
    assert_eq!(
        kind_of(&ask(), "ccm", "~/.cc-monitor/bin/ccm"),
        "absent",
        "这台上真不在的远端落点 ⇒ 缺（不再是「本页不连 SSH」）"
    );
    assert_eq!(
        kind_of(&rows, "cc-bus", "~/.claude/skills/cc-bus"),
        "undetermined",
        "「本机或远端」那一行在这台上没找到，被说成了「缺」"
    );
    let _ = std::fs::remove_dir_all(&h);
}

/// ★ 本机那一栏一问：monitor 只交它自己进程的那几条（家目录 · PATH；agent 家用这台后端自己解析的那一个），人群 == monitor 那台视角（含住 monitor 那台的那一族），
/// 那一族按 **monitor 交来的**家目录解、由这台 stat（同一台、同一用户）—— 放在那个家目录下的文件答「在」，这台自己的家目录不掺进来。
#[test]
fn the_local_column_resolves_the_monitor_rows_under_the_monitor_facts() {
    let h = test_home::scratch("local");
    let agent = h.join(".claude");
    let m = test_home::scratch("local-monitor");
    let ask = || {
        let client = json!({ "home": m.display().to_string(), "path": "/m/bin" });
        answer_with(
            &env_of(vec![("HOME", h.display().to_string())]),
            &no_session,
            &agent,
            &json!({ "client": client }),
        )
        .unwrap()
        .rows
    };
    // 只放在这台自己的家目录下 ⇒ monitor 那一族不认它。
    std::fs::create_dir_all(h.join(".cc-monitor/bin")).unwrap();
    std::fs::write(h.join(".cc-monitor/bin/ccm"), "x").unwrap();
    let rows = ask();
    assert_eq!(
        rows.iter().map(key).collect::<Vec<_>>(),
        monitor_vantage_population(&h, true),
        "本机那一栏的人群不等于 monitor 那台视角"
    );
    assert_ne!(
        kind_of(&rows, "ccm", "~/.cc-monitor/bin/ccm*"),
        "present",
        "按这台的家目录解了 monitor 那一族"
    );
    // 放在 monitor 交来的家目录下 ⇒ 在。
    std::fs::create_dir_all(m.join(".cc-monitor/bin")).unwrap();
    std::fs::write(m.join(".cc-monitor/bin/ccm"), "x").unwrap();
    assert_eq!(
        kind_of(&ask(), "ccm", "~/.cc-monitor/bin/ccm*"),
        "present",
        "monitor 家目录下那一份没被这台 stat 到"
    );
    let _ = std::fs::remove_dir_all(&h);
    let _ = std::fs::remove_dir_all(&m);
}

#[test]
fn a_directory_over_the_cap_is_unlistable_not_truncated() {
    let d = test_home::scratch("cap");
    for i in 0..=MAX_ENTRIES {
        std::fs::write(d.join(format!("n{i}")), "").unwrap();
    }
    assert_eq!(meta_of(&d), Some((true, 0)));
    assert_eq!(
        list_of(&d),
        None,
        "超过 {MAX_ENTRIES} 个名字应当报「列不动」，不是一份截断的清单"
    );
    std::fs::remove_file(d.join("n0")).unwrap();
    assert_eq!(list_of(&d).map(|v| v.len()), Some(MAX_ENTRIES));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_arguments_are_refused() {
    let d = test_home::scratch("args");
    let get = env_of(vec![("HOME", d.display().to_string())]);
    let cases = [
        json!({ "client": [] }),
        json!({ "client": { "home": "rel" } }),
        json!({ "client": { "path": "/m/bin" } }),
    ];
    for args in cases {
        let err = answer_with(&get, &no_session, &d, &args).expect_err("坏入参还成功了");
        assert_eq!(
            err.code.as_deref(),
            Some("bad_args"),
            "{args} 应当是 bad_args，实得 {err:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn home_falls_back_to_userprofile_and_is_required() {
    let d = test_home::scratch("home");
    let got = answer_with(
        &env_of(vec![("USERPROFILE", d.display().to_string())]),
        &no_session,
        &d.join(".claude"),
        &json!({}),
    )
    .unwrap();
    assert_eq!(
        got.home,
        d.display().to_string(),
        "HOME 缺 ⇒ 退 USERPROFILE"
    );
    let e = answer_with(&env_of(vec![]), &no_session, &d, &json!({})).unwrap_err();
    assert_eq!(
        e.code.as_deref(),
        Some("failed"),
        "没有家目录就解不了 `~/…`，不猜"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 那一行（按工具）的状态。
fn kind_of_tool(rows: &[SurfaceRow], tool: &str) -> &'static str {
    kind(
        &rows
            .iter()
            .find(|r| r.tool_id == tool)
            .unwrap_or_else(|| panic!("找不到 {tool} 那一行"))
            .state,
    )
}

/// ★ agent 命令行装没装，按**起会话那个 shell** 的 `PATH` 判，不按后端进程自己的：
/// 后端经 ssh 非登录 shell 起时 `PATH` 里常常没有用户级 `bin`，而会话是在用户的登录 shell 里起的。
/// 后端进程的 `PATH` 里没有、起会话的 shell 里有 ⇒ 在；那个 shell 问不出来 ⇒ 判不了（不说缺）；那里也确实没有 ⇒ 缺。
#[test]
fn the_agent_cli_is_judged_in_the_session_shell_not_the_backend_process() {
    let h = test_home::scratch("session-shell");
    let user_bin = h.join("user-bin");
    let sys_bin = h.join("sys-bin");
    std::fs::create_dir_all(&user_bin).unwrap();
    std::fs::create_dir_all(&sys_bin).unwrap();
    std::fs::write(user_bin.join("claude"), "x").unwrap();
    let agent = h.join(".claude");
    let get = env_of(vec![
        ("HOME", h.display().to_string()),
        ("PATH", sys_bin.display().to_string()),
    ]);
    let ask = |session: &dyn Fn() -> Option<String>| {
        answer_with(&get, session, &agent, &json!({})).unwrap().rows
    };
    let login = format!("{}:{}", user_bin.display(), sys_bin.display());
    assert_eq!(
        kind_of_tool(&ask(&|| Some(login.clone())), "claude-cli"),
        "present",
        "起会话的 shell 里找得到，却按后端进程的 PATH 判成了别的"
    );
    assert_eq!(
        kind_of_tool(&ask(&no_session), "claude-cli"),
        "undetermined",
        "起会话的 shell 问不出来，却下了结论"
    );
    let bare = sys_bin.display().to_string();
    assert_eq!(
        kind_of_tool(&ask(&|| Some(bare.clone())), "claude-cli"),
        "absent",
        "起会话的 shell 里也确实没有 ⇒ 该说缺"
    );
    let _ = std::fs::remove_dir_all(&h);
}

/// 整份足迹照注入的注册表收（两栏都是）：两家替身的足迹面跟生产那家不同 ⇒ 行照替身出，生产那家的行一行都不出。
#[test]
fn both_columns_follow_the_injected_registry() {
    let h = test_home::scratch("injected-registry");
    let get = env_of(vec![("HOME", h.display().to_string())]);
    let fakes = crate::agents::fake::footprint::two_footprint_families();
    let client = h.join("client-home");
    for req in [
        FootprintReq::default(),
        FootprintReq {
            client: Some(FootprintClient {
                home: client.display().to_string(),
                path: None,
                webview: None,
            }),
        },
    ] {
        let rows = super::answer_with(fakes, &get, &no_session, &h, &|| None, &req)
            .unwrap()
            .rows;
        let ids: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.tool_id).collect();
        for id in ["fake-a-records", "fake-b-records"] {
            assert!(ids.contains(id), "少了替身那一行 {id}：{ids:?}（{req:?}）");
        }
        assert!(
            !ids.contains("claude-cli"),
            "照生产注册表收了：{ids:?}（{req:?}）"
        );
    }
}

/// monitor 那一栏也照注入的那一份：替身家只认 `~/.fake-a/…` 为自己家底下、那一条只住 monitor 那台；
/// 照替身答 ⇒ 落到替身的家（在）；照生产注册表答 ⇒ 认不出这一家、落回 monitor 交来的家目录（不在）。
#[test]
fn the_client_column_follows_the_injected_registry_too() {
    use crate::agents::footprint_decl::{
        Carrier, ToolDestination, ToolSource, ToolSpec, TouchEffect, TouchedFile,
    };
    const CLIENT_TOOLS: &[ToolSpec] = &[ToolSpec {
        id: "fake-client-records",
        display_name: crate::agents::footprint_decl::Text(|| {
            "替身 monitor 那台的记录树".to_string()
        }),
        installable: false,
        uninstallable: false,
        carriers: &[Carrier {
            source: ToolSource::NotOurs,
            destination: ToolDestination::NotInstalledByUs,
            touches: &[TouchedFile {
                path: "~/.fake-a/convos/",
                host: HostScope::Client,
                effect: TouchEffect::ReadOnly,
            }],
        }],
    }];
    fn under(rest: &str) -> Option<&str> {
        rest.strip_prefix(".fake-a/")
    }
    let row = crate::agents::fake::family_row("fake", "CCM_FAKE_A", "fakea", true, || None);
    let registry = [crate::agents::Adapter {
        footprint: Some(crate::agents::FootprintFace {
            tools: CLIENT_TOOLS,
            env: &[],
            under_agent_home: under,
            ..crate::agents::fake::footprint::FACE_A
        }),
        ..row
    }];
    let h = test_home::scratch("injected-client");
    let fake_home = h.join(".fa");
    std::fs::create_dir_all(fake_home.join("convos")).unwrap();
    let client = h.join("client-home");
    std::fs::create_dir_all(&client).unwrap();
    let homes = [crate::agents::AgentHome {
        kind: "fake",
        home: fake_home,
    }];
    let get = env_of(vec![("HOME", h.display().to_string())]);
    let req = FootprintReq {
        client: Some(FootprintClient {
            home: client.display().to_string(),
            path: None,
            webview: None,
        }),
    };
    let rows = super::answer_with(&registry, &get, &no_session, &homes, &|| None, &req)
        .unwrap()
        .rows;
    let r = rows
        .iter()
        .find(|r| r.tool_id == "fake-client-records")
        .expect("替身那一行");
    assert_eq!(
        r.state,
        SurfaceState::Present,
        "monitor 那一栏没照注入的那一家答家底下那一段"
    );
}

/// 家交成一个目录（判据那一形）时，那个目录当注入的那几家的家：替身家只认 `~/.fake-a/…` 为自己家底下 ⇒ 落到交来的那个目录（在）；
/// 照生产注册表展开 ⇒ 列的是生产那几家、认不出替身 ⇒ 落回家目录（不在）。
#[test]
fn the_homes_are_spread_over_the_injected_registry() {
    use crate::agents::footprint_decl::{
        Carrier, ToolDestination, ToolSource, ToolSpec, TouchEffect, TouchedFile,
    };
    const TOOLS: &[ToolSpec] = &[ToolSpec {
        id: "fake-either-records",
        display_name: crate::agents::footprint_decl::Text(|| "替身两边都有的记录树".to_string()),
        installable: false,
        uninstallable: false,
        carriers: &[Carrier {
            source: ToolSource::NotOurs,
            destination: ToolDestination::NotInstalledByUs,
            touches: &[TouchedFile {
                path: "~/.fake-a/logs/",
                host: HostScope::Either,
                effect: TouchEffect::ReadOnly,
            }],
        }],
    }];
    fn under(rest: &str) -> Option<&str> {
        rest.strip_prefix(".fake-a/")
    }
    let row = crate::agents::fake::family_row("fake", "CCM_FAKE_A", "fakea", true, || None);
    let registry = [crate::agents::Adapter {
        footprint: Some(crate::agents::FootprintFace {
            tools: TOOLS,
            env: &[],
            under_agent_home: under,
            ..crate::agents::fake::footprint::FACE_A
        }),
        ..row
    }];
    let h = test_home::scratch("injected-homes");
    let fake_home = h.join(".fa");
    std::fs::create_dir_all(fake_home.join("logs")).unwrap();
    let get = env_of(vec![("HOME", h.display().to_string())]);
    let rows = super::answer_with(
        &registry,
        &get,
        &no_session,
        fake_home.as_path(),
        &|| None,
        &FootprintReq::default(),
    )
    .unwrap()
    .rows;
    let r = rows
        .iter()
        .find(|r| r.tool_id == "fake-either-records")
        .expect("替身那一行");
    assert_eq!(
        r.state,
        SurfaceState::Present,
        "交来的家没照注入的注册表展开"
    );
}
