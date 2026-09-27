//! 〔HOST · V139〕monitor 这一侧的远端常驻后端（`remote_resident.rs`）。
//! 守的要求：`99 §1` V139「远端常驻、本机远端同形」；只升不降 `设计/15 §4.8`「部署只在『我的比盘上的新』时才换」。

use super::*;

fn hello(build: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"{build}","host_arch":"x86_64","claude_dir":"/h/.claude"}}"#
    )
}

/// H6：比手上这一版旧 ⇒ 换一次；换过之后仍旧 ⇒ 照接（不来回）；同版 / 比我新 / 不可比 ⇒ 照接；不是 hello ⇒ 不是我们的。
#[test]
fn the_hello_decides_replace_only_upward_and_only_once() {
    assert_eq!(
        hello_decision(&hello("p4a-x"), "p4j-y", false),
        HelloDecision::Replace
    );
    assert_eq!(
        hello_decision(&hello("p4a-x"), "p4j-y", true),
        HelloDecision::Attach,
        "换过一次还旧就再换 —— 两个 monitor 会来回互杀"
    );
    assert_eq!(
        hello_decision(&hello("p4j-y"), "p4j-y", false),
        HelloDecision::Attach
    );
    assert_eq!(
        hello_decision(&hello("p5a-z"), "p4j-y", false),
        HelloDecision::Attach,
        "比我新的被换掉了 —— 部署只升不降"
    );
    assert_eq!(
        hello_decision(&hello("dev"), "p4j-y", false),
        HelloDecision::Attach
    );
    assert!(matches!(
        hello_decision(r#"{"attach":"refused"}"#, "p4j-y", false),
        HelloDecision::NotOurs(_)
    ));
}

fn exec(stdout: &str, stderr: &str, code: Option<u32>) -> crate::ssh_source::RemoteExec {
    crate::ssh_source::RemoteExec {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_status: code,
    }
}

/// `--resident-ensure` 的三种答：成了 · 起不了常驻（回落流模式）· 别的失败；老后端掉进流模式发 hello ⇒ 当「起不了」。
#[test]
fn the_ensure_answer_separates_unsupported_from_failed() {
    let ok = parse_answer(&exec(r#"{"port":51000,"token":"ab","pid":7}"#, "", Some(0))).unwrap();
    assert_eq!(parse_ensured(&ok).unwrap().port, 51000);
    assert!(matches!(
        parse_answer(&exec(
            "",
            r#"{"code":"unsupported","message":"不是 unix"}"#,
            Some(2)
        )),
        Err(AttachErr::Unsupported(_))
    ));
    assert!(matches!(
        parse_answer(&exec(
            "",
            r#"{"code":"spawn_failed","message":"x"}"#,
            Some(2)
        )),
        Err(AttachErr::Failed(_))
    ));
    assert!(matches!(
        parse_answer(&exec(&hello("p1a-old"), "", None)),
        Err(AttachErr::Unsupported(_))
    ));
    assert!(parse_ensured(&serde_json::json!({"port":51000})).is_err());
    assert!(
        !format!("{:?}", parse_ensured(&ok).unwrap()).contains("ab"),
        "钥匙进了 Debug 输出"
    );
}

/// attach 行是远端 `listen::attach_flags` 读得懂的形状：钥匙 ＋ 这条连接的旗标。
#[test]
fn the_attach_line_carries_the_token_and_exactly_the_negotiated_flags() {
    let l = attach_line("t0k", (false, true, true));
    assert!(l.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(l.trim()).unwrap();
    assert_eq!(v["attach"], "t0k");
    assert_eq!(
        v["flags"],
        serde_json::json!(["--tail-only", "--with-rbind-token"])
    );
}
