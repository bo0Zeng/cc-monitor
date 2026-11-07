//! monitor 这一侧的远端常驻后端（`remote_resident.rs`）。
//! 守的要求：「远端常驻、本机远端同形」；只升不降「部署只在『我的比盘上的新』时才换」。

use super::*;

fn hello(build: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"{build}","host_arch":"x86_64","claude_dir":"/h/.claude"}}"#
    )
}

fn exec(stdout: &str, stderr: &str, code: Option<u32>) -> cmd_wire::dial::Captured {
    cmd_wire::dial::Captured {
        stdout: stdout.into(),
        stderr: stderr.into(),
        ended: code.map_or(cmd_wire::dial::CaptureEnd::ClosedBare, |code| {
            cmd_wire::dial::CaptureEnd::Exited { code }
        }),
    }
}

/// 停那一趟（`--resident-stop`，本机那一趟同读）的失败与重连循环读 `--resident-ensure` 同一份读法
/// （`resident_client::feed::read_one_shot`，逐格见 `feed_tests.rs::the_ensure_answer_is_read_the_same_on_both_ends`）：
/// 那台脱离不了（`unsupported`，非 unix）⇒ **明说**「远端只支持 Unix」、那台原话 ＋ 详情进复制详情；127 ⇒ 没装；
/// 别的失败 ⇒ 那台那一句、它写的详情原样；老后端掉进流模式发 hello ⇒「太旧」。都是失败，没有「回落」那一格。
/// 要求：「非 unix 远端（Windows 远端，不承诺）断开常驻时**明说**不支持，不静默」。
#[test]
fn the_stop_failures_are_read_the_same_as_the_ensure_answer() {
    let envelope = |code: &str, said: &str, detail: &str| {
        serde_json::json!({"code": code, "said": said, "detail": detail}).to_string()
    };
    let s = read_stop(
        &exec(
            "",
            &envelope("unsupported", "不是 unix", "那台写的详情"),
            Some(2),
        ),
        "devbox",
    )
    .unwrap_err();
    assert_eq!(
        s.said,
        copy_text("rsRemoteResident.ensure.unsupported", &[])
    );
    let raw = s.raw.clone().unwrap_or_default();
    assert!(raw.contains("不是 unix"), "{raw}");
    assert!(raw.contains("那台写的详情"), "{raw}");
    let s = read_stop(
        &exec(
            "",
            &envelope("spawn_failed", "x", "Permission denied (os error 13)"),
            Some(2),
        ),
        "devbox",
    )
    .unwrap_err();
    assert_eq!(s.said, "x");
    assert!(
        s.raw
            .as_deref()
            .unwrap_or_default()
            .contains("Permission denied (os error 13)"),
        "{:?}",
        s.raw
    );
    assert_eq!(
        read_stop(&exec("", "", Some(127)), "devbox")
            .unwrap_err()
            .said,
        copy_text("remoteHealth.head.notDeployed", &[])
    );
    assert_eq!(
        read_stop(&exec(&hello("p1a-old"), "", None), "devbox")
            .unwrap_err()
            .said,
        copy_core::backend_old("devbox")
    );
}

/// 远端那条流的 attach 行：「我要流」＋ 时区 ＋ 名单；没有旗标、没有钥匙、没有出口声明（会话帧远端本机同一形）。
/// 本机远端同一处拼（`local_backend_host::attach_line`）。
#[test]
fn the_attach_line_asks_for_the_stream_and_nothing_else() {
    let watch = cmd_wire::watch::WatchAsk {
        sids: vec!["s1".into()],
        ..Default::default()
    };
    let l = crate::local_backend_host::attach_line(Some("Asia/Shanghai"), None, Some(watch));
    assert!(l.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(l.trim()).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"attach": true, "tz": "Asia/Shanghai", "watch": {"sids": ["s1"], "branch": []}})
    );
    let src =
        guard_core::production_code(include_str!("../../../src/comms/resident-client/feed.rs"));
    assert!(
        !src.contains("remote_stream_view"),
        "远端那一行又带上了出口声明"
    );
    guard_core::find_pinned(&src, "attach::handshake(")
        .unwrap_or_else(|e| panic!("远端那一行不经 attach 握手：{e}"));
}

/// T4 **停的结局只认三个词**（本机远端同一个读法）：`graceful` · `killed` · `not_running` 各落一格、pid 原样；
/// 认不出的词 / 老后端那一形（`{"stopped":<pid>}`）⇒ 错，不猜成「停了」；退出 2 ⇒ stderr 那句原样。
/// 要求：「回结局 `{stopped: "graceful" | "killed" | "not_running"}` …… monitor 发一次远端 exec、按结局出声」。
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
        assert_eq!(
            read_stop(&exec(line, "", Some(0)), "devbox"),
            Ok(want),
            "{line}"
        );
    }
    for line in [r#"{"stopped":42}"#, r#"{"stopped":"stopped"}"#, r#"{}"#] {
        assert!(
            read_stop(&exec(line, "", Some(0)), "devbox").is_err(),
            "{line}"
        );
    }
    let err = read_stop(
        &exec(
            "",
            r#"{"code":"stop_failed","said":"强杀之后还在","detail":""}"#,
            Some(2),
        ),
        "devbox",
    )
    .unwrap_err();
    assert_eq!(err.said, "强杀之后还在");
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

/// `machine-stop` 的回包照类型（`cmd_wire::machine::MachineStopped`）收，三个词之外的同样是错。
#[test]
fn the_machine_stop_reply_is_read_by_its_type() {
    use cmd_wire::machine::MachineStopped;
    let got = |stopped: &str, pid| {
        answer_of_stopped(&MachineStopped {
            stopped: stopped.into(),
            pid,
        })
    };
    assert_eq!(
        got("killed", Some(3)),
        Ok(StopAnswer {
            stopped: StopWord::Killed,
            pid: Some(3),
        })
    );
    assert_eq!(
        got("not_running", None),
        Ok(StopAnswer {
            stopped: StopWord::NotRunning,
            pid: None,
        })
    );
    assert!(got("stopped", None).is_err());
    assert!(got("", Some(1)).is_err());
}

/// **monitor 等那一趟的期限 > 那台停它的最长时间**（宽限期 ＋ 强杀后再等的那一段）：否则 monitor 先放弃、
/// 那台其实停成了，界面却说「超时」。跨半边：现抠后端 `control/resident.rs` 两个毫秒字面量（各恰好一处），与 [`STOP_BUDGET`] 比。
/// 要求：「远端控制方只发一次『停』并拿回结局，不远程轮询」。
#[test]
fn the_stop_budget_outlasts_the_remote_stop() {
    let be = guard_core::production_code(include_str!("../../../src/backend/control/resident.rs"));
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
    let ours = STOP_BUDGET.as_millis();
    assert!(
        worst < ours,
        "那台停一次最长 {worst}ms，monitor 只等 {ours}ms —— 会在结局出来之前放弃"
    );
}

// 中继 `absent` 时 ensure 之后只再接一次、没成照退避 · 「没带字节就不比新旧」随重连循环住 `resident-client`：
//   `feed_tests.rs::a_relay_still_absent_after_ensure_is_one_transient_failure` · `feed_tests.rs::without_its_own_build_the_relation_is_incomparable`。

/// 小中继的第一行（读法住 `resident-client`，两端同一份）：拒绝那一形读出理由，别的（hello）照 hello 读。理由词与后端那一侧逐字相同（跨 crate 字面量）。
#[test]
fn the_relay_refusal_line_is_read_and_its_words_match_the_backend() {
    use resident_client::attach::{relay_refusal, RELAY_ABSENT};
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"absent"}"#).as_deref(),
        Some(RELAY_ABSENT)
    );
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"unreachable"}"#).as_deref(),
        Some("unreachable")
    );
    assert_eq!(
        relay_refusal(r#"{"kind":"hello","v":1,"build_id":"p1"}"#),
        None
    );
    let be = include_str!("../../../src/backend/wire/listen.rs");
    assert!(
        be.contains(&format!(
            "pub const REFUSE_ABSENT: &str = \"{RELAY_ABSENT}\";"
        )),
        "后端那一侧「没人在听」的理由词与这里不同了"
    );
}
