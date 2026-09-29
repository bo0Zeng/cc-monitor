//! 〔TAP · V124〕**tee 的消费侧（后端这一半）**：常驻后端进程内那一份中转抄出来的 SSE 事件，
//! 经这里交给**当前那条流连接**的写者，变成 `tap` 帧（`wire::Frame::Tap`）。
//!
//! # 形状（设计住仓外 `调研/第四波记录/TAP.md §1.1`；出处 `设计/20 §8`「走常驻后端现有的那条 wire，不另开通道」）
//!
//! ```text
//! 中转转发线程 ── TeeSink(tap 口) ── TapHub::offer ── try_send ──▶ 有界通道（每条流连接一条）
//!                                                                    │
//!                     main.rs::writer_task：出方向 ＞ 应答 ＞ tap（最低优先）──▶ wire
//! ```
//!
//! - **一个进程一个 hub**（[`hub`]），装着「此刻那条流连接」的发送端。流连接接上 ⇒ [`TapHub::attach`] 新建一条通道、
//!   发送端换进来、接收端交那条连接的 `writer_task`；那条连接走了 ⇒ 接收端没了，`try_send` 失败 ⇒ 当场丢。
//!   没人连着时照丢 —— 没有人可说，而位置号 `n` 照占，下一个接上的人看得见「这个响应缺了前面几件」。
//! - **不许阻塞转发**（`设计/05 §4.5.3` ③：抄流跟不上绝不回推主路）：只有 `try_send`。
//! - **不许挤掉内容帧**：tap 走**自己的**通道，不进出方向那条 10 000 容量的大通道（那条满了会丢 `line` 帧）；
//!   `writer_task` 只在出方向与应答都空时才取它。
//!
//! # 上界（`05 §3.3.4`：级 3 禁止）
//!
//! [`TAP_CAPACITY`] 件 × 每件原文 ≤ `relay::TAP_DATA_CAP` ⇒ 这一跳最坏 4 MiB。满了落级 2（丢，位置号原位说）。

use crate::relay::{TapBody, TapEvent, TapPort};
use crate::stream::wire::{Frame, TapEnd};

/// tap 通道能排多少**件**（每条流连接一条）。
///
/// ⚠ **是条数不是体量**（体量由 `relay::TAP_DATA_CAP` 封）。值：token 级的 SSE 一秒几十到上百件，
/// 256 件够吸收写者被内容帧占住的一小段；再多攒着只是让活卡更晚，不如丢了让 jsonl 定稿。
/// 登记住址 `src/frontend/shell/src/byte_cap_registry.rs` 的 `NOT_A_SIZE_CAP`。
pub(crate) const TAP_CAPACITY: usize = 256;

/// 进程级 hub：此刻每条流连接的 tap 发送端。
/// 〔HOST · `设计/01 §3.3b ⑥`〕多客户：每条连接一条，扇出；那条连接走了（接收端没了）下一次交的时候摘掉。
#[derive(Default)]
pub(crate) struct TapHub {
    current: std::sync::Mutex<Vec<tokio::sync::mpsc::Sender<TapEvent>>>,
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

impl TapPort for TapHub {
    /// 立刻答收没收（`try_send`）：每条连接各交一份，至少一条收下 ⇒ `true`。没人连着 / 都满 ⇒ `false`
    /// （号已由 tee 占掉，缺口在各自接收侧可算）。已走的连接当场摘掉。
    fn offer(&self, ev: TapEvent) -> bool {
        let mut g = self.current.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|tx| !tx.is_closed());
        let mut took = false;
        for tx in g.iter() {
            took |= tx.try_send(ev.clone()).is_ok();
        }
        took
    }
}

/// 进程级那一个 hub。中转（`relay::host`）拿它当 tap 口，两条载体的写者从它 `attach`。
pub(crate) fn hub() -> std::sync::Arc<TapHub> {
    static HUB: std::sync::OnceLock<std::sync::Arc<TapHub>> = std::sync::OnceLock::new();
    std::sync::Arc::clone(HUB.get_or_init(Default::default))
}

/// 一件 tee 事件 → 一个 `tap` 帧（字段一一照搬，不解释 `data` 的内容）。
pub(crate) fn to_frame(ev: TapEvent) -> Frame {
    let (data, end) = match ev.body {
        TapBody::Data(d) => (Some(d), None),
        TapBody::End { broken } => (
            None,
            Some(if broken { TapEnd::Broken } else { TapEnd::Done }),
        ),
    };
    Frame::Tap {
        stream: ev.stream,
        resp: ev.resp,
        n: ev.n,
        data,
        end,
    }
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

/// 一条流连接的 tap 接收端（写者那一侧拿它）。只交出**帧** —— tee 的事件类型不出本 crate。
pub struct TapRx(tokio::sync::mpsc::Receiver<TapEvent>);

impl TapSource for TapRx {
    async fn next(&mut self) -> Option<Frame> {
        self.0.recv().await.map(to_frame)
    }
}

/// 一条流连接接上了 ⇒ 从进程级 hub 拿一条新的 tap 接收端（两条载体各在「流开始」那一处调一次）。
pub fn attach() -> TapRx {
    TapRx(hub().attach())
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/tap_tests.rs"]
mod tests;
