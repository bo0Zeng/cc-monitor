//! 通道这一跳没成 ⇒ 给人看的那一句。全产品只这一份：壳交给主界面的失败（`Fail.said`）与文件窗口的失败都从这里取。
//! 按「发没发出去 × 断在哪一种」说；跳号、标签、哪一段只进复制详情（[`super::wire::HopFacts`]）。

use super::wire::{CallError, HopFault, OursFault, PeerFault, Reach};
use crate::origin::{Origin, LOCAL};
use cmd_wire::machine::Blank;
use copy_core::copy_text;

/// 给人看的那台的称呼：本机 ⇒「本机」，远端 ⇒ 它的名字。
pub fn machine(origin: &Origin) -> String {
    if origin.is_local() {
        copy_core::local_machine()
    } else {
        origin.to_string()
    }
}

/// 那一句。**穷尽 `match`，不许 `_ =>`。**
pub fn said(e: &CallError, origin: &Origin) -> String {
    let name = machine(origin);
    let name = name.as_str();
    match e {
        CallError::Hop { reach, why, .. } => match (reach, why) {
            (Reach::NotSent, HopFault::Overrun) => copy_text("rsChanSaid.hop.expired", &[]),
            (Reach::NotSent, HopFault::Unreachable | HopFault::Dropped) if origin.is_local() => {
                copy_text("rsChanSaid.hop.localDown", &[])
            }
            (Reach::NotSent, HopFault::Unreachable | HopFault::Dropped) => {
                copy_text("rsChanSaid.hop.remoteDown", &[("machine", name)])
            }
            (Reach::Sent | Reach::Unknown, HopFault::Overrun) => {
                copy_text("rsChanSaid.hop.overrun", &[("machine", name)])
            }
            (Reach::Sent | Reach::Unknown, HopFault::Unreachable | HopFault::Dropped) => {
                copy_text("rsChanSaid.hop.dropped", &[("machine", name)])
            }
        },
        CallError::Peer {
            why: PeerFault::Unsupported,
        } => copy_core::backend_old(name),
        CallError::Peer {
            why: PeerFault::Refused { body },
        } => refused(&said_of(&body.0), origin),
        CallError::Ours { why, runs_on } => match why {
            OursFault::Cancelled if *runs_on => {
                copy_text("rsChanSaid.ours.cancelledRunsOn", &[("machine", name)])
            }
            OursFault::Cancelled => copy_text("reason.op.cancelled", &[]),
            OursFault::Misuse | OursFault::Broken => copy_text("rsShellCmd.self.crashed", &[]),
        },
    }
}

/// 那台离线那一句（`machine` 是给人看的称呼）。全产品只这一处取字：连接层交出的离线那一句都从这里来，出口照摆。
pub fn offline(machine: &str) -> String {
    copy_text("link.machine.offline", &[("machine", machine)])
}

/// 机器名空白、`route` 拒了那一句：点名是哪条命令，并逐字给出本机该送什么。
pub fn blank_origin(b: &Blank) -> String {
    copy_text(
        "rsOrigin.route.blank",
        &[("command", &b.command), ("local", LOCAL)],
    )
}

/// 对端拒了：它写的那一句原样；没写（空 · 体读不出）⇒「{machine} 后端拒绝 · 原因不明」。
pub fn refused(said: &str, origin: &Origin) -> String {
    if said.trim().is_empty() {
        copy_text(
            "rsChanSaid.peer.refusedNoReason",
            &[("machine", &machine(origin))],
        )
    } else {
        said.to_string()
    }
}

/// 拒绝体 `{code, said, …}`（宿主拼的，`backend_route::refusal_body`）里那一句；读不出 ⇒ 空串。
fn said_of(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("said")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../../../../tests/comms/inward/chan/said_tests.rs"]
mod tests;
