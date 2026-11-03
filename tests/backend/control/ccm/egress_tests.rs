//! 起会话时接管 agent 原本的出网设置：注入中转、设了代理 ⇒ agent 的免代理表（两形）加上中转口，计划里与打印那一行同一份。
//! 不起真 agent、不连网：计划面是纯的（继承来的环境由夹具答）。

use super::*;
use crate::control::ccm::plan::{self, Env, Plan};

fn ask(
    _: &str,
    _: &crate::accounts::upstream_select::endpoint::LaunchAccount,
    _: bool,
) -> Result<Option<String>, String> {
    Ok(Some("http://127.0.0.1:8788/t/claude-code/x".into()))
}

fn no_relay(
    _: &str,
    _: &crate::accounts::upstream_select::endpoint::LaunchAccount,
    _: bool,
) -> Result<Option<String>, String> {
    Ok(None)
}

fn env(relay: plan::RelayAsk) -> Env {
    Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        base_url_env: crate::agents::claudecode::paths::BASE_URL_ENV.into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        relay: Some(relay),
        origin_token: Some("0123abcd".into()),
        ..Default::default()
    }
}

/// 起一次 claude（`--resume s1`，直路），计划交给接管那一下，继承来的环境由 `vars` 答。
fn launch(relay: plan::RelayAsk, vars: &[(&str, &str)]) -> plan::Direct {
    launch_taken(relay, vars).0
}

/// 同 [`launch`]，连同交回的那一份（这条会话要留的上游描述）。
fn launch_taken(relay: plan::RelayAsk, vars: &[(&str, &str)]) -> (plan::Direct, Option<Taken>) {
    launch_from(relay, None, vars)
}

/// 同 [`launch_taken`]，继承来的上游地址变量是 `base`。
fn launch_from(
    relay: plan::RelayAsk,
    base: Option<&str>,
    vars: &[(&str, &str)],
) -> (plan::Direct, Option<Taken>) {
    let a: Vec<String> = ["--resume", "s1"].iter().map(|s| s.to_string()).collect();
    let crate::ccm_line::argv::Parsed::Opts(o) = crate::ccm_line::argv::test_support::parse(
        &crate::ccm_line::argv::test_support::mixed_to_split(&a),
    )
    .expect("解析得动") else {
        panic!()
    };
    let mut e = env(relay);
    e.inherited_base_url = base.map(str::to_string);
    let mut p = plan::build(&o, &e, &plan::test_support::no_library, None).expect("算得出计划");
    let get = |k: &str| {
        vars.iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| v.to_string())
    };
    let taken = onto(&mut p, crate::agents::proxy_of("claude"), &get);
    let Plan::Direct(d) = p else {
        panic!("该是直路")
    };
    (d, taken)
}

/// 交回的那一份：这一发的来处 ＋ 继承来的、这一家认的出网设置变量原样（不认的不带）；没设代理 · 不注入 ⇒ 不交回。
#[test]
fn the_session_part_is_this_launch_origin_and_the_inherited_proxy_variables() {
    let (d, taken) = launch_taken(
        ask,
        &[
            ("HTTPS_PROXY", "http://127.0.0.1:3128"),
            ("NO_PROXY", "internal.example"),
            ("ALL_PROXY", "http://127.0.0.1:9"),
        ],
    );
    assert!(
        d.relay
            .as_deref()
            .is_some_and(|u| u.ends_with("/~0123abcd")),
        "{:?}",
        d.relay
    );
    let t = taken.expect("设了代理、地址带来处 ⇒ 交回那一份");
    assert_eq!(t.origin, "0123abcd");
    assert_eq!(
        t.egress,
        vec![
            (
                "HTTPS_PROXY".to_string(),
                "http://127.0.0.1:3128".to_string()
            ),
            ("NO_PROXY".to_string(), "internal.example".to_string()),
        ]
    );
    assert!(launch_taken(ask, &[("NO_PROXY", "x")]).1.is_none());
    assert!(
        launch_taken(no_relay, &[("HTTPS_PROXY", "http://127.0.0.1:9")])
            .1
            .is_none()
    );
}

/// ★ 用户自己的上游地址也进交回的那一份（`base`）：没设代理也交；与代理同时有 ⇒ 两格都在；没注入中转 ⇒ 不交。
#[test]
fn the_session_part_carries_the_users_own_upstream() {
    let gw = "https://gw.example/anthropic";
    let (d, taken) = launch_from(ask, Some(gw), &[]);
    assert_eq!(d.own_upstream.as_deref(), Some(gw));
    let t = taken.expect("上游是用户的、地址带来处 ⇒ 交回那一份");
    assert_eq!(
        (t.origin.as_str(), t.base.as_deref()),
        ("0123abcd", Some(gw))
    );
    assert!(t.egress.is_empty());
    let (_, taken) = launch_from(ask, Some(gw), &[("HTTPS_PROXY", "http://127.0.0.1:3128")]);
    let t = taken.expect("两者都有");
    assert_eq!(t.base.as_deref(), Some(gw));
    assert_eq!(t.egress.len(), 1);
    assert!(launch_from(no_relay, Some(gw), &[]).1.is_none());
    assert_eq!(
        launch_taken(ask, &[("HTTPS_PROXY", "http://127.0.0.1:3128")])
            .1
            .expect("只有代理")
            .base,
        None
    );
}

fn value<'a>(d: &'a plan::Direct, k: &str) -> Option<&'a str> {
    d.relay_env
        .iter()
        .find(|(n, _)| n == k)
        .map(|(_, v)| v.as_str())
}

/// ★ 第一条会红：带 `HTTPS_PROXY=<代理>` 起会话 ⇒ agent 的免代理表（两形）含中转口、原来那一份在前，打印那一行照实印；
/// 代理变量本身不由 ccm 改（照继承留给 agent）。
#[test]
fn a_launch_with_a_proxy_keeps_the_relay_off_it() {
    let d = launch(
        ask,
        &[
            ("HTTPS_PROXY", "http://127.0.0.1:3128"),
            ("NO_PROXY", "internal.example"),
        ],
    );
    for k in ["NO_PROXY", "no_proxy"] {
        assert_eq!(
            value(&d, k),
            Some("internal.example,127.0.0.1"),
            "{k} 没含中转口：{:?}",
            d.relay_env
        );
    }
    let line = plan::render(&Plan::Direct(d.clone()));
    assert!(
        line.contains("export NO_PROXY=") && line.contains("export no_proxy="),
        "打印那一行没说免代理表：{line}"
    );
    assert!(
        !d.relay_env
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("https_proxy")),
        "代理变量该照留（不由 ccm 改）：{:?}",
        d.relay_env
    );
}

/// 没设代理 ⇒ 免代理变量不设（继承来的免代理表也不动）。
#[test]
fn without_a_proxy_nothing_is_taken_over() {
    let d = launch(ask, &[("NO_PROXY", "internal.example")]);
    assert_eq!(value(&d, "NO_PROXY"), None, "{:?}", d.relay_env);
    assert_eq!(value(&d, "no_proxy"), None);
}

/// 这一家不认的代理变量（Claude 不认 `ALL_PROXY`）⇒ 当没设。
#[test]
fn a_proxy_variable_the_family_does_not_read_is_not_taken() {
    let d = launch(ask, &[("ALL_PROXY", "http://127.0.0.1:9")]);
    assert_eq!(value(&d, "NO_PROXY"), None);
}

/// 不注入中转 ⇒ 都不动。
#[test]
fn without_the_relay_nothing_is_taken_over() {
    let d = launch(no_relay, &[("HTTPS_PROXY", "http://127.0.0.1:9")]);
    assert_eq!(d.relay, None);
    assert_eq!(value(&d, "NO_PROXY"), None);
}

/// 免代理表里已经有中转口的主机 ⇒ 不重复加；只设了小写那一形 ⇒ 两形都设成同一份。
#[test]
fn the_relay_host_is_added_once_and_both_forms_agree() {
    let d = launch(
        ask,
        &[
            ("https_proxy", "http://127.0.0.1:9"),
            ("no_proxy", "127.0.0.1,.corp"),
        ],
    );
    assert_eq!(value(&d, "no_proxy"), Some("127.0.0.1,.corp"));
    assert_eq!(value(&d, "NO_PROXY"), Some("127.0.0.1,.corp"));
}

/// exec 之前留描述：写得下 ⇒ 落盘、pid 是 ccm 自己的；写不成 ⇒ 带用户上游地址那一份不起（`Err`），只有出网那一份照常起。
#[test]
fn the_description_is_left_before_exec_and_a_lost_own_upstream_stops_the_launch() {
    use crate::accounts::upstream_select::session_upstream::of;
    let root = std::env::temp_dir().join(format!("ccm-leave-upstream-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("sandbox");
    let own_upstream = Taken {
        origin: "0123abcd".into(),
        egress: Vec::new(),
        base: Some("https://gw.example/anthropic".into()),
    };
    let proxy_only = Taken {
        origin: "4567cdef".into(),
        egress: vec![("HTTPS_PROXY".into(), "http://127.0.0.1:3128".into())],
        base: None,
    };
    let own = crate::platform::fs::Own::issue_data_at(&root);
    assert_eq!(super::super::leave_upstream(&own_upstream, &own), Ok(()));
    let back = of(&root, "0123abcd").expect("落了盘");
    assert_eq!(
        (back.base.as_deref(), back.pid),
        (Some("https://gw.example/anthropic"), std::process::id())
    );
    // 数据目录那一格是个文件 ⇒ 写不成。
    let blocked = root.join("blocked");
    std::fs::write(&blocked, "x").expect("file");
    let own = crate::platform::fs::Own::issue_data_at(&blocked);
    assert!(super::super::leave_upstream(&own_upstream, &own).is_err());
    assert_eq!(super::super::leave_upstream(&proxy_only, &own), Ok(()));
    std::fs::remove_dir_all(&root).ok();
}
