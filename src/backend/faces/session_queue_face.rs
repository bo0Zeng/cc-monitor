//! 会话输入队那三条（typed）：`session-queue-put`（投进队）· `session-queue-cancel`（撤）· `session-queue-read`（读名单）。
//! 本体在 `history/session_queue.rs`（判定与写都在那里）；会话不在这台 ⇒ 经 [`super::session_queue_hop`] 交给那台。
//! 这里只接上名字 · 入参 · 回包 · 码表与写句柄档：三条都读写后端自家状态（`~/.cc-monitor/`）⇒ 要写句柄那一档。

use super::session_queue_hop::route;
use crate::dial::remote_ask::{DialRemote, REACH};
use crate::history::session_queue as q;
use crate::history::session_queue_wire::{
    QueueCancelled, QueuePut, SessionQueue, SessionQueueCancelReq, SessionQueuePutReq,
    SessionQueueReadReq,
};
use crate::wire::typed::{self, codes, Codes as _, Command};

codes! {
    /// `session-queue-put` 的码。
    pub(crate) enum SessionQueuePutErr {
        /// 那台的家目录解析不出来 · 建目录 · 拿锁 · 写没成。
        IoFailed = "io_failed",
        /// 盘上那一份读不懂 · 版本比这一版新（不覆盖）。
        QueueUnreadable = "queue_unreadable",
        /// 那条会话的队满了。
        QueueFull = "queue_full",
        /// 那一句太长。
        TextTooLong = "text_too_long",
        /// 会话所在那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）：本机一个字节没写。
        Offline = "offline",
    }
}

/// `session-queue-put`：往一条会话投一句（序号由会话所在那台发）。
pub(crate) struct SessionQueuePutCmd;

impl Command for SessionQueuePutCmd {
    const NAME: &'static str = "session-queue-put";
    type Req = SessionQueuePutReq;
    type Reply = QueuePut;
    type Err = SessionQueuePutErr;
}

impl typed::Handle for SessionQueuePutCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: SessionQueuePutReq,
        cx: typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<QueuePut, SessionQueuePutErr>> + Send + 'static
    {
        let (cx, own) = cx.split();
        async move {
            let r = req.clone();
            route::<Self, _>(
                &req,
                move || q::put(&own, &r, crate::common::time::now_ms()),
                SessionQueuePutErr::ALL,
                &cx.tz,
                cx.until,
                &REACH,
                &DialRemote,
            )
            .await
            .map_err(|f| typed::of_fail(f, SessionQueuePutErr::IoFailed))
        }
    }
}

codes! {
    /// `session-queue-cancel` 的码。
    pub(crate) enum SessionQueueCancelErr {
        /// 同 `session-queue-put`。
        IoFailed = "io_failed",
        /// 同 `session-queue-put`。
        QueueUnreadable = "queue_unreadable",
        /// 同 `session-queue-put`。
        Offline = "offline",
    }
}

/// `session-queue-cancel`：撤一条还没送的（送过的 ⇒ `cancelled: false`），回撤完的整份名单。
pub(crate) struct SessionQueueCancelCmd;

impl Command for SessionQueueCancelCmd {
    const NAME: &'static str = "session-queue-cancel";
    type Req = SessionQueueCancelReq;
    type Reply = QueueCancelled;
    type Err = SessionQueueCancelErr;
}

impl typed::Handle for SessionQueueCancelCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: SessionQueueCancelReq,
        cx: typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<QueueCancelled, SessionQueueCancelErr>> + Send + 'static
    {
        let (cx, own) = cx.split();
        async move {
            let r = req.clone();
            route::<Self, _>(
                &req,
                move || q::cancel(&own, &r.sid, &r.id, crate::common::time::now_ms()),
                SessionQueueCancelErr::ALL,
                &cx.tz,
                cx.until,
                &REACH,
                &DialRemote,
            )
            .await
            .map_err(|f| typed::of_fail(f, SessionQueueCancelErr::IoFailed))
        }
    }
}

codes! {
    /// `session-queue-read` 的码。
    pub(crate) enum SessionQueueReadErr {
        /// 同 `session-queue-put`。
        IoFailed = "io_failed",
        /// 同 `session-queue-put`。
        QueueUnreadable = "queue_unreadable",
        /// 同 `session-queue-put`。
        Offline = "offline",
    }
}

/// `session-queue-read`：一条会话此刻的名单（问会话所在那台）。
pub(crate) struct SessionQueueReadCmd;

impl Command for SessionQueueReadCmd {
    const NAME: &'static str = "session-queue-read";
    type Req = SessionQueueReadReq;
    type Reply = SessionQueue;
    type Err = SessionQueueReadErr;
}

impl typed::Handle for SessionQueueReadCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: SessionQueueReadReq,
        cx: typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SessionQueue, SessionQueueReadErr>> + Send + 'static
    {
        let (cx, own) = cx.split();
        async move {
            let r = req.clone();
            route::<Self, _>(
                &req,
                move || q::read(&own, &r.sid, crate::common::time::now_ms()),
                SessionQueueReadErr::ALL,
                &cx.tz,
                cx.until,
                &REACH,
                &DialRemote,
            )
            .await
            .map_err(|f| typed::of_fail(f, SessionQueueReadErr::IoFailed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_queue_face_tests.rs"]
mod tests;
