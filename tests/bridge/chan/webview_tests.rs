//! 通道 · webview 那一侧（`chan/webview.rs`）的判据 —— 合成句柄 ＋ 真 `router::settle`。
//!
//! | 性质 | 判据 |
//! |---|---|
//! | 载荷原样（含非 UTF-8 字节），`origin` / `op` 原样到句柄 | [`payload_origin_and_op_reach_the_backend_verbatim`] |
//! | 第 1 跳超时 ⇒ `Hop{1 wait, Unknown, Overrun}` ＋ 句柄的撤单手柄被拨下 | [`a_backend_that_never_answers_is_bounded_by_left`] |
//! | 句柄的错误原样过线（不在这一跳改层） | [`backend_errors_pass_through_unchanged`] |
//! | 交回 webview 的失败形状 == 金标准（TS 那侧解的是同一份文件） | [`the_fail_shape_equals_the_golden_file_the_ts_side_decodes`] |
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
        Origin("aya".into()),
        "echo".into(),
        payload.clone(),
        Duration::from_secs(5),
    )
    .await
    .expect("回声句柄不该失败");
    assert_eq!(got.0, payload, "载荷在 webview 那一跳被改了字节");
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "aya", "origin 没原样到句柄");
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
        Origin("aya".into()),
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
        Origin("aya".into()),
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
