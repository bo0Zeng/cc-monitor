//! 会话输入队那三条的**跨台那一跳**：队只住会话所在那台（主人），写只发给主人。
//!
//! - 入参里的 `machine` 是本机 ⇒ 就地做（阻塞那一段挪到阻塞线程上）。
//! - 是别台 ⇒ 经 [`crate::dial::remote_ask::hop`] 把同一条命令交给那台的常驻（`machine` 换成那台自己眼里的本机），
//!   序号由那台发；那台的回执 · 那台答的码原样交回。
//! - 那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）⇒ 当场拒、码 `offline`，那一句照连接层写的；
//!   本机一个字节不写、不攒着回头再发。

#[cfg(test)]
use crate::dial::remote_ask::Said;
use crate::dial::remote_ask::{Remote, Table};
use crate::platform::child::Until;
use crate::wire::spec::Fail;
use crate::wire::typed::Command;
use cmd_wire::queue::{SessionQueueCancelReq, SessionQueuePutReq, SessionQueueReadReq};
use cmd_wire::Machine;

/// 三条的入参都带「会话在哪台」：读它 · 换成那台自己眼里的本机。
pub(crate) trait Aimed {
    fn machine(&self) -> &Machine;
    /// 同一份入参，`machine` 换成本机（交给那台时它就是本机）。
    fn here(&self) -> Self;
}

macro_rules! aimed {
    ($($t:ty),*) => {$(
        impl Aimed for $t {
            fn machine(&self) -> &Machine {
                &self.machine
            }
            fn here(&self) -> Self {
                Self {
                    machine: Machine::local(),
                    ..self.clone()
                }
            }
        }
    )*};
}
aimed!(
    SessionQueuePutReq,
    SessionQueueCancelReq,
    SessionQueueReadReq
);

/// 照 `req.machine()` 分：本机 ⇒ `here`（阻塞线程上跑）；别台 ⇒ 交给那台的常驻。
/// `ours` ＝ 这一条码表里的码：那台答了其中一个（那台说「不行」，不是够不到）⇒ 原样交回；别的 ⇒ `offline`。
pub(crate) async fn route<C, H>(
    req: &C::Req,
    here: H,
    ours: &[&str],
    tz: &crate::Tz,
    until: Option<Until>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<C::Reply, Fail>
where
    C: Command,
    C::Req: Aimed + serde::Serialize,
    C::Reply: serde::de::DeserializeOwned + Send + 'static,
    H: FnOnce() -> Result<C::Reply, Fail> + Send + 'static,
{
    let machine = req.machine();
    if machine.is_local() {
        return tokio::task::spawn_blocking(here).await.unwrap_or_else(|e| {
            Err(Fail::from((
                "io_failed",
                crate::common::said::Said::with_raw(
                    copy_core::copy_text("reason.code.bug", &[]),
                    e,
                ),
            )))
        });
    }
    crate::dial::remote_ask::hop::<C>(machine, &req.here(), ours, tz, until, table, remote).await
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_queue_hop_tests.rs"]
mod tests;
