//! 会话输入队那三条的**跨台那一跳**：队只住会话所在那台（主人），写只发给主人。
//!
//! - 入参里的 `machine` 是本机 ⇒ 就地做（阻塞那一段挪到阻塞线程上）。
//! - 是别台 ⇒ 经 [`crate::dial::remote_ask::ask_resident`] 把同一条命令交给那台的常驻（`machine` 换成那台自己眼里的本机），
//!   序号由那台发；那台的回执 · 那台答的码原样交回。
//! - 那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）⇒ 当场拒、码 `offline`，那一句照连接层写的；
//!   本机一个字节不写、不攒着回头再发。
//!
//! 跨台发信（`bus-send`：收件箱住收件会话那台）走同一跳，只用别台那一半：[`hop`]。

use crate::control::cc_bus_wire::BusEnvelopeReq;
use crate::dial::remote_ask::{Remote, Said, Table};
use crate::history::session_queue_wire::{
    SessionQueueCancelReq, SessionQueuePutReq, SessionQueueReadReq,
};
use crate::platform::child::{Deadline, Until};
use crate::wire::spec::Fail;
use crate::wire::typed::Command;
use cmd_wire::Machine;

/// 交给那台常驻最多等多久（发起方给得更短 ⇒ 照发起方）。
const HOP_CAP: Deadline = Deadline::secs(10);

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
    SessionQueueReadReq,
    BusEnvelopeReq
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
    hop::<C>(req, ours, tz, until, table, remote).await
}

/// 别台那一半：经那台的常驻做同一条（`machine` 换成那台眼里的本机）。`ours` 同 [`route`]。
pub(crate) async fn hop<C>(
    req: &C::Req,
    ours: &[&str],
    tz: &crate::Tz,
    until: Option<Until>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<C::Reply, Fail>
where
    C: Command,
    C::Req: Aimed + serde::Serialize,
    C::Reply: serde::de::DeserializeOwned,
{
    let Some(host) = req.machine().host_name().map(str::to_string) else {
        return Err(Fail::new(
            "bad_args",
            crate::common::contract::malformed("`machine` must name a machine"),
        ));
    };
    crate::dial::remote_ask::ask_resident::<C>(
        &host,
        &req.here(),
        tz,
        Deadline::within(HOP_CAP, until),
        table,
        remote,
    )
    .await
    .map_err(|Said { code, message }| match code.as_deref() {
        Some(c) if ours.contains(&c) => Fail::new(c, message),
        _ => Fail::new("offline", message),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_queue_hop_tests.rs"]
mod tests;
