use super::*;
use std::time::Duration;

fn plain(code: &str, message: &str) -> String {
    format!("{code}/{message}")
}

/// ★★ **本模块的核心性质**：只有「能证明没发出去」的那几档判成没发出去（[`provably_not_sent`]，界面那一份照同一条线）。
///
/// 反过来错法（把 `Remote{wrong_owner}` 也当成没发出去）会让一次**被门拒绝**被说成「没发出去、可以再来一次」。
#[test]
fn only_the_errors_that_prove_nothing_was_sent_count_as_not_sent() {
    let not_sent = |e: &CallError| provably_not_sent(&layer_call_error(e, 0).error);
    let proven = [
        CallError::Unsupported {
            cmd: "kill".into(),
            offered: vec!["ping".into()],
        },
        CallError::TooManyPending,
    ];
    for e in &proven {
        assert!(not_sent(e), "{e:?} 是在写出去之前返回的，应当判成没发出去");
    }
    let unsure = [
        CallError::Disconnected,
        CallError::Timeout {
            after: Duration::from_secs(1),
            withdraw: crate::chan::wire::Withdraw::Asked,
        },
        CallError::Cancelled,
        CallError::Remote(cmd_wire::failed::Failed {
            code: "wrong_owner".into(),
            said: "sid=".into(),
            detail: String::new(),
            data: None,
        }),
        CallError::Remote(cmd_wire::failed::Failed {
            code: "too_many_windows".into(),
            said: "windows=3".into(),
            detail: String::new(),
            data: None,
        }),
        CallError::Remote(cmd_wire::failed::Failed {
            code: "kill_failed".into(),
            said: "boom".into(),
            detail: String::new(),
            data: None,
        }),
        CallError::Remote(cmd_wire::failed::Failed {
            code: "no_such_session".into(),
            said: "".into(),
            detail: String::new(),
            data: None,
        }),
        CallError::Remote(cmd_wire::failed::Failed {
            code: "bad_args".into(),
            said: "未知 mode `attach-only`".into(),
            detail: String::new(),
            data: None,
        }),
    ];
    for e in &unsure {
        assert!(
            !not_sent(e),
            "{e:?} **不能证明**这条命令没发出去（或后端已经说了话）——\n\
                 判成没发出去就等于说在未知状态上可以再做一次动作，\
                 而 `wrong_owner`/`too_many_windows` 更是把门拒绝说成没发"
        );
    }
    // 「没有通道」那一档也是没发出去，那一句是那台离线那一句。
    assert!(provably_not_sent(&layer_no_channel(0)));
    assert_eq!(no_channel("h1"), copy_core::offline("h1"));
}

/// ★★ **那一句逐档穷举**—— `route_call_error` 对 `inbound_client::CallError` 的**每一个变体**说的那一句逐字节钉死。
///
/// 右侧（期望值）按文案键取、不调分层函数自证：拿新函数去算期望值，改错了两边一起错，恒真。
/// 每个变体只喂了**一个**样本（字段值取一种）；字段值不同的输入由各调用方自己的判据管。
#[test]
fn the_sentence_of_every_variant_is_pinned() {
    // 穷尽见证：`inbound_client::CallError` 多一个变体，这里编译不过 —— 逼人回来补下表。
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::Differs { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote(_) => {}
        }
    }
    let t = |k: &str| copy_text(k, &[]);
    let unsure = |s: String| copy_text("rsBackendRoute.route.unsure", &[("s", &s)]);
    let table: Vec<(CallError, String)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec!["ping".into(), "cancel".into()],
            },
            copy_core::backend_old("devbox"),
        ),
        (CallError::TooManyPending, t("rsBackendRoute.layer.tooMany")),
        (
            CallError::Disconnected,
            unsure(t("rsInboundClient.error.closed")),
        ),
        (
            CallError::Timeout {
                after: Duration::from_millis(1500),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            unsure(copy_text(
                "rsInboundClient.error.timeout",
                &[("dur", "1.5 秒")],
            )),
        ),
        (CallError::Cancelled, unsure(t("reason.op.cancelled"))),
        (
            CallError::Remote(cmd_wire::failed::Failed {
                code: "wrong_owner".into(),
                said: "sid=x".into(),
                detail: String::new(),
                data: None,
            }),
            "wrong_owner/sid=x".into(),
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
                said: "devbox 无 tmux".into(),
            },
            "no_tmux/devbox 无 tmux".into(),
        ),
    ];
    assert_eq!(table.len(), 7, "表的行数与穷尽见证的变体数对不上");
    for (e, want) in table {
        witness(&e);
        assert_eq!(
            route_call_error(&e, "devbox", plain),
            want,
            "`{e:?}` 说的那一句变了"
        );
    }
}

/// ★★ **分层表逐档穷举** —— `layer_call_error` 对 `inbound_client::CallError` 每一个变体的
/// 形状（层 · `reach` · `why` · 跳号标签 · 不透明 body）逐格钉死。
///
/// 期望值是**字面量**（与 `layer_call_error` 头注那张分层表逐行对应），不调任何映射函数去算。
/// 那一句由 `the_sentence_of_every_variant_is_pinned` 管；
/// 两张字面表各钉一层，改错哪一层红哪一张。
#[test]
fn the_layering_table_is_pinned_cell_by_cell() {
    use crate::chan::wire as w;
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::Differs { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote(_) => {}
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
            hop("write", w::Reach::NotSent, w::HopFault::Full),
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
            CallError::Remote(cmd_wire::failed::Failed {
                code: "wrong_owner".into(),
                said: "x\u{0}y".into(),
                detail: String::new(),
                data: None,
            }),
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(
                        br#"{"code":"wrong_owner","said":"x\u0000y","detail":""}"#.to_vec(),
                    ),
                },
            },
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
                said: "devbox 无 tmux".into(),
            },
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(
                        r#"{"code":"no_tmux","said":"devbox 无 tmux","detail":""}"#
                            .as_bytes()
                            .to_vec(),
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
    let e = CallError::Remote(cmd_wire::failed::Failed {
        code: "account_unavailable".into(),
        said: "m".into(),
        detail: String::new(),
        data: Some(
            serde_json::from_str(
                r#"{"requested":"z","pinned":true,"listKnown":true,"alternative":null}"#,
            )
            .unwrap(),
        ),
    });
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
            "said": "m",
            "detail": "",
            "data": {"requested": "z", "pinned": true, "listKnown": true, "alternative": null},
        })
    );
}

/// 每种 `CallError` 的码只取 `cmd_wire` 那几个常量（桌面 · 手机 · 常驻转发同读这一份，不各写一张翻码表）；
/// 那台交来码的两种（握手时说做不到 · 事后答失败）原样交那台的码。
#[test]
fn every_call_error_code_is_a_cmd_wire_constant() {
    let ours = [
        cmd_wire::UNKNOWN_COMMAND,
        cmd_wire::SHAPE_DIFFERS,
        cmd_wire::TOO_MANY_PENDING,
        cmd_wire::DISCONNECTED,
        cmd_wire::TIMEOUT,
        cmd_wire::CANCELLED,
    ];
    let table: Vec<(CallError, &str)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec![],
            },
            cmd_wire::UNKNOWN_COMMAND,
        ),
        (
            CallError::Differs {
                cmd: "kill".into(),
                said: "s".into(),
            },
            cmd_wire::SHAPE_DIFFERS,
        ),
        (CallError::TooManyPending, cmd_wire::TOO_MANY_PENDING),
        (CallError::Disconnected, cmd_wire::DISCONNECTED),
        (
            CallError::Timeout {
                after: Duration::from_secs(1),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            cmd_wire::TIMEOUT,
        ),
        (CallError::Cancelled, cmd_wire::CANCELLED),
    ];
    for (e, want) in &table {
        assert_eq!(e.code(), *want, "`{e:?}` 的码不是那个常量");
        assert!(ours.contains(&e.code()), "`{e:?}` 的码不在 cmd_wire 常量里");
    }
    let mut distinct = ours.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), ours.len(), "两种失败共用了一个码");
    let unavailable = CallError::Unavailable {
        cmd: "kill".into(),
        code: "no_tmux".into(),
        said: "s".into(),
    };
    assert_eq!(unavailable.code(), "no_tmux");
    let remote = CallError::Remote(cmd_wire::failed::Failed {
        code: "wrong_owner".into(),
        ..Default::default()
    });
    assert_eq!(remote.code(), "wrong_owner");
}

/// 本侧在等的满了（`TooManyPending`）经通道说出来的是它自己那一句（「待处理命令已达上限」），
/// 不借「已超时」那一格：那一问没出本侧、期限也没到。
#[test]
fn a_full_pending_table_is_not_said_as_a_timeout() {
    let e = layer_call_error(&CallError::TooManyPending, 1).error;
    for origin in [
        crate::origin::Origin::local(),
        crate::origin::Origin::named("devbox"),
    ] {
        let said = crate::chan::said::said(&e, &origin);
        assert_eq!(
            said,
            copy_text("rsBackendRoute.layer.tooMany", &[]),
            "@ {origin}"
        );
        assert_ne!(said, copy_text("rsChanSaid.hop.expired", &[]), "@ {origin}");
    }
}
