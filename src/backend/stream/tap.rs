//! **tee 的消费侧（后端这一半）**：常驻后端进程内那一份中转抄出来的 SSE 事件，
//! 经这里交给**当前那条流连接**的写者，变成 `tap` 帧（`wire::Frame::Tap`）。
//!
//! # 形状（设计住仓外；要求：「走常驻后端现有的那条 wire，不另开通道」）
//!
//! ```text
//! 中转转发线程 ── TeeSink(tap 口) ── TapHub::offer ── try_send ──▶ 有界通道（每条流连接一条）
//!                                                                    │
//!          TapRx：流归位（折成归一事件 · 定归哪个运行）──▶ main.rs::writer_task：出方向 ＞ 应答 ＞ tap（最低优先）──▶ wire
//! ```
//!
//! - **一个进程一个 hub**（[`hub`]），装着「此刻那条流连接」的发送端。流连接接上 ⇒ [`TapHub::attach`] 新建一条通道、
//!   发送端换进来、接收端交那条连接的 `writer_task`；那条连接走了 ⇒ 接收端没了，`try_send` 失败 ⇒ 当场丢。
//!   没人连着时照丢 —— 没有人可说，而位置号 `n` 照占，下一个接上的人看得见「这个响应缺了前面几件」。
//! - **不许阻塞转发**（抄流跟不上绝不回推主路）：只有 `try_send`。
//! - **不许挤掉内容帧**：tap 走**自己的**通道，不进出方向那条 10 000 容量的大通道（那条满了会丢 `line` 帧）；
//!   `writer_task` 只在出方向与应答都空时才取它。
//!
//! # 上界（级 3 禁止）
//!
//! [`TAP_CAPACITY`] 件 × 每件原文 ≤ `relay::TAP_DATA_CAP` ⇒ 这一跳最坏 4 MiB。满了落级 2（丢，位置号原位说）。

use crate::relay::{TapEvent, TapPort};
use crate::stream::wire::{Frame, Topic};

/// tap 通道能排多少**件**（每条流连接一条）。
///
/// ⚠ **是条数不是体量**（体量由 `relay::TAP_DATA_CAP` 封）。值：token 级的 SSE 一秒几十到上百件，
/// 256 件够吸收写者被内容帧占住的一小段；再多攒着只是让活卡更晚，不如丢了让 jsonl 定稿。
/// 登记住址 `tests/frontend/shell/byte_cap_registry.rs` 的 `NOT_A_SIZE_CAP`。
pub(crate) const TAP_CAPACITY: usize = 256;

/// 进程级 hub：此刻每条流连接的 tap 发送端。
/// 多客户：每条连接一条，扇出；那条连接走了（接收端没了）下一次交的时候摘掉。
/// 丢了的件按原因数（没人连着 · 某条连接的通道满了），每种第 1、2、4、8… 次说一行。
#[derive(Default)]
pub(crate) struct TapHub {
    current: std::sync::Mutex<Vec<tokio::sync::mpsc::Sender<TapEvent>>>,
    /// 没人连着时丢的件数。
    unheard: std::sync::atomic::AtomicU64,
    /// 某条连接的通道满了丢的件数（每条连接各算一件）。
    full: std::sync::atomic::AtomicU64,
}

impl TapHub {
    /// 一条流连接接上了：新建一条有界通道，发送端加进来，接收端交给它的写者。
    pub(crate) fn attach(&self) -> tokio::sync::mpsc::Receiver<TapEvent> {
        self.attach_bounded(TAP_CAPACITY)
    }

    /// 同 [`Self::attach`]，容量由调用方给（生产只经 `attach` 走 [`TAP_CAPACITY`]；判据拿小容量造「跟不上」）。
    pub(crate) fn attach_bounded(&self, cap: usize) -> tokio::sync::mpsc::Receiver<TapEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel::<TapEvent>(cap);
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }
}

/// 数一件，回这一种累计到几（第 1、2、4、8… 次时调用方说一行）。
fn bump(c: &std::sync::atomic::AtomicU64) -> u64 {
    c.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

impl TapPort for TapHub {
    /// 中转看见的这一发请求里名单上那几项在不在 ⇒ 记进会话的请求标记（会话事实定上下文上限时读）。
    fn note_marks(&self, stream: &str, marks: &[((&'static str, &'static str), bool)]) {
        crate::observe::relay_marks::note(stream, marks);
    }

    /// 立刻答收没收（`try_send`）：每条连接各交一份，至少一条收下 ⇒ `true`。没人连着 / 都满 ⇒ `false`
    /// （号已由 tee 占掉，缺口在各自接收侧可算）。已走的连接当场摘掉。
    fn offer(&self, ev: TapEvent) -> bool {
        let mut g = self.current.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|tx| !tx.is_closed());
        if g.is_empty() {
            let n = bump(&self.unheard);
            if n.is_power_of_two() {
                tracing::info!("[tap] 没有流连接，抄出来的事件丢了（累计 {n} 件）");
            }
            return false;
        }
        let mut took = false;
        for tx in g.iter() {
            match tx.try_send(ev.clone()) {
                Ok(()) => took = true,
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    let n = bump(&self.full);
                    if n.is_power_of_two() {
                        tracing::warn!(
                            "[tap] 一条流连接的 tap 通道满了（容量 {} 件），事件丢了（累计 {n} 件）；那一段在界面上会断",
                            tx.max_capacity()
                        );
                    }
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
        took
    }
}

/// 进程级那一个 hub。中转（`relay::host`）拿它当 tap 口，两条载体的写者从它 `attach`。
pub(crate) fn hub() -> std::sync::Arc<TapHub> {
    static HUB: std::sync::OnceLock<std::sync::Arc<TapHub>> = std::sync::OnceLock::new();
    std::sync::Arc::clone(HUB.get_or_init(Default::default))
}

/// 中转（`relay::host`）要的那个 tap 口：就是进程级那一个 hub。
pub(crate) fn port() -> std::sync::Arc<dyn TapPort> {
    hub()
}

/// 写者那一侧要的东西：「下一个 `tap` 帧」。写者（`main.rs::writer_task`）只认这个口，不认帧从哪来 ——
/// 生产里是 [`TapRx`]（hub 那条通道）；判据拿一条手喂的帧通道当它，量「tap 灌满时内容帧一条不少」。
pub trait TapSource {
    /// 下一个 `tap` 帧；`None` = 这条来源不会再有东西了（发送端已被换掉）。
    fn next(&mut self) -> impl std::future::Future<Output = Option<Frame>> + Send;
}

/// 一条流连接的 tap 接收端（写者那一侧拿它）：hub 那条通道 ＋ 这条连接的流归位（折成归一事件、定归哪个运行）。
/// 只交出**帧** —— tee 的事件类型不出本 crate。
pub struct TapRx {
    rx: tokio::sync::mpsc::Receiver<TapEvent>,
    /// 额度账显示变了的通道（变了推一帧 `changed {quota}`）；`None` ＝ 不订（判据自己接的那一形）。
    quota: Option<tokio::sync::watch::Receiver<u64>>,
    /// 某个会话的轮换 / 「账号」格变了的通道（变了推一帧 `changed {rotation, key: sid}`）；`None` ＝ 不订。
    rotation: Option<tokio::sync::broadcast::Receiver<String>>,
    /// 这台的规则表 / 默认指向变了的通道（变了推一帧 `changed {rotation_rules}`）；`None` ＝ 不订。
    rules: Option<tokio::sync::broadcast::Receiver<()>>,
    /// 某个 pb 工作区的计划变了的通道（变了推一帧 `changed {plan, key: 工作区, rev, body: {needs}}`）；`None` ＝ 不订。
    plan: Option<tokio::sync::broadcast::Receiver<crate::plan::watch::Change>>,
    /// 帧里的小成品怎么现算（生产 ＝ [`crate::stream::topic_hook::body`]；判据自己接的那一形不现算）。
    bodies: crate::stream::topic_hook::Bodies,
    /// 这条连接看的那一台的时区（attach 带来的）：小成品里的时刻字按它写。
    tz: crate::Tz,
    book: std::sync::Arc<crate::observe::runs::RunBook>,
    router: super::run_route::RunRouter,
    out: std::collections::VecDeque<Frame>,
}

impl TapRx {
    /// 一帧 `changed`，小成品照主题表现算（同步：读这台自己的盘，毫秒级；算完才进 `out`，被别的分支抢先也不丢）。
    fn changed(&self, topic: Topic, key: Option<String>) -> Frame {
        let body = (self.bodies)(topic, key.as_deref(), &self.tz);
        Frame::changed(topic, key, None, body)
    }
}

/// 不现算小成品（判据自己接的那一形）。
fn no_bodies(_: Topic, _: Option<&str>, _: &crate::Tz) -> Option<serde_json::Value> {
    None
}

impl TapSource for TapRx {
    /// 取消安全：收到的那一件在同一次 poll 里折完、进 `out`，被别的分支抢先时什么都不丢。
    async fn next(&mut self) -> Option<Frame> {
        loop {
            if let Some(f) = self.out.pop_front() {
                return Some(f);
            }
            tokio::select! {
                ev = self.rx.recv() => match ev {
                    Some(ev) => {
                        let fs = self.router.on_tap(ev);
                        self.out.extend(fs);
                    }
                    None => return None,
                },
                () = self.book.learned() => {
                    let fs = self.router.on_learned();
                    self.out.extend(fs);
                }
                alive = quota_moved(&mut self.quota) => {
                    if alive {
                        self.out.push_back(self.changed(Topic::Quota, None));
                    } else {
                        self.quota = None;
                    }
                }
                moved = rotation_moved(&mut self.rotation) => match moved {
                    Some(Some(sid)) => {
                        let f = self.changed(Topic::Rotation, Some(sid));
                        self.out.push_back(f);
                    }
                    // 落后丢了几件：可丢的通道，客户端下一次变化或重问就补上。
                    Some(None) => {}
                    None => self.rotation = None,
                },
                moved = rules_moved(&mut self.rules) => match moved {
                    // 落后丢了几件也照推一帧（客户端反正整份重问）。
                    Some(()) => {
                        let f = self.changed(Topic::RotationRules, None);
                        self.out.push_back(f);
                    }
                    None => self.rules = None,
                },
                moved = plan_moved(&mut self.plan) => match moved {
                    Some(Some((workspace, rev, needs))) => self.out.push_back(Frame::changed(
                        Topic::Plan,
                        Some(workspace),
                        Some(rev),
                        Some(serde_json::json!({ "needs": needs })),
                    )),
                    // 落后丢了几件：可丢的通道，下一次变化或重问就补上。
                    Some(None) => {}
                    None => self.plan = None,
                }
            }
        }
    }
}

/// 一条流连接接上了 ⇒ 从进程级 hub 拿一条新的 tap 接收端（两条载体各在「流开始」那一处调一次）。
/// `book` 是这条连接的 watcher 写的那一本运行簿（流归位按它定归哪个运行）；`tz` ＝ 这条连接看的那一台的时区（帧里小成品的时刻字按它写）。
pub fn attach(book: std::sync::Arc<crate::observe::runs::RunBook>, tz: crate::Tz) -> TapRx {
    let mut rx = attach_rx(hub().attach(), book);
    rx.quota = Some(crate::accounts::quota::ledger::bell().subscribe());
    rx.rotation = Some(crate::accounts::quota::rotation::changes().subscribe());
    rx.rules = Some(crate::accounts::quota::rotation::rules_changes().subscribe());
    rx.plan = Some(crate::plan::watch::changes().subscribe());
    rx.bodies = crate::stream::topic_hook::body;
    rx.tz = tz;
    rx
}

/// 某个工作区的计划变了 ⇒ `Some(Some((工作区, 摘要, 要你看的数)))`；落后丢了几件 ⇒ `Some(None)`；通道没了 ⇒ `None`；没订 ⇒ 永远不醒。
async fn plan_moved(
    r: &mut Option<tokio::sync::broadcast::Receiver<crate::plan::watch::Change>>,
) -> Option<Option<crate::plan::watch::Change>> {
    use tokio::sync::broadcast::error::RecvError;
    match r {
        Some(r) => match r.recv().await {
            Ok(x) => Some(Some(x)),
            Err(RecvError::Lagged(_)) => Some(None),
            Err(RecvError::Closed) => None,
        },
        None => std::future::pending().await,
    }
}

/// 规则表变了（含落后丢了几件）⇒ `Some(())`；通道没了 ⇒ `None`；没订 ⇒ 永远不醒。
async fn rules_moved(r: &mut Option<tokio::sync::broadcast::Receiver<()>>) -> Option<()> {
    use tokio::sync::broadcast::error::RecvError;
    match r {
        Some(r) => match r.recv().await {
            Ok(()) | Err(RecvError::Lagged(_)) => Some(()),
            Err(RecvError::Closed) => None,
        },
        None => std::future::pending().await,
    }
}

/// 额度账显示变了 ⇒ `true`；通道没了 ⇒ `false`（调用方把它摘掉）；没订 ⇒ 永远不醒。
async fn quota_moved(q: &mut Option<tokio::sync::watch::Receiver<u64>>) -> bool {
    match q {
        Some(r) => r.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

/// 某个会话变了 ⇒ `Some(Some(sid))`；落后丢了几件 ⇒ `Some(None)`；通道没了 ⇒ `None`（调用方把它摘掉）；没订 ⇒ 永远不醒。
async fn rotation_moved(
    r: &mut Option<tokio::sync::broadcast::Receiver<String>>,
) -> Option<Option<String>> {
    use tokio::sync::broadcast::error::RecvError;
    match r {
        Some(r) => match r.recv().await {
            Ok(sid) => Some(Some(sid)),
            Err(RecvError::Lagged(_)) => Some(None),
            Err(RecvError::Closed) => None,
        },
        None => std::future::pending().await,
    }
}

/// [`attach`] 的可喂那一半：接收端由调用方给（判据拿小容量通道）。
pub(crate) fn attach_rx(
    rx: tokio::sync::mpsc::Receiver<TapEvent>,
    book: std::sync::Arc<crate::observe::runs::RunBook>,
) -> TapRx {
    TapRx {
        rx,
        router: super::run_route::RunRouter::new(book.clone(), crate::agents::stream_families()),
        book,
        out: std::collections::VecDeque::new(),
        quota: None,
        rotation: None,
        rules: None,
        plan: None,
        bodies: no_bodies,
        tz: crate::Tz::default(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/tap_tests.rs"]
mod tests;
