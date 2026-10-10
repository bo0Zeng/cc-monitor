//! `tap.rs` 的 hub（帧怎么折、归哪个运行住 `run_route_tests.rs`；设计住仓外；出处）。
//!
//! 经真中转走一遍的那几条（T1 / T2）住 `relay/host_tests.rs`（它们要中转的门与夹具）；本文件只管 hub 自己。

use super::*;
use crate::relay::{TapBody, TapEvent, TapPort};

fn ev(n: u64) -> TapEvent {
    TapEvent {
        stream: "s".into(),
        owner: String::new(),
        resp: 7,
        n,
        body: TapBody::Data(format!("{{\"i\":{n}}}")),
    }
}

/// 没人连着 ⇒ `offer` 当场答「没收」（不阻塞、不攒）；接上之后收得到。
#[test]
fn offer_refuses_when_nobody_is_attached_and_accepts_once_someone_is() {
    let hub = TapHub::default();
    assert!(!hub.offer(ev(0)), "没人连着却答收了");
    let mut rx = hub.attach();
    assert!(hub.offer(ev(1)));
    assert_eq!(rx.try_recv().ok(), Some(ev(1)));
}

/// 多客户：每条连着的流各收一份；走了的那条摘掉，剩下的照收。
#[test]
fn every_attached_stream_gets_its_own_copy_and_a_gone_one_is_dropped() {
    let hub = TapHub::default();
    let mut a = hub.attach();
    let mut b = hub.attach();
    assert!(hub.offer(ev(0)));
    assert_eq!(a.try_recv().ok(), Some(ev(0)), "第一条没收到");
    assert_eq!(
        b.try_recv().ok(),
        Some(ev(0)),
        "第二条没收到 —— 扇出只交了一条"
    );
    drop(a);
    assert!(hub.offer(ev(1)), "一条走了，剩下那条却不收了");
    assert_eq!(b.try_recv().ok(), Some(ev(1)));
    assert_eq!(
        hub.current.lock().unwrap().len(),
        1,
        "走了的那条没摘掉 —— hub 会越攒越多"
    );
}

/// 满了 ⇒ 答「没收」，不阻塞；生产容量就是 `TAP_CAPACITY`（第 `TAP_CAPACITY + 1` 件被拒）。
#[test]
fn a_full_channel_refuses_without_blocking_at_exactly_the_capacity() {
    let hub = TapHub::default();
    let _rx = hub.attach();
    for i in 0..TAP_CAPACITY as u64 {
        assert!(hub.offer(ev(i)), "第 {i} 件就被拒了");
    }
    assert!(!hub.offer(ev(TAP_CAPACITY as u64)), "超过容量还答收了");
}

/// 丢了的件按原因数：没人连着 · 某条连接的通道满了（每条各算）；每种第 1、2、4… 次说一行。
#[test]
fn every_dropped_event_is_counted_by_why_and_spoken_at_powers_of_two() {
    let hub = TapHub::default();
    let said = crate::stream::run_route::tests::heard(|| {
        for i in 0..3 {
            assert!(!hub.offer(ev(i)));
        }
        let _a = hub.attach_bounded(1);
        let _b = hub.attach_bounded(2);
        for i in 0..4 {
            hub.offer(ev(i));
        }
    });
    // 一条容量 1、一条容量 2：4 件里前者丢 3、后者丢 2。
    let lost = |c: &std::sync::atomic::AtomicU64| c.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!((lost(&hub.unheard), lost(&hub.full)), (3, 5));
    let unheard = said.iter().filter(|l| l.contains("没有流连接")).count();
    let full = said.iter().filter(|l| l.contains("通道满了")).count();
    assert_eq!((unheard, full), (2, 3), "出声的节奏不对：{said:#?}");
}

/// 读数（不是判据）：十几路同时在流时 `TAP_CAPACITY` 那条通道的占用。
///
/// 真的那几段：hub（生产容量）→ 流归位（生产的协议面）→ 取帧；写者用一个模型代替（它住 `main.rs`、本 crate 够不着）：
/// 帧按 wire 一行的字节数走一条限速的链路，另可在中途「被内容帧占住」停一段。每路按给定速率、成簇地交 Anthropic 形的事件
/// （一路主运行 ＋ 其余子运行）。打印每种情形的峰值占用与丢了几件。
/// 跑法：`cargo test --lib -- --ignored --nocapture stream::tap::tests::tap_capacity_reading`
#[test]
#[ignore = "读数不是判据：十几路并发时 tap 通道的占用；跑法见头注"]
fn tap_capacity_reading_with_a_dozen_concurrent_streams() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn sse(i: usize, k: usize) -> String {
        match k {
            0 => format!(r#"{{"type":"message_start","message":{{"id":"msg_{i}","model":"m"}}}}"#),
            1 => r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#.into(),
            _ => r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"一小段思考，几个词"}}"#.into(),
        }
    }

    // (情形, 路数, 每路每秒几件, 一簇几件, 链路字节/秒, 写者被内容帧占住多久)
    let cases: &[(&str, usize, u64, usize, u64, u64)] = &[
        (
            "18 路 · 50 件/秒 · 不成簇 · 10 MB/s",
            18,
            50,
            1,
            10_000_000,
            0,
        ),
        (
            "18 路 · 50 件/秒 · 5 件一簇 · 1 MB/s",
            18,
            50,
            5,
            1_000_000,
            0,
        ),
        (
            "18 路 · 50 件/秒 · 5 件一簇 · 10 MB/s · 写者停 300 ms",
            18,
            50,
            5,
            10_000_000,
            300,
        ),
        (
            "18 路 · 100 件/秒 · 10 件一簇 · 1 MB/s · 写者停 300 ms",
            18,
            100,
            10,
            1_000_000,
            300,
        ),
        (
            "12 路 · 50 件/秒 · 5 件一簇 · 10 MB/s · 写者停 600 ms",
            12,
            50,
            5,
            10_000_000,
            600,
        ),
    ];
    for &(label, streams, per_sec, clump, rate, stall_ms) in cases {
        let hub = Arc::new(TapHub::default());
        let rx = hub.attach();
        let mut tap_rx = attach_rx(rx, Arc::new(crate::observe::runs::RunBook::default()));
        let peak = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let run_for = Duration::from_millis(1500);

        let consumer = {
            let done = done.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move {
                    let t0 = Instant::now();
                    let mut stalled = stall_ms == 0;
                    let mut owed = 0u64;
                    loop {
                        if !stalled && t0.elapsed() >= Duration::from_millis(400) {
                            tokio::time::sleep(Duration::from_millis(stall_ms)).await;
                            stalled = true;
                        }
                        let next =
                            tokio::time::timeout(Duration::from_millis(50), tap_rx.next()).await;
                        match next {
                            Ok(Some(f)) => {
                                owed += crate::stream::wire::to_line(&f)
                                    .map(|l| l.len() as u64)
                                    .unwrap_or(0);
                                if owed >= 4096 {
                                    tokio::time::sleep(Duration::from_micros(
                                        owed * 1_000_000 / rate,
                                    ))
                                    .await;
                                    owed = 0;
                                }
                            }
                            Ok(None) => break,
                            Err(_) if done.load(Ordering::Relaxed) => break,
                            Err(_) => {}
                        }
                    }
                });
            })
        };

        let producers: Vec<_> = (0..streams)
            .map(|s| {
                let hub = hub.clone();
                let peak = peak.clone();
                std::thread::spawn(move || {
                    let gap = Duration::from_micros(1_000_000 * clump as u64 / per_sec);
                    let t0 = Instant::now();
                    // 错开起步，免得每一簇都对齐。
                    std::thread::sleep(gap * s as u32 / streams as u32);
                    let mut n = 0u64;
                    while t0.elapsed() < run_for {
                        for _ in 0..clump {
                            hub.offer(TapEvent {
                                stream: "s-main".into(),
                                owner: if s == 0 {
                                    String::new()
                                } else {
                                    format!("agent-{s}")
                                },
                                resp: s as u64,
                                n,
                                body: TapBody::Data(sse(s, n as usize)),
                            });
                            n += 1;
                            let used = hub
                                .current
                                .lock()
                                .unwrap()
                                .first()
                                .map(|tx| TAP_CAPACITY - tx.capacity())
                                .unwrap_or(0);
                            peak.fetch_max(used, Ordering::Relaxed);
                        }
                        std::thread::sleep(gap);
                    }
                })
            })
            .collect();
        for p in producers {
            p.join().unwrap();
        }
        done.store(true, Ordering::Relaxed);
        let lost = hub.full.load(Ordering::Relaxed);
        drop(hub);
        consumer.join().unwrap();
        let absorbs_ms = TAP_CAPACITY as u64 * 1000 / (streams as u64 * per_sec);
        eprintln!(
            "[读数] {label}：峰值占用 {}/{TAP_CAPACITY} · 丢 {lost} 件 · 这一速率下通道能吸收写者停 ≈{absorbs_ms} ms",
            peak.load(Ordering::Relaxed)
        );
    }
}

/// ★ 订阅计划：某个工作区的计划变了 ⇒ 这条流连接推一帧 `changed {plan, key: 工作区, rev, body: {needs}}`；没再变 ⇒ 不再推。
#[test]
fn a_plan_change_becomes_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let (_ev_tx, ev_rx) = tokio::sync::mpsc::channel::<TapEvent>(4);
        let mut t = attach_rx(
            ev_rx,
            std::sync::Arc::new(crate::observe::runs::RunBook::default()),
        );
        let tx = tokio::sync::broadcast::channel::<crate::plan::watch::Change>(4).0;
        t.plan = Some(tx.subscribe());
        tx.send(("/w".to_string(), "r2".to_string(), 3)).unwrap();
        let f = tokio::time::timeout(std::time::Duration::from_secs(5), t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed {
                topic: Topic::Plan,
                key,
                rev,
                body,
            }) => assert_eq!(
                (key.as_deref(), rev.as_deref(), body),
                (
                    Some("/w"),
                    Some("r2"),
                    Some(serde_json::json!({"needs": 3}))
                )
            ),
            other => panic!("{other:?}"),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err()
        );
    });
}

/// ★ 订阅额度：额度账那条通道响一下 ⇒ 这条流连接推一帧 `changed {quota}`（客户端去重拉 `quota-read`）；
/// 没订那条通道的连接永远不推。
#[test]
fn a_ring_on_the_quota_bell_becomes_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let (_ev_tx, ev_rx) = tokio::sync::mpsc::channel::<TapEvent>(4);
        let mut t = attach_rx(
            ev_rx,
            std::sync::Arc::new(crate::observe::runs::RunBook::default()),
        );
        let bell = tokio::sync::watch::channel::<u64>(0).0;
        t.quota = Some(bell.subscribe());
        bell.send_modify(|n| *n += 1);
        let f = tokio::time::timeout(std::time::Duration::from_secs(5), t.next())
            .await
            .expect("该推一帧");
        assert!(
            matches!(
                f,
                Some(Frame::Changed {
                    topic: Topic::Quota,
                    key: None,
                    rev: None,
                    body: None
                })
            ),
            "{f:?}"
        );
        // 没再响 ⇒ 不再推。
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err()
        );
    });
}

/// 帧里的小成品经这条连接的 `bodies` 现算（生产 ＝ 重问那条命令的处理器；这里喂假的）：轮换那一帧带上那个会话的那一份。
#[test]
fn a_rotation_change_carries_the_body_the_table_asks_for() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let (_ev_tx, ev_rx) = tokio::sync::mpsc::channel::<TapEvent>(4);
        let mut t = attach_rx(
            ev_rx,
            std::sync::Arc::new(crate::observe::runs::RunBook::default()),
        );
        fn fake(topic: Topic, key: Option<&str>) -> Option<serde_json::Value> {
            Some(serde_json::json!({ "topic": topic.name(), "key": key }))
        }
        t.bodies = fake;
        let tx = tokio::sync::broadcast::channel::<String>(4).0;
        t.rotation = Some(tx.subscribe());
        tx.send("s1".to_string()).unwrap();
        let f = tokio::time::timeout(std::time::Duration::from_secs(5), t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed {
                topic: Topic::Rotation,
                key,
                body,
                ..
            }) => assert_eq!(
                (key.as_deref(), body),
                (
                    Some("s1"),
                    Some(serde_json::json!({"topic": "rotation", "key": "s1"}))
                )
            ),
            other => panic!("{other:?}"),
        }
    });
}
