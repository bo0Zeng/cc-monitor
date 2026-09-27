//! 通道 · **客户端**：外部前端手里那一半 —— 拿一条连好的流和一把钥匙，换来 `call` / `subscribe`。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔面 A 第一个外部客户端，2026-09-24〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/bridge/comm_boundary_registry_tests.rs` 的 `REGISTERED`，两向集合相等）。
//!
//! **凭什么它属于通信层**：`01 §2.2` 逐字「前端只有两个动作」—— 本文件就是那两个动作
//! 在进程外前端手里的样子（[`Client`] 实现 `05 §3.3.0` 的 `Comms`）。外部前端除了它与
//! 线上类型（`super::wire`）之外**不碰 app 的任何东西**，是用户裁决「窗口变成独立前端」的落点。
//!
//! # 🔴 它**不做**的事
//!
//! - **不拨号、不找地址、不读钥匙**（`C4` / `C5`）：[`Client::open`] 收的是一条**已经连好**的流
//!   与一把**已经交到手里**的钥匙。拨到哪、钥匙从 stdin 怎么来，归外部前端的宿主。
//! - **不造期限**（`X2`）：每一次 `call` 都要调用者显式给 [`Budget`]（`X6`），本文件一个期限常量都没有。
//! - **不重试、不排队**（`05 §4.5.1`）。
//!
//! # 一次 `call` 的三段，以及每一段超时各答什么 `reach`
//!
//! | 段 | 卡在这里意味着 | 超时答 |
//! |---|---|---|
//! | ① 入写队列 | 这一帧还没进队列 | `Hop{第 0 跳 write, NotSent, Overrun}` |
//! | ② 写上连接 | 进了队列，还没写完 | `Hop{第 0 跳 write, Unknown, Overrun}` —— 可能写了一半 |
//! | ③ 等答 | 写完了，答案没回来 | `Hop{第 0 跳 wait, Sent, Overrun}` ＋ 尽力补发一帧撤单 |
//!
//! 🔴 **拿不准一律 `Unknown`**（`05 §3.3.1`）：第 ② 段是唯一「分不清」的那段，它就答 `Unknown`。
//!
//! # 买到什么
//!
//! - `05 §3.3` 的签名在一个真客户端上成立：`budget` 是绝对时刻、`from` 原样过线、`want` 是 credit。
//! - 订阅的缓冲**有上界**：它只收路由器按 credit 发来的格数；对面越过 credit ⇒ 流里原位出
//!   `Closed{Ours(Broken)}`（`05 §3.3.4`：不许静默堆，也不许静默丢）。
//! - 连接断了 ⇒ 在飞的 `call` 一律 `Hop{第 0 跳 read, Unknown, Dropped}`，订阅流里原位出 `Unseen`
//!   （**不是** `Closed`，`05 §4.5.2`）。
//!
//! # 买不到什么
//!
//! - **不买自动重连**：它只有交给它的那一条流，断了就断了；订阅停在 `Unseen`，不会自己回来。
//!   `§4.5.2` 要的「通信层自己重连」需要地址与钥匙，今天那两样在外部前端的宿主手里 —— 登记为欠账。
//! - **不买「对面真的停了」**：撤单那一帧只是尽力（`05 §3.3.3`）。

use super::wire::{
    err_from_wire, item_from_wire, read_frame, write_frame, Body, Budget, By, CallError, Comms,
    Cursor, Head, HopFault, HopId, Item, Key, Kind, Offer, Op, Origin, OursFault, PeerFault, Reach,
    ReadFault, Sub, Withdraw,
};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

/// 写任务的一格：一帧 ＋（可选）「写完了」的回执。
struct Job {
    head: Head,
    body: Vec<u8>,
    written: Option<oneshot::Sender<()>>,
}

/// 一次 `call` 的回信口。
type Answer = oneshot::Sender<Result<Body, CallError>>;

/// 一条订阅在客户端这一侧的状态。
#[derive(Default)]
struct Tap {
    queue: VecDeque<Item>,
    waker: Option<Waker>,
    /// 累计给出去的 credit。
    granted: u64,
    /// 累计收到的格数。**永远 ≤ `granted`**，越过即对面违约。
    got: u64,
    /// 订阅方已经撤了 / 流已经 `Closed`。
    over: bool,
}

impl Tap {
    fn push(&mut self, item: Item) {
        self.queue.push_back(item);
        if let Some(w) = self.waker.take() {
            w.wake();
        }
    }
}

/// 读写两个任务与各个调用方共享的东西。
struct Shared {
    tx: mpsc::Sender<Job>,
    next: AtomicU64,
    /// 在飞的 `call`，以及「连接还在不在」（`None` = 已经断了）。两件事同一把锁，
    /// 为的是「断线时清空」与「新登记」不会交错出一个永远没人答的编号。
    calls: Mutex<Option<HashMap<u64, Answer>>>,
    taps: Mutex<HashMap<u64, Arc<Mutex<Tap>>>>,
    /// 〔NET2〕问过的那几台的能力事实（[`Client::offer`] 填）：本地撤单时据此说「那台可能还在跑」。
    offers: Mutex<HashMap<String, Offer>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn hop(idx: u8, tag: &'static str, reach: Reach, why: HopFault) -> CallError {
    CallError::Hop {
        at: HopId { idx, tag },
        reach,
        why,
    }
}

fn cancelled() -> CallError {
    OursFault::Cancelled.into()
}

/// 已发出的那条被本地撤掉：`runs_on` = 那台对这一条不认撤（`05 §3.3.3`「在结果里说明」）。
fn withdrawn(runs_on: bool) -> CallError {
    CallError::Ours {
        why: OursFault::Cancelled,
        runs_on,
    }
}

/// 外部前端手里的通道客户端。克隆便宜（共享同一条连接）。
#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
    rt: tokio::runtime::Handle,
}

impl Client {
    /// 在一条**已经连好**的流上出示钥匙，换一个客户端。
    ///
    /// - 钥匙不对 ⇒ `Peer{Refused}`（通道是通的，对面答「不行」）；
    /// - 期限内对面没答 ⇒ `Hop{第 0 跳 auth, …, Overrun}`；
    /// - 对面没答就断了 ⇒ `Hop{第 0 跳 auth, …, Dropped}`。
    ///
    /// `frame` 是帧头 / 帧体各自的字节上限，由宿主给（`C4`）。
    ///
    /// # Errors
    ///
    /// 见上。
    pub async fn open<S>(io: S, key: &Key, frame: usize, budget: Budget) -> Result<Self, CallError>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (mut rd, mut wr) = tokio::io::split(io);
        let wake = budget.deadline();
        let hello = Head::Hello { key: key.0.clone() };
        let sent = tokio::select! {
            r = tokio::time::timeout_at(wake, write_frame(&mut wr, &hello, &[])) => r,
            () = budget.cancel.cancelled() => return Err(cancelled()),
        };
        match sent {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(hop(0, "auth", Reach::Unknown, HopFault::Dropped)),
            Err(_elapsed) => return Err(hop(0, "auth", Reach::Unknown, HopFault::Overrun)),
        }
        let answer = tokio::select! {
            r = tokio::time::timeout_at(wake, read_frame(&mut rd, frame)) => r,
            () = budget.cancel.cancelled() => return Err(cancelled()),
        };
        match answer {
            Ok(Ok((Head::Welcome, _))) => {}
            Ok(Ok((Head::Denied, body))) => {
                return Err(CallError::Peer {
                    why: PeerFault::Refused { body: Body(body) },
                })
            }
            Ok(Ok(_)) | Ok(Err(ReadFault::Bad(_))) => return Err(OursFault::Broken.into()),
            Ok(Err(ReadFault::Eof)) | Ok(Err(ReadFault::Io(_))) => {
                return Err(hop(0, "auth", Reach::Sent, HopFault::Dropped))
            }
            Err(_elapsed) => return Err(hop(0, "auth", Reach::Sent, HopFault::Overrun)),
        }

        let (tx, mut rx) = mpsc::channel::<Job>(16);
        let shared = Arc::new(Shared {
            tx,
            next: AtomicU64::new(1),
            calls: Mutex::new(Some(HashMap::new())),
            taps: Mutex::new(HashMap::new()),
            offers: Mutex::new(HashMap::new()),
        });
        // 写任务：只有它碰写半边 ⇒ 帧不交错。写不出去就收工（读任务会看到断线并通知各方）。
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                if write_frame(&mut wr, &job.head, &job.body).await.is_err() {
                    break;
                }
                if let Some(w) = job.written {
                    // 调用方已经不等了也无妨。
                    w.send(()).ok();
                }
            }
        });
        let reader_shared = Arc::clone(&shared);
        tokio::spawn(async move { read_loop(rd, frame, reader_shared).await });
        Ok(Self {
            shared,
            rt: tokio::runtime::Handle::current(),
        })
    }

    /// 发一帧控制头（撤单 / credit / 撤订阅），不等。连接没了就算了 —— 它本来就只是尽力。
    fn post(&self, head: Head) {
        let tx = self.shared.tx.clone();
        self.rt.spawn(async move {
            tx.send(Job {
                head,
                body: Vec::new(),
                written: None,
            })
            .await
            .ok();
        });
    }

    /// 摘掉一个 `call` 的回信口（调用方不等了）。
    fn forget(&self, id: u64) {
        if let Some(map) = lock(&self.shared.calls).as_mut() {
            map.remove(&id);
        }
    }

    /// 〔NET2〕问 `origin` 那台的能力事实（`Offer`：认哪些 op · 这台做不到哪几条 · 哪几条撤不动）。
    /// `None` = 那台今天没有控制通道。问到的那份记下来，之后本地撤单据它说「那台可能还在跑」。
    ///
    /// # Errors
    ///
    /// 同 `call`（期限 · 撤单 · 断线）；体解不出来 ⇒ `Ours{Broken}`。
    pub async fn offer(&self, origin: &Origin, budget: Budget) -> Result<Option<Offer>, CallError> {
        let o = origin.clone();
        let body = self
            .exchange(
                |id| Head::OfferOf { id, origin: o },
                Body::default(),
                budget,
                false,
            )
            .await?;
        let offer: Option<Offer> =
            serde_json::from_slice(&body.0).map_err(|_| CallError::from(OursFault::Broken))?;
        let mut cache = lock(&self.shared.offers);
        match &offer {
            Some(x) => cache.insert(origin.as_wire_str().to_string(), x.clone()),
            None => cache.remove(origin.as_wire_str()),
        };
        Ok(offer)
    }

    /// 这一条被本地撤掉之后，那台是不是可能还在跑（手里那份 `Offer` 说它不认撤这一条）。没问过那台 ⇒ `false`。
    fn runs_on_after_cancel(&self, origin: &Origin, op: &Op) -> bool {
        lock(&self.shared.offers)
            .get(origin.as_wire_str())
            .is_some_and(|o| o.withdraw(&op.0) == Withdraw::NotOffered)
    }

    async fn call_inner(
        &self,
        origin: &Origin,
        op: &Op,
        payload: Body,
        budget: Budget,
    ) -> Result<Body, CallError> {
        let left = budget.remaining();
        let runs_on = self.runs_on_after_cancel(origin, op);
        let (o, name) = (origin.clone(), op.0.clone());
        self.exchange(
            move |id| Head::Call {
                id,
                origin: o,
                op: name,
                left,
            },
            payload,
            budget,
            runs_on,
        )
        .await
    }

    /// 一问一答的三段（见头注那张表）。`runs_on` 只用在「已发出之后被本地撤掉」那两格。
    async fn exchange(
        &self,
        head: impl FnOnce(u64) -> Head,
        payload: Body,
        budget: Budget,
        runs_on: bool,
    ) -> Result<Body, CallError> {
        if budget.cancel.is_cancelled() {
            return Err(cancelled());
        }
        let left = budget.remaining();
        if left.is_zero() {
            return Err(hop(0, "write", Reach::NotSent, HopFault::Overrun));
        }
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let (answer_tx, answer_rx) = oneshot::channel();
        match lock(&self.shared.calls).as_mut() {
            Some(map) => {
                map.insert(id, answer_tx);
            }
            None => return Err(hop(0, "open", Reach::NotSent, HopFault::Dropped)),
        }
        let (written_tx, written_rx) = oneshot::channel();
        let job = Job {
            head: head(id),
            body: payload.0,
            written: Some(written_tx),
        };
        let wake = budget.deadline();

        // ① 入写队列。
        let queued = tokio::select! {
            r = tokio::time::timeout_at(wake, self.shared.tx.send(job)) => r,
            () = budget.cancel.cancelled() => {
                self.forget(id);
                return Err(cancelled());
            }
        };
        match queued {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.forget(id);
                return Err(hop(0, "write", Reach::NotSent, HopFault::Dropped));
            }
            Err(_elapsed) => {
                self.forget(id);
                return Err(hop(0, "write", Reach::NotSent, HopFault::Overrun));
            }
        }

        // ② 写上连接。
        let written = tokio::select! {
            r = tokio::time::timeout_at(wake, written_rx) => r,
            () = budget.cancel.cancelled() => {
                self.forget(id);
                self.post(Head::Cancel { id });
                return Err(withdrawn(runs_on));
            }
        };
        match written {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.forget(id);
                return Err(hop(0, "write", Reach::Unknown, HopFault::Dropped));
            }
            Err(_elapsed) => {
                self.forget(id);
                self.post(Head::Cancel { id });
                return Err(hop(0, "write", Reach::Unknown, HopFault::Overrun));
            }
        }

        // ③ 等答。
        let answer = tokio::select! {
            r = tokio::time::timeout_at(wake, answer_rx) => r,
            () = budget.cancel.cancelled() => {
                self.forget(id);
                self.post(Head::Cancel { id });
                return Err(withdrawn(runs_on));
            }
        };
        match answer {
            Ok(Ok(r)) => r,
            // 回信口被丢了而没说话 —— 只有读任务收工时会这样，而它收工前会先说话；到这里是不变量破了。
            Ok(Err(_)) => Err(OursFault::Broken.into()),
            Err(_elapsed) => {
                self.forget(id);
                self.post(Head::Cancel { id });
                Err(hop(0, "wait", Reach::Sent, HopFault::Overrun))
            }
        }
    }

    fn subscribe_inner(
        &self,
        origin: &Origin,
        kind: &Kind,
        from: Option<Cursor>,
        want: u32,
    ) -> Subscription {
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let tap = Arc::new(Mutex::new(Tap {
            granted: u64::from(want),
            ..Tap::default()
        }));
        let connected = lock(&self.shared.calls).is_some();
        if connected {
            lock(&self.shared.taps).insert(id, Arc::clone(&tap));
            self.post(Head::Subscribe {
                id,
                origin: origin.clone(),
                kind: kind.0.clone(),
                from,
                want,
            });
        } else {
            // 连接已经没了：订阅照样成立（`05 §3.3.5`），流里第一格就是「看不见」。
            lock(&tap).push(Item::Unseen {
                at: HopId {
                    idx: 0,
                    tag: "open",
                },
                why: HopFault::Dropped,
            });
        }
        Subscription {
            id,
            tap,
            client: self.clone(),
        }
    }
}

impl Comms for Client {
    fn call(
        &self,
        origin: &Origin,
        op: &Op,
        payload: Body,
        budget: Budget,
    ) -> impl std::future::Future<Output = Result<Body, CallError>> + Send {
        self.call_inner(origin, op, payload, budget)
    }

    fn subscribe(
        &self,
        origin: &Origin,
        kind: &Kind,
        from: Option<Cursor>,
        want: u32,
    ) -> impl Sub + use<> {
        self.subscribe_inner(origin, kind, from, want)
    }
}

/// 读任务：把路由器说的每一帧分给对应的调用方 / 订阅。
async fn read_loop<R: AsyncRead + Unpin>(mut rd: R, frame: usize, shared: Arc<Shared>) {
    loop {
        let Ok((head, body)) = read_frame(&mut rd, frame).await else {
            break;
        };
        match head {
            Head::Done { id } => answer(&shared, id, Ok(Body(body))),
            Head::Failed { id, err } => answer(&shared, id, Err(err_from_wire(err, body))),
            Head::Next { id, item } => {
                let tap = lock(&shared.taps).get(&id).cloned();
                let Some(tap) = tap else {
                    // 已经撤掉的订阅，路由器在撤单之前多发的那几格：没人收了。
                    continue;
                };
                let mut t = lock(&tap);
                t.got += 1;
                let item = match item_from_wire(item, body) {
                    Some(i) if t.got <= t.granted => i,
                    // 越过 credit / 解不出来：对面违约。**原位说出来**，然后这条订阅到此为止。
                    Some(_) | None => Item::Closed {
                        by: By::Ours(OursFault::Broken),
                    },
                };
                let last = matches!(item, Item::Closed { .. });
                t.push(item);
                if last {
                    t.over = true;
                    drop(t);
                    lock(&shared.taps).remove(&id);
                }
            }
            Head::Hello { .. }
            | Head::Call { .. }
            | Head::Cancel { .. }
            | Head::Subscribe { .. }
            | Head::Want { .. }
            | Head::Stop { .. }
            | Head::OfferOf { .. }
            | Head::Welcome
            | Head::Denied => break,
        }
    }
    // 断了：在飞的 `call` 一律答「断在读那一跳，发没发到拿不准」；订阅原位出「看不见」。
    let pending = lock(&shared.calls).take().unwrap_or_default();
    for (_, tx) in pending {
        tx.send(Err(hop(0, "read", Reach::Unknown, HopFault::Dropped)))
            .ok();
    }
    let taps: Vec<Arc<Mutex<Tap>>> = lock(&shared.taps).drain().map(|(_, t)| t).collect();
    for tap in taps {
        lock(&tap).push(Item::Unseen {
            at: HopId {
                idx: 0,
                tag: "read",
            },
            why: HopFault::Dropped,
        });
    }
}

fn answer(shared: &Shared, id: u64, r: Result<Body, CallError>) {
    let tx = lock(&shared.calls).as_mut().and_then(|m| m.remove(&id));
    if let Some(tx) = tx {
        // 调用方已经不等了（期限到了 / 撤了）也无妨。
        tx.send(r).ok();
    }
}

/// 一条订阅（`05 §3.3.0` 的 `Sub`）。丢掉它等于 `stop()`。
pub struct Subscription {
    id: u64,
    tap: Arc<Mutex<Tap>>,
    client: Client,
}

impl futures::Stream for Subscription {
    type Item = Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Item>> {
        let mut t = lock(&self.tap);
        if let Some(i) = t.queue.pop_front() {
            return Poll::Ready(Some(i));
        }
        if t.over {
            return Poll::Ready(None);
        }
        t.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Sub for Subscription {
    fn want(&self, more: u32) {
        let mut t = lock(&self.tap);
        if t.over {
            return;
        }
        t.granted += u64::from(more);
        drop(t);
        self.client.post(Head::Want { id: self.id, more });
    }

    fn stop(&self) {
        let mut t = lock(&self.tap);
        if t.over {
            return;
        }
        t.over = true;
        if let Some(w) = t.waker.take() {
            w.wake();
        }
        drop(t);
        lock(&self.client.shared.taps).remove(&self.id);
        self.client.post(Head::Stop { id: self.id });
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop();
    }
}
