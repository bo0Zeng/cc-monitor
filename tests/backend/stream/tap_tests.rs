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
