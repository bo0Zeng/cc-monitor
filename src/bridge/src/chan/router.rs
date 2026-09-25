//! 通道 · **路由器**：一条已经接进来的连接 ⇒ 认证 ⇒ 按 `origin` 把 `call` / `subscribe` 转给注入的句柄。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔面 A 第一个外部客户端，2026-09-24〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/bridge/comm_boundary_registry_tests.rs` 的 `REGISTERED`，两向集合相等）。
//!
//! **凭什么它属于通信层**：`设计/01 §2.4` 面 A 逐字「前端 ↔ 各处后端」「寻址：`origin`」——
//! 本文件就是面 A 上**第一个进程外前端**进来的那扇门。它只知道 `05 §2` 那四样：
//! 地址（`origin`）· 操作名（不透明串）· 载荷（不透明字节）· 流的订阅与分发。
//!
//! # 🔴 纯路由器：它**不做**的事，逐条
//!
//! - **不绑端口、不 `accept`**（`C5`）：宿主绑回环、`accept`，把接到的连接交给 [`serve`]
//!   —— 与面 B 那个先例同形（`01 §2.1 C5` 逐字：「由**后端** `bind`/`listen`……把 `accept`
//!   到的连接交给面 B；面 B 只有 `serve(stream)`」）。
//! - **不读盘、不读环境变量、不造钥匙**（`C4`）：钥匙、帧长上限、认证等待时长全在 [`Terms`] 里，
//!   由宿主交进来。
//! - **不解释 `op` / `kind` / 载荷**（`C1`）：它们只被原样交给 [`Backends`]。
//!   「这个 `origin` 今天由谁服务」「那台机器看不看得见」也不在这里判 —— 那是句柄的活。
//! - **不排队、不重试**（`05 §4.5.1`：通信层不知道重发一次安不安全）。
//!
//! # 它做的事
//!
//! 1. **认证**：第一帧必须是 `Hello{key}`，钥匙比对通过才回 `Welcome`；否则回 `Denied` 并关连接。
//!    认证之前一帧业务都不收（`Hello` 之前来的任何东西都等同于认证失败）。
//! 2. **配对**：客户端给每个 `call` / `subscribe` 一个编号，路由器只拿它配对应答。
//! 3. **撤单**：`Cancel{id}` / `Stop{id}` / 连接断开 ⇒ 对应的撤单手柄拨下去（句柄那一侧是尽力）。
//! 4. **期限执行**（`05 §3.3.2`「执行归通信层」）：线上来的是「还剩多少」，路由器按它给
//!    句柄的那一跳装一个上界；超了回 `Hop{at: 第 1 跳 wait, reach: Unknown, why: Overrun}`。
//!    🔴 路由器**不造**绝对时刻往下传 —— 它交给句柄的就是那段「还剩多少」。
//! 5. **背压**（`05 §3.3.4` 级 1）：订阅流按客户端给的 credit（`want`）取，credit 用完就**不取**，
//!    句柄那一侧的流于是被回推；**零处丢弃**，也就不需要 `Gap`（`Gap` 只在句柄自己丢了时由它原位给出）。
//!
//! # 买到什么
//!
//! - 一条**进程外**的前端第一次能用 `call` / `subscribe` 两个动作走到后端，
//!   而路由器身上零业务、零读盘、零起进程、零期限常量（十一条判据现打）。
//! - 没过认证的连接**一帧业务都进不来**（判据里有一个拿错钥匙的假客户端）。
//!
//! # 买不到什么
//!
//! - **不买「那个 `origin` 真的有人服务」** —— 句柄说没有，就是 `Hop{第 1 跳 open, NotSent, Unreachable}`。
//! - **不买对端撤活成功**（`05 §3.3.3`：只是尽力）。
//! - **不买连接级重连** —— 这条连接一断，客户端那一侧的订阅得到 `Unseen`，之后不会自己回来；
//!   重拨归拿着地址与钥匙的那一方（外部前端的宿主），不归本文件。
//! - **不买「钥匙本身够不够随机」** —— 钥匙由宿主造，本文件只比对。

use super::wire::{
    err_to_wire, item_to_wire, read_frame, write_frame, Body, By, CallError, CancelToken, Cursor,
    Head, HopFault, HopId, Item, Key, Kind, Op, Origin, OursFault, Reach, ReadFault,
};
use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, Semaphore};

/// 宿主注入的**后端句柄** —— 路由器按 `origin` 把两个动作交给它。
///
/// 🔴 `left` 是**这一跳还剩多少**，不是绝对时刻：跨进程那一段线上传的就是它（`05 §3.3.2`），
/// 句柄拿它给自己的下一跳装期限。路由器自己另外按同一个值装了一个上界。
pub trait Backends: Send + Sync {
    /// 一次性请求。失败一定是一个 `CallError`，不许回一个空答案冒充。
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>>;

    /// 订阅。**不返回 `Result`**（`05 §3.3.5`）：看不见的机器在流里给 `Unseen`。
    /// 句柄的流**应当**以 `Closed` 结尾；没说 `Closed` 就结束了，路由器补一个 `Closed{Ours(Broken)}`。
    fn subscribe(
        &self,
        origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item>;
}

/// 宿主交进来的全部条件（`C4`：一个都不由路由器自己去拿）。
#[derive(Clone)]
pub struct Terms {
    /// 连接者要出示的那把钥匙。
    pub key: Key,
    /// 帧头 / 帧体各自的字节上限。
    pub frame: usize,
    /// 连上之后多久之内必须出示钥匙。
    pub hello_within: Duration,
}

/// 一条连接为什么结束了（给宿主记一行日志用；**不含钥匙**）。
#[derive(Debug)]
pub enum Ended {
    /// 对面正常走了。
    Left,
    /// 没过认证（钥匙不对 / 第一帧不是 `Hello` / 限时内没出示）。
    Denied,
    /// 对面没按协议说话，或读写出错。
    Broken(String),
}

/// 写出去的一帧。
type Out = (Head, Vec<u8>);

/// 在飞的那些编号 ⇒ 各自的撤单手柄。
type Inflight = Arc<Mutex<HashMap<u64, Slot>>>;

/// 一个编号上挂着的东西。
struct Slot {
    cancel: CancelToken,
    /// 订阅才有：它的 credit。
    credit: Option<Arc<Semaphore>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// **服务一条已经接进来的连接**，直到它结束。
///
/// 宿主 `accept` 之后把连接交进来（`C5`）；每条连接各跑一份，彼此不共享状态。
pub async fn serve<S>(io: S, terms: Terms, backends: Arc<dyn Backends>) -> Ended
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (mut rd, mut wr) = tokio::io::split(io);

    // ── ① 认证：第一帧必须是 `Hello`，钥匙对得上。之前什么都不收。
    let first = tokio::time::timeout(terms.hello_within, read_frame(&mut rd, terms.frame)).await;
    let passed = match first {
        Ok(Ok((Head::Hello { key }, _))) => terms.key.matches(&key),
        Ok(Ok(_)) | Ok(Err(_)) | Err(_) => false,
    };
    if !passed {
        // 回一帧 `Denied` 是给正经客户端一个准话；写不出去也无妨，连接照关。
        write_frame(&mut wr, &Head::Denied, &[]).await.ok();
        return Ended::Denied;
    }
    if let Err(e) = write_frame(&mut wr, &Head::Welcome, &[]).await {
        return Ended::Broken(format!("回 Welcome 写不出去：{e}"));
    }

    // ── ② 写出去的一律经这一条队列（只有写任务碰写半边，帧不会交错）。
    //    队列满了发送方**等**（回推），不丢。
    let (tx, mut rx) = mpsc::channel::<Out>(16);
    let writer = tokio::spawn(async move {
        while let Some((head, body)) = rx.recv().await {
            if write_frame(&mut wr, &head, &body).await.is_err() {
                break;
            }
        }
    });

    // ── ③ 读循环：配对、撤单、credit。
    let inflight: Inflight = Arc::new(Mutex::new(HashMap::new()));
    // 本函数无论怎么结束（正常返回，或宿主把这个 future 整个丢掉），收尾都由它做。
    let _teardown = Teardown {
        inflight: Arc::clone(&inflight),
        writer,
    };
    let ended = loop {
        let (head, body) = match read_frame(&mut rd, terms.frame).await {
            Ok(f) => f,
            Err(ReadFault::Eof) => break Ended::Left,
            Err(ReadFault::Io(e)) => break Ended::Broken(format!("读出错：{e}")),
            Err(ReadFault::Bad(why)) => break Ended::Broken(why),
        };
        match head {
            Head::Call {
                id,
                origin,
                op,
                left,
            } => {
                let Some(cancel) = open_slot(&inflight, id, None) else {
                    break Ended::Broken(format!("编号 {id} 还在飞，又来了一次"));
                };
                let call = backends.call(origin, Op(op), Body(body), left, cancel.clone());
                tokio::spawn(run_call(
                    id,
                    call,
                    left,
                    cancel,
                    tx.clone(),
                    inflight.clone(),
                ));
            }
            Head::Subscribe {
                id,
                origin,
                kind,
                from,
                want,
            } => {
                let credit = Arc::new(Semaphore::new(want as usize));
                let Some(cancel) = open_slot(&inflight, id, Some(credit.clone())) else {
                    break Ended::Broken(format!("编号 {id} 还在飞，又来了一次"));
                };
                let stream = backends.subscribe(origin, Kind(kind), from);
                tokio::spawn(pump(
                    id,
                    stream,
                    credit,
                    cancel,
                    tx.clone(),
                    inflight.clone(),
                ));
            }
            Head::Cancel { id } | Head::Stop { id } => {
                // 已经答完的编号再撤是合法的（竞态），什么都不用做。
                if let Some(slot) = lock(&inflight).get(&id) {
                    slot.cancel.cancel();
                }
            }
            Head::Want { id, more } => {
                if let Some(credit) = lock(&inflight).get(&id).and_then(|s| s.credit.clone()) {
                    credit.add_permits(more as usize);
                }
            }
            Head::Hello { .. }
            | Head::Welcome
            | Head::Denied
            | Head::Done { .. }
            | Head::Failed { .. }
            | Head::Next { .. } => {
                break Ended::Broken("客户端发来了一帧只该由路由器发的头".to_string());
            }
        }
    };

    // ── ④ 连接没了：收尾交给 `_teardown` 的析构（见 [`Teardown`]）。
    ended
}

/// 一条连接的收尾：所有在飞的一律本地撤单（句柄那一侧尽力停）＋ 写任务直接收掉。
///
/// 🔴 写成析构而不是 [`serve`] 末尾的几行：宿主把 `serve` 这个 future **整个丢掉**时
/// （运行时关停、任务被 abort），末尾那几行根本不跑 —— 而写任务与句柄那一侧的活
/// 是另外 `spawn` 出去的，没人收它们就一直握着这条连接的写半边。
/// 写任务**不等排空**：对面已经走了（或说了坏话），没有哪一帧还值得送；
/// 而一个「不读也不关」的对面会让排空那一下永远等下去。
struct Teardown {
    inflight: Inflight,
    writer: tokio::task::JoinHandle<()>,
}

impl Drop for Teardown {
    fn drop(&mut self) {
        for slot in lock(&self.inflight).values() {
            slot.cancel.cancel();
        }
        self.writer.abort();
    }
}

/// 登记一个编号。已在飞 ⇒ `None`（客户端的编号必须在连接内唯一）。
fn open_slot(inflight: &Inflight, id: u64, credit: Option<Arc<Semaphore>>) -> Option<CancelToken> {
    let mut map = lock(inflight);
    if map.contains_key(&id) {
        return None;
    }
    let cancel = CancelToken::new();
    map.insert(
        id,
        Slot {
            cancel: cancel.clone(),
            credit,
        },
    );
    Some(cancel)
}

/// 句柄那一跳超了期限：拨下它的撤单手柄，回 `05 §3.3.2` 那一格。
fn overrun(cancel: &CancelToken) -> CallError {
    cancel.cancel();
    CallError::Hop {
        at: HopId {
            idx: 1,
            tag: "wait",
        },
        reach: Reach::Unknown,
        why: HopFault::Overrun,
    }
}

/// 句柄那一跳的**期限执行 ＋ 本地撤单** —— 一次 `call` 在第 1 跳上的全部结局。
///
/// 〔C4a · 2026-09-24〕从 [`run_call`] 里抽出来，让第二条进来的路（webview 那一侧，`chan/webview.rs`）
/// **用同一份**：两处各写一份 `timeout(left, …)` ＋「超了拨撤单、回 `Hop{1 wait, Unknown, Overrun}`」，
/// 迟早一份改了另一份没改（`D1`：一个判定只有一个家）。它不认识帧、编号、连接 —— 只认句柄那一跳。
pub(crate) async fn settle(
    call: BoxFuture<'static, Result<Body, CallError>>,
    left: Duration,
    cancel: CancelToken,
) -> Result<Body, CallError> {
    tokio::select! {
        r = tokio::time::timeout(left, call) => match r {
            Ok(r) => r,
            // 句柄那一跳在「还剩多少」之内没回来：它收到了没有、做了没有，路由器都不知道。
            // 它的撤单手柄同时拨下去（对端撤活，尽力）—— 客户端随后补发的那一帧撤单
            // 到的时候这个编号已经摘掉了，不靠这一下就没人通知句柄。
            Err(_elapsed) => Err(overrun(&cancel)),
        },
        () = cancel.cancelled() => Err(CallError::Ours { why: OursFault::Cancelled }),
    }
}

/// 跑一次 `call` 并把结局送回去。
async fn run_call(
    id: u64,
    call: BoxFuture<'static, Result<Body, CallError>>,
    left: Duration,
    cancel: CancelToken,
    tx: mpsc::Sender<Out>,
    inflight: Inflight,
) {
    let outcome = settle(call, left, cancel).await;
    lock(&inflight).remove(&id);
    let out = match outcome {
        Ok(body) => (Head::Done { id }, body.0),
        Err(e) => {
            let (err, body) = err_to_wire(e);
            (Head::Failed { id, err }, body)
        }
    };
    // 连接已经没了 ⇒ 写不出去；这一格的结局本来也没人收了。
    tx.send(out).await.ok();
}

/// 按 credit 把句柄的流抽到连接上。credit 用完就停下不取 —— 回推给句柄那一侧。
async fn pump(
    id: u64,
    mut stream: BoxStream<'static, Item>,
    credit: Arc<Semaphore>,
    cancel: CancelToken,
    tx: mpsc::Sender<Out>,
    inflight: Inflight,
) {
    loop {
        let step = tokio::select! {
            permit = credit.acquire() => match permit {
                Ok(p) => {
                    p.forget();
                    tokio::select! {
                        next = stream.next() => Some(next),
                        () = cancel.cancelled() => None,
                    }
                }
                // 信号量只在被关时出错，本文件从不关它。
                Err(_closed) => None,
            },
            () = cancel.cancelled() => None,
        };
        let Some(next) = step else {
            // 客户端撤了订阅（或连接没了）：不再多发一格。
            break;
        };
        let item = next.unwrap_or(Item::Closed {
            by: By::Ours(OursFault::Broken),
        });
        let last = matches!(item, Item::Closed { .. });
        let (w, body) = item_to_wire(item);
        if tx.send((Head::Next { id, item: w }, body)).await.is_err() {
            break;
        }
        if last {
            break;
        }
    }
    lock(&inflight).remove(&id);
}
