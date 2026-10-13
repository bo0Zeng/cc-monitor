//! 客户端：一次 `call` 三段各自的超时答什么 `reach`、本地撤单、断线时在飞的与订阅各得到什么、对面越过 credit。
//! 对面是这里手写的帧（扮路由器），不起真路由器，结局逐帧可控。
use super::*;
use crate::chan::wire::{
    Body, Budget, CancelToken, Comms, Offer, Sub, WireErr, WireItem, WITHDRAW_OP,
};
use futures::StreamExt as _;
use std::time::{Duration, Instant};
use tokio::io::{duplex, DuplexStream};

const CAP: usize = 64 * 1024;
/// 等一件事发生的兜底（不判快慢）。
const WAIT: Duration = guard_core::wall::CEILING;

fn budget(d: Duration) -> Budget {
    Budget {
        until: Instant::now() + d,
        cancel: CancelToken::new(),
    }
}

/// 起一个客户端：返回它与「路由器那一端」。
fn start() -> (Client, DuplexStream) {
    let (ours, theirs) = duplex(CAP);
    let (rd, wr) = tokio::io::split(ours);
    (Client::over(rd, wr, CAP), theirs)
}

async fn recv(r: &mut DuplexStream) -> (Head, Vec<u8>) {
    tokio::time::timeout(WAIT, read_frame(r, CAP))
        .await
        .expect("客户端该发一帧")
        .unwrap()
}

fn op(s: &str) -> Op {
    Op(s.into())
}

/// 一问一答：路由器那一端照编号回 `Done`，调用方拿到原样的体；回 `Failed{Refused}` ⇒ 对端拒、体原样。
#[tokio::test]
async fn a_call_gets_the_answer_for_its_own_id() {
    let (c, mut r) = start();
    let call = tokio::spawn({
        let c = c.clone();
        async move {
            c.call(
                &Origin::local(),
                &op("ping"),
                Body(b"q".to_vec()),
                budget(WAIT),
            )
            .await
        }
    });
    let (h, body) = recv(&mut r).await;
    let Head::Call {
        id, op: name, view, ..
    } = h
    else {
        panic!("{h:?}")
    };
    assert_eq!(
        (name.as_str(), view, body.as_slice()),
        ("ping", None, &b"q"[..])
    );
    write_frame(&mut r, &Head::Done { id }, b"pong")
        .await
        .unwrap();
    assert_eq!(call.await.unwrap(), Ok(Body(b"pong".to_vec())));

    let call = tokio::spawn({
        let c = c.clone();
        async move {
            c.call(&Origin::local(), &op("kill"), Body::default(), budget(WAIT))
                .await
        }
    });
    let (Head::Call { id, .. }, _) = recv(&mut r).await else {
        panic!()
    };
    write_frame(
        &mut r,
        &Head::Failed {
            id,
            err: WireErr::Refused,
        },
        b"why",
    )
    .await
    .unwrap();
    assert_eq!(
        call.await.unwrap(),
        Err(CallError::Peer {
            why: crate::chan::wire::PeerFault::Refused {
                body: Body(b"why".to_vec())
            }
        })
    );
}

/// 发之前就过期 ⇒ 第 0 跳 write、没发出；发之前就撤 ⇒ 本侧撤单。两种都一个字节不写。
#[tokio::test]
async fn a_spent_or_cancelled_budget_sends_nothing() {
    let (c, mut r) = start();
    let spent = Budget {
        until: Instant::now() - Duration::from_millis(1),
        cancel: CancelToken::new(),
    };
    assert_eq!(
        c.call(&Origin::local(), &op("ping"), Body::default(), spent)
            .await,
        Err(hop(0, "write", Reach::NotSent, HopFault::Overrun))
    );
    let b = budget(WAIT);
    b.cancel.cancel();
    assert_eq!(
        c.call(&Origin::local(), &op("ping"), Body::default(), b)
            .await,
        Err(OursFault::Cancelled.into())
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), read_frame(&mut r, CAP))
            .await
            .is_err(),
        "什么都不该写出去"
    );
}

/// 写完了、答案没在期限里回来 ⇒ 第 0 跳 wait、已发出，并尽力补发一帧撤单。
#[tokio::test]
async fn an_unanswered_call_overruns_on_wait_and_posts_a_cancel() {
    let (c, mut r) = start();
    let (here, slow) = (Origin::local(), op("slow"));
    let res = c.call(
        &here,
        &slow,
        Body::default(),
        budget(Duration::from_millis(80)),
    );
    let (res, (h, _)) = tokio::join!(res, recv(&mut r));
    let Head::Call { id, .. } = h else {
        panic!("{h:?}")
    };
    assert_eq!(res, Err(hop(0, "wait", Reach::Sent, HopFault::Overrun)));
    assert_eq!(recv(&mut r).await.0, Head::Cancel { id });
}

/// 发出之后本地撤：问过那台、它不认撤这一条 ⇒ 结果里说「那台可能还在跑」（`runs_on`）；认撤 ⇒ 不说。
#[tokio::test]
async fn a_withdrawn_call_says_whether_the_peer_may_still_run_it() {
    let (c, mut r) = start();
    let offer = Offer::new(
        vec![WITHDRAW_OP.into(), "ping".into(), "put".into()],
        Vec::new(),
        vec!["put".into()],
    );
    let ask = tokio::spawn({
        let c = c.clone();
        async move { c.offer(&Origin::local(), budget(WAIT)).await }
    });
    let (Head::OfferOf { id, .. }, _) = recv(&mut r).await else {
        panic!()
    };
    write_frame(
        &mut r,
        &Head::Done { id },
        &serde_json::to_vec(&Some(offer.clone())).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(ask.await.unwrap(), Ok(Some(offer)));

    for (name, runs_on) in [("put", true), ("ping", false)] {
        let b = budget(WAIT);
        let cancel = b.cancel.clone();
        let call = tokio::spawn({
            let c = c.clone();
            async move {
                c.call(&Origin::local(), &op(name), Body::default(), b)
                    .await
            }
        });
        let (Head::Call { id, .. }, _) = recv(&mut r).await else {
            panic!()
        };
        cancel.cancel();
        assert_eq!(
            call.await.unwrap(),
            Err(CallError::Ours {
                why: OursFault::Cancelled,
                runs_on
            }),
            "{name}"
        );
        assert_eq!(recv(&mut r).await.0, Head::Cancel { id });
    }
}

/// 连接断了：在飞的 `call` ⇒ 第 0 跳 read、发没发到拿不准、断了；订阅原位出 `Unseen`（不是 `Closed`）；
/// 之后新起的订阅第一格就是 `Unseen`，新的 `call` 是没发出。
#[tokio::test]
async fn a_dropped_connection_answers_everyone_in_place() {
    let (c, mut r) = start();
    let mut sub = c.subscribe(&Origin::local(), &Kind("k".into()), None, 4);
    let call = tokio::spawn({
        let c = c.clone();
        async move {
            c.call(&Origin::local(), &op("ping"), Body::default(), budget(WAIT))
                .await
        }
    });
    // 两帧都到了路由器那一端，再断。
    let mut seen = 0;
    while seen < 2 {
        recv(&mut r).await;
        seen += 1;
    }
    drop(r);
    assert_eq!(
        call.await.unwrap(),
        Err(hop(0, "read", Reach::Unknown, HopFault::Dropped))
    );
    let first = tokio::time::timeout(WAIT, sub.next()).await.unwrap();
    assert_eq!(
        first,
        Some(Item::Unseen {
            at: HopId {
                idx: 0,
                tag: "read"
            },
            why: HopFault::Dropped
        })
    );
    let mut late = c.subscribe(&Origin::local(), &Kind("k".into()), None, 1);
    assert_eq!(
        tokio::time::timeout(WAIT, late.next()).await.unwrap(),
        Some(Item::Unseen {
            at: HopId {
                idx: 0,
                tag: "open"
            },
            why: HopFault::Dropped
        })
    );
    assert_eq!(
        c.call(&Origin::local(), &op("ping"), Body::default(), budget(WAIT))
            .await,
        Err(hop(0, "open", Reach::NotSent, HopFault::Dropped))
    );
}

/// 对面越过 credit ⇒ 流里原位出 `Closed{Ours(Broken)}`，这条订阅到此为止；`want` 补的 credit 照发 `Want` 帧。
#[tokio::test]
async fn a_peer_that_overruns_the_credit_closes_the_subscription_in_place() {
    let (c, mut r) = start();
    let mut sub = c.subscribe(&Origin::local(), &Kind("k".into()), None, 1);
    let (Head::Subscribe { id, want, .. }, _) = recv(&mut r).await else {
        panic!()
    };
    assert_eq!(want, 1);
    for seq in [1, 2] {
        write_frame(
            &mut r,
            &Head::Next {
                id,
                item: WireItem::Frame { seq },
            },
            b"x",
        )
        .await
        .unwrap();
    }
    assert_eq!(
        tokio::time::timeout(WAIT, sub.next()).await.unwrap(),
        Some(Item::Frame {
            seq: 1,
            body: Body(b"x".to_vec())
        })
    );
    assert_eq!(
        tokio::time::timeout(WAIT, sub.next()).await.unwrap(),
        Some(Item::Closed {
            by: By::Ours(OursFault::Broken)
        })
    );
    assert_eq!(tokio::time::timeout(WAIT, sub.next()).await.unwrap(), None);

    let sub2 = c.subscribe(&Origin::local(), &Kind("k".into()), None, 1);
    let (Head::Subscribe { id: id2, .. }, _) = recv(&mut r).await else {
        panic!()
    };
    sub2.want(3);
    assert_eq!(recv(&mut r).await.0, Head::Want { id: id2, more: 3 });
    drop(sub2);
    assert_eq!(recv(&mut r).await.0, Head::Stop { id: id2 });
}
