//! 壳这一端写的复制详情：远端拒绝补「本机」、本机拒绝不补；通道断了壳写全份；壳命令失败那一形。

use super::*;
use crate::origin::Origin;

fn label(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn a_remote_refusal_gets_a_local_line_and_a_local_one_does_not() {
    let remote = format!("{}：kill_failed", label("detail.label.code"));
    let got = relayed(&Origin("devbox".into()), &remote);
    assert!(got.starts_with(&remote), "{got}");
    assert!(
        got.contains(&format!("\n{}：cc-monitor ", label("detail.label.local"))),
        "{got}"
    );
    assert_eq!(relayed(&Origin::local(), &remote), remote);
}

#[test]
fn the_refusal_body_detail_is_read_and_missing_reads_empty() {
    assert_eq!(
        of_refusal_body(br#"{"code":"x","message":"m","detail":"d"}"#),
        "d"
    );
    assert_eq!(of_refusal_body(br#"{"code":"x","message":"m"}"#), "");
    assert_eq!(of_refusal_body(b"not json"), "");
}

#[test]
fn a_channel_failure_detail_names_the_machine_command_hop_and_code() {
    let e = w::CallError::Hop {
        at: w::HopId {
            idx: 1,
            tag: "open",
        },
        reach: w::Reach::NotSent,
        why: w::HopFault::Unreachable,
    };
    let d = of_channel(&Origin("devbox".into()), "accounts-list", &e);
    for want in [
        format!(
            "{}：devbox（{}）",
            label("detail.label.machine"),
            label("detail.value.notConnected")
        ),
        format!("{}：accounts-list", label("detail.label.command")),
        format!("{}：1:open NotSent", label("detail.label.hop")),
        format!("{}：Unreachable", label("detail.label.code")),
        format!("{}：cc-monitor ", label("detail.label.local")),
    ] {
        assert!(d.contains(&want), "缺「{want}」：\n{d}");
    }
    let local = of_channel(&Origin::local(), "kill", &w::OursFault::Broken.into());
    assert!(
        !local.contains(&format!("{}：cc-monitor", label("detail.label.local"))),
        "{local}"
    );
    assert!(
        local.contains(&format!("{}：Broken", label("detail.label.code"))),
        "{local}"
    );
}

#[test]
fn a_shell_command_failure_is_one_shape_with_a_nonempty_detail() {
    let s = Said::new("读取配置失败", "load_config", Some("os error 2"));
    assert_eq!(s.said, "读取配置失败");
    assert!(s
        .detail
        .contains(&format!("{}：load_config", label("detail.label.command"))));
    assert!(s
        .detail
        .contains(&format!("{}：os error 2", label("detail.label.raw"))));
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2, "{v}");
}
