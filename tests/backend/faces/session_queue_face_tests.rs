//! 会话输入队那三条的帧面一层：入参照类型解（多一个键 · 缺 id 拒）· 码表 · 本机那一支落到这台的队上、回执带序号与排第几。
//!
//! ⚠ 家目录一律是临时目录（`Own::issue_at`），真 `~/.cc-monitor/` 一个字节都不碰；本机那一支不碰可达表与对面。

use super::*;
use crate::history::session_queue_wire::QueuePutOutcome;
use serde_json::json;

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-squeue-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put_req(v: serde_json::Value) -> Result<SessionQueuePutReq, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn put_args_decode_by_type() {
    let ok = put_req(json!({
        "machine": "<local>", "sid": "s1", "id": "m1", "end": "desk", "n": 1, "atMs": 5, "text": "hi"
    }))
    .expect("照类型解");
    assert_eq!((ok.id.as_str(), ok.n, ok.at_ms), ("m1", Some(1), 5));
    assert!(
        put_req(json!({"machine": "<local>", "sid": "s1", "end": "d", "atMs": 1, "text": "t"}))
            .is_err(),
        "缺 id ⇒ 拒"
    );
    assert!(
        put_req(json!({
            "machine": "<local>", "sid": "s1", "id": "m", "end": "d", "atMs": 1, "text": "t", "extra": 1
        }))
        .is_err(),
        "多一个键 ⇒ 拒"
    );
    assert!(
        put_req(json!({"machine": "<local>", "sid": "../x", "id": "m", "end": "d", "atMs": 1, "text": "t"}))
            .is_err(),
        "sid 不合值域 ⇒ 拒（落不成路径）"
    );
}

#[test]
fn every_command_declares_offline() {
    for all in [
        SessionQueuePutErr::ALL,
        SessionQueueCancelErr::ALL,
        SessionQueueReadErr::ALL,
    ] {
        assert!(all.contains(&"offline"), "{all:?}");
        assert!(all.contains(&"queue_unreadable"), "{all:?}");
    }
}

#[tokio::test]
async fn local_put_lands_on_this_machine() {
    let h = home("local");
    let own = crate::platform::fs::Own::issue_at(&h);
    let req = put_req(json!({
        "machine": "<local>", "sid": "s1", "id": "m1", "end": "desk", "atMs": 5, "text": "hi"
    }))
    .unwrap();
    let r = req.clone();
    let o = own.clone();
    let got = route::<SessionQueuePutCmd, _>(
        &req,
        move || q::put(&o, &r, 100),
        SessionQueuePutErr::ALL,
        &crate::Tz::default(),
        None,
        &crate::dial::remote_ask::test_peers::named(&[]),
        &DialRemote,
    )
    .await
    .expect("本机那一支");
    assert_eq!(
        (got.outcome, got.seq, got.place),
        (QueuePutOutcome::Queued, Some(1), Some(1))
    );
    let list = q::read(&own, "s1", 101).unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].text, "hi");
}
