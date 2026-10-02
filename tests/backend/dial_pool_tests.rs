//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「链路四条」（按拨号身份复用 · 最后一条走了就断）
//!
//! 核原文：「链路四条」逐字「后端把到同一台远端的所有链路**复用在同一族 SSH 连接上**（按拨号身份：
//! `host · port · user · key_path · host_key_fingerprint · 竞速地址 · 跳板`；默认一条、按需多开至多 3 条；最后一条链路走了连接就断，没有空闲定时器）」
//! —— 同身份复用 · 身份口径逐项 · 按需多开封顶 · 最后一个走了就收，是这一句的四半（那一句原是「复用在一条 SSH 连接上」）。
//! 并发同身份只拨一次、`evict` 只摘自己、关了的不复用：契约里没有逐字，守的是「远端一台一条连接」不被破坏。
//!
//! 用户要求：「今天每台机器只有一条连接可以看情况多开. 智能一点.
//! 这是属于 ssh 优化的部分. 智能多开链接\压缩等等」；复用成一条连接后撞 `MaxSessions` ⇒ **必须有每连接通道上限**。
//! 下面 NT1 那一段（P1–P7）钉的是「什么信号下多开 · 封顶 · 按事件收」，全是**次数 / 条数的相等断言**，不用墙钟。
//!
//! 连接池的记账判据（`dial/pool.rs`）—— 拿一个假连接喂池，**不起 SSH**。
//!
//! 真 SSH 上「同身份只握一次手」那一维由 `tests/evidence/SR1a-link-loopback.py`（真回环 sshd）读数给；
//! 这里钉的是池自己的四件事：同身份复用 · 换身份另拨 · 最后一个用它的走了就收掉 · 关了的不复用。

use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fake {
    closed: AtomicBool,
    budget: Budget,
    held: Mutex<Vec<Arc<Fake>>>,
    /// 停着的「空闲会话」：这里只要它攥着的那一格（与 `Linked::idle_sftp` 里的 `Parked` 同一种攥法）。
    idle: Mutex<Option<Permit>>,
}

impl Conn for Fake {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    fn budget(&self) -> &Budget {
        &self.budget
    }
    fn adopt(&self, other: Arc<Self>) {
        self.held.lock().unwrap().push(other);
    }
    fn reclaim_idle(&self) -> bool {
        self.idle.lock().unwrap().take().is_some()
    }
}

fn fake() -> Fake {
    Fake {
        closed: AtomicBool::new(false),
        budget: Budget::new(),
        held: Mutex::new(Vec::new()),
        idle: Mutex::new(None),
    }
}

/// 拨号计数器 ＋ 一次放置。
///
/// ⚠ 带一道 5 s 的**兜底**（不是判据 —— 判据全是次数 / 条数）：死值验现打过，把「分道」那一格砍掉之后放置不是答错，
/// 而是**永远等下去**（族里没有能放的成员、判准又说不该多开）⇒ 判据挂住而不是红。挂住 ⇒ 这里 panic ⇒ 红。
async fn place(pool: &Pool<Fake>, key: &str, lane: Lane, dials: &AtomicUsize) -> Placed<Fake> {
    let placing = pool.place(key, lane, || async {
        dials.fetch_add(1, Ordering::SeqCst);
        Ok::<Fake, String>(fake())
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), placing)
        .await
        .expect("放置挂住了：该放下的没放下（判准说该等、却没有任何东西会叫醒它）")
        .expect("假拨号不会失败")
}

/// 让出几轮，给别的任务一个机会（判「它还在等」用：等 = 没完成，不是墙钟）。
async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

/// ★ L2 的池那一半：同身份两次 ⇒ 拨一次、第二次是复用、拿到的是**同一个**对象；换身份 ⇒ 再拨一次。
#[tokio::test]
async fn the_same_identity_is_dialed_once_and_shared() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let a = place(&pool, "k1", Lane::Query, &dials).await;
    let b = place(&pool, "k1", Lane::Query, &dials).await;
    assert_eq!(dials.load(Ordering::SeqCst), 1, "同身份拨了不止一次");
    assert_eq!(
        (a.how, b.how),
        (How::Fresh, How::Reused),
        "第一次该是新拨、第二次该是复用"
    );
    assert!(Arc::ptr_eq(&a.conn, &b.conn), "复用拿到的不是同一条连接");
    let c = place(&pool, "k2", Lane::Query, &dials).await;
    assert_eq!(dials.load(Ordering::SeqCst), 2, "换了身份却没另拨");
    assert!(c.how == How::Fresh && !Arc::ptr_eq(&a.conn, &c.conn));
    assert_eq!(pool.live(), 2);
}

/// ★ L3 的池那一半：最后一个用它的走了 ⇒ 族里那一条升不起来，下一次是新拨（没有「空闲多久再关」）。
#[tokio::test]
async fn the_last_user_leaving_drops_the_connection() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let a = place(&pool, "k", Lane::Query, &dials).await;
    let b = place(&pool, "k", Lane::Query, &dials).await;
    drop(a);
    assert_eq!(pool.live(), 1, "还有人拿着，它不该被收");
    drop(b);
    assert_eq!(pool.live(), 0, "最后一个用它的走了，池里还当它活着");
    let c = place(&pool, "k", Lane::Query, &dials).await;
    assert_eq!(c.how, How::Fresh, "收掉之后居然还是复用");
    assert_eq!(dials.load(Ordering::SeqCst), 2);
}

/// 已经关了的连接不复用（`is_closed()` 为真 ⇒ 当它不在）。
#[tokio::test]
async fn a_closed_connection_is_not_reused() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let a = place(&pool, "k", Lane::Query, &dials).await;
    a.conn.closed.store(true, Ordering::SeqCst);
    let b = place(&pool, "k", Lane::Query, &dials).await;
    assert!(
        b.how == How::Fresh && !Arc::ptr_eq(&a.conn, &b.conn),
        "关了的连接被复用了"
    );
    assert_eq!(dials.load(Ordering::SeqCst), 2);
}

/// `evict` 只摘指名的那一条：别人换上的新连接不许被一次过期的摘除带走。摘掉 ≠ 关掉：手里还拿着的照样活着。
#[tokio::test]
async fn evict_only_takes_out_the_one_it_names() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let a = place(&pool, "k", Lane::Query, &dials).await;
    pool.evict("k", &a.conn);
    let b = place(&pool, "k", Lane::Query, &dials).await;
    assert_eq!(b.how, How::Fresh, "摘掉之后还是复用");
    assert!(!a.conn.is_closed(), "摘掉 ≠ 关掉：手里还拿着它的照样能用");
    // 再拿旧的那条去摘 ⇒ 不许动新的。
    pool.evict("k", &a.conn);
    let c = place(&pool, "k", Lane::Query, &dials).await;
    assert!(
        c.how == How::Reused && Arc::ptr_eq(&b.conn, &c.conn),
        "一次过期的摘除把新连接带走了"
    );
}

/// ★ P6：同身份并发冷启动 ⇒ 串行、只拨一次（第二个等第一个拨完、复用）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_opens_of_one_identity_dial_once() {
    let pool: Arc<Pool<Fake>> = Arc::new(Pool::new());
    let dials = Arc::new(AtomicUsize::new(0));
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel::<()>();
    let p1 = Arc::clone(&pool);
    let d1 = Arc::clone(&dials);
    // 第一个：拨号卡在闸上，直到第二个已经在排队。
    let first = tokio::spawn(async move {
        let mut gate = Some(gate_rx);
        p1.place("k", Lane::Stream, move || {
            let d1 = Arc::clone(&d1);
            let g = gate.take();
            async move {
                d1.fetch_add(1, Ordering::SeqCst);
                if let Some(g) = g {
                    let _ = g.await;
                }
                Ok::<Fake, String>(fake())
            }
        })
        .await
    });
    // 让第一个先拿到拨号锁。
    while dials.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let p2 = Arc::clone(&pool);
    let d2 = Arc::clone(&dials);
    let second = tokio::spawn(async move {
        p2.place("k", Lane::Query, || async {
            d2.fetch_add(1, Ordering::SeqCst);
            Ok::<Fake, String>(fake())
        })
        .await
    });
    let _ = gate_tx.send(());
    let a = first.await.unwrap().unwrap();
    let b = second.await.unwrap().unwrap();
    assert_eq!(
        dials.load(Ordering::SeqCst),
        1,
        "同身份并发来拨，拨了不止一次"
    );
    assert!(b.how == How::Reused && Arc::ptr_eq(&a.conn, &b.conn));
}

/// 身份：决定「连到哪、以谁的身份」的那几项在里面；用法 / 命令 / 阶段 / agent 套接字**不在**。
#[test]
fn the_identity_ignores_what_a_link_uses_it_for() {
    let base =
        r#"{"host":"h","port":22,"user":"u","key_path":"/k","host_key_fingerprint":"SHA256:x"}"#;
    let with = |extra: &str| {
        let mut v: serde_json::Value = serde_json::from_str(base).unwrap();
        let e: serde_json::Value = serde_json::from_str(extra).unwrap();
        for (k, x) in e.as_object().unwrap() {
            v[k] = x.clone();
        }
        identity(&serde_json::from_value::<super::super::DialRequest>(v).unwrap())
    };
    let id0 = with("{}");
    // 不进身份的：用法 · 命令 · 阶段 · 探活 · agent 套接字。
    for extra in [
        r#"{"use":"capture","capture":{"max_bytes":1}}"#,
        r#"{"command":"echo hi"}"#,
        r#"{"agent_sock":"/tmp/a.sock"}"#,
    ] {
        assert_eq!(with(extra), id0, "`{extra}` 不该改变连接身份");
    }
    // 进身份的：每一项单独改都换身份。
    for extra in [
        r#"{"host":"h2"}"#,
        r#"{"port":2222}"#,
        r#"{"user":"u2"}"#,
        r#"{"key_path":"/k2"}"#,
        r#"{"host_key_fingerprint":"SHA256:y"}"#,
        r#"{"endpoints":[{"host":"a","port":1}]}"#,
        r#"{"jump":{"host":"j","port":22,"user":"ju","key_path":null,"host_key_fingerprint":null}}"#,
    ] {
        assert_ne!(with(extra), id0, "`{extra}` 该换一个连接身份");
    }
}

// ═══ 一条连接上的通道预算（`Budget`）══════════════════════════════

/// 🔴 **B4**：通道闸 == `SESSION_CHANNEL_CAP`（第 9 条借不到）；传输车道 == `TRANSFER_LANE_CAP`
/// （第 5 趟传输借不到，**而且不占通道格**；此时一条 capture 仍借得到）。读数取自闸本身，不取自常量（异源）。
/// 闸只答「借得到 / 借不到」（`try_take`），等由族那一层做 —— 等的那一半在下面 P3。
#[test]
fn the_budget_refuses_the_ninth_channel_and_the_fifth_transfer_but_not_a_query_behind_them() {
    // 要求里的数：8 格通道、其中传输至多 4 格。
    assert_eq!((SESSION_CHANNEL_CAP, TRANSFER_LANE_CAP), (8, 4));
    let b = Budget::new();
    assert_eq!(b.free(), (8, 4, 8));
    let mut xfers: Vec<Permit> = (0..4)
        .map(|_| b.try_take(Lane::Transfer).expect("前四趟传输该借到"))
        .collect();
    assert_eq!(b.free(), (4, 0, 8));
    assert!(
        b.try_take(Lane::Transfer).is_none(),
        "车道满了第五趟传输居然借到了"
    );
    assert_eq!(b.free(), (4, 0, 8), "借不到车道的传输占了一格通道");
    let mut sessions: Vec<Permit> = (0..4)
        .map(|_| b.try_take(Lane::Query).expect("传输满载时查询该借得到通道"))
        .collect();
    assert_eq!(b.free(), (0, 0, 8));
    assert!(
        b.try_take(Lane::Query).is_none(),
        "8 格通道用满了第九条居然借到了"
    );
    assert!(b.try_take(Lane::Tunnel).is_none(), "隧道不借格");
    drop(sessions.pop());
    assert!(b.try_take(Lane::Query).is_some(), "还了一格之后该借得到");
    drop(xfers.pop());
    assert!(
        b.try_take(Lane::Transfer).is_some(),
        "还了一格车道之后第五趟该借到"
    );
}

/// 长流数：借 `Stream` 那一格 +1、还掉 −1；别的道不动它。
#[test]
fn the_stream_count_follows_stream_permits_only() {
    let b = Budget::new();
    let s1 = b.try_take(Lane::Stream).unwrap();
    let _q = b.try_take(Lane::Query).unwrap();
    let _t = b.try_take(Lane::Transfer).unwrap();
    assert_eq!(b.streams(), 1);
    let s2 = b.try_take(Lane::Stream).unwrap();
    assert_eq!(b.streams(), 2);
    drop(s1);
    drop(s2);
    assert_eq!(b.streams(), 0);
}

// ═══ **按需智能多开**（`NT1.md §2`）════════════════════════════════════

/// ★ P1 ＋ P5（收）：主连接上**有长流** ⇒ 一趟传输 ⇒ 多开一条批量连接（拨号次数 == 2、`Extra(Bulk)`、落在第 2 条）；
/// 随后一条查询落回主连接。传输走了 ⇒ 批量连接**还活着**（被主连接托着，下一趟传输复用它、不再拨）；
/// 长流也走了 ⇒ 两条都没了（`live == 0`）。
#[tokio::test]
async fn a_transfer_next_to_a_stream_gets_its_own_connection_held_by_the_primary() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let stream = place(&pool, "k", Lane::Stream, &dials).await;
    let xfer = place(&pool, "k", Lane::Transfer, &dials).await;
    assert_eq!(
        dials.load(Ordering::SeqCst),
        2,
        "主连接上有长流，传输却没有分道"
    );
    assert_eq!(xfer.how, How::Extra(Why::Bulk));
    assert!(!Arc::ptr_eq(&stream.conn, &xfer.conn));
    let q = place(&pool, "k", Lane::Query, &dials).await;
    assert!(
        q.how == How::Reused && Arc::ptr_eq(&q.conn, &stream.conn),
        "查询该落回主连接（最老的有空格的那条）"
    );
    let bulk = Arc::downgrade(&xfer.conn);
    drop(xfer);
    drop(q);
    assert_eq!(pool.live(), 2, "批量连接没人用了，但它该被主连接托着");
    let again = place(&pool, "k", Lane::Transfer, &dials).await;
    assert!(
        again.how == How::Reused && Arc::ptr_eq(&again.conn, &bulk.upgrade().unwrap()),
        "第二趟传输该复用托着的那条批量连接"
    );
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    drop(again);
    drop(stream);
    assert_eq!(
        pool.live(),
        0,
        "长流走了（主连接没了），托着的批量连接该随之没了"
    );
    assert!(bulk.upgrade().is_none());
}

/// ★ P2（P1 的另一向）：主连接上**没有长流** ⇒ 传输就落在主连接上，不多花一次握手。
#[tokio::test]
async fn a_transfer_without_a_stream_stays_on_the_primary() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let q = place(&pool, "k", Lane::Query, &dials).await;
    let x = place(&pool, "k", Lane::Transfer, &dials).await;
    assert_eq!(dials.load(Ordering::SeqCst), 1);
    assert!(x.how == How::Reused && Arc::ptr_eq(&q.conn, &x.conn));
}

/// ★ P3：交互把主连接的 8 格占满 ⇒ 第 9 条多开一条（`Extra(Full)`）；三条都满（封顶）⇒ 第 25 条**不拨、等**；
/// 还回一格 ⇒ 等着的那一条恰好拿到，拨号次数仍 == 3。
/// 单线程运行时：等着的那一条只在本任务让出时跑 ⇒ `settle()` 之后它必然已经走到「等」那一步（不靠墙钟）。
#[tokio::test]
async fn a_full_family_dials_up_to_the_cap_then_waits_for_a_slot() {
    assert_eq!(MAX_CONNECTIONS_PER_HOST, 3);
    let pool: Arc<Pool<Fake>> = Arc::new(Pool::new());
    let dials = Arc::new(AtomicUsize::new(0));
    let mut held = Vec::new();
    for _ in 0..SESSION_CHANNEL_CAP {
        held.push(place(&pool, "k", Lane::Query, &dials).await);
    }
    assert_eq!(dials.load(Ordering::SeqCst), 1);
    let ninth = place(&pool, "k", Lane::Query, &dials).await;
    assert_eq!(ninth.how, How::Extra(Why::Full));
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    held.push(ninth);
    while held.len() < MAX_CONNECTIONS_PER_HOST * SESSION_CHANNEL_CAP {
        held.push(place(&pool, "k", Lane::Query, &dials).await);
    }
    assert_eq!(dials.load(Ordering::SeqCst), MAX_CONNECTIONS_PER_HOST);
    let p = Arc::clone(&pool);
    let d = Arc::clone(&dials);
    let waiter = tokio::spawn(async move { place(&p, "k", Lane::Query, &d).await.how });
    settle().await;
    assert!(!waiter.is_finished(), "封顶了还放下了（多开了第 4 条？）");
    assert_eq!(
        dials.load(Ordering::SeqCst),
        MAX_CONNECTIONS_PER_HOST,
        "封顶了还在拨"
    );
    drop(held.pop());
    assert_eq!(
        waiter.await.unwrap(),
        How::Reused,
        "还回一格之后等着的那一条该拿到它"
    );
    assert_eq!(dials.load(Ordering::SeqCst), MAX_CONNECTIONS_PER_HOST);
}

/// ★ P4：远端回拒（`MaxSessions`）⇒ 那条连接学到上限 == 此刻在用的格数、空格 == 0、**不摘**（长流还在上面）；
/// 下一条查询落到新连接上（`Extra(Full)`）；在用的那格还回来之后照常可借（上限就是那个数）。
#[tokio::test]
async fn a_remote_refusal_teaches_the_connection_its_cap_without_evicting_it() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let stream = place(&pool, "k", Lane::Stream, &dials).await;
    let q1 = place(&pool, "k", Lane::Query, &dials).await;
    // 第三条被远端拒了：调用方先还掉被拒那一格，再报给闸。
    let refused = place(&pool, "k", Lane::Query, &dials).await;
    drop(refused);
    assert_eq!(
        stream.conn.budget().refused(),
        2,
        "学到的上限该是被拒那一刻在用的格数（长流 ＋ 一条查询）"
    );
    assert_eq!(stream.conn.budget().free(), (0, 4, 2));
    let q2 = place(&pool, "k", Lane::Query, &dials).await;
    assert_eq!(q2.how, How::Extra(Why::Full), "学到上限之后该落到新连接上");
    assert!(!Arc::ptr_eq(&q2.conn, &stream.conn));
    assert!(!stream.conn.is_closed(), "回拒不是连接死了，不许摘 / 关它");
    drop(q1);
    assert_eq!(
        stream.conn.budget().free().0,
        1,
        "在用的那格还回来之后照常可借"
    );
}

/// ★ P7：主连接有长流、批量连接已在而它的车道满了 ⇒ 第 5 趟传输**等**，不再多开第二条批量。
#[tokio::test]
async fn a_second_bulk_connection_is_never_dialed() {
    let pool: Arc<Pool<Fake>> = Arc::new(Pool::new());
    let dials = Arc::new(AtomicUsize::new(0));
    let _stream = place(&pool, "k", Lane::Stream, &dials).await;
    let mut xfers = Vec::new();
    for _ in 0..TRANSFER_LANE_CAP {
        xfers.push(place(&pool, "k", Lane::Transfer, &dials).await);
    }
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    let p = Arc::clone(&pool);
    let d = Arc::clone(&dials);
    let fifth = tokio::spawn(async move { place(&p, "k", Lane::Transfer, &d).await.how });
    settle().await;
    assert!(!fifth.is_finished(), "批量连接的车道满了，第 5 趟却放下了");
    assert_eq!(dials.load(Ordering::SeqCst), 2, "多开了第二条批量连接");
    drop(xfers.pop());
    assert_eq!(fifth.await.unwrap(), How::Reused);
}

/// ★ S1（`NT1.md §3`）：停着的空闲会话**连同它那一格**停在连接里；别的放置借不到格时，池**先挤掉它**、还出那一格，
/// 再放 —— 不因为一个空位多开一条（拨号次数仍 == 1）。
#[tokio::test]
async fn a_parked_idle_session_is_squeezed_out_before_anyone_dials() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let x = place(&pool, "k", Lane::Transfer, &dials).await;
    let conn = Arc::clone(&x.conn);
    *conn.idle.lock().unwrap() = x.permit;
    let mut held = Vec::new();
    for _ in 1..SESSION_CHANNEL_CAP {
        held.push(place(&pool, "k", Lane::Query, &dials).await);
    }
    assert_eq!(
        conn.budget().free().0,
        0,
        "7 条查询 ＋ 1 个停着的会话该占满 8 格"
    );
    let eighth = place(&pool, "k", Lane::Query, &dials).await;
    assert!(
        eighth.how == How::Reused && Arc::ptr_eq(&eighth.conn, &conn),
        "该先挤掉停着的会话、落在这条上"
    );
    assert!(conn.idle.lock().unwrap().is_none(), "停着的会话没被挤掉");
    assert_eq!(dials.load(Ordering::SeqCst), 1, "为了一个空位多开了一条");
}

/// ★ S1 的另一半：找停着的会话只在**传输能放的成员**上找（与放置同一条分道规矩）——
/// 主连接上有长流 ⇒ 候选只有批量连接；没长流 ⇒ 主连接在先。
#[tokio::test]
async fn parked_sessions_are_looked_up_where_a_transfer_may_land() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let q = place(&pool, "k", Lane::Query, &dials).await;
    let c = pool.transfer_candidates("k");
    assert!(
        c.len() == 1 && Arc::ptr_eq(&c[0], &q.conn),
        "没长流 ⇒ 主连接就是候选"
    );
    let s = place(&pool, "k", Lane::Stream, &dials).await;
    assert!(
        pool.transfer_candidates("k").is_empty(),
        "主连接上有长流、还没有批量连接 ⇒ 没有候选"
    );
    let x = place(&pool, "k", Lane::Transfer, &dials).await;
    let c = pool.transfer_candidates("k");
    assert!(c.len() == 1 && Arc::ptr_eq(&c[0], &x.conn) && !Arc::ptr_eq(&c[0], &s.conn));
    assert!(pool.transfer_candidates("另一个身份").is_empty());
}

/// ★ W1（`NT1.md §4`）：在一条连接上等远端回话的那一段**被丢了**（链路被关 ⇒ 任务被 abort）⇒ 这条连接从族里摘掉，
/// 下一条放置拨新的；**等到了**（`done`）⇒ 不动。两向。摘掉 ≠ 关掉：手里还攥着它的照样活着。
#[tokio::test]
async fn an_interrupted_wait_on_a_connection_evicts_it_and_a_finished_one_does_not() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let a = place(&pool, "k", Lane::Stream, &dials).await;
    Watch::new(&pool, "k", &a.conn).done();
    let b = place(&pool, "k", Lane::Query, &dials).await;
    assert!(
        b.how == How::Reused && Arc::ptr_eq(&a.conn, &b.conn),
        "等到了回话却把连接摘了"
    );
    drop(Watch::new(&pool, "k", &a.conn));
    let c = place(&pool, "k", Lane::Query, &dials).await;
    assert_eq!(
        c.how,
        How::Fresh,
        "等回话被打断之后，新的放置还落在那条（可能是黑洞的）连接上"
    );
    assert!(!Arc::ptr_eq(&a.conn, &c.conn));
    assert!(!a.conn.is_closed(), "摘掉 ≠ 关掉");
    assert_eq!(dials.load(Ordering::SeqCst), 2);
}
