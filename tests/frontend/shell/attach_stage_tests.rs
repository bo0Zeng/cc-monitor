//! `attach_stage.rs` 的判据 —— 聊天附件的分块传由壳替界面走 `chunk_core::stage`（合成句柄 ＋ 真 `call_via`）。
//!
//! | 判据 | 钉的那一形 |
//! |---|---|
//! | [`the_bytes_go_up_in_chunks_under_one_key_and_come_back_staged`] | 逐块 `files-stage-chunk`、块号从 0 连着、同一个键；块拼回来 == 交来的那份；回的块数 · 字节 · 摘要照实；进度逐块报 |
//! | [`a_stop_sends_no_further_chunk`] | 撤了 ⇒ 下一块不发，失败那一形是 `ours/Cancelled` |
//! | [`a_cut_hop_comes_back_as_the_hop_layer`] | 那台断开 ⇒ 失败照 `chan_call` 那一形交回（`err.Hop`，界面照它认离线） |
//! | [`a_ticket_stopped_before_its_upload_starts_still_stops_it`] | 撤单先于开传到达也撤得住；撤单按窗口分 |
//!
//! 摘要的期望是死值（同一段字节另算的 SHA-256），不经 `chunk_core` 自己算。

use super::{stage_via, Tickets};
use crate::chan::router::Backends;
use crate::chan::wire::{
    Body, CallError, CancelToken, Cursor, HopFault, HopId, Item, Kind, Op, Origin, Reach,
};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// 合成句柄：记下每一问（op · 请求 JSON）；`cut_at` ⇒ 第几块那一问答「那一跳断了」。
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<(String, serde_json::Value)>>,
    cut_at: Option<u64>,
}

impl Backends for Fake {
    fn call(
        &self,
        _origin: Origin,
        op: Op,
        payload: Body,
        _view: Option<serde_json::Value>,
        _left: Duration,
        _cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        let req: serde_json::Value = serde_json::from_slice(&payload.0).expect("请求体是 JSON");
        let seq = req["seq"].as_u64();
        let n = req["content"]["b16"].as_str().map_or(0, |s| s.len() / 2);
        self.seen.lock().unwrap().push((op.0.clone(), req));
        if seq.is_some() && seq == self.cut_at {
            return Box::pin(async {
                Err(CallError::Hop {
                    at: HopId {
                        idx: 1,
                        tag: "open",
                    },
                    reach: Reach::NotSent,
                    why: HopFault::Unreachable,
                })
            });
        }
        Box::pin(async move { Ok(Body(format!(r#"{{"bytes":{n}}}"#).into_bytes())) })
    }

    fn subscribe(&self, _o: Origin, _k: Kind, _f: Option<Cursor>) -> BoxStream<'static, Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 两块整 ＋ 一截：`i * 7 % 251`。
fn body() -> Vec<u8> {
    (0..(chunk_core::CHUNK * 2 + 1000))
        .map(|i| (i * 7 % 251) as u8)
        .collect()
}

/// [`body`] 那段字节的 SHA-256（python3 `hashlib` 对 `bytes(i*7%251 for i in range(256*1024*2+1000))` 现打）。
const BODY_SHA256: &str = "95a38c23fa5f921178bc7d93b7b40e31004e5ae46c1f6cd781f511034e390d1f";

const KEY: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn the_bytes_go_up_in_chunks_under_one_key_and_come_back_staged() {
    let fake = Fake::default();
    let bytes = body();
    let stop = AtomicBool::new(false);
    let mut moved = Vec::new();
    let staged = stage_via(
        &fake,
        &Origin::named("devbox"),
        KEY.to_string(),
        &bytes,
        &stop,
        |d| moved.push(d),
    )
    .await
    .expect("合成句柄都答了，该传完");
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen.iter().all(|(op, _)| op == "files-stage-chunk"),
        "壳只送块，提交由界面问：{:?}",
        seen.iter().map(|(o, _)| o).collect::<Vec<_>>()
    );
    let seqs: Vec<u64> = seen
        .iter()
        .map(|(_, r)| r["seq"].as_u64().unwrap())
        .collect();
    assert_eq!(seqs, vec![0, 1, 2], "块号从 0 连着来");
    assert!(seen.iter().all(|(_, r)| r["key"] == KEY), "每一块同一个暂存键");
    let rebuilt: Vec<u8> = seen
        .iter()
        .flat_map(|(_, r)| {
            cmd_wire::files::from_hex(r["content"]["b16"].as_str().unwrap()).expect("块是十六进制")
        })
        .collect();
    assert_eq!(rebuilt, bytes, "块拼回来不是交来的那份");
    assert_eq!(
        (
            staged.key.as_str(),
            staged.chunks,
            staged.bytes,
            staged.sha256.as_str()
        ),
        (KEY, 3, bytes.len() as u64, BODY_SHA256)
    );
    let c = chunk_core::CHUNK as u64;
    assert_eq!(moved, vec![0, c, 2 * c, bytes.len() as u64], "进度逐块报");
}

#[tokio::test]
async fn a_stop_sends_no_further_chunk() {
    let fake = Fake::default();
    let stop = AtomicBool::new(false);
    let bytes = body();
    let fail = stage_via(
        &fake,
        &Origin::named("devbox"),
        KEY.to_string(),
        &bytes,
        &stop,
        |d| {
            if d > 0 {
                stop.store(true, Ordering::SeqCst);
            }
        },
    )
    .await
    .expect_err("撤了不该传完");
    assert_eq!(fake.seen.lock().unwrap().len(), 1, "撤了之后又送了块");
    let v = serde_json::to_value(&fail).unwrap();
    assert_eq!(v["err"], serde_json::json!({"Ours": "Cancelled"}), "{v}");
}

#[tokio::test]
async fn a_cut_hop_comes_back_as_the_hop_layer() {
    let fake = Fake {
        cut_at: Some(1),
        ..Fake::default()
    };
    let stop = AtomicBool::new(false);
    let bytes = body();
    let fail = stage_via(
        &fake,
        &Origin::named("devbox"),
        KEY.to_string(),
        &bytes,
        &stop,
        |_| {},
    )
    .await
    .expect_err("那一跳断了不该传完");
    let v = serde_json::to_value(&fail).unwrap();
    assert!(
        v["err"].get("Hop").is_some(),
        "断开要照 `chan_call` 那一形交回跳失败，界面才认得离线：{v}"
    );
    assert!(
        v["said"].as_str().is_some_and(|s| !s.is_empty()),
        "带通道那一句：{v}"
    );
}

#[test]
fn a_ticket_stopped_before_its_upload_starts_still_stops_it() {
    let t = Tickets::default();
    t.stop("main", 7);
    let flag = t.open("main", 7);
    assert!(flag.load(Ordering::SeqCst), "先到的撤单丢了");
    t.close("main", 7);
    let again = t.open("main", 7);
    assert!(
        !again.load(Ordering::SeqCst),
        "收完那一单的撤单不许留给下一单"
    );
    let other = t.open("other", 7);
    t.stop("main", 7);
    assert!(!other.load(Ordering::SeqCst), "撤单按窗口分");
}
