//! 上游描述落盘与读回（出网那两格）：落下的读得回 · 读回的照那个代理走 · 来处不过段闸不写 · 写的那一下清掉进程已不在的那几份。

use super::*;

fn desc(pid: u32, vars: &[(&str, &str)]) -> UpstreamDesc {
    UpstreamDesc {
        egress: vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        base: None,
        account: None,
        pid,
        at: 1,
    }
}

/// 用户自己的上游地址那一格落盘读回一致。
#[test]
fn the_users_own_upstream_reads_back() {
    let data = test_home::scratch("base");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let mut d = desc(7, &[]);
    d.base = Some("https://gw.example/anthropic".into());
    leave(&own, "0123abcd", &d, &|_| true).expect("写得下");
    assert_eq!(
        of(&data, "0123abcd").and_then(|d| d.base).as_deref(),
        Some("https://gw.example/anthropic")
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_left_description_reads_back_and_routes_through_its_proxy() {
    let data = test_home::scratch("back");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let d = desc(
        7,
        &[
            ("HTTPS_PROXY", "http://127.0.0.1:3128"),
            ("NO_PROXY", "internal.example"),
        ],
    );
    leave(&own, "0123abcd", &d, &|_| true).expect("写得下");
    let back = of(&data, "0123abcd").expect("读得回");
    assert_eq!(back, d);
    let e = back.egress();
    match e.hop_for("api.anthropic.com") {
        egress::Hop::Via(p) => assert_eq!((p.host.as_str(), p.port), ("127.0.0.1", 3128)),
        other => panic!("到上游没经那个代理：{other:?}"),
    }
    assert_eq!(e.hop_for("internal.example"), egress::Hop::Direct);
    assert_eq!(of(&data, "ffff0000"), None, "别的来处读出了东西");
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn an_origin_that_fails_the_segment_gate_is_not_written() {
    let data = test_home::scratch("gate");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let d = desc(7, &[("HTTPS_PROXY", "http://127.0.0.1:3128")]);
    assert!(leave(&own, "../x", &d, &|_| true).is_err());
    assert_eq!(of(&data, "../x"), None);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn leaving_one_clears_those_whose_process_is_gone() {
    let data = test_home::scratch("prune");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let p = [("HTTPS_PROXY", "http://127.0.0.1:3128")];
    leave(&own, "aaaa", &desc(10, &p), &|_| true).unwrap();
    leave(&own, "bbbb", &desc(11, &p), &|_| true).unwrap();
    // 写第三份那一下：10 已不在、11 还在。
    leave(&own, "cccc", &desc(12, &p), &|pid| pid != 10).unwrap();
    assert_eq!(of(&data, "aaaa"), None, "进程已不在的那一份没清");
    assert!(of(&data, "bbbb").is_some(), "进程还在的那一份被清了");
    assert!(of(&data, "cccc").is_some());
    std::fs::remove_dir_all(&data).ok();
}

/// 会话血缘里把来处 `token` 绑给 `sid`（落盘那一份，与中转写的同一形）。
fn bind(data: &std::path::Path, token: &str, sid: &str) {
    let mut book = match crate::lineage::read_at(&data.join(crate::lineage::FILE_NAME)) {
        crate::common::own_state::Read::Present(b) => b,
        _ => crate::lineage::Book::default(),
    };
    book.origins.insert(
        token.into(),
        crate::lineage::Bound {
            sid: sid.into(),
            at: 1,
        },
    );
    std::fs::write(
        data.join(crate::lineage::FILE_NAME),
        serde_json::to_vec(&book).expect("序列化"),
    )
    .expect("写血缘");
}

/// ★ 上游是不是用户自己的：绑给这条会话的来处、那份描述有用户的地址 ⇒ 是；描述只有出网那两格 · 来处没绑给它 · 没有血缘 ⇒ 不是。
#[test]
fn a_session_is_own_upstream_only_when_its_bound_origin_carries_a_base() {
    let data = test_home::scratch("own-of");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    assert!(!own_upstream_of(&data, "s-own"), "没有血缘也说成自设");
    let mut d = desc(7, &[]);
    d.base = Some("https://gw.example/anthropic".into());
    leave(&own, "tok1", &d, &|_| true).unwrap();
    let p = [("HTTPS_PROXY", "http://127.0.0.1:3128")];
    leave(&own, "tok2", &desc(8, &p), &|_| true).unwrap();
    bind(&data, "tok1", "s-own");
    bind(&data, "tok2", "s-proxy");
    assert!(own_upstream_of(&data, "s-own"));
    assert!(
        !own_upstream_of(&data, "s-proxy"),
        "只有出网代理也说成自设上游"
    );
    assert!(!own_upstream_of(&data, "s-other"), "没绑来处的会话说成自设");
    std::fs::remove_dir_all(&data).ok();
}

/// 落盘那一份顶层带声明的版本，读回照常。
#[test]
fn a_left_description_carries_its_version() {
    let data = test_home::scratch("ver");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    leave(&own, "0123abcd", &desc(7, &[]), &|_| true).expect("写得下");
    let file = data
        .join(NOTES_DIR)
        .join(SUB)
        .join(file_name("0123abcd").unwrap());
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(v["version"], serde_json::json!(1), "没带版本：{v}");
    assert_eq!(
        of(&data, "0123abcd").map(|d| d.pid),
        Some(7),
        "带版本的那一份读不回"
    );
    std::fs::remove_dir_all(&data).ok();
}

/// 起会话那个号那一格落盘读回一致；没写 ⇒ 读回没有（旧的那份没有这一格也读得懂）。
#[test]
fn the_launch_account_reads_back() {
    let data = test_home::scratch("acct");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let mut d = desc(7, &[]);
    d.account = Some("b".into());
    leave(&own, "0123abcd", &d, &|_| true).expect("写得下");
    assert_eq!(
        of(&data, "0123abcd").and_then(|d| d.account).as_deref(),
        Some("b")
    );
    leave(&own, "4567cdef", &desc(7, &[]), &|_| true).expect("写得下");
    assert_eq!(of(&data, "4567cdef").and_then(|d| d.account), None);
    std::fs::remove_dir_all(&data).ok();
}

/// 起会话那条中转地址 ⇒ 它路由第 2 段的号；没说是哪个号（`_`）· 不是中转地址 ⇒ 没有。
#[test]
fn the_launch_account_is_read_off_the_relay_address() {
    assert_eq!(
        account_in("http://127.0.0.1:7000/t/claude-code/b/~0123abcd").as_deref(),
        Some("b")
    );
    assert_eq!(
        account_in("http://127.0.0.1:7000/t/claude-code/b").as_deref(),
        Some("b")
    );
    assert_eq!(
        account_in("http://127.0.0.1:7000/t/claude-code/_/~0123abcd"),
        None
    );
    assert_eq!(account_in("https://api.example.com/v1"), None);
}

/// ★ 替身家声明 `Bind::Origin`、第一发带着来处 ⇒ 这条会话钉在那个来处描述里记的号；描述里没有号 · 没有描述 ⇒ 照起会话那一段的号。
/// 先定会话号的那一家（`Bind::Preset`）一律照那一段。
#[test]
fn an_origin_bound_family_is_pinned_to_the_account_in_its_description() {
    let data = test_home::scratch("bind");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let mut d = desc(7, &[]);
    d.account = Some("second".into());
    leave(&own, "0123abcd", &d, &|_| true).expect("写得下");
    leave(&own, "4567cdef", &desc(7, &[]), &|_| true).expect("写得下");
    let origin_bind = crate::agents::fake::upstream::UPSTREAM.bind;
    assert_eq!(origin_bind, crate::agents::Bind::Origin);
    assert_eq!(
        bound_account(&data, origin_bind, "_", Some("0123abcd")),
        "second"
    );
    assert_eq!(
        bound_account(&data, origin_bind, "first", Some("4567cdef")),
        "first"
    );
    assert_eq!(
        bound_account(&data, origin_bind, "first", Some("89abcdef")),
        "first"
    );
    assert_eq!(bound_account(&data, origin_bind, "first", None), "first");
    assert_eq!(
        bound_account(
            &data,
            crate::agents::Bind::Preset,
            "first",
            Some("0123abcd")
        ),
        "first"
    );
    std::fs::remove_dir_all(&data).ok();
}

/// 这个进程起来时注入了中转：有一份描述记着它的 pid ⇒ 是；没有 ⇒ unix 上不是（ccm 就地 exec，pid 就是 agent 的），
/// 别处说不清（ccm 起子进程等它，描述记的是 ccm 的 pid）。
#[test]
fn a_description_carrying_the_pid_says_that_process_was_launched_through_the_relay() {
    let data = test_home::scratch("relayed");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let absent = if cfg!(unix) { Some(false) } else { None };
    assert_eq!(relayed(&data, 7), absent, "一份描述都没有");
    leave(&own, "0123abcd", &desc(7, &[]), &|_| true).expect("写得下");
    assert_eq!(relayed(&data, 7), Some(true));
    assert_eq!(relayed(&data, 8), absent, "别的进程");
    std::fs::remove_dir_all(&data).ok();
}
