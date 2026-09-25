//! 〔SR1a〕连接池的记账判据（`dial/pool.rs`）—— 拿一个假连接喂池，**不起 SSH**。
//!
//! 真 SSH 上「同身份只握一次手」那一维由 `tests/evidence/SR1a-link-loopback.py`（真回环 sshd）读数给；
//! 这里钉的是池自己的四件事：同身份复用 · 换身份另拨 · 最后一个用它的走了就收掉 · 关了的不复用。

use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fake {
    closed: AtomicBool,
}

impl Conn for Fake {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

fn fake() -> Fake {
    Fake {
        closed: AtomicBool::new(false),
    }
}

/// 拨号计数器 ＋ 一次拨号。
async fn get(pool: &Pool<Fake>, key: &str, dials: &AtomicUsize) -> (Arc<Fake>, bool) {
    pool.get(key, || async {
        dials.fetch_add(1, Ordering::SeqCst);
        Ok::<Fake, String>(fake())
    })
    .await
    .expect("假拨号不会失败")
}

/// ★ L2 的池那一半：同身份两次 ⇒ 拨一次、第二次是复用、拿到的是**同一个**对象；换身份 ⇒ 再拨一次。
#[tokio::test]
async fn the_same_identity_is_dialed_once_and_shared() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let (a, a_reused) = get(&pool, "k1", &dials).await;
    let (b, b_reused) = get(&pool, "k1", &dials).await;
    assert_eq!(dials.load(Ordering::SeqCst), 1, "同身份拨了不止一次");
    assert!(!a_reused && b_reused, "第一次该是新拨、第二次该是复用");
    assert!(Arc::ptr_eq(&a, &b), "复用拿到的不是同一条连接");
    let (c, c_reused) = get(&pool, "k2", &dials).await;
    assert_eq!(dials.load(Ordering::SeqCst), 2, "换了身份却没另拨");
    assert!(!c_reused && !Arc::ptr_eq(&a, &c));
    assert_eq!(pool.live(), 2);
}

/// ★ L3 的池那一半：最后一个用它的走了 ⇒ 池里那一格升不起来，下一次是新拨（没有「空闲多久再关」）。
#[tokio::test]
async fn the_last_user_leaving_drops_the_connection() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let (a, _) = get(&pool, "k", &dials).await;
    let (b, _) = get(&pool, "k", &dials).await;
    drop(a);
    assert_eq!(pool.live(), 1, "还有人拿着，它不该被收");
    drop(b);
    assert_eq!(pool.live(), 0, "最后一个用它的走了，池里还当它活着");
    let (_c, reused) = get(&pool, "k", &dials).await;
    assert!(!reused, "收掉之后居然还是复用");
    assert_eq!(dials.load(Ordering::SeqCst), 2);
}

/// 已经关了的连接不复用（`is_closed()` 为真 ⇒ 当它不在）。
#[tokio::test]
async fn a_closed_connection_is_not_reused() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let (a, _) = get(&pool, "k", &dials).await;
    a.closed.store(true, Ordering::SeqCst);
    let (b, reused) = get(&pool, "k", &dials).await;
    assert!(!reused && !Arc::ptr_eq(&a, &b), "关了的连接被复用了");
    assert_eq!(dials.load(Ordering::SeqCst), 2);
}

/// `evict` 只摘还指着**它**的那一格：别人换上的新连接不许被一次过期的摘除带走。
#[tokio::test]
async fn evict_only_takes_out_the_one_it_names() {
    let pool: Pool<Fake> = Pool::new();
    let dials = AtomicUsize::new(0);
    let (a, _) = get(&pool, "k", &dials).await;
    pool.evict("k", &a);
    let (b, reused) = get(&pool, "k", &dials).await;
    assert!(!reused, "摘掉之后还是复用");
    // 再拿旧的那条去摘 ⇒ 不许动新的。
    pool.evict("k", &a);
    let (c, reused2) = get(&pool, "k", &dials).await;
    assert!(
        reused2 && Arc::ptr_eq(&b, &c),
        "一次过期的摘除把新连接带走了"
    );
}

/// 同身份并发来拨 ⇒ 串行、只拨一次（第二个等第一个拨完、复用）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_opens_of_one_identity_dial_once() {
    let pool: Arc<Pool<Fake>> = Arc::new(Pool::new());
    let dials = Arc::new(AtomicUsize::new(0));
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel::<()>();
    let p1 = Arc::clone(&pool);
    let d1 = Arc::clone(&dials);
    // 第一个：拨号卡在闸上，直到第二个已经在排队。
    let first = tokio::spawn(async move {
        p1.get("k", || async move {
            d1.fetch_add(1, Ordering::SeqCst);
            let _ = gate_rx.await;
            Ok::<Fake, String>(fake())
        })
        .await
        .map(|(c, _)| c)
    });
    // 让第一个先拿到那一格的锁。
    while dials.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let p2 = Arc::clone(&pool);
    let d2 = Arc::clone(&dials);
    let second = tokio::spawn(async move {
        p2.get("k", || async move {
            d2.fetch_add(1, Ordering::SeqCst);
            Ok::<Fake, String>(fake())
        })
        .await
    });
    let _ = gate_tx.send(());
    let a = first.await.unwrap().unwrap();
    let (b, reused) = second.await.unwrap().unwrap();
    assert_eq!(
        dials.load(Ordering::SeqCst),
        1,
        "同身份并发来拨，拨了不止一次"
    );
    assert!(reused && Arc::ptr_eq(&a, &b));
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
        identity(&super::super::parse_request_value(&v).unwrap())
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
