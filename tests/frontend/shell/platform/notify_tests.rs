//! Linux 系统通知那一条长连接（`platform/notify.rs::linux::Pool`）：发 N 条只连一次；一条线程按通知 id 分发关掉 / 点了；
//! 连接真断了才扔掉、下一条重连，扔掉时收掉旧连接（旧那条收信线程随之退）；通知服务只是回了一个错 ⇒ 照旧用这条。
//! 假总线代替会话总线（判据不碰真桌面）。

use super::linux::{Bus, NotifyFail, OnAction, Pool, Signal};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static OPENS: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
/// 下一条发送怎么坏：0 不坏 · 1 连接断了 · 2 通知服务回了一个错（连接还好）。
static FAIL_NEXT: AtomicUsize = AtomicUsize::new(0);
/// 此刻还阻塞在收信上的线程数（每条连接一条；连接收掉 ⇒ 它退出）。
static READERS: AtomicUsize = AtomicUsize::new(0);
/// 往「总线」里塞信号的那一头（每次连上换一条）。
static FEED: OnceLock<Mutex<Option<Sender<Signal>>>> = OnceLock::new();

struct FakeBus {
    rx: Mutex<Receiver<Signal>>,
    /// 收掉这条连接（丢掉它那一头的发信端 ⇒ 收信那条线程读到头）。
    me: Mutex<Option<Sender<Signal>>>,
    reading: std::sync::atomic::AtomicBool,
}

fn open_fake() -> Result<FakeBus, String> {
    OPENS.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = channel();
    *FEED.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(tx.clone());
    Ok(FakeBus {
        rx: Mutex::new(rx),
        me: Mutex::new(Some(tx)),
        reading: Default::default(),
    })
}

impl Bus for FakeBus {
    fn notify(&self, _title: &str, _body: &str) -> Result<u32, NotifyFail> {
        match FAIL_NEXT.swap(0, Ordering::SeqCst) {
            1 => Err(NotifyFail {
                said: "bus gone".into(),
                dead: true,
            }),
            2 => Err(NotifyFail {
                said: "org.freedesktop.DBus.Error.Failed".into(),
                dead: false,
            }),
            _ => Ok(NEXT_ID.fetch_add(1, Ordering::SeqCst)),
        }
    }
    fn next_signal(&self) -> Option<Signal> {
        if !self.reading.swap(true, Ordering::SeqCst) {
            READERS.fetch_add(1, Ordering::SeqCst);
        }
        let got = self.rx.lock().unwrap().recv().ok();
        if got.is_none() {
            READERS.fetch_sub(1, Ordering::SeqCst);
        }
        got
    }
    fn close(&self) {
        self.me.lock().unwrap().take();
        // 喂信号那一头也是这条连接的发信端：一起丢掉。
        FEED.get().unwrap().lock().unwrap().take();
    }
}

fn feed(sig: Signal) {
    FEED.get()
        .unwrap()
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(sig)
        .unwrap();
}

fn wait_until(what: &str, ok: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ok() {
        assert!(Instant::now() < end, "等了 5s 还没：{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn n_notifications_share_one_connection_and_signals_are_dispatched_by_id() {
    static POOL: Pool<FakeBus> = Pool::new(open_fake);
    let clicked: Arc<Mutex<Vec<(u32, String, Option<String>)>>> = Arc::default();
    let c = clicked.clone();
    let on_action: OnAction = Arc::new(move |id, a, t| {
        c.lock()
            .unwrap()
            .push((id, a.to_string(), t.map(str::to_string)))
    });

    let ids: Vec<u32> = (0..5)
        .map(|i| {
            POOL.send_with(on_action.clone(), &format!("t{i}"), "b")
                .unwrap()
        })
        .collect();
    assert_eq!(OPENS.load(Ordering::SeqCst), 1, "发了 5 条，连了不止一次");
    assert_eq!(POOL.live(), ids);

    // 关掉一条 ⇒ 不再记着它；点了另一条 ⇒ 按它的 id 分发，连同点击前桌面发来的那枚激活令牌；
    // 没令牌的那一下照样分发（令牌是 None）；别人的通知（不是我们发的 id）不理。
    feed(Signal::Closed(ids[1]));
    wait_until("关掉那一条从名单里去掉", || {
        !POOL.live().contains(&ids[1])
    });
    feed(Signal::Token(ids[2], "tok-2".into()));
    feed(Signal::Action(ids[2], "default".into()));
    feed(Signal::Action(ids[3], "default".into()));
    feed(Signal::Token(9999, "tok-x".into()));
    feed(Signal::Action(9999, "default".into()));
    wait_until("点了那两条分发到", || {
        clicked.lock().unwrap().len() >= 2
    });
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        *clicked.lock().unwrap(),
        vec![
            (ids[2], "default".to_string(), Some("tok-2".to_string())),
            (ids[3], "default".to_string(), None),
        ]
    );

    wait_until("那一条收信线程在读", || {
        READERS.load(Ordering::SeqCst) == 1
    });
    // 通知服务回了一个错（连接还好）⇒ 这一条报错，连接照旧用，不重连、不多起线程。
    FAIL_NEXT.store(2, Ordering::SeqCst);
    assert!(POOL.send_with(on_action.clone(), "x", "y").is_err());
    POOL.send_with(on_action.clone(), "x", "y").unwrap();
    assert_eq!(OPENS.load(Ordering::SeqCst), 1, "服务回个错就重连了");
    // 连接断了 ⇒ 这一条报错、收掉旧连接（旧那条线程退出），下一条重连一次；收信线程始终只有一条。
    FAIL_NEXT.store(1, Ordering::SeqCst);
    assert!(POOL.send_with(on_action.clone(), "x", "y").is_err());
    wait_until("旧连接那条收信线程退出", || {
        READERS.load(Ordering::SeqCst) == 0
    });
    POOL.send_with(on_action, "x", "y").unwrap();
    assert_eq!(OPENS.load(Ordering::SeqCst), 2, "坏了之后该重连恰好一次");
    wait_until("新连接那条收信线程在读", || {
        READERS.load(Ordering::SeqCst) == 1
    });
}
