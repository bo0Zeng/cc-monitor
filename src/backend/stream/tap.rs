//! **tee 的消费侧（后端这一半）**：常驻后端进程内那一份中转抄出来的 SSE 事件交给整台一份的活卡（[`super::live`]），
//! 活卡按每条流连接的名单把 `live` 帧排进它的发件箱；这条连接的写者最低优先取。
//!
//! ```text
//! 中转转发线程 ── TeeSink(tap 口) ── TapHub::offer ──▶ Live（锁里折、记账、分发件箱）──Notify──▶ TapRx ──▶ main.rs::writer_task（出方向 ＞ 应答 ＞ 这一路）──▶ wire
//! ```
//!
//! - **一个进程一个 hub**（[`hub`]）。没人连着时照收：卡照样长，后来接上的连接先拿到每张卡的整张。
//! - **不许阻塞转发**：`offer` 只在锁里做内存里的折与记账，不等 IO、不等任何连接。
//! - **不许挤掉内容帧**：这一路不进出方向那条大通道；`writer_task` 只在出方向与应答都空时才取它。
//! - 上界：每条连接的发件箱 ≤ `live::LIVE_OUTBOX_CAP` 件，满了合成整卡（见 `live.rs` 头注）。

use crate::relay::{TapEvent, TapPort};
use crate::stream::live::Live;
use crate::stream::wire::{Frame, Topic};
use std::sync::{Arc, Mutex};

/// 进程级 hub：整台一份的活卡。
pub(crate) struct TapHub {
    live: Mutex<Live>,
}

impl Default for TapHub {
    fn default() -> Self {
        Self::new(crate::agents::stream_families())
    }
}

impl TapHub {
    /// 一份 hub：流按这几家的协议面折（生产 ＝ 注册表里的各家；判据给假适配层那一形）。运行簿先是空的一本，起 watcher 时 [`adopt`]。
    pub(crate) fn new(families: Vec<crate::agents::StreamFamily>) -> Self {
        Self {
            live: Mutex::new(Live::new(
                Arc::new(crate::observe::runs::RunBook::default()),
                families,
            )),
        }
    }

    pub(crate) fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TapPort for TapHub {
    /// 中转看见的这一发请求里名单上那几项在不在 ⇒ 记进会话的请求标记（会话事实定上下文上限时读）。
    fn note_marks(&self, stream: &str, marks: &[((&'static str, &'static str), bool)]) {
        crate::observe::relay_marks::note(stream, marks);
    }

    /// 交给活卡（锁里只做内存里的事）。恒收：号已由 tee 占掉，卡上的缺口由活卡判。
    fn offer(&self, ev: TapEvent) -> bool {
        self.live().tap(ev);
        true
    }
}

impl crate::observe::runs::BookWatch for TapHub {
    fn rid(&self, sid: &str, rid: &str, owner: Option<Option<String>>) {
        self.live().record(sid, rid, owner);
    }
    fn gone(&self, sid: &str) {
        self.live().gone(sid);
    }
}

/// 进程级那一个 hub。中转（`relay::host`）拿它当 tap 口，两条载体的写者从它接。
pub(crate) fn hub() -> Arc<TapHub> {
    static HUB: std::sync::OnceLock<Arc<TapHub>> = std::sync::OnceLock::new();
    Arc::clone(HUB.get_or_init(Default::default))
}

/// 中转（`relay::host`）要的那个 tap 口：就是进程级那一个 hub。
pub fn port() -> Arc<dyn TapPort> {
    hub()
}

/// 起这台 watcher 之后一次：活卡改用它写的那一本运行簿（流归位按它定归哪个运行），运行簿记录到了 / 会话走了告诉活卡。
pub fn adopt(book: &Arc<crate::observe::runs::RunBook>) {
    let h = hub();
    h.live()
        .adopt(book.clone(), crate::agents::stream_families());
    book.watch_with(h);
}

/// 写者那一侧要的东西：「最低优先那一路的下一帧」。写者（`main.rs::writer_task`）只认这个口，不认帧从哪来 ——
/// 生产里是 [`TapRx`]（活卡的发件箱）；判据拿一条手喂的帧通道当它，量「这一路灌满时内容帧一条不少」。
pub trait TapSource {
    /// 下一帧；`None` = 这条来源不会再有东西了（发送端已被换掉）。
    fn next(&mut self) -> impl std::future::Future<Output = Option<Frame>> + Send;
}

/// 一条流连接最低优先的那一路（写者那一侧拿它）：活卡的发件箱 ＋ 几种 `changed` 推送。只交出**帧**。
pub struct TapRx {
    /// 活卡那一头（哪个 hub · 这条连接的号 · 叫醒口）。
    hub: Arc<TapHub>,
    id: u64,
    wake: Arc<tokio::sync::Notify>,
    /// 轮换的节点通道（[`crate::accounts::quota::nodes`]）：事实变了推 `changed {quota}`；某个会话那一条变了推
    /// `changed {rotation, key: sid}`；策略目录 · 新会话用哪份变了推 `changed {rotation_strategies}`。`None` ＝ 不订（判据自己接的那一形）。
    nodes: Option<tokio::sync::broadcast::Receiver<crate::accounts::quota::nodes::Node>>,
    /// 某个 pb 工作区的计划变了的通道（变了推一帧 `changed {plan, key: 工作区, rev, body: {needs}}`）；`None` ＝ 不订。
    plan: Option<tokio::sync::broadcast::Receiver<crate::plan::watch::Change>>,
    /// 某个会话挂着的待答请求变了的通道（挂上 · 撤掉 · 答掉，推一帧 `changed {needs, key: sid}`）；`None` ＝ 不订。
    needs: Option<tokio::sync::broadcast::Receiver<String>>,
    /// 会话外的需手动清单可能变了的铃（推一帧 `changed {manual}`）；`None` ＝ 不订。
    manual: Option<tokio::sync::broadcast::Receiver<()>>,
    /// 这一端的机器表变了的铃（推一帧 `changed {machines}`）；`None` ＝ 不订。
    machines: Option<tokio::sync::broadcast::Receiver<()>>,
    /// 帧里的小成品怎么现算（生产 ＝ [`crate::stream::topic_hook::body`]；判据自己接的那一形不现算）。
    bodies: crate::stream::topic_hook::Bodies,
    /// 这条连接看的那一台的时区（attach 带来的）：小成品里的时刻字按它写。
    tz: crate::Tz,
    out: std::collections::VecDeque<Frame>,
}

impl TapRx {
    /// 这条连接的号（`stream-watch` 那一处同拍把名单交给活卡时用）。
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 这条连接此刻在看哪几个会话（整份换；与 watcher 那份名单同拍交）。
    pub fn watch_handle(&self) -> LiveWatch {
        LiveWatch {
            hub: self.hub.clone(),
            id: self.id,
        }
    }
}

/// 一条连接把名单交给活卡的口（可以有好几份）。
#[derive(Clone)]
pub struct LiveWatch {
    hub: Arc<TapHub>,
    id: u64,
}

impl LiveWatch {
    pub fn watch(&self, sids: &[String]) {
        self.hub.live().watch(self.id, sids);
    }
}

impl Drop for TapRx {
    fn drop(&mut self) {
        self.hub.live().leave(self.id);
    }
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
            if let Some(f) = self.hub.live().pull(self.id) {
                return Some(f);
            }
            tokio::select! {
                () = self.wake.notified() => {}
                moved = node_moved(&mut self.nodes) => match moved {
                    Some(fs) => {
                        for (t, sid) in fs {
                            let f = self.changed(t, sid);
                            self.out.push_back(f);
                        }
                    }
                    None => self.nodes = None,
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
                },
                moved = bell_moved(&mut self.manual) => match moved {
                    // 落后丢了几件也照推一帧（客户端反正整份重问）。
                    Some(()) => {
                        let f = self.changed(Topic::Manual, None);
                        self.out.push_back(f);
                    }
                    None => self.manual = None,
                },
                moved = bell_moved(&mut self.machines) => match moved {
                    // 落后丢了几件也照推一帧（客户端反正整份重问）。
                    Some(()) => {
                        let f = self.changed(Topic::Machines, None);
                        self.out.push_back(f);
                    }
                    None => self.machines = None,
                },
                moved = rotation_moved(&mut self.needs) => match moved {
                    Some(Some(sid)) => {
                        let f = self.changed(Topic::Needs, Some(sid));
                        self.out.push_back(f);
                    }
                    // 落后丢了几件：可丢的通道，客户端下一次变化或重问就补上。
                    Some(None) => {}
                    None => self.needs = None,
                }
            }
        }
    }
}

/// 一条流连接接上了 ⇒ 在进程级 hub 的活卡上登记一条连接（两条载体各在「流开始」那一处调一次）。
/// `tz` ＝ 这条连接看的那一台的时区（帧里小成品的时刻字按它写）。
pub fn attach(tz: crate::Tz) -> TapRx {
    let mut rx = attach_rx(hub());
    rx.nodes = Some(crate::accounts::quota::nodes::bus().subscribe());
    rx.plan = Some(crate::plan::watch::changes().subscribe());
    rx.needs = Some(crate::control::asks::changes().subscribe());
    rx.manual = Some(crate::accounts::sign_in::manual_changes().subscribe());
    rx.machines = Some(crate::control::machines::changes().subscribe());
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

/// 只打铃不带东西的那几条（会话外的需手动清单 · 这一端的机器表）变了 ⇒ `Some(())`（落后丢了几件也算变了）；通道没了 ⇒ `None`；没订 ⇒ 永远不醒。
async fn bell_moved(r: &mut Option<tokio::sync::broadcast::Receiver<()>>) -> Option<()> {
    use tokio::sync::broadcast::error::RecvError;
    match r {
        Some(r) => match r.recv().await {
            Ok(()) | Err(RecvError::Lagged(_)) => Some(()),
            Err(RecvError::Closed) => None,
        },
        None => std::future::pending().await,
    }
}

/// 一件节点 ⇒ 该推的那几帧（topic · key）；落后丢了几件 ⇒ 不知道哪个会话，推不带会话的两帧（客户端整份重问）；
/// 通道没了 ⇒ `None`（调用方把它摘掉）；没订 ⇒ 永远不醒。
async fn node_moved(
    r: &mut Option<tokio::sync::broadcast::Receiver<crate::accounts::quota::nodes::Node>>,
) -> Option<Vec<(Topic, Option<String>)>> {
    use crate::accounts::quota::nodes::Source;
    use tokio::sync::broadcast::error::RecvError;
    let Some(r) = r else {
        return std::future::pending().await;
    };
    match r.recv().await {
        Ok(n) => {
            let mut out = Vec::new();
            if n.source.is_fact() {
                out.push((Topic::Quota, None));
            }
            if n.source == Source::Strategies {
                out.push((Topic::RotationStrategies, None));
            }
            if let Some(sid) = n.sid {
                out.push((Topic::Rotation, Some(sid)));
            }
            Some(out)
        }
        Err(RecvError::Lagged(_)) => Some(vec![
            (Topic::Quota, None),
            (Topic::RotationStrategies, None),
        ]),
        Err(RecvError::Closed) => None,
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

/// [`attach`] 的可喂那一半：活卡由调用方给（判据各造一个 hub）。
pub(crate) fn attach_rx(hub: Arc<TapHub>) -> TapRx {
    let (id, wake) = hub.live().join();
    TapRx {
        hub,
        id,
        wake,
        out: std::collections::VecDeque::new(),
        nodes: None,
        plan: None,
        needs: None,
        manual: None,
        machines: None,
        bodies: no_bodies,
        tz: crate::Tz::default(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/tap_tests.rs"]
mod tests;
