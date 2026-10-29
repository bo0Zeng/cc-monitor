//! 上游描述落盘与读回（出网那两格）：落下的读得回 · 读回的照那个代理走 · 来处不过段闸不写 · 写的那一下清掉进程已不在的那几份。

use super::*;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-upstream-desc-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("sandbox");
    d
}

fn desc(pid: u32, vars: &[(&str, &str)]) -> UpstreamDesc {
    UpstreamDesc {
        egress: vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        base: None,
        pid,
        at: 1,
    }
}

/// 用户自己的上游地址那一格落盘读回一致。
#[test]
fn the_users_own_upstream_reads_back() {
    let data = sandbox("base");
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
    let data = sandbox("back");
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
    let data = sandbox("gate");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let d = desc(7, &[("HTTPS_PROXY", "http://127.0.0.1:3128")]);
    assert!(leave(&own, "../x", &d, &|_| true).is_err());
    assert_eq!(of(&data, "../x"), None);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn leaving_one_clears_those_whose_process_is_gone() {
    let data = sandbox("prune");
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
    let data = sandbox("own-of");
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
    let data = sandbox("ver");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    leave(&own, "0123abcd", &desc(7, &[]), &|_| true).expect("写得下");
    let dir = data.join(NOTES_DIR).join(SUB);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
    assert_eq!(v["version"], serde_json::json!(1), "没带版本：{v}");
    assert_eq!(
        of(&data, "0123abcd").map(|d| d.pid),
        Some(7),
        "带版本的那一份读不回"
    );
    std::fs::remove_dir_all(&data).ok();
}
