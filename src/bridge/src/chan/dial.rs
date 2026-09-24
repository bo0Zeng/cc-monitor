//! 通道 · **外部前端的宿主那一侧**：拿着交接件，连上回环口、出示钥匙，换一个客户端。
//!
//! # 为什么它不在 `client.rs` 里
//!
//! `client.rs` 是通信层成员，按 `C5` 只使用**别人交给它**的流。「拨到哪个地址」
//! 是外部前端宿主的事 —— 地址来自交接件，交接件来自 monitor 写进 stdin 的那一份。
//! ⇒ 拨号这一下放在成员之外，与 monitor 那一侧 `host.rs` 绑口的道理对称。
//!
//! # 买到 / 买不到
//!
//! **买到**：外部前端一行就能连上（[`dial`]），期限照样是调用者给的 `Budget`，
//! 拨号那一段超时答 `Hop{第 0 跳 open, NotSent, Overrun}`。
//! **买不到**：重拨。断了之后要不要再拨、隔多久拨，是外部前端自己的策略，本文件不替它定。

use super::client::Client;
use super::host::Handoff;
use super::wire::{Budget, CallError, HopFault, HopId, OursFault, Reach};

/// 连上交接件里那个口并出示钥匙。
///
/// # Errors
///
/// 连不上 ⇒ `Hop{第 0 跳 open, NotSent, Unreachable}`；期限内没连上 ⇒ `…, Overrun`；
/// 撤了 ⇒ `Ours{Cancelled}`；其余见 [`Client::open`]。
pub async fn dial(h: &Handoff, budget: Budget) -> Result<Client, CallError> {
    let open = |why: HopFault| CallError::Hop {
        at: HopId {
            idx: 0,
            tag: "open",
        },
        reach: Reach::NotSent,
        why,
    };
    let connected = tokio::select! {
        r = tokio::time::timeout_at(budget.deadline(), tokio::net::TcpStream::connect(h.addr)) => r,
        () = budget.cancel.cancelled() => return Err(CallError::Ours { why: OursFault::Cancelled }),
    };
    let stream = match connected {
        Ok(Ok(s)) => s,
        Ok(Err(_)) => return Err(open(HopFault::Unreachable)),
        Err(_elapsed) => return Err(open(HopFault::Overrun)),
    };
    Client::open(stream, &h.key, h.frame, budget).await
}
