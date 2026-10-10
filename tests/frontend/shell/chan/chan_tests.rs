//! 通道的判据 —— 一个**合成的外部前端**（一对内存管子 ＋ 真路由器 ＋ 真客户端）对着一个**合成后端句柄**说话。
//! 生产里那一对管子是窗口进程的 stdin / stdout（父子管道，没有监听口、没有钥匙）；路由器与客户端只认一对读写半边，管子换成内存的一模一样。
//!
//! # 它证明什么（逐条对任务书那五件）
//!
//! | 件 | 判据 |
//! |---|---|
//! | 连得上 | [`a_synthetic_frontend_connects_over_a_pipe_pair_and_calls_through`] |
//! | 没有监听口 | [`nothing_in_the_channel_listens`] |
//! | `call` 走得通 | 同第一条（载荷逐字节、含非 UTF-8 字节；`origin`/`op` 原样到句柄） |
//! | `subscribe` 收得到帧 | [`subscribe_receives_frames_in_order_and_from_passes_through`] ＋ [`credit_is_backpressure_not_loss`] |
//! | 期限到了按约定的形状报错 | [`a_passed_deadline_answers_hop_overrun_in_the_05_shape`] |
//!
//! # 买不到什么
//!
//! - **不买「真窗口能用」** —— 合成前端与合成句柄都是判据自己造的；接进文件窗口是下一波 F2。
//! - **不买生产句柄的 `subscribe`** —— 生产今天没有流（`chan::host` 头注），这里只钉它「原位说没有」。
//! - 🔴 **「credit 用完就不再取」那一格证的是「一趟往返之内没多取」**（栅栏见 `round_trip`），
//!   证明不了「永远不会多取」：哪天抽流改成按计时器隔一阵才取，一趟往返之内看不见它。

use super::client::Client;
use super::host::{self, InboundBackends};
use super::router::{self, Backends, Terms};
use super::wire::{
    err_from_wire, err_to_wire, item_from_wire, item_to_wire, read_frame, write_frame, Body,
    Budget, By, CallError, CancelToken, Comms, Cursor, Head, HopFault, HopId, Item, Kind, Offer,
    Op, Origin, OursFault, PeerFault, Reach, Sub, HOP_TAGS,
};
use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 判据用的帧长上限（真值由宿主交进来；这里就是那个「宿主」）。
const FRAME: usize = 1 << 20;

fn budget(ms: u64) -> Budget {
    Budget {
        until: Instant::now() + Duration::from_millis(ms),
        cancel: CancelToken::new(),
    }
}

/// 句柄看见的一次 `call`：`(origin, op, payload, left, cancel)`。
type Seen = (String, String, Vec<u8>, Duration, CancelToken);

/// 合成后端句柄：记下它看见了什么，按 `op` / `kind` 演几种对端。
#[derive(Default)]
struct Fake {
    /// 每一次 `call` 看到的东西。
    calls: Mutex<Vec<Seen>>,
    /// 无尽流那一种已经被取走了几格。
    pulled: Arc<AtomicUsize>,
    /// 每一次 `call` 带来的出口声明。
    views: Mutex<Vec<Option<serde_json::Value>>>,
}

impl Backends for Fake {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        view: Option<serde_json::Value>,
        left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        self.views.lock().unwrap().push(view);
        self.calls.lock().unwrap().push((
            origin.0.clone(),
            op.0.clone(),
            payload.0.clone(),
            left,
            cancel,
        ));
        Box::pin(async move {
            match op.0.as_str() {
                "echo" => Ok(payload),
                "refuse" => Err(CallError::Peer {
                    why: PeerFault::Refused {
                        body: Body(b"\x00not today\xff".to_vec()),
                    },
                }),
                "nowhere" => Err(CallError::Hop {
                    at: HopId {
                        idx: 1,
                        tag: "open",
                    },
                    reach: Reach::NotSent,
                    why: HopFault::Unreachable,
                }),
                // 永不回答 —— 期限 / 撤单那几条用它。
                _ => std::future::pending().await,
            }
        })
    }

    fn subscribe(
        &self,
        _origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        match kind.0.as_str() {
            "three" => {
                let mut items = vec![Item::Seen { from }];
                for seq in 1..=3u64 {
                    items.push(Item::Frame {
                        seq,
                        body: Body(vec![seq as u8, 0xff]),
                    });
                }
                items.push(Item::Closed {
                    by: By::Peer(Body(b"done".to_vec())),
                });
                Box::pin(futures::stream::iter(items))
            }
            _ => {
                let pulled = Arc::clone(&self.pulled);
                Box::pin(futures::stream::unfold(0u64, move |seq| {
                    let pulled = Arc::clone(&pulled);
                    async move {
                        pulled.fetch_add(1, Ordering::SeqCst);
                        Some((
                            Item::Frame {
                                seq,
                                body: Body(seq.to_be_bytes().to_vec()),
                            },
                            seq + 1,
                        ))
                    }
                }))
            }
        }
    }

    /// 合成那台的能力事实：`hang` 撤不动，`stall` 撤得动。
    fn offer(&self, _origin: &Origin) -> Option<Offer> {
        Some(Offer::new(
            vec!["cancel".into(), "hang".into(), "stall".into()],
            vec![],
            vec!["hang".into()],
        ))
    }
}

/// 起一条挂着合成句柄的真通道：一对内存管子，一头交给宿主（[`host::serve_with`]，生产里它接的是窗口进程的 stdout / stdin），
/// 另一头起客户端（生产里是窗口进程自己的 stdin / stdout）。
fn rig() -> (Arc<Fake>, Client) {
    let fake = Arc::new(Fake::default());
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    let (host_rd, host_wr) = tokio::io::split(theirs);
    host::serve_with(host_rd, host_wr, fake.clone());
    let (rd, wr) = tokio::io::split(ours);
    (fake, Client::over(rd, wr, host::FRAME_MAX_BYTES))
}

// ════════════════════════════════════════════════════════════════════════════
//  一、连得上 · call 走得通
// ════════════════════════════════════════════════════════════════════════════

/// ★ 合成外部前端经一对管子连上、`call` 一次：载荷**逐字节**回来（含非 UTF-8 字节），
/// `origin` 与 `op` **原样**到了句柄手里（路由器没解释它们）。
#[tokio::test]
async fn a_synthetic_frontend_connects_over_a_pipe_pair_and_calls_through() {
    let (fake, c) = rig();
    let payload = vec![0u8, 1, 2, 0xfe, 0xff, b'{'];
    let got = c
        .call(
            &Origin("某台机器".into()),
            &Op("echo".into()),
            Body(payload.clone()),
            budget(5_000),
        )
        .await
        .expect("echo 走得通");
    assert_eq!(
        got.0, payload,
        "载荷过了一趟通道就变了 —— 它必须是不透明字节"
    );

    let seen = fake.calls.lock().unwrap();
    assert_eq!(seen.len(), 1, "句柄应当恰好看见一次 call");
    assert_eq!(
        (seen[0].0.as_str(), seen[0].1.as_str(), seen[0].2.as_slice()),
        ("某台机器", "echo", payload.as_slice()),
        "origin / op / 载荷没有原样到句柄手里"
    );
    // `left` 只收紧：句柄拿到的「还剩多少」不会比调用者给的多。
    assert!(
        seen[0].3 <= Duration::from_millis(5_000),
        "句柄拿到的期限（{:?}）比调用者给的还宽 —— 期限在路上被放宽了",
        seen[0].3
    );
}

/// ★ 出口声明（`view`）是 `Call` 帧头上的一格：帧上带了 ⇒ 句柄原样拿到（路由器不解释它）；
/// 没带 ⇒ 头里没有这一格（不写 `null`），句柄拿到 `None`。进程外客户端（文件窗口）不交声明。
#[tokio::test]
async fn a_view_on_the_call_head_reaches_the_handle_verbatim() {
    let (fake, c) = rig();
    c.call(
        &Origin("<local>".into()),
        &Op("echo".into()),
        Body(b"x".to_vec()),
        budget(5_000),
    )
    .await
    .expect("echo 走得通");
    assert_eq!(
        *fake.views.lock().unwrap(),
        vec![None],
        "进程外客户端交了声明"
    );

    let fake = Arc::new(Fake::default());
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    let (host_rd, host_wr) = tokio::io::split(theirs);
    host::serve_with(host_rd, host_wr, fake.clone());
    let (mut rd, mut wr) = tokio::io::split(ours);
    let view = serde_json::json!({"omit": {"record": ["blocks[type=tool_use].input"]}});
    let head = Head::Call {
        id: 1,
        origin: Origin("<local>".into()),
        op: "echo".into(),
        left: Duration::from_secs(5),
        view: Some(view.clone()),
    };
    write_frame(&mut wr, &head, b"x").await.expect("写得出去");
    let (back, body) = read_frame(&mut rd, FRAME).await.expect("读得回来");
    assert_eq!((back, body), (Head::Done { id: 1 }, b"x".to_vec()));
    assert_eq!(
        *fake.views.lock().unwrap(),
        vec![Some(view)],
        "声明没原样到句柄"
    );

    let bare = serde_json::to_string(&Head::Call {
        id: 2,
        origin: Origin("<local>".into()),
        op: "echo".into(),
        left: Duration::from_secs(5),
        view: None,
    })
    .unwrap();
    assert!(!bare.contains("view"), "没声明却写了 view：{bare}");
}

/// 对端错（`Peer{Refused}`）与路由那一跳的传输错（`Hop{第 1 跳}`）**原样**过线，
/// `Refused` 那份不透明体一个字节不改。
#[tokio::test]
async fn peer_and_hop_errors_cross_the_wire_intact() {
    let (_fake, c) = rig();
    let o = Origin("<local>".into());
    let r = c
        .call(&o, &Op("refuse".into()), Body::default(), budget(5_000))
        .await;
    assert_eq!(
        r,
        Err(CallError::Peer {
            why: PeerFault::Refused {
                body: Body(b"\x00not today\xff".to_vec())
            }
        })
    );
    let r = c
        .call(&o, &Op("nowhere".into()), Body::default(), budget(5_000))
        .await;
    assert_eq!(
        r,
        Err(CallError::Hop {
            at: HopId {
                idx: 1,
                tag: "open"
            },
            reach: Reach::NotSent,
            why: HopFault::Unreachable
        })
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  三、subscribe 收得到帧 · credit 是回推不是丢
// ════════════════════════════════════════════════════════════════════════════

/// ★ 订阅收得到帧，**原位有序**；`from`（续传游标）原样到了句柄；以 `Closed{Peer}` 收尾后流结束。
#[tokio::test]
async fn subscribe_receives_frames_in_order_and_from_passes_through() {
    let (_fake, c) = rig();
    let from = Some(Cursor(b"at-7\x00".to_vec()));
    let sub = c.subscribe(
        &Origin("<local>".into()),
        &Kind("three".into()),
        from.clone(),
        16,
    );
    let got: Vec<Item> = tokio::time::timeout(Duration::from_secs(5), sub.collect())
        .await
        .expect("五格应当在期限内收齐");
    let mut want = vec![Item::Seen { from }];
    for seq in 1..=3u64 {
        want.push(Item::Frame {
            seq,
            body: Body(vec![seq as u8, 0xff]),
        });
    }
    want.push(Item::Closed {
        by: By::Peer(Body(b"done".to_vec())),
    });
    assert_eq!(got, want, "订阅流与句柄给的不一致（顺序 / 内容 / 游标）");
}

/// ★ credit 就是回推：给多少取多少，**不多取、不丢**（级 1）。
/// 同一条连接上走一趟 `echo` 往返，当「路由器那一侧该做的已经做了」的栅栏。
///
/// 判据跑在单线程运行时（`#[tokio::test]` 默认）上：路由器的抽流任务只要还能取（credit 没挡住它），
/// 它在就绪队列里；这一趟往返要转好几圈调度（客户端写 → 路由器读 → 句柄答 → 写回 → 客户端读），
/// 每一圈它都会被轮到 ⇒ 往返回来时，多取的那一格已经取了。不按墙钟，机器再忙也一样。
async fn round_trip(c: &Client) {
    let got = c
        .call(
            &Origin("<local>".into()),
            &Op("echo".into()),
            Body(b"fence".to_vec()),
            budget(5_000),
        )
        .await
        .expect("栅栏那一趟 echo 走不通");
    assert_eq!(got.0, b"fence");
}

#[tokio::test]
async fn credit_is_backpressure_not_loss() {
    let (fake, c) = rig();
    let mut sub = c.subscribe(&Origin("<local>".into()), &Kind("endless".into()), None, 2);
    let mut seqs = Vec::new();
    for _ in 0..2 {
        match tokio::time::timeout(Duration::from_secs(5), sub.next()).await {
            Ok(Some(Item::Frame { seq, .. })) => seqs.push(seq),
            other => panic!("应当收到一格帧，实得 {other:?}"),
        }
    }
    // credit 用完：路由器不该再从句柄那一侧取。同一条连接上走一趟往返当栅栏（见 `round_trip`）。
    round_trip(&c).await;
    assert_eq!(
        fake.pulled.load(Ordering::SeqCst),
        2,
        "credit 只给了 2，句柄那一侧却被取了更多 —— 背压没有推回去"
    );
    sub.want(3);
    for _ in 0..3 {
        match tokio::time::timeout(Duration::from_secs(5), sub.next()).await {
            Ok(Some(Item::Frame { seq, .. })) => seqs.push(seq),
            other => panic!("追加 credit 之后应当再收到帧，实得 {other:?}"),
        }
    }
    assert_eq!(
        seqs,
        vec![0, 1, 2, 3, 4],
        "中间少了 / 乱了 —— 丢帧必须说 Gap，而这里一格都不该丢"
    );
    round_trip(&c).await;
    assert_eq!(
        fake.pulled.load(Ordering::SeqCst),
        5,
        "追加 3 之后应当恰好取到 5 格"
    );
    sub.stop();
}

// ════════════════════════════════════════════════════════════════════════════
//  四、期限 · 撤单 · 断线
// ════════════════════════════════════════════════════════════════════════════

/// ★ 期限到了 ⇒ `Hop{at: 第 0 跳 wait, reach: Sent, why: Overrun}`（
/// 一个预算、说得出卡在哪一跳）；按时回来；并且**尽力**补发的撤单真的到了句柄那一侧。
#[tokio::test]
async fn a_passed_deadline_answers_hop_overrun_in_the_05_shape() {
    let (fake, c) = rig();
    let t0 = Instant::now();
    let r = c
        .call(
            &Origin("<local>".into()),
            &Op("hang".into()),
            Body::default(),
            budget(300),
        )
        .await;
    let took = t0.elapsed();
    assert_eq!(
        r,
        Err(CallError::Hop {
            at: HopId {
                idx: 0,
                tag: "wait"
            },
            reach: Reach::Sent,
            why: HopFault::Overrun,
        }),
        "期限到了，错误的形状不是那一格"
    );
    assert!(
        took >= Duration::from_millis(250) && took < Duration::from_secs(3),
        "期限 300ms，实际 {took:?} 才回来 —— 绝对时刻没被执行"
    );
    // 对端撤活是尽力的，但在这条回环上它应当到得了。
    let cancel = {
        let seen = fake.calls.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0].3 <= Duration::from_millis(300),
            "句柄拿到的期限被放宽了"
        );
        seen[0].4.clone()
    };
    tokio::time::timeout(Duration::from_secs(3), cancel.cancelled())
        .await
        .expect("期限到了之后补发的撤单没有到句柄那一侧");

    // 已经过了期限的 Budget：一个字节都不发，`NotSent`。
    let dead = Budget {
        until: Instant::now(),
        cancel: CancelToken::new(),
    };
    let r = c
        .call(
            &Origin("<local>".into()),
            &Op("echo".into()),
            Body::default(),
            dead,
        )
        .await;
    assert_eq!(
        r,
        Err(CallError::Hop {
            at: HopId {
                idx: 0,
                tag: "write"
            },
            reach: Reach::NotSent,
            why: HopFault::Overrun,
        })
    );
    assert_eq!(fake.calls.lock().unwrap().len(), 1, "过期的调用不该到句柄");
}

/// 本地撤单：立即、`Ours{Cancelled}`，**不是**空答案。
#[tokio::test]
async fn a_local_cancel_is_immediate_and_says_cancelled() {
    let (fake, c) = rig();
    let b = budget(10_000);
    let cancel = b.cancel.clone();
    let task = tokio::spawn({
        let c = c.clone();
        async move {
            c.call(
                &Origin("<local>".into()),
                &Op("hang".into()),
                Body::default(),
                b,
            )
            .await
        }
    });
    // 等句柄真的看见了这次调用再撤。
    tokio::time::timeout(Duration::from_secs(3), async {
        while fake.calls.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("调用应当到达句柄");
    let t0 = Instant::now();
    cancel.cancel();
    let r = task.await.expect("任务没炸");
    assert_eq!(r, Err(OursFault::Cancelled.into()));
    assert!(t0.elapsed() < Duration::from_secs(1), "本地撤单不是立即的");
}

/// 连接断了：在飞的 `call` ⇒ `Hop{第 0 跳 read, Unknown, Dropped}`；
/// 订阅流里原位出 `Unseen`，而且**之后不出 `Closed`**。
#[tokio::test]
async fn losing_the_connection_turns_calls_into_dropped_and_subs_into_unseen_not_closed() {
    let (a, b) = tokio::io::duplex(1 << 16);
    let fake: Arc<dyn Backends> = Arc::new(Fake::default());
    let (brd, bwr) = tokio::io::split(b);
    let server = tokio::spawn(router::serve(brd, bwr, Terms { frame: FRAME }, fake));
    let (ard, awr) = tokio::io::split(a);
    let c = Client::over(ard, awr, FRAME);
    let mut sub = c.subscribe(&Origin("<local>".into()), &Kind("endless".into()), None, 1);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), sub.next()).await,
        Ok(Some(Item::Frame { .. }))
    ));
    let pending = tokio::spawn({
        let c = c.clone();
        async move {
            c.call(
                &Origin("<local>".into()),
                &Op("hang".into()),
                Body::default(),
                budget(10_000),
            )
            .await
        }
    });
    tokio::task::yield_now().await;
    server.abort();
    let r = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("断线之后在飞的调用应当马上回来")
        .expect("任务没炸");
    assert!(
        matches!(
            r,
            Err(CallError::Hop {
                reach: Reach::Unknown,
                why: HopFault::Dropped,
                ..
            })
        ),
        "断线时在飞的调用答的是 {r:?}"
    );
    let next = tokio::time::timeout(Duration::from_secs(5), sub.next()).await;
    assert!(
        matches!(
            next,
            Ok(Some(Item::Unseen {
                at: HopId {
                    idx: 0,
                    tag: "read"
                },
                why: HopFault::Dropped
            }))
        ),
        "断线之后订阅流里应当原位出 Unseen，实得 {next:?}"
    );
    let after = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
    assert!(
        after.is_err(),
        "Unseen 之后流又吐了东西（{after:?}）—— `Unseen` 不许被翻成 `Closed`，也不许结束"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  五、生产句柄 · 钥匙 · 绑口 · 线上形状
// ════════════════════════════════════════════════════════════════════════════

/// 生产句柄：没有控制通道的 `origin` ⇒ 立即 `Hop{第 1 跳 open, NotSent, Unreachable}`；
/// `subscribe` 原位说「没有这条流」（`Closed{Peer}`），不装作订阅成功。
#[tokio::test]
async fn the_production_handle_says_unreachable_and_no_such_stream_out_loud() {
    let prod = InboundBackends;
    let r = prod
        .call(
            Origin("判据里不存在的机器·chan".into()),
            Op("files-ls".into()),
            Body(b"{}".to_vec()),
            None,
            Duration::from_secs(1),
            CancelToken::new(),
        )
        .await;
    assert_eq!(
        r,
        Err(CallError::Hop {
            at: HopId {
                idx: 1,
                tag: "open"
            },
            reach: Reach::NotSent,
            why: HopFault::Unreachable
        })
    );
    let items: Vec<Item> = prod
        .subscribe(Origin("<local>".into()), Kind("anything".into()), None)
        .collect()
        .await;
    assert_eq!(items.len(), 1, "生产订阅应当恰好一格就说完");
    assert!(
        matches!(&items[0], Item::Closed { by: By::Peer(_) }),
        "生产订阅没有原位说「没有这条流」：{items:?}"
    );
}

/// 通道里没有监听口：壳里 `chan/` ＋ 通信层 crate 的 `chan/` 两棵，生产段零处绑口、零处拨号（谁连得上由父子管道给）。
///
/// 通道分住两处：壳里 `chan/`（monitor 自己的宿主那几份）＋ 通信层 crate `comms-inward` 的 `chan/`（线上词汇 · 客户端 · 路由器）。
#[test]
fn nothing_in_the_channel_listens() {
    let root = crate::guard_support::repo_root();
    let mut files = Vec::new();
    for (dir, who) in [
        (root.join("src/frontend/shell/src/chan"), "壳"),
        (root.join("src/comms/inward/chan"), "comms-inward"),
    ] {
        for (p, text) in guard_core::scan_tree_excluding(&dir, &["rs"], &[]) {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            files.push((format!("{who}:{name}"), text));
        }
    }
    let mut sorted: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "comms-inward:client.rs",
            "comms-inward:router.rs",
            "comms-inward:wire.rs",
            "壳:host.rs",
            "壳:mod.rs",
            "壳:webview.rs",
        ],
        "通道目录的份数变了 —— 回来重读本条与 `chan/mod.rs` 那张表"
    );
    for (name, text) in &files {
        let prod = guard_core::production_code(text);
        for word in ["TcpListener", "TcpStream", "UnixListener", "NamedPipe"] {
            assert!(
                !guard_core::contains_word(&prod, word),
                "`{name}` 里又有 `{word}` —— 通道是父子管道，不监听、不拨号"
            );
        }
    }
}

/// 线上形状：每一种错误、每一种 `Item` 过一趟线都原样回来；认不出的跳号标签 ⇒ 协议坏了。
#[test]
fn every_error_and_item_shape_round_trips() {
    let mut errs = Vec::new();
    for (i, tag) in HOP_TAGS.into_iter().enumerate() {
        for reach in [Reach::NotSent, Reach::Sent, Reach::Unknown] {
            for why in [HopFault::Unreachable, HopFault::Dropped, HopFault::Overrun] {
                errs.push(CallError::Hop {
                    at: HopId { idx: i as u8, tag },
                    reach,
                    why,
                });
            }
        }
    }
    errs.push(CallError::Peer {
        why: PeerFault::Unsupported,
    });
    errs.push(CallError::Peer {
        why: PeerFault::Refused {
            body: Body(vec![0, 0xff]),
        },
    });
    for why in [OursFault::Cancelled, OursFault::Misuse, OursFault::Broken] {
        errs.push(why.into());
    }
    // 撤了、那台可能还在跑。
    errs.push(CallError::Ours {
        why: OursFault::Cancelled,
        runs_on: true,
    });
    for e in errs {
        let (w, body) = err_to_wire(e.clone());
        let json = serde_json::to_string(&w).unwrap();
        let back = err_from_wire(serde_json::from_str(&json).unwrap(), body);
        assert_eq!(back, e);
    }
    let items = vec![
        Item::Frame {
            seq: 9,
            body: Body(vec![1, 0xff]),
        },
        Item::Gap {
            from_seq: 3,
            to_seq: Some(7),
        },
        // 知道丢了、不知道丢到哪 —— 线上省掉 `to_seq`，读回来仍是 `None`
        Item::Gap {
            from_seq: 8,
            to_seq: None,
        },
        Item::Unseen {
            at: HopId {
                idx: 1,
                tag: "read",
            },
            why: HopFault::Dropped,
        },
        Item::Seen {
            from: Some(Cursor(vec![0, 1])),
        },
        Item::Seen { from: None },
        Item::Closed {
            by: By::Peer(Body(vec![0xfe])),
        },
        Item::Closed {
            by: By::Ours(OursFault::Broken),
        },
    ];
    for i in items {
        let (w, body) = item_to_wire(i.clone());
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(
            item_from_wire(serde_json::from_str(&json).unwrap(), body),
            Some(i)
        );
    }
    // 认不出的跳号标签：不当成新标签收下。
    let (w, body) = err_to_wire(CallError::Hop {
        at: HopId {
            idx: 0,
            tag: "wait",
        },
        reach: Reach::Sent,
        why: HopFault::Overrun,
    });
    let forged = serde_json::to_string(&w)
        .unwrap()
        .replace("\"wait\"", "\"nap\"");
    assert_eq!(
        err_from_wire(serde_json::from_str(&forged).unwrap(), body),
        OursFault::Broken.into()
    );
}

/// 签名对：客户端实现的就是那个 `Comms`（编译期），订阅就是那个 `Sub`。
#[test]
fn the_client_implements_the_05_signature() {
    fn is_comms<C: Comms>() {}
    fn is_sub<S: Sub>() {}
    is_comms::<Client>();
    is_sub::<super::client::Subscription>();
}

/// 回环客户端拿到那台的 `Offer`（经真路由器）之后，本地撤单的结果说清
/// 「那台可能还在跑」：撤不动的 `hang` ⇒ `runs_on: true`；撤得动的 `stall` ⇒ `false`。两侧异源：左边是真撤一次的结果，
/// 右边是合成句柄交出的那份 `Offer` 里的名单。
#[tokio::test]
async fn a_local_cancel_says_the_peer_may_run_on_when_the_offer_says_so() {
    let (fake, c) = rig();
    let origin = Origin("<local>".into());
    let got = c.offer(&origin, budget(5_000)).await.expect("问得到");
    assert_eq!(got, fake.offer(&origin));
    for (op, runs_on) in [("hang", true), ("stall", false)] {
        let before = fake.calls.lock().unwrap().len();
        let b = budget(10_000);
        let cancel = b.cancel.clone();
        let task = tokio::spawn({
            let (c, o) = (c.clone(), origin.clone());
            async move { c.call(&o, &Op(op.into()), Body::default(), b).await }
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while fake.calls.lock().unwrap().len() == before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("调用应当到达句柄");
        cancel.cancel();
        assert_eq!(
            task.await.expect("任务没炸"),
            Err(CallError::Ours {
                why: OursFault::Cancelled,
                runs_on
            }),
            "{op}"
        );
    }
}
