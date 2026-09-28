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

/// 〔DEL〕D2 `--resident-ensure` 的答：成了 ⇒ 端口；那台脱离不了（`unsupported`，非 unix）⇒ **明说**「远端只支持 Unix」、
/// 带上那台原话；别的失败 ⇒ 那台原话原样；老后端掉进流模式发 hello ⇒ 「太旧」。四样都是失败，没有「回落」那一格。
/// 守的要求：`4d-lanes.md` `## DEL` 逐字「非 unix 远端（Windows 远端，V29 / V132 不承诺）连不上常驻时**明说**不支持，不静默」。
#[test]
fn the_ensure_answer_names_a_non_unix_remote_as_unsupported_and_never_falls_back() {
    let ok = parse_answer(&exec(r#"{"port":51000,"token":"ab","pid":7}"#, "", Some(0))).unwrap();
    assert_eq!(parse_ensured(&ok).unwrap().port, 51000);
    assert_eq!(
        parse_answer(&exec(
            "",
            r#"{"code":"unsupported","message":"不是 unix"}"#,
            Some(2)
        )),
        Err(copy_text(
            "rsRemoteResident.ensure.unsupported",
            &[("why", "不是 unix")]
        ))
    );
    assert_eq!(
        parse_answer(&exec(
            "",
            r#"{"code":"spawn_failed","message":"x"}"#,
            Some(2)
        )),
        Err("x".to_string())
    );
    assert_eq!(
        parse_answer(&exec(&hello("p1a-old"), "", None)),
        Err(copy_text("rsRemoteResident.ensure.tooOld", &[]))
    );
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

/// 〔STOP〕T4 **停的结局只认三个词**（本机远端同一个读法）：`graceful` · `killed` · `not_running` 各落一格、pid 原样；
/// 认不出的词 / 老后端那一形（`{"stopped":<pid>}`）⇒ 错，不猜成「停了」；退出 2 ⇒ stderr 那句原样。
/// 守的要求：`4d-lanes.md` `### STOP`（主会话裁）逐字「回结局 `{stopped: "graceful" | "killed" | "not_running"}` …… monitor 发一次远端 exec、按结局出声」。
#[test]
fn the_stop_answer_is_one_of_three_words_or_an_error() {
    for (line, want) in [
        (
            r#"{"stopped":"graceful","pid":42}"#,
            StopAnswer {
                stopped: StopWord::Graceful,
                pid: Some(42),
            },
        ),
        (
            r#"{"stopped":"killed","pid":7}"#,
            StopAnswer {
                stopped: StopWord::Killed,
                pid: Some(7),
            },
        ),
        (
            r#"{"stopped":"not_running","pid":null}"#,
            StopAnswer {
                stopped: StopWord::NotRunning,
                pid: None,
            },
        ),
    ] {
        assert_eq!(read_stop(&exec(line, "", Some(0))), Ok(want), "{line}");
    }
    for line in [r#"{"stopped":42}"#, r#"{"stopped":"stopped"}"#, r#"{}"#] {
        assert!(read_stop(&exec(line, "", Some(0))).is_err(), "{line}");
    }
    let err = read_stop(&exec(
        "",
        r#"{"code":"stop_failed","message":"强杀之后还在"}"#,
        Some(2),
    ))
    .unwrap_err();
    assert_eq!(err, "强杀之后还在");
    // 交给机器页的线上形：三个词原样（snake_case），不是 Rust 的变体名。
    let wire: Vec<String> = [StopWord::Graceful, StopWord::Killed, StopWord::NotRunning]
        .iter()
        .map(|w| {
            serde_json::to_value(w)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(wire, ["graceful", "killed", "not_running"]);
}

/// 〔STOP〕**monitor 等那一趟的期限 > 那台停它的最长时间**（宽限期 ＋ 强杀后再等的那一段）：否则 monitor 先放弃、
/// 那台其实停成了，界面却说「超时」。跨半边：现抠后端 `control/resident.rs` 两个毫秒字面量（各恰好一处），与 `dial_host::ONE_SHOT_DEADLINE` 比。
/// 守的要求：`4d-lanes.md` `### STOP` 逐字「远端控制方只发一次『停』并拿回结局，不远程轮询」。
#[test]
fn the_one_shot_deadline_outlasts_the_remote_stop() {
    let be = guard_core::production_code(include_str!("../../src/backend/control/resident.rs"));
    let lit = |name: &str| -> u128 {
        let anchor = format!("pub(crate) const {name}: u32 = ");
        assert_eq!(
            be.matches(&anchor).count(),
            1,
            "`{name}` 的声明不是恰好一处"
        );
        let tail = &be[be.find(&anchor).expect("声明") + anchor.len()..];
        tail.chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .filter(|c| *c != '_')
            .collect::<String>()
            .parse()
            .expect("毫秒字面量")
    };
    let worst = lit("STOP_GRACE_MS") + lit("KILL_WAIT_MS");
    let ours = crate::dial_host::ONE_SHOT_DEADLINE.as_millis();
    assert!(
        worst < ours,
        "那台停一次最长 {worst}ms，monitor 只等 {ours}ms —— 会在结局出来之前放弃"
    );
}
