//! 〔HX2〕`platform/lock.rs` 的判据：跨进程锁（锁目录）。
//!
//! 要求住址：题面 HX2 逐字「后端自有状态文件跨进程锁（`flock` 一类，Windows 对应）」；`INVARIANTS §41.6` 第四层
//! （后端自己的状态文件：动词只许建那一层目录 · 原子挪 · 删自己的临时文件 —— 这把锁一个写动词都不加）。
//! 审计 `E-compat.md` §E6（「skill 装记录只有进程内锁」）· E14（资产目录首建两个 id）。
//!
//! ⚠ 测的是同一进程里两个线程各开一次描述：`flock` 锁在打开文件描述上，这与两个进程是同一种互斥（内核同一张表）。
//! 限期只在判据里（生产那一侧阻塞等、不掐表）。

use super::hold;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

/// 一个本判据独占的临时目录（落地即删）。
struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ccm-hx2-lock-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 等「拿不到」的限期：没有锁的话另一边是微秒级拿到。
const HELD_OFF: Duration = Duration::from_millis(400);

/// 🔴 L1：持锁时另一个线程拿**同一个目录**的锁，限期内拿不到；放了之后拿得到。换一个目录互不相干（正控）。
#[test]
fn hx2_a_held_dir_lock_keeps_the_next_holder_waiting_until_it_is_dropped() {
    let a = Dir::new("a");
    let b = Dir::new("b");
    let held = hold(a.path()).expect("拿不到第一把");

    let (tx, rx) = mpsc::channel();
    let dir = a.path().to_path_buf();
    let waiter = std::thread::spawn(move || {
        let _g = hold(&dir).expect("第二把拿不到");
        tx.send(()).unwrap();
    });
    assert!(
        rx.recv_timeout(HELD_OFF).is_err(),
        "第一把还没放，第二个线程就拿到了同一个目录的锁"
    );

    // 正控：另一个目录不受这把锁影响。
    let (tx2, rx2) = mpsc::channel();
    let other = b.path().to_path_buf();
    std::thread::spawn(move || {
        let _g = hold(&other).expect("另一个目录拿不到");
        tx2.send(()).unwrap();
    });
    rx2.recv_timeout(Duration::from_secs(5))
        .expect("另一个目录被连坐了");

    drop(held);
    rx.recv_timeout(Duration::from_secs(5))
        .expect("放了锁，第二个线程还是没拿到");
    waiter.join().unwrap();
}

/// 目录不在 ⇒ 如实报错（不替它建：`platform/` 一个写动词都不许有）。
#[test]
fn hx2_locking_a_missing_dir_is_an_error_not_a_creation() {
    let a = Dir::new("missing");
    let gone = a.path().join("nope");
    assert!(hold(&gone).is_err());
    assert!(!gone.exists(), "拿锁时把目录建出来了");
}
