//! 通道的判据 —— 一个**合成的外部前端**（真 TCP 回环 ＋ 真钥匙 ＋ 真路由器）对着一个**合成后端句柄**说话。
//!
//! # 它证明什么（逐条对任务书那五件）
//!
//! | 件 | 判据 |
//! |---|---|
//! | 连得上 | [`a_synthetic_frontend_connects_over_loopback_and_calls_through`] |
//! | 认证不过的被拒 | [`a_wrong_key_is_refused_and_nothing_reaches_the_backend`] ＋ [`a_frame_before_hello_or_silence_is_refused_too`] |
//! | `call` 走得通 | 同第一条（载荷逐字节、含非 UTF-8 字节；`origin`/`op` 原样到句柄） |
//! | `subscribe` 收得到帧 | [`subscribe_receives_frames_in_order_and_from_passes_through`] ＋ [`credit_is_backpressure_not_loss`] |
//! | 期限到了按 `05 §3.3` 的形状报错 | [`a_passed_deadline_answers_hop_overrun_in_the_05_shape`] |
//!
//! # 买不到什么
//!
//! - **不买「真窗口能用」** —— 合成前端与合成句柄都是判据自己造的；接进文件窗口是下一波 F2。
//! - **不买生产句柄的 `subscribe`** —— 生产今天没有流（`chan::host` 头注），这里只钉它「原位说没有」。
//! - 🔴 **「credit 用完就不再取」那一格用的是一段短等待**：它只能证明「那段时间里没多取」，
//!   证明不了「永远不会多取」。它红了一定是真的（多取了），绿了只是这一趟没看见。

use super::client::Client;
use super::dial::dial;
use super::host::{self, start_with, Handoff, InboundBackends};
use super::router::{self, Backends, Terms};
use super::wire::{
    err_from_wire, err_to_wire, item_from_wire, item_to_wire, read_frame, write_frame, Body,
    Budget, By, CallError, CancelToken, Comms, Cursor, Head, HopFault, HopId, Item, Key, Kind,
    Offer, Op, Origin, OursFault, PeerFault, Reach, Sub, HOP_TAGS,
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
}

impl Backends for Fake {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
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

    /// 〔NET2〕合成那台的能力事实：`hang` 撤不动，`stall` 撤得动。
    fn offer(&self, _origin: &Origin) -> Option<Offer> {
        Some(Offer::new(
            vec!["cancel".into(), "hang".into(), "stall".into()],
            vec![],
            vec!["hang".into()],
        ))
    }
}

/// 起一个挂着合成句柄的真通道口（真回环、真钥匙）。
async fn rig(hello_within: Duration) -> (Arc<Fake>, Handoff) {
    let fake = Arc::new(Fake::default());
    let h = start_with(fake.clone(), host::mint_key(), FRAME, hello_within)
        .await
        .expect("回环口绑得上");
    (fake, h)
}

// ════════════════════════════════════════════════════════════════════════════
//  一、连得上 · call 走得通
// ════════════════════════════════════════════════════════════════════════════

/// ★ 合成外部前端经真回环连上、出示钥匙、`call` 一次：载荷**逐字节**回来（含非 UTF-8 字节），
/// `origin` 与 `op` **原样**到了句柄手里（路由器没解释它们）。
#[tokio::test]
async fn a_synthetic_frontend_connects_over_loopback_and_calls_through() {
    let (fake, h) = rig(Duration::from_secs(5)).await;
    assert!(
        h.addr.ip().is_loopback(),
        "通道口绑在了 {} —— 它只许绑回环（`01 §4`：一个对外端口必须仍然成立）",
        h.addr
    );
    let c = dial(&h, budget(5_000)).await.expect("连得上并过认证");
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

/// 对端错（`Peer{Refused}`）与路由那一跳的传输错（`Hop{第 1 跳}`）**原样**过线，
/// `Refused` 那份不透明体一个字节不改。
#[tokio::test]
async fn peer_and_hop_errors_cross_the_wire_intact() {
    let (_fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
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
//  二、认证不过的被拒
// ════════════════════════════════════════════════════════════════════════════

/// ★ 钥匙不对 ⇒ `Peer{Refused}`，而且**一帧业务都没到句柄**。
#[tokio::test]
async fn a_wrong_key_is_refused_and_nothing_reaches_the_backend() {
    let (fake, h) = rig(Duration::from_secs(5)).await;
    let mut wrong = h.clone();
    wrong.key = host::mint_key();
    assert_ne!(wrong.key, h.key, "两把新钥匙撞了 —— 随机源坏了");
    let r = dial(&wrong, budget(5_000)).await;
    assert!(
        matches!(
            r,
            Err(CallError::Peer {
                why: PeerFault::Refused { .. }
            })
        ),
        "错钥匙没有被拒（实得 {:?}）",
        r.as_ref().err()
    );
    // 差一个字节的钥匙也不行（比对不是前缀比对）。
    let mut near = h.clone();
    near.key = Key(format!("{}x", h.key.0));
    assert!(
        dial(&near, budget(5_000)).await.is_err(),
        "多一个字节的钥匙被放进来了"
    );
    assert!(
        fake.calls.lock().unwrap().is_empty(),
        "没过认证的连接让句柄看见了调用"
    );
}

/// 不出示钥匙就发业务帧 / 一言不发 ⇒ 同样被拒，句柄同样什么都没看见。
#[tokio::test]
async fn a_frame_before_hello_or_silence_is_refused_too() {
    let (fake, h) = rig(Duration::from_millis(300)).await;

    // ① 第一帧就是 `Call`。
    let mut s = tokio::net::TcpStream::connect(h.addr)
        .await
        .expect("连得上");
    let call = Head::Call {
        id: 1,
        origin: Origin("<local>".into()),
        op: "echo".into(),
        left: Duration::from_secs(1),
    };
    write_frame(&mut s, &call, b"{}").await.expect("写得出去");
    let (head, _) = read_frame(&mut s, FRAME).await.expect("路由器应当回一帧");
    assert_eq!(head, Head::Denied, "Hello 之前的业务帧没有被拒");

    // ② 一言不发：`hello_within` 之后被关。
    let mut s = tokio::net::TcpStream::connect(h.addr)
        .await
        .expect("连得上");
    let t0 = Instant::now();
    let (head, _) = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut s, FRAME))
        .await
        .expect("路由器在认证等待时长之后应当说话，而不是一直挂着")
        .expect("应当回一帧");
    assert_eq!(head, Head::Denied);
    assert!(
        t0.elapsed() >= Duration::from_millis(250),
        "认证等待时长没按宿主给的值执行（{:?} 就关了）",
        t0.elapsed()
    );
    assert!(fake.calls.lock().unwrap().is_empty());
}

// ════════════════════════════════════════════════════════════════════════════
//  三、subscribe 收得到帧 · credit 是回推不是丢
// ════════════════════════════════════════════════════════════════════════════

/// ★ 订阅收得到帧，**原位有序**；`from`（续传游标）原样到了句柄；以 `Closed{Peer}` 收尾后流结束。
#[tokio::test]
async fn subscribe_receives_frames_in_order_and_from_passes_through() {
    let (_fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
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

/// ★ credit 就是回推：给多少取多少，**不多取、不丢**（`05 §3.3.4` 级 1）。
#[tokio::test]
async fn credit_is_backpressure_not_loss() {
    let (fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
    let mut sub = c.subscribe(&Origin("<local>".into()), &Kind("endless".into()), None, 2);
    let mut seqs = Vec::new();
    for _ in 0..2 {
        match tokio::time::timeout(Duration::from_secs(5), sub.next()).await {
            Ok(Some(Item::Frame { seq, .. })) => seqs.push(seq),
            other => panic!("应当收到一格帧，实得 {other:?}"),
        }
    }
    // credit 用完：路由器不该再从句柄那一侧取（短等待，见头注「买不到」第三条）。
    tokio::time::sleep(Duration::from_millis(150)).await;
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
    tokio::time::sleep(Duration::from_millis(150)).await;
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

/// ★ 期限到了 ⇒ `Hop{at: 第 0 跳 wait, reach: Sent, why: Overrun}`（`05 §3.3.2`：
/// 一个预算、说得出卡在哪一跳）；按时回来；并且**尽力**补发的撤单真的到了句柄那一侧。
#[tokio::test]
async fn a_passed_deadline_answers_hop_overrun_in_the_05_shape() {
    let (fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
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
        "期限到了，错误的形状不是 `05 §3.3` 那一格"
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
    let (fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
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
/// 订阅流里原位出 `Unseen`，而且**之后不出 `Closed`**（`05 §4.5.2`）。
#[tokio::test]
async fn losing_the_connection_turns_calls_into_dropped_and_subs_into_unseen_not_closed() {
    let (a, b) = tokio::io::duplex(1 << 16);
    let key = host::mint_key();
    let fake: Arc<dyn Backends> = Arc::new(Fake::default());
    let terms = Terms {
        key: key.clone(),
        frame: FRAME,
        hello_within: Duration::from_secs(5),
    };
    let server = tokio::spawn(router::serve(b, terms, fake));
    let c = Client::open(a, &key, FRAME, budget(5_000))
        .await
        .expect("过认证");
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

/// 钥匙不进日志：交接件与钥匙的 `Debug` 都不含钥匙内容。
#[test]
fn the_key_never_shows_up_in_debug_output() {
    let key = host::mint_key();
    assert_eq!(key.0.len(), 64, "钥匙应当是 64 位十六进制");
    assert!(key.0.chars().all(|c| c.is_ascii_hexdigit()));
    let h = Handoff {
        addr: "127.0.0.1:1".parse().unwrap(),
        key: key.clone(),
        frame: FRAME,
    };
    for shown in [format!("{key:?}"), format!("{h:?}"), format!("{h:#?}")] {
        assert!(
            !shown.contains(&key.0) && !shown.contains(&key.0[..16]),
            "钥匙出现在了 Debug 输出里：{shown}"
        );
    }
    // 交接件能整份过一次进程边界（F2 会把它写进子进程 stdin）。
    let wire = serde_json::to_string(&h).expect("序列化");
    let back: Handoff = serde_json::from_str(&wire).expect("反序列化");
    assert_eq!((back.addr, back.key, back.frame), (h.addr, h.key, h.frame));
}

/// 钥匙比对：相等才过；前缀 / 多一字节 / 空串一律不过。
#[test]
fn key_matching_is_whole_string_equality() {
    let k = Key("abc123".into());
    assert!(k.matches("abc123"));
    for bad in ["abc12", "abc1234", "", "abc124", "ABC123"] {
        assert!(!k.matches(bad), "`{bad}` 被当成了对的钥匙");
    }
}

/// 绑口只在宿主那一份里、只绑回环、全模块恰好一处（`C5` 的分界在盘上看得见）。
#[test]
fn only_the_host_binds_and_only_to_loopback() {
    let root = crate::guard_support::repo_root();
    let dir = root.join("src/frontend/shell/src/chan");
    let files = guard_core::scan_tree_excluding(&dir, &["rs"], &[]);
    let names: Vec<String> = files
        .iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "client.rs",
            "dial.rs",
            "host.rs",
            "mod.rs",
            "router.rs",
            "webview.rs",
            "wire.rs"
        ],
        "通道目录的份数变了 —— 回来重读本条与 `chan/mod.rs` 那张表\n\
         （〔C4a〕`webview.rs` 是主界面那一跳的宿主：不绑口，下面那条「绑口恰好一处」照样量它）"
    );
    let bind = format!("TcpListener::{}(", "bind");
    for (p, text) in &files {
        let prod = guard_core::production_code(text);
        let n = prod.matches(bind.as_str()).count();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let expect = usize::from(name == "host.rs");
        assert_eq!(n, expect, "`{name}` 里绑口 {n} 处（应当 {expect} 处）");
    }
    let host_src = files
        .iter()
        .find(|(p, _)| p.file_name().is_some_and(|n| n == "host.rs"))
        .map(|(_, t)| t.clone())
        .expect("host.rs 在上面那张名单里");
    guard_core::find_pinned(
        &guard_core::production_code(&host_src),
        &format!("TcpListener::{}((Ipv4Addr::LOCALHOST, 0))", "bind"),
    )
    .expect("host.rs 里那一处绑口不是「回环 ＋ 内核挑口」");
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
    // 〔NET2 · additive〕撤了、那台可能还在跑。
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
        // 〔RENDER2 · `99 §2.1` ㉓①〕知道丢了、不知道丢到哪 —— 线上省掉 `to_seq`，读回来仍是 `None`
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

/// 签名对 `05 §3.3.0`：客户端实现的就是那个 `Comms`（编译期），订阅就是那个 `Sub`。
#[test]
fn the_client_implements_the_05_signature() {
    fn is_comms<C: Comms>() {}
    fn is_sub<S: Sub>() {}
    is_comms::<Client>();
    is_sub::<super::client::Subscription>();
}

/// 〔NET2 · 主会话 09-27 裁 C · `设计/05 §3.3.3`〕回环客户端拿到那台的 `Offer`（经真路由器）之后，本地撤单的结果说清
/// 「那台可能还在跑」：撤不动的 `hang` ⇒ `runs_on: true`；撤得动的 `stall` ⇒ `false`。两侧异源：左边是真撤一次的结果，
/// 右边是合成句柄交出的那份 `Offer` 里的名单。
#[tokio::test]
async fn a_local_cancel_says_the_peer_may_run_on_when_the_offer_says_so() {
    let (fake, h) = rig(Duration::from_secs(5)).await;
    let c = dial(&h, budget(5_000)).await.expect("连得上");
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
