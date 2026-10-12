//! 路由器：一条连接上的配对 · 期限执行 · 撤单 · credit 背压 · 收尾。对面是这里手写的帧（不经客户端），
//! 句柄是合成的（按 op 名决定立刻答 · 永远不答）。
use super::*;
use crate::chan::wire::{WireErr, WireItem};
use futures::FutureExt as _;
use tokio::io::{duplex, DuplexStream};

const CAP: usize = 64 * 1024;
/// 等一件事发生的兜底（不判快慢）。
const WAIT: Duration = guard_core::wall::CEILING;

/// 合成的句柄：`echo` 立刻把体原样交回（`view` 不空就换成它的 JSON）；`hang` 永远不答、只记下撤单手柄；
/// 订阅交出 `items` 那几格（不以 `Closed` 结尾，看路由器补不补）。
#[derive(Default)]
struct Fake {
    hung: Mutex<Vec<CancelToken>>,
    items: Vec<Item>,
}

impl Backends for Fake {
    fn call(
        &self,
        _origin: Origin,
        op: Op,
        payload: Body,
        view: Option<serde_json::Value>,
        _left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        match op.0.as_str() {
            "echo" => {
                let out = match view {
                    Some(v) => Body(v.to_string().into_bytes()),
                    None => payload,
                };
                async move { Ok(out) }.boxed()
            }
            _ => {
                lock(&self.hung).push(cancel);
                futures::future::pending().boxed()
            }
        }
    }

    fn subscribe(
        &self,
        _origin: Origin,
        _kind: Kind,
        _from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        futures::stream::iter(self.items.clone()).boxed()
    }
}

/// 起一条连接：返回「客户端那一端」与路由器那个任务。
fn start(fake: Arc<Fake>) -> (DuplexStream, tokio::task::JoinHandle<Ended>) {
    let (ours, theirs) = duplex(CAP);
    let (rd, wr) = tokio::io::split(theirs);
    let task = tokio::spawn(serve(rd, wr, Terms { frame: CAP }, fake));
    (ours, task)
}

async fn send(c: &mut DuplexStream, head: Head, body: &[u8]) {
    write_frame(c, &head, body).await.unwrap();
}

async fn recv(c: &mut DuplexStream) -> (Head, Vec<u8>) {
    tokio::time::timeout(WAIT, read_frame(c, CAP))
        .await
        .expect("路由器该回一帧")
        .unwrap()
}

fn call(id: u64, op: &str, left: Duration, view: Option<serde_json::Value>) -> Head {
    Head::Call {
        id,
        origin: Origin::local(),
        op: op.into(),
        left,
        view,
    }
}

/// 一问一答按编号配对；体原样过去原样回来；出口声明原样交给句柄。
#[tokio::test]
async fn a_call_is_answered_under_its_own_id_with_the_body_untouched() {
    let (mut c, _t) = start(Arc::new(Fake::default()));
    send(&mut c, call(41, "echo", WAIT, None), b"\x00raw").await;
    assert_eq!(
        recv(&mut c).await,
        (Head::Done { id: 41 }, b"\x00raw".to_vec())
    );
    send(
        &mut c,
        call(42, "echo", WAIT, Some(serde_json::json!({"only": ["a"]}))),
        b"",
    )
    .await;
    let (h, body) = recv(&mut c).await;
    assert_eq!(h, Head::Done { id: 42 });
    assert_eq!(body, br#"{"only":["a"]}"#);
}

/// 句柄在「还剩多少」里没回来 ⇒ `Hop{第 1 跳 wait, Unknown, Overrun}`，同时拨下它的撤单手柄。
#[tokio::test]
async fn a_backend_that_outlives_the_budget_is_an_overrun_on_hop_one() {
    let fake = Arc::new(Fake::default());
    let (mut c, _t) = start(fake.clone());
    send(
        &mut c,
        call(1, "hang", Duration::from_millis(30), None),
        b"",
    )
    .await;
    let (h, _) = recv(&mut c).await;
    assert_eq!(
        h,
        Head::Failed {
            id: 1,
            err: WireErr::Hop {
                idx: 1,
                tag: "wait".into(),
                reach: Reach::Unknown,
                why: HopFault::Overrun
            }
        }
    );
    assert!(
        lock(&fake.hung)[0].is_cancelled(),
        "超时那一下没拨句柄的撤单手柄"
    );
}

/// 客户端撤单 ⇒ 句柄的手柄被拨下、这一条回 `Ours(Cancelled)`。
#[tokio::test]
async fn a_cancel_from_the_client_reaches_the_backend() {
    let fake = Arc::new(Fake::default());
    let (mut c, _t) = start(fake.clone());
    send(&mut c, call(3, "hang", WAIT, None), b"").await;
    // 句柄已经拿到那一条，才撤。
    for () in guard_core::wall::rounds() {
        if !lock(&fake.hung).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    send(&mut c, Head::Cancel { id: 3 }, b"").await;
    let (h, _) = recv(&mut c).await;
    assert_eq!(
        h,
        Head::Failed {
            id: 3,
            err: WireErr::Ours(OursFault::Cancelled)
        }
    );
    assert!(lock(&fake.hung)[0].is_cancelled());
}

/// 同一个编号还在飞又来一次 ⇒ 连接按协议坏了收掉，在飞的那一条一并撤。
#[tokio::test]
async fn a_reused_id_in_flight_breaks_the_connection_and_cancels_what_was_running() {
    let fake = Arc::new(Fake::default());
    let (mut c, t) = start(fake.clone());
    send(&mut c, call(9, "hang", WAIT, None), b"").await;
    send(&mut c, call(9, "hang", WAIT, None), b"").await;
    let ended = tokio::time::timeout(WAIT, t).await.unwrap().unwrap();
    assert!(matches!(ended, Ended::Broken(_)), "{ended:?}");
    assert!(lock(&fake.hung).iter().all(CancelToken::is_cancelled));
}

/// 客户端发来只该由路由器发的头 ⇒ `Broken`；客户端干净地走了 ⇒ `Left`。
#[tokio::test]
async fn how_a_connection_ends() {
    let (mut c, t) = start(Arc::new(Fake::default()));
    send(&mut c, Head::Done { id: 1 }, b"").await;
    assert!(matches!(
        tokio::time::timeout(WAIT, t).await.unwrap().unwrap(),
        Ended::Broken(_)
    ));

    let (c, t) = start(Arc::new(Fake::default()));
    drop(c);
    assert!(matches!(
        tokio::time::timeout(WAIT, t).await.unwrap().unwrap(),
        Ended::Left
    ));
}

/// 订阅按 credit 取：给 1 格就只发 1 格，补了 credit 才往下；句柄的流没说 `Closed` 就结束了 ⇒ 路由器补 `Closed{Ours(Broken)}`。
#[tokio::test]
async fn a_subscription_never_outruns_its_credit_and_always_ends_closed() {
    let frame = |seq| Item::Frame {
        seq,
        body: Body(vec![seq as u8]),
    };
    let fake = Arc::new(Fake {
        items: vec![frame(1), frame(2)],
        ..Fake::default()
    });
    let (mut c, _t) = start(fake);
    send(
        &mut c,
        Head::Subscribe {
            id: 7,
            origin: Origin::local(),
            kind: "k".into(),
            from: None,
            want: 1,
        },
        b"",
    )
    .await;
    assert_eq!(
        recv(&mut c).await,
        (
            Head::Next {
                id: 7,
                item: WireItem::Frame { seq: 1 }
            },
            vec![1]
        )
    );
    // credit 用完：在补之前什么都不该来。
    assert!(
        tokio::time::timeout(Duration::from_millis(100), read_frame(&mut c, CAP))
            .await
            .is_err(),
        "credit 用完了还在发"
    );
    send(&mut c, Head::Want { id: 7, more: 5 }, b"").await;
    assert_eq!(
        recv(&mut c).await,
        (
            Head::Next {
                id: 7,
                item: WireItem::Frame { seq: 2 }
            },
            vec![2]
        )
    );
    assert_eq!(
        recv(&mut c).await.0,
        Head::Next {
            id: 7,
            item: WireItem::ClosedByOurs(OursFault::Broken)
        }
    );
}

/// 能力事实原样转交（今天没有控制通道 ⇒ `null`）。
#[tokio::test]
async fn offer_of_is_relayed_as_is() {
    let (mut c, _t) = start(Arc::new(Fake::default()));
    send(
        &mut c,
        Head::OfferOf {
            id: 2,
            origin: Origin::local(),
        },
        b"",
    )
    .await;
    assert_eq!(recv(&mut c).await, (Head::Done { id: 2 }, b"null".to_vec()));
}
