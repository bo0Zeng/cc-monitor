//! 目录锁的两种拿法（`hold` 等 · `try_hold` 不等）是同一把：同一目录互斥，放了就拿得到。

use crate::dir::{hold, try_hold, LockStage};

/// 拿着 `hold` 的时候 `try_hold` 拿不到；放了就拿得到。
#[test]
fn try_hold_is_refused_while_hold_has_the_dir() {
    let base = test_home::scratch("lock-two-ways");
    let held = hold(&base).expect("拿锁");
    assert!(
        try_hold(&base).expect("抢锁").is_none(),
        "hold 攥着时 try_hold 还拿得到"
    );
    drop(held);
    assert!(
        try_hold(&base).expect("抢锁").is_some(),
        "hold 放了 try_hold 还拿不到"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// `try_hold` 拿着的时候再 `try_hold` 同一目录拿不到（同一线程也一样）；放了就拿得到。
#[test]
fn try_hold_twice_on_one_dir_gets_it_once() {
    let base = test_home::scratch("lock-try-twice");
    let first = try_hold(&base).expect("抢锁").expect("第一个该拿到");
    assert!(
        try_hold(&base).expect("抢锁").is_none(),
        "攥着时第二个还拿得到"
    );
    drop(first);
    assert!(
        try_hold(&base).expect("抢锁").is_some(),
        "放了第二个还拿不到"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 目录不在 ⇒ 卡在打开那一步（不建目录）。
#[test]
fn try_hold_on_a_missing_dir_fails_at_open() {
    let base = test_home::scratch("lock-try-missing");
    let err = try_hold(&base.join("nope"))
        .err()
        .expect("目录不在还拿到了锁");
    assert_eq!(err.stage, LockStage::Open);
    std::fs::remove_dir_all(&base).ok();
}
