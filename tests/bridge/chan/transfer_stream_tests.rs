//! 〔F7c · 第三波 · 2026-09-24〕**生产上的第一条流**：传输台的进度（`设计/60 §13.6` 判据 4）。
//!
//! # 台架
//!
//! **真回环口 ＋ 真钥匙 ＋ 真路由器 ＋ 生产句柄 [`InboundBackends`]**（不是 `chan_tests` 那个合成句柄）。
//! 唯一合成的是传输台那张票里的「那件事」：一趟真的 [`crate::sftp_pool::upload_to_staging`]，
//! 跑在 `sftp_staging_tests` 那台逐条记路径的合成 SFTP 服务端上（本仓红线不许起真连接）。
//!
//! # 买到什么
//!
//! - 生产句柄对 `transfer/<id>` **真的出帧**：进度格序号从 1 连续、已传单调不减、最后一格是
//!   `Closed{Peer({"state":"done","bytes"})}`、`bytes` == 语料长度；暂存件逐字节是语料。
//! - **停订就是撤**：一件只有被撤才会收场的事，经真回环停订之后真的看见了撤的旗。
//! - 票在收场 / 停订之后**从台上摘掉**（在册票数回到原值）。
//! - 没有这张票 / 第二次订阅 / 带了 `from` ⇒ 原位 `Closed{Peer}` 说清楚，不装作订阅成功。
//! - 开单口：没有这台机器的配置 ⇒ `Peer{Refused{"no_such_origin"}}`（不起任何连接）。
//!
//! # 买不到什么
//!
//! - 真 sshd 上的一趟；真窗口进程（窗口那一侧由 `filewin` 的判据另判）。

use super::super::dial::dial;
use super::super::wire::{
    Body, Budget, By, CallError, CancelToken, Comms, Item, Op, PeerFault, Sub,
};
use super::*;
use crate::sftp_pool::staging_tests::{
    corpus, home_with_backend, session_on, ticket_in_desk, Local,
};
use crate::sftp_pool::{open_ticket, staging_part, upload_to_staging, TransferJob};
use futures::stream::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// 🔴🔴 **生产句柄对 `transfer/<id>` 真的出帧**，而且那些帧说的是一趟真传输的真读数。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_production_handle_streams_a_real_upload_over_real_loopback() {
    let client = rig().await;
    let fs = home_with_backend();
    let sftp = session_on(fs.clone()).await;
    let body = corpus(300 * 1024 + 17);
    let local = Local::new("chan", &body);
    let path = local.path();
    let key = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";
    let job: TransferJob = Box::new(move |cancel, sink| {
        Box::pin(async move { upload_to_staging(&sftp, &path, key, &cancel, &*sink).await })
    });
    let origin = "判据机器·transfer-stream";
    let id = open_ticket(
        &crate::chan::wire::Origin(origin.to_string()),
        Some(key.to_string()),
        job,
    )
    .expect("开单");
    assert!(ticket_in_desk(&id), "刚开的单不在册");
    let sub = client.subscribe(
        &crate::chan::wire::Origin(origin.to_string()),
        &crate::chan::wire::Kind(format!("{}{id}", crate::sftp_pool::TRANSFER_KIND_PREFIX)),
        None,
        4,
    );
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
    // 第一格是「此刻」：那件事还没报过进度时它就是 `{0,0}`（如实，不编）。之后每一格的总长都是语料长度。
    for (i, (_, v)) in frames.iter().enumerate() {
        let total = v["total"].as_u64().expect("total");
        if i == 0 && total == 0 {
            assert_eq!(
                v["got"].as_u64(),
                Some(0),
                "还没报过进度的那一格已传不是 0：{v}"
            );
            continue;
        }
        assert_eq!(total, body.len() as u64, "总长报错了：{v}");
    }
    assert!(
        frames
            .iter()
            .any(|(_, v)| v["total"].as_u64() == Some(body.len() as u64)),
        "一格真进度都没出（全是起跑前那一格）"
    );
    let Some(Item::Closed { by: By::Peer(end) }) = items.last() else {
        panic!("最后一格不是对端收场：{items:?}");
    };
    let end = body_json(end);
    assert_eq!(end["state"], "done", "{end}");
    assert_eq!(end["bytes"].as_u64(), Some(body.len() as u64));
    let staged = fs.lock().unwrap().files_snapshot(&staging_part(key));
    assert_eq!(staged, Some(body), "流说传完了，暂存件却不是那份语料");
    // 收场 ＋ 停订之后票从台上摘掉（等路由器那一侧把流丢掉 —— 有界自旋，不睡觉）。
    let gone = tokio::time::timeout(Duration::from_secs(5), async {
        while ticket_in_desk(&id) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(gone.is_ok(), "收场之后票 {id} 还在台上");
}

/// 🔴 **停订就是撤**：一件只有被撤才会收场的事 ⇒ 经真回环停订 ⇒ 它看见了撤的旗。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_the_subscription_cancels_the_transfer() {
    let client = rig().await;
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let saw = saw_cancel.clone();
    let job: TransferJob = Box::new(move |cancel, sink| {
        Box::pin(async move {
            sink(1, 100);
            while !cancel.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            saw.store(true, Ordering::SeqCst);
            Err("已取消".to_string())
        })
    });
    let origin = "判据机器·transfer-stop";
    let id = open_ticket(&crate::chan::wire::Origin(origin.to_string()), None, job).expect("开单");
    let mut sub = client.subscribe(
        &crate::chan::wire::Origin(origin.to_string()),
        &crate::chan::wire::Kind(format!("{}{id}", crate::sftp_pool::TRANSFER_KIND_PREFIX)),
        None,
        8,
    );
    let first = tokio::time::timeout(Duration::from_secs(10), sub.next())
        .await
        .expect("第一格该来")
        .expect("流没断");
    assert!(matches!(first, Item::Frame { .. }), "{first:?}");
    assert!(!saw_cancel.load(Ordering::SeqCst), "还没停订就被撤了");
    sub.stop();
    let r = tokio::time::timeout(Duration::from_secs(10), async {
        while !saw_cancel.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(r.is_ok(), "停订了，那一趟没看见撤的旗 —— 撤没有接到传输上");
}

/// ★ 没有这张票 / 第二次订阅 / 带了 `from` ⇒ 原位 `Closed{Peer}`，码说得清是哪一种。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_transfer_subscription_says_why_in_place() {
    let client = rig().await;
    let origin = crate::chan::wire::Origin("判据机器·transfer-bad".to_string());
    let kind = |id: &str| {
        crate::chan::wire::Kind(format!("{}{id}", crate::sftp_pool::TRANSFER_KIND_PREFIX))
    };
    let code_of = |items: &[Item]| match items {
        [Item::Closed { by: By::Peer(b) }] => {
            body_json(b)["code"].as_str().unwrap_or("").to_string()
        }
        other => panic!("该恰好一格 Closed{{Peer}}：{other:?}"),
    };
    let items = drain(client.subscribe(&origin, &kind("xfer-nope"), None, 4)).await;
    assert_eq!(code_of(&items), "no-such-transfer");

    // 第二次订阅同一张票：两条订阅帧在线上谁先到不定（订阅帧是另起任务发的）⇒
    // 不押顺序，只押「恰好一条拿到那一趟（先出进度格）、另一条原位被拒 already-watched」。
    let job: TransferJob = Box::new(|cancel, sink| {
        Box::pin(async move {
            sink(0, 1);
            while !cancel.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            Err("已取消".to_string())
        })
    });
    let id = open_ticket(&origin, None, job).expect("开单");
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
    let (watching, refused_one) = match (&ia, &ib) {
        (Item::Frame { .. }, Item::Closed { .. }) => (ia.clone(), ib.clone()),
        (Item::Closed { .. }, Item::Frame { .. }) => (ib.clone(), ia.clone()),
        other => panic!("该恰好一条出进度、一条被拒：{other:?}"),
    };
    assert!(matches!(watching, Item::Frame { .. }));
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
