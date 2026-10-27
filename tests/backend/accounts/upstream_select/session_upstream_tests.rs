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
        pid,
        at: 1,
    }
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
