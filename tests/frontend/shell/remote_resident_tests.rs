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
            r#"{"stopped":"graceful","pid":42,"said":"那台·收尾退了"}"#,
            StopAnswer {
                stopped: StopWord::Graceful,
                pid: Some(42),
                said: "那台·收尾退了".into(),
            },
        ),
        (
            r#"{"stopped":"killed","pid":7,"said":"那台·强杀了"}"#,
            StopAnswer {
                stopped: StopWord::Killed,
                pid: Some(7),
                said: "那台·强杀了".into(),
            },
        ),
        (
            r#"{"stopped":"not_running","pid":null,"said":"那台·没在跑"}"#,
            StopAnswer {
                stopped: StopWord::NotRunning,
                pid: None,
                said: "那台·没在跑".into(),
            },
        ),
    ] {
        assert_eq!(
            read_stop(&exec(line, "", Some(0)), "devbox"),
            Ok(want),
            "{line}"
        );
    }
    // 那台没写那一句（老后端那一形）⇒ 错，壳不替它补字。
    for line in [
        r#"{"stopped":42}"#,
        r#"{"stopped":"stopped"}"#,
        r#"{}"#,
        r#"{"stopped":"graceful","pid":42}"#,
    ] {
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

/// `--resident-stop` 那一行经 [`stop_answer_of`] 读进来、再交给机器页：那台写好的那一句原样过去，壳不改字、不补字。
#[test]
fn the_remote_sentence_passes_through_to_the_machine_page() {
    let line = serde_json::json!({"stopped": "killed", "pid": 7, "said": "那台·强杀了"});
    let v = serde_json::to_value(stop_answer_of(&line, "devbox").unwrap()).unwrap();
    assert_eq!(v, line);
}

/// 那台的结局里没有那一句（那台后端版本过旧，还不写它）：不替那台补字；那一句点明是那台后端要更新，「停」本身可能已经成了。
#[test]
fn a_stop_line_without_its_sentence_says_the_remote_is_too_old() {
    let err = stop_answer_of(
        &serde_json::json!({"stopped": "graceful", "pid": 42}),
        "devbox",
    )
    .unwrap_err();
    assert_eq!(
        err.said,
        copy_core::backend_old("devbox"),
        "照版本族那一句说那台后端要更新"
    );
    assert!(
        err.raw.as_deref().is_some_and(|r| r.contains("graceful")),
        "原话进复制详情"
    );
}

/// 壳自己那一侧的两种结局（没问到哪台的 `--resident-stop`）各说各的那一句：亲手强杀带 pid · 口上没人在听。
#[test]
fn the_shells_own_stop_outcomes_say_their_own_sentence() {
    assert_eq!(
        StopAnswer::killed_here(Some(9)),
        StopAnswer {
            stopped: StopWord::Killed,
            pid: Some(9),
            said: copy_text("backend.stopSaid.killed", &[("pid", "9")]),
        }
    );
    assert_eq!(
        StopAnswer::not_running_here(),
        StopAnswer {
            stopped: StopWord::NotRunning,
            pid: None,
            said: copy_text("backend.stopSaid.notRunning", &[]),
        }
    );
}

/// `machine-stop` 的回包照类型（`cmd_wire::machine::MachineStopped`）收，三个词之外的 · 没那一句的同样是错。
#[test]
fn the_machine_stop_reply_is_read_by_its_type() {
    use cmd_wire::machine::MachineStopped;
    let got = |stopped: &str, pid, said: &str| {
        answer_of_stopped(
            &MachineStopped {
                stopped: stopped.into(),
                pid,
                said: said.into(),
            },
            "devbox",
        )
    };
    assert_eq!(
        got("killed", Some(3), "那台·强杀了"),
        Ok(StopAnswer {
            stopped: StopWord::Killed,
            pid: Some(3),
            said: "那台·强杀了".into(),
        })
    );
    assert_eq!(
        got("not_running", None, "那台·没在跑"),
        Ok(StopAnswer {
            stopped: StopWord::NotRunning,
            pid: None,
            said: "那台·没在跑".into(),
        })
    );
    assert!(got("stopped", None, "x").is_err());
    assert!(got("", Some(1), "x").is_err());
    assert!(got("killed", Some(1), "").is_err(), "那台没写那一句");
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

/// 小中继的第一行（读法住 `resident-client`，两端同一份）：拒绝那一形读出理由（理由词住契约 `cmd_wire::attach`，后端写的也是它），别的（hello）照 hello 读。
#[test]
fn the_relay_refusal_line_is_read() {
    use cmd_wire::attach::{REFUSE_ABSENT, REFUSE_UNREACHABLE};
    use resident_client::attach::relay_refusal;
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"absent"}"#).as_deref(),
        Some(REFUSE_ABSENT)
    );
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"unreachable"}"#).as_deref(),
        Some(REFUSE_UNREACHABLE)
    );
    assert_eq!(
        relay_refusal(r#"{"kind":"hello","v":1,"build_id":"p1"}"#),
        None
    );
}
