use super::*;
use std::time::Duration;

fn plain(code: &str, message: &str) -> String {
    format!("{code}/{message}")
}

/// ★★ **本模块的核心性质**：只有「能证明没发出去」的三档才允许回落。
///
/// 反过来错法（把 `Remote{wrong_owner}` 也当成 `NoChannel`）会让一次**被门拒绝**
/// 转头走 SSH 再做一次 —— 那是把门拒绝洗成另一条路的成功。
#[test]
fn only_the_errors_that_prove_nothing_was_sent_allow_a_fallback() {
    let fallback_ok = [
        CallError::Unsupported {
            cmd: "kill".into(),
            offered: vec!["ping".into()],
        },
        CallError::TooManyPending,
    ];
    for e in &fallback_ok {
        assert!(
            matches!(route_call_error(e, "devbox", plain), Routed::NoChannel(_)),
            "{e:?} 是在写出去之前返回的，应当允许回落"
        );
    }
    let no_fallback = [
        CallError::Disconnected,
        CallError::Timeout {
            after: Duration::from_secs(1),
            withdraw: crate::chan::wire::Withdraw::Asked,
        },
        CallError::Cancelled,
        CallError::Remote {
            code: "wrong_owner".into(),
            message: "sid=".into(),
            detail: String::new(),
            data: None,
        },
        CallError::Remote {
            code: "too_many_windows".into(),
            message: "windows=3".into(),
            detail: String::new(),
            data: None,
        },
        CallError::Remote {
            code: "kill_failed".into(),
            message: "boom".into(),
            detail: String::new(),
            data: None,
        },
        CallError::Remote {
            code: "no_such_session".into(),
            message: "".into(),
            detail: String::new(),
            data: None,
        },
        CallError::Remote {
            code: "bad_args".into(),
            message: "未知 mode `attach-only`".into(),
            detail: String::new(),
            data: None,
        },
    ];
    for e in &no_fallback {
        assert!(
            matches!(route_call_error(e, "devbox", plain), Routed::Refused(_)),
            "{e:?} **不能证明**这条命令没发出去（或后端已经说了话）——\n\
                 允许回落就等于在未知状态上再做一次动作，\
                 而 `wrong_owner`/`too_many_windows` 更是把门拒绝洗成另一条路的成功"
        );
    }
    // 「没有通道」那一档也必须是可回落的。
    assert!(matches!(no_channel("h1"), Routed::NoChannel(_)));
}

/// ★★ **收拢表逐档穷举**〔面 A 通道那一拍，2026-09-24〕—— `route_call_error` 对
/// `inbound_client::CallError` 的**每一个变体**，产出的三态**连同那句话**逐字节钉死。
///
/// # 两侧为什么异源
///
/// 右侧（期望值）是**字面量**，抄自改动之前那份 `route_call_error` 在同一输入上的产出
/// —— 先在旧实现上跑绿、再动实现（「分层判定只许一份，三态从它收拢」那一拍）。
/// 它**不**调新的分层函数自证：拿新函数去算期望值，改错了两边一起错，恒真。
///
/// # 买到 / 买不到
///
/// **买到**：收拢前后同一输入 ⇒ 同一三态、同一句话（kill/launch/send_keys/cc_bus/tmux/find
/// 六个调用方看到的一个字节都没变）。
/// **买不到**：每个变体只喂了**一个**样本（字段值取一种）；字段值不同的输入由各调用方自己的判据管。
#[test]
fn the_collapse_to_three_states_is_byte_identical_to_the_table_before_layering() {
    // 穷尽见证：`inbound_client::CallError` 多一个变体，这里编译不过 —— 逼人回来补下表。
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote { .. } => {}
        }
    }
    // 〔CP1 裁「改·§2.4」〕说法换了（去掉 ** 与「另一条路」，不点命令名与声明的能力），三态一格没动。
    // 那句话按文案键断言、不抄原文：三态（NoChannel / Refused）与取的是哪一条仍逐格钉死。
    let t = |k: &str| copy_text(k, &[]);
    let unsure = |s: String| copy_text("rsBackendRoute.route.unsure", &[("s", &s)]);
    let table: Vec<(CallError, Routed)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec!["ping".into(), "cancel".into()],
            },
            Routed::NoChannel(copy_core::backend_old("devbox")),
        ),
        (
            CallError::TooManyPending,
            Routed::NoChannel(t("rsBackendRoute.layer.tooMany")),
        ),
        (
            CallError::Disconnected,
            Routed::Refused(unsure(t("rsInboundClient.error.closed"))),
        ),
        (
            CallError::Timeout {
                after: Duration::from_millis(1500),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            Routed::Refused(unsure(copy_text(
                "rsInboundClient.error.timeout",
                &[("dur", "1.5 秒")],
            ))),
        ),
        (
            CallError::Cancelled,
            Routed::Refused(unsure(t("rsInboundClient.error.cancelled"))),
        ),
        (
            CallError::Remote {
                code: "wrong_owner".into(),
                message: "sid=x".into(),
                detail: String::new(),
                data: None,
            },
            Routed::Refused("wrong_owner/sid=x".into()),
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
            },
            Routed::Refused(format!(
                "no_tmux/{}",
                copy_text("rsInboundClient.error.unavailable", &[])
            )),
        ),
    ];
    assert_eq!(table.len(), 7, "收拢表的行数与穷尽见证的变体数对不上");
    for (e, want) in table {
        witness(&e);
        assert_eq!(
            route_call_error(&e, "devbox", plain),
            want,
            "`{e:?}` 收拢出来的三态（或那句话）变了 —— 旧三态一个字节都不许变"
        );
    }
}

/// ★★ **分层表逐档穷举** —— `layer_call_error` 对 `inbound_client::CallError` 每一个变体的
/// 形状（层 · `reach` · `why` · 跳号标签 · 不透明 body）逐格钉死。
///
/// 期望值是**字面量**（与 `layer_call_error` 头注那张分层表逐行对应），不调任何映射函数去算。
/// 收拢那一侧由 `the_collapse_to_three_states_is_byte_identical_to_the_table_before_layering` 管；
/// 两张字面表各钉一层，改错哪一层红哪一张。
#[test]
fn the_layering_table_is_pinned_cell_by_cell() {
    use crate::chan::wire as w;
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote { .. } => {}
        }
    }
    let hop = |tag: &'static str, reach: w::Reach, why: w::HopFault| w::CallError::Hop {
        at: w::HopId { idx: 7, tag },
        reach,
        why,
    };
    let table: Vec<(CallError, w::CallError)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec![],
            },
            w::CallError::Peer {
                why: w::PeerFault::Unsupported,
            },
        ),
        (
            CallError::TooManyPending,
            hop("write", w::Reach::NotSent, w::HopFault::Overrun),
        ),
        (
            CallError::Disconnected,
            hop("read", w::Reach::Unknown, w::HopFault::Dropped),
        ),
        (
            CallError::Timeout {
                after: Duration::from_millis(5),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            hop("wait", w::Reach::Unknown, w::HopFault::Overrun),
        ),
        (CallError::Cancelled, w::OursFault::Cancelled.into()),
        (
            CallError::Remote {
                code: "wrong_owner".into(),
                message: "x\u{0}y".into(),
                detail: String::new(),
                data: None,
            },
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(br#"{"code":"wrong_owner","message":"x\u0000y"}"#.to_vec()),
                },
            },
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
            },
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(
                        format!(
                            r#"{{"code":"no_tmux","message":"{}"}}"#,
                            copy_core::copy_static!("rsInboundClient.error.unavailable")
                        )
                        .into_bytes(),
                    ),
                },
            },
        ),
    ];
    assert_eq!(table.len(), 7, "分层表的行数与穷尽见证的变体数对不上");
    for (e, want) in table {
        witness(&e);
        assert_eq!(
            layer_call_error(&e, 7).error,
            want,
            "`{e:?}` 的分层结果不是那张表那一格"
        );
    }
    assert_eq!(
        layer_no_channel(7),
        hop("open", w::Reach::NotSent, w::HopFault::Unreachable),
        "「没有控制通道」那一格不是 `Hop{{open, NotSent, Unreachable}}`"
    );
}

/// 带 `data` 的失败（按码定形，今天只有 `account_unavailable`）：拒绝体多一格 `data`；没带的那一形字节不变（上表那几行）。
/// 比的是解出来的 JSON（键序随 serde_json 的特性开关变，不是契约）。
#[test]
fn a_refusal_with_data_carries_it_in_the_body() {
    let e = CallError::Remote {
        code: "account_unavailable".into(),
        message: "m".into(),
        detail: String::new(),
        data: Some(r#"{"requested":"z","pinned":true,"listKnown":true,"alternative":null}"#.into()),
    };
    let w::CallError::Peer {
        why: w::PeerFault::Refused { body },
    } = layer_call_error(&e, 0).error
    else {
        panic!("带 data 的失败没落成对端拒绝");
    };
    let got: serde_json::Value = serde_json::from_slice(&body.0).expect("拒绝体是 JSON");
    assert_eq!(
        got,
        serde_json::json!({
            "code": "account_unavailable",
            "message": "m",
            "data": {"requested": "z", "pinned": true, "listKnown": true, "alternative": null},
        })
    );
}
