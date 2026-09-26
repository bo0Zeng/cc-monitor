//! 〔TAP · V124〕`tap.rs` 的 hub 与帧转换（设计住仓外 `调研/第四波记录/TAP.md §1.1 · §3`；出处 `设计/20 §8`）。
//!
//! 经真中转走一遍的那几条（T1 / T2）住 `relay/host_tests.rs`（它们要中转的门与夹具）；本文件只管 hub 自己。

use super::*;
use crate::relay::{TapBody, TapEvent, TapPort};

fn ev(n: u64) -> TapEvent {
    TapEvent {
        stream: "s".into(),
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

/// 又一条流连接接上 ⇒ 上一条的接收端拿完已有的就是 `None`（不会再有东西进去），新的一条接着收。
#[test]
fn a_new_attach_retires_the_previous_receiver() {
    let hub = TapHub::default();
    let mut old = hub.attach();
    assert!(hub.offer(ev(0)));
    let mut new = hub.attach();
    assert!(hub.offer(ev(1)));
    assert_eq!(old.try_recv().ok(), Some(ev(0)));
    assert!(
        matches!(
            old.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
        ),
        "旧接收端应当已断开"
    );
    assert_eq!(new.try_recv().ok(), Some(ev(1)));
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

/// 事件 → 帧：字段一一照搬（期望是手写的帧值）。
#[test]
fn to_frame_copies_every_field_and_maps_the_end() {
    assert_eq!(
        format!("{:?}", to_frame(ev(3))),
        format!(
            "{:?}",
            Frame::Tap {
                stream: "s".into(),
                resp: 7,
                n: 3,
                data: Some("{\"i\":3}".into()),
                end: None
            }
        )
    );
    for (broken, want) in [(false, TapEnd::Done), (true, TapEnd::Broken)] {
        let f = to_frame(TapEvent {
            stream: "s".into(),
            resp: 1,
            n: 9,
            body: TapBody::End { broken },
        });
        assert_eq!(
            format!("{f:?}"),
            format!(
                "{:?}",
                Frame::Tap {
                    stream: "s".into(),
                    resp: 1,
                    n: 9,
                    data: None,
                    end: Some(want)
                }
            )
        );
    }
}
