//! 通道 · webview 那一侧（`chan/webview.rs`）的判据 —— 合成句柄 ＋ 真 `router::settle`。
//!
//! | 性质 | 判据 |
//! |---|---|
//! | 载荷原样（含非 UTF-8 字节），`origin` / `op` 原样到句柄 | [`payload_origin_and_op_reach_the_backend_verbatim`] |
//! | 第 1 跳超时 ⇒ `Hop{1 wait, Unknown, Overrun}` ＋ 句柄的撤单手柄被拨下 | [`a_backend_that_never_answers_is_bounded_by_left`] |
//! | 句柄的错误原样过线（不在这一跳改层） | [`backend_errors_pass_through_unchanged`] |
//! | 交回 webview 的失败形状 == 金标准（TS 那侧解的是同一份文件） | [`the_fail_shape_equals_the_golden_file_the_ts_side_decodes`] |
//! | 〔C4e〕「能不能回落」的收拢 == 金标准（TS 那侧 `provablyNotSent` 判的是同一份文件） | [`the_fallback_rule_equals_the_golden_file_the_ts_side_judges`] |
//!
//! 买不到：真 Tauri IPC 那一跳（`#[tauri::command]` 的实参反序列化、`Response` 的字节交付）——
//! 那一跳要一个活的 webview，红线内起不来；这里证的是它之后的全部。

use super::super::router::Backends;
use super::super::wire::{
    Body, CallError, CancelToken, Cursor, HopFault, HopId, Item, Kind, Op, Origin, OursFault,
    PeerFault, Reach,
};
use super::{call_via, fail};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use std::sync::Mutex;
use std::time::Duration;

/// 句柄看见的一次 `call`：`(origin, op, payload, cancel)`。
type Seen = (String, String, Vec<u8>, CancelToken);

/// 合成句柄：按 `op` 演三种对端 —— 回声 · 永不回答 · 说「不行」。
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<Seen>>,
}

impl Backends for Fake {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        _left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        self.seen.lock().unwrap().push((
            origin.as_wire_str().to_string(),
            op.0.clone(),
            payload.0.clone(),
            cancel,
        ));
        match op.0.as_str() {
            "echo" => Box::pin(async move { Ok(payload) }),
            "hang" => Box::pin(futures::future::pending()),
            "refuse" => Box::pin(async {
                Err(CallError::Peer {
                    why: PeerFault::Refused {
                        body: Body(
                            br#"{"code":"no","message":"\xe4\xb8\x8d\xe8\xa1\x8c"}"#.to_vec(),
                        ),
                    },
                })
            }),
            _ => Box::pin(async {
                Err(CallError::Hop {
                    at: HopId {
                        idx: 1,
                        tag: "open",
                    },
                    reach: Reach::NotSent,
                    why: HopFault::Unreachable,
                })
            }),
        }
    }

    fn subscribe(&self, _o: Origin, _k: Kind, _f: Option<Cursor>) -> BoxStream<'static, Item> {
        Box::pin(futures::stream::empty())
    }
}

#[tokio::test]
async fn payload_origin_and_op_reach_the_backend_verbatim() {
    let fake = Fake::default();
    // 含非 UTF-8 字节与 NUL：载荷在这一跳不许被当成文本。
    let payload = vec![0u8, 0xff, 0xfe, b'{', b'}', 0x80];
    let got = call_via(
        &fake,
        Origin("devbox".into()),
        "echo".into(),
        payload.clone(),
        Duration::from_secs(5),
    )
    .await
    .expect("回声句柄不该失败");
    assert_eq!(got.0, payload, "载荷在 webview 那一跳被改了字节");
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "devbox", "origin 没原样到句柄");
    assert_eq!(seen[0].1, "echo", "op 没原样到句柄");
    assert_eq!(seen[0].2, payload, "句柄看见的载荷与发出的不同");
}

#[tokio::test]
async fn a_backend_that_never_answers_is_bounded_by_left() {
    let fake = Fake::default();
    let started = std::time::Instant::now();
    let err = call_via(
        &fake,
        Origin("<local>".into()),
        "hang".into(),
        Vec::new(),
        Duration::from_millis(80),
    )
    .await
    .expect_err("永不回答的句柄必须在 left 内被截断");
    assert_eq!(
        err,
        CallError::Hop {
            at: HopId {
                idx: 1,
                tag: "wait"
            },
            reach: Reach::Unknown,
            why: HopFault::Overrun,
        },
        "第 1 跳超时的形状不对（`05 §3.3` 那张表：Hop{{1 wait, Unknown, Overrun}}）"
    );
    // 上界不是「永远」：给 80ms，允许调度抖动，但不许等到几秒。
    assert!(started.elapsed() < Duration::from_secs(3), "没被截断");
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen[0].3.is_cancelled(),
        "超时了却没拨句柄的撤单手柄 —— 对端撤活那一下（尽力）丢了"
    );
}

#[tokio::test]
async fn backend_errors_pass_through_unchanged() {
    let fake = Fake::default();
    let refused = call_via(
        &fake,
        Origin("devbox".into()),
        "refuse".into(),
        Vec::new(),
        Duration::from_secs(5),
    )
    .await
    .expect_err("说「不行」的句柄");
    assert!(
        matches!(
            refused,
            CallError::Peer {
                why: PeerFault::Refused { .. }
            }
        ),
        "对端的「不行」在这一跳被改了层：{refused:?}"
    );
    let unreachable = call_via(
        &fake,
        Origin("devbox".into()),
        "other".into(),
        Vec::new(),
        Duration::from_secs(5),
    )
    .await
    .expect_err("没有通道");
    assert_eq!(
        unreachable,
        CallError::Hop {
            at: HopId {
                idx: 1,
                tag: "open"
            },
            reach: Reach::NotSent,
            why: HopFault::Unreachable,
        }
    );
}

/// 金标准住址：TS 那侧（`tests/ipc/chan.vitest.ts`）读**同一份文件**解回三层 —— 两侧不各写一份字面量。
const GOLDEN: &str = "tests/__fixtures__/chan-webview-fail.golden.json";

#[test]
fn the_fail_shape_equals_the_golden_file_the_ts_side_decodes() {
    let cases: Vec<(&str, CallError)> = vec![
        (
            "hop",
            CallError::Hop {
                at: HopId {
                    idx: 1,
                    tag: "wait",
                },
                reach: Reach::Unknown,
                why: HopFault::Overrun,
            },
        ),
        (
            "unsupported",
            CallError::Peer {
                why: PeerFault::Unsupported,
            },
        ),
        (
            "refused",
            CallError::Peer {
                why: PeerFault::Refused {
                    body: Body(br#"{"code":"no"}"#.to_vec()),
                },
            },
        ),
        ("ours", OursFault::Misuse.into()),
    ];
    let mut got = serde_json::Map::new();
    for (name, e) in cases {
        got.insert(
            name.to_string(),
            serde_json::to_value(fail(e)).expect("可序列化"),
        );
    }
    let got = serde_json::Value::Object(got);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let want: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(GOLDEN))
            .unwrap_or_else(|e| panic!("读不到 {GOLDEN}：{e}")),
    )
    .expect("金标准不是 JSON");
    assert_eq!(
        got, want,
        "交回 webview 的失败形状与金标准不同 —— TS 那侧按金标准解，两边会各说各的。\n\
         真改了线上形状就重打金标准，并同拍改 `src/ipc/chan.ts` 的解码"
    );
}

#[test]
fn a_blank_origin_never_reaches_a_backend() {
    // `chan_call` 用 `Origin::route` 那道闸挡空白名；这里钉那道闸对三种空白都说不。
    for blank in ["", " ", "\t"] {
        assert!(
            Origin(blank.to_string()).route("chan_call").is_err(),
            "空白名 {blank:?} 过了闸"
        );
    }
    let _ = OursFault::Misuse; // 挡下时回的那一层（见 `chan_call`）
}

// ════════════════════════════════════════════════════════════════════════════
//  〔CF2 · 第四波 4B〕`subscribe` 那一半：流里的格在 webview 这一跳上的样子
//
//  要求住址：`设计/05 §3.3.4`（`Item` 五个变体；「`Gap` 必须在流里的原位」）· `§3.3.0`「载荷是不透明字节」。
// ════════════════════════════════════════════════════════════════════════════

/// 金标准住址：TS 那侧（`tests/ipc/chan.vitest.ts`）读**同一份文件**解回 `Item` —— 两侧不各写一份字面量。
const ITEMS_GOLDEN: &str = "tests/__fixtures__/chan-webview-items.golden.json";

/// ★ S5（Rust 那一半）：每个 `Item` 变体交回 webview 的形状 == 金标准（体按 UTF-8 原样成字符串，不解析）。
#[test]
fn the_item_shapes_equal_the_golden_file_the_ts_side_decodes() {
    use super::{webview_item, Delivery};
    use crate::chan::wire::By;
    let items = vec![
        Item::Frame {
            seq: 7,
            body: Body(br#"{"line":{"x":1}}"#.to_vec()),
        },
        Item::Gap {
            from_seq: 8,
            to_seq: Some(12),
        },
        // 〔RENDER2 · `99 §2.1` ㉓①〕知道丢了、不知道丢到哪
        Item::Gap {
            from_seq: 13,
            to_seq: None,
        },
        Item::Unseen {
            at: HopId {
                idx: 1,
                tag: "open",
            },
            why: HopFault::Unreachable,
        },
        Item::Seen { from: None },
        Item::Seen {
            from: Some(Cursor(vec![1, 2])),
        },
        Item::Closed {
            by: By::Peer(Body(br#"{"code":"no-such-stream"}"#.to_vec())),
        },
        Item::Closed {
            by: By::Ours(OursFault::Broken),
        },
    ];
    let got = serde_json::to_value(Delivery {
        sub: 3,
        items: items.into_iter().map(webview_item).collect(),
    })
    .expect("可序列化");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let want: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(ITEMS_GOLDEN))
            .unwrap_or_else(|e| panic!("读不到 {ITEMS_GOLDEN}：{e}")),
    )
    .expect("金标准不是 JSON");
    assert_eq!(
        got,
        want,
        "交回 webview 的格与金标准不同 —— TS 那侧按金标准解，两边会各说各的。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// ★ S5：体不是 UTF-8（这一跳是文本，过不来）⇒ 那一格换成 `Closed{Ours(Broken)}`，不猜、不有损替换。
#[test]
fn a_body_that_is_not_utf8_becomes_broken_not_lossy() {
    use super::{webview_item, WebviewItem};
    use crate::chan::wire::By;
    for i in [
        Item::Frame {
            seq: 0,
            body: Body(vec![0xff, 0xfe]),
        },
        Item::Closed {
            by: By::Peer(Body(vec![0xc3])),
        },
    ] {
        assert_eq!(
            webview_item(i),
            WebviewItem::ClosedByOurs {
                why: OursFault::Broken
            }
        );
    }
}

/// ★★〔C4e · 第四波 4C〕**F14 那条「能不能回落」的规则两份实现对拍**：Rust `backend_route::route_call_error`
/// 那一收拢 == TS `ipc/chan-caller.ts::provablyNotSent`，跨语言金样 `tests/__fixtures__/reach-collapse.golden.json`。
///
/// 守的要求：`设计/05 §4.5.2`（面 A「连不上时怎么办」）与 `backend_route` 头注那条分界线逐字
/// 「**能不能证明这条命令根本没发出去**」—— 就地 resume 迁到界面之后（`src/tmux-control.ts::sendInto`），
/// 同一条规则在 TS 那一侧有了第二份（它读的是 webview 这一跳上线后的分层错误，不是 inbound 的枚举）。
/// 两份一漂，后果就是 F14 记的那一形：一次「后端已键入但应答超时」被判成可回落 ⇒ 载荷第二次键入正在跑的 claude。
///
/// 异源：本侧把 inbound 的**每一种**失败（外加「没有控制通道」）经生产的 `layer_call_error` / `layer_no_channel`
/// 分层、经生产的 [`fail`] 序列化成 webview 那一跳的线上形状，「可回落」取自生产的 `route_call_error`
/// （`NoChannel` 那一档）—— 产出 == 金样；TS 那侧用生产的 `decodeFail` 解同一份、`provablyNotSent` 判。
#[test]
fn the_fallback_rule_equals_the_golden_file_the_ts_side_judges() {
    use crate::backend::control::backend_route::{
        layer_call_error, layer_no_channel, route_call_error, Routed,
    };
    use crate::backend::control::inbound_client::CallError as Inbound;
    let cases: Vec<(&str, Inbound)> = vec![
        (
            "unsupported",
            Inbound::Unsupported {
                cmd: "launch".into(),
                offered: vec![],
            },
        ),
        ("too_many_pending", Inbound::TooManyPending),
        ("disconnected", Inbound::Disconnected),
        (
            "timeout",
            Inbound::Timeout {
                after: Duration::from_secs(10),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
        ),
        ("cancelled", Inbound::Cancelled),
        (
            "remote_refused",
            Inbound::Remote {
                code: "wrong_owner".into(),
                message: "m".into(),
            },
        ),
    ];
    let mut rows = vec![serde_json::json!({
        "case": "no_channel",
        "fail": serde_json::to_value(fail(layer_no_channel(1))).expect("可序列化"),
        // `client_for` 回 None ⇒ `backend_route::no_channel` ⇒ `Routed::NoChannel`（一个字节都没发）。
        "provablyNotSent": true,
    })];
    for (name, e) in cases {
        let may_fall_back = matches!(
            route_call_error(&e, |_, _| String::new()),
            Routed::NoChannel(_)
        );
        rows.push(serde_json::json!({
            "case": name,
            "fail": serde_json::to_value(fail(layer_call_error(&e, 1).error)).expect("可序列化"),
            "provablyNotSent": may_fall_back,
        }));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("tests/__fixtures__/reach-collapse.golden.json"))
            .expect("读不到 reach-collapse 金样"),
    )
    .expect("金样不是 JSON");
    assert_eq!(
        serde_json::Value::Array(rows),
        golden["rows"],
        "Rust 那条「能不能回落」的收拢与金样对不上 —— TS 那侧（`provablyNotSent`）按金样判，两份会各说各的。\n\
         真改了规则就重打金样，并同拍改 `src/ipc/chan-caller.ts::provablyNotSent`"
    );
}
