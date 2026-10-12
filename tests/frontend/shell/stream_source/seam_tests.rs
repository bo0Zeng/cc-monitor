use super::*;
use crate::copy_table::copy_text;

fn hello_line(commands: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":{commands}}}"#
    )
}

/// ★★ **有不可恢复的丢失时，不许再对用户说「重开会话可看完整历史」**〔audit-0805 F21，E4〕。
///
/// 那句话对**内容帧**是真的（行还在远端 jsonl 里），对**状态增量帧**是**假的** ——
/// 它是一次差分的结果、别处不存在。这句 message 是用户唯一看得见的东西，
/// 所以它对不对本身就是正题。
#[test]
fn the_overflow_message_stops_lying_when_state_was_lost() {
    let lost = vec![
        super::LostFrameInfo {
            kind: "session_removed".into(),
            subject: Some("sid-a".into()),
        },
        super::LostFrameInfo {
            kind: "session_added".into(),
            subject: Some("sid-b".into()),
        },
    ];
    let m = super::overflow_health_message("box1", 7, &lost, false);
    assert!(
        !copy_core::copy_matches("rsSshSource.health.overflowLines", &m),
        "丢了状态增量帧还说「重开会话可看完整历史」—— 那是假话。实得：{m}"
    );
    assert!(
        m.contains("sid-a") && m.contains("sid-b"),
        "要点名受影响的会话：{m}"
    );
    assert!(
        copy_core::copy_matches("rsSshSource.health.overflowLost", &m),
        "要说清这部分补不回来：{m}"
    );
    assert!(!m.contains("清单**不全**"), "没截断就别说截断：{m}");
}

/// 只丢内容帧时**沿用老说法那一句**（按文案键）—— 旧后端（`p1x` 之前）不发 `lost`，
/// 也落这一档，说的必须是同一句。
#[test]
fn the_overflow_message_is_byte_identical_when_only_lines_were_lost() {
    let m = super::overflow_health_message("box1", 3, &[], false);
    assert_eq!(
        m,
        copy_text(
            "rsSshSource.health.overflowLines",
            &[("host", "box1"), ("dropped", "3")]
        )
    );
}

/// 身份表被截断时要**再说一句**（暗示理性做法是整体重取）。
#[test]
fn the_overflow_message_says_so_when_the_identity_list_was_truncated() {
    let lost = vec![super::LostFrameInfo {
        kind: "session_removed".into(),
        subject: Some("sid-a".into()),
    }];
    let m = super::overflow_health_message("box1", 99, &lost, true);
    assert!(
        m.contains(copy_core::copy_static!(
            "rsSshSource.health.overflowTruncated"
        )),
        "截断了就要说出来：{m}"
    );
}

/// ★ **旧后端的 overflow 帧必须照旧能解析**〔additive 的真正代价在这里〕。
///
/// 把 `lost` 当必需字段会让整帧变成坏帧、**连 `dropped` 都丢掉** —— 比不认识新字段更糟。
#[test]
fn overflow_from_an_old_backend_still_parses() {
    match parse_frame(r#"{"kind":"overflow","dropped":5}"#) {
        Ok(InboundFrame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => {
            assert_eq!(dropped, 5);
            assert!(lost.is_empty(), "旧后端不发 lost ⇒ 空集");
            assert!(!lost_truncated);
        }
        other => panic!("旧后端的 overflow 解析不出来了：{other:?}"),
    }
}

/// 新后端的 `lost` / `lost_truncated` 要真的被读进来。
#[test]
fn overflow_identity_fields_are_actually_parsed() {
    let json = r#"{"kind":"overflow","dropped":2,"lost":[{"kind":"session_removed","subject":"sid-x"},{"kind":"tmux_sessions"}],"lost_truncated":true}"#;
    match parse_frame(json) {
        Ok(InboundFrame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => {
            assert_eq!(dropped, 2);
            assert!(lost_truncated);
            assert_eq!(lost.len(), 2, "两条身份都要收进来");
            assert_eq!(lost[0].kind, "session_removed");
            assert_eq!(lost[0].subject.as_deref(), Some("sid-x"));
            assert_eq!(lost[1].subject, None, "没有 subject 的那条也要留住 kind");
        }
        other => panic!("带身份的 overflow 解析失败：{other:?}"),
    }
}

// hello 解冻写半边并登记 · 应答路由回等待者 · 「重新连接」只叫醒那一台：随重连循环住 `resident-client`
//   （`feed.rs`：握手换出客户端、应答经 `route` 交回；`feed_tests.rs` 的问答与叫醒那几条）；登记在桌面收事那一头
//   （`stream_source/run.rs::on_attached`），`reconnect_cells_tests.rs` 走真的那一串。

// 控制通道往返探测那两条（真发 ping 并等应答 · 对旧后端一个字节不发）随测试连接搬进后端：
//   `tests/backend/dial_probe_tests.rs`（`a_hello_that_answers_ping_is_all_green` · `an_old_backend_without_commands_says_too_old`）。
