//! 〔F7c · 第三波 · 2026-09-24〕**生产上的第一条流**：传输台的进度（`设计/60 §13.6` 判据 4）。
//!
//! # 台架
//!
//! **真回环口 ＋ 真钥匙 ＋ 真路由器 ＋ 生产句柄 [`InboundBackends`]**（不是 `chan_tests` 那个合成句柄）。
//! 〔SR1b · 2026-09-24〕传输台搬进了本机常驻后端 ⇒ 这里唯一合成的是**那个本机后端**：借中继判据那台
//! 「真 client ＋ 真吸收点 ＋ 假后端」（`sftp_pool::tests::rig`），它按剧本回开单 / 起跑的应答、往本机那条流上塞 `transfer` 帧。
//! 〔墓碑 —— F7c 那一版合成的是票里「那件事」：一趟真的暂存区上传，跑在合成 SFTP 服务端上；
//!  那一半的判据跟着传输本体搬去了后端（`tests/backend/control/transfer_tests.rs`）。〕
//!
//! # 买到什么
//!
//! - 生产句柄对 `transfer/<id>` **真的出帧**：本机后端推上来的进度 ⇒ 进度格序号从 1 连续、已传单调不减、
//!   最后一格是 `Closed{Peer({"state":"done","bytes"})}`。
//! - **停订就是撤**：经真回环停订 ⇒ 本机后端收到 `transfer-stop {id}`。
//! - 没有这张票 / 第二次订阅 / 带了 `from` ⇒ 原位 `Closed{Peer}` 说清楚，不装作订阅成功。
//! - 开单口：没有这台机器的配置 ⇒ `Peer{Refused{"no_such_origin"}}`（不起任何连接）。
//!
//! # 买不到什么
//!
//! - 真 sshd 上的一趟（`tests/evidence/SR1b-sftp-loopback.py`）；真窗口进程（窗口那一侧由 `filewin` 的判据另判）。

use super::super::dial::dial;
use super::super::wire::{
    Body, Budget, By, CallError, CancelToken, Comms, Item, Op, PeerFault, Sub,
};
use super::*;
use crate::sftp_pool::tests::{cfg, rig as backend_rig, ALL};
use futures::stream::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

const FRAME: usize = 1 << 20;

fn budget(ms: u64) -> Budget {
    Budget {
        until: Instant::now() + Duration::from_millis(ms),
        cancel: CancelToken::new(),
    }
}

/// 起一个挂着**生产句柄**的真通道口，拨上去。
async fn rig() -> crate::chan::client::Client {
    let h = start_with(
        Arc::new(InboundBackends),
        mint_key(),
        FRAME,
        Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    assert!(h.addr.ip().is_loopback());
    dial(&h, budget(5_000)).await.expect("连得上并过认证")
}

/// 收一条订阅直到 `Closed`（或期限到）。
async fn drain(mut sub: impl Sub + Unpin) -> Vec<Item> {
    let mut out = Vec::new();
    let r = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(i) = sub.next().await {
            let last = matches!(i, Item::Closed { .. });
            out.push(i);
            sub.want(1);
            if last {
                break;
            }
        }
    })
    .await;
    assert!(r.is_ok(), "20 秒内流没收场 —— 收到的：{out:?}");
    out
}

fn body_json(b: &Body) -> serde_json::Value {
    serde_json::from_slice(&b.0).expect("流里的体是 JSON")
}

fn kind(id: &str) -> crate::chan::wire::Kind {
    crate::chan::wire::Kind(format!("{}{id}", crate::sftp_pool::TRANSFER_KIND_PREFIX))
}

/// 开一张上传单（经中继、转给台架那个本机后端），回票号。
async fn open_upload(label: &str) -> String {
    let v = crate::sftp_pool::transfer_call(
        cfg(label),
        crate::sftp_pool::TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": "/tmp/ccm-sr1b-chan.bin" }),
    )
    .await
    .expect("开单");
    v["id"].as_str().expect("票号").to_string()
}

/// 🔴🔴 **生产句柄对 `transfer/<id>` 真的出帧**，而且那些帧说的就是本机后端推上来的那几格。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_production_handle_streams_the_local_backends_frames_over_real_loopback() {
    let _g = crate::backend::control::inbound_client::local_origin_test_lock();
    let mut be = backend_rig(&ALL);
    let client = rig().await;
    let label = "判据机器·transfer-stream";
    let id = open_upload(label).await;
    let sub = client.subscribe(
        &crate::chan::wire::Origin(label.to_string()),
        &kind(&id),
        None,
        4,
    );
    let start = be.next("transfer-start").await;
    assert_eq!(start["args"]["id"], id.as_str(), "订阅没起跑那一趟");
    let total = 300 * 1024 + 17;
    for got in [65_536u64, 196_608, total] {
        be.frame(&format!(
            r#"{{"kind":"transfer","id":{id:?},"got":{got},"total":{total}}}"#
        ));
    }
    be.frame(&format!(
        r#"{{"kind":"transfer","id":{id:?},"got":{total},"total":{total},"end":{{"state":"done","bytes":{total}}}}}"#
    ));
    let items = drain(sub).await;
    let frames: Vec<(u64, serde_json::Value)> = items
        .iter()
        .filter_map(|i| match i {
            Item::Frame { seq, body } => Some((*seq, body_json(body))),
            _ => None,
        })
        .collect();
    assert!(!frames.is_empty(), "一格进度都没出：{items:?}");
    let seqs: Vec<u64> = frames.iter().map(|(s, _)| *s).collect();
    let want: Vec<u64> = (1..=frames.len() as u64).collect();
    assert_eq!(seqs, want, "进度格的序号不是从 1 连续的");
    let gots: Vec<u64> = frames
        .iter()
        .map(|(_, v)| v["got"].as_u64().expect("got"))
        .collect();
    assert!(
        gots.windows(2).all(|w| w[0] <= w[1]),
        "已传不是单调不减的：{gots:?}"
    );
    let Some(Item::Closed { by: By::Peer(end) }) = items.last() else {
        panic!("最后一格不是对端收场：{items:?}");
    };
    let end = body_json(end);
    assert_eq!(end["state"], "done", "{end}");
    assert_eq!(end["bytes"].as_u64(), Some(total));
}

/// 🔴 **停订就是撤**：经真回环停订 ⇒ 本机后端收到 `transfer-stop {id}`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_the_subscription_cancels_the_transfer() {
    let _g = crate::backend::control::inbound_client::local_origin_test_lock();
    let mut be = backend_rig(&ALL);
    let client = rig().await;
    let label = "判据机器·transfer-stop";
    let id = open_upload(label).await;
    let mut sub = client.subscribe(
        &crate::chan::wire::Origin(label.to_string()),
        &kind(&id),
        None,
        8,
    );
    be.next("transfer-start").await;
    let first = tokio::time::timeout(Duration::from_secs(10), sub.next())
        .await
        .expect("第一格该来")
        .expect("流没断");
    assert!(matches!(first, Item::Frame { .. }), "{first:?}");
    sub.stop();
    let stop = be.next("transfer-stop").await;
    assert_eq!(stop["args"]["id"], id.as_str(), "停订了，本机后端没收到撤");
}

/// ★ 没有这张票 / 第二次订阅 / 带了 `from` ⇒ 原位 `Closed{Peer}`，码说得清是哪一种。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_transfer_subscription_says_why_in_place() {
    let _g = crate::backend::control::inbound_client::local_origin_test_lock();
    let _be = backend_rig(&ALL);
    let client = rig().await;
    let label = "判据机器·transfer-bad";
    let origin = crate::chan::wire::Origin(label.to_string());
    let code_of = |items: &[Item]| match items {
        [Item::Closed { by: By::Peer(b) }] => {
            body_json(b)["code"].as_str().unwrap_or("").to_string()
        }
        other => panic!("该恰好一格 Closed{{Peer}}：{other:?}"),
    };
    let items = drain(client.subscribe(&origin, &kind("xfer-nope"), None, 4)).await;
    assert_eq!(code_of(&items), "no-such-transfer");

    // 第二次订阅同一张票：两条订阅帧在线上谁先到不定 ⇒ 不押顺序，
    // 只押「恰好一条拿到那一趟（先出进度格）、另一条原位被拒 already-watched」。
    let id = open_upload(label).await;
    let mut a = client.subscribe(&origin, &kind(&id), None, 4);
    let mut b = client.subscribe(&origin, &kind(&id), None, 4);
    let first = |i: Option<Item>| i.expect("流没断");
    let ia = first(
        tokio::time::timeout(Duration::from_secs(10), a.next())
            .await
            .expect("a 第一格"),
    );
    let ib = first(
        tokio::time::timeout(Duration::from_secs(10), b.next())
            .await
            .expect("b 第一格"),
    );
    let refused_one = match (&ia, &ib) {
        (Item::Frame { .. }, Item::Closed { .. }) => ib.clone(),
        (Item::Closed { .. }, Item::Frame { .. }) => ia.clone(),
        other => panic!("该恰好一条出进度、一条被拒：{other:?}"),
    };
    assert_eq!(
        code_of(std::slice::from_ref(&refused_one)),
        "already-watched"
    );
    drop((a, b));

    let items = drain(client.subscribe(
        &origin,
        &kind("xfer-whatever"),
        Some(crate::chan::wire::Cursor(b"7".to_vec())),
        4,
    ))
    .await;
    assert_eq!(code_of(&items), "bad_args");
}

/// ★ 开单口：没有这台机器的配置 ⇒ `Peer{Refused{"no_such_origin"}}`，一条连接都不起。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_a_transfer_for_an_unknown_machine_is_refused_out_loud() {
    let client = rig().await;
    let r = client
        .call(
            &crate::chan::wire::Origin("判据里不存在的机器·transfer".to_string()),
            &Op(crate::sftp_pool::TRANSFER_UPLOAD.to_string()),
            Body(br#"{"local_path":"/nonexistent"}"#.to_vec()),
            budget(5_000),
        )
        .await;
    match r {
        Err(CallError::Peer {
            why: PeerFault::Refused { body },
        }) => assert_eq!(body_json(&body)["code"], "no_such_origin"),
        other => panic!("该是对端拒：{other:?}"),
    }
}

/// 〔FILES2 · Q5〕带码的失败经生产句柄原样到窗口：`{"state":"failed","why","code"}` —— 窗口据 `sftp_home_mismatch` 换路。
/// 要求住址：`设计/60 §7` 第 9 条 Q5（主会话 09-27 裁「不一致 ⇒ 这台的上传改走后端链路分块写」）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_coded_failure_reaches_the_window_with_its_code() {
    let _g = crate::backend::control::inbound_client::local_origin_test_lock();
    let mut be = backend_rig(&ALL);
    let client = rig().await;
    let label = "判据机器·transfer-coded";
    let id = open_upload(label).await;
    let sub = client.subscribe(
        &crate::chan::wire::Origin(label.to_string()),
        &kind(&id),
        None,
        4,
    );
    be.next("transfer-start").await;
    be.frame(&format!(
        r#"{{"kind":"transfer","id":{id:?},"got":0,"total":0,"end":{{"state":"failed","why":"w","code":"sftp_home_mismatch"}}}}"#
    ));
    let items = drain(sub).await;
    let Some(Item::Closed { by: By::Peer(end) }) = items.last() else {
        panic!("最后一格不是对端收场：{items:?}");
    };
    assert_eq!(
        body_json(end),
        serde_json::json!({ "state": "failed", "why": "w", "code": "sftp_home_mismatch" })
    );
}
