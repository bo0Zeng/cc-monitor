//! F04c：**「这条命令能不能回落」的唯一判定**（monitor 侧走后端的所有控制命令共用）。
//!
//! # 为什么它必须只有一份
//!
//! F04b 给 `kill` 定下了三态分流，F04c 给 `send-keys` 也要同一套。**这条规则一旦有两份实现，
//! 它们就会漂**，而漂开的后果不是「行为不一致」这么轻 —— 它是**静默的权限旁路**：
//! 把后端的一次 `wrong_owner` 当成「backend 不可用」而回落到 SSH 路再做一次，
//! 等于把**一次被门拒绝洗成另一条路的成功**。
//! 今天两条路的门恰好等价（都是 §34 三道门）所以功能上看不出差别 —— **那正是它危险的地方**。
//!
//! ⇒ 同 `gate-core` 的手法（定框 C1「一份代码、两种承载」的同一条纪律）：判定收成一份，
//! 调用方只负责给「拒绝该怎么对用户说」。由 `every_backend_sender_is_registered_and_uses_the_one_router`
//! 钉住 —— ⚠ **它的发现机制是遍历目录，不是手写清单**（F12 的 `/full-audit` 逮到手写那版
//! 漏掉了第三个发送端，见那条判据的头注）。
//!
//! # 分界线不是「成功/失败」，是「**能不能证明这条命令根本没发出去**」
//!
//! 逐档读 `inbound_client::call` 的源码定的（不是猜）：
//!
//! | 档 | 在 `call` 里的位置 | 判定 |
//! |---|---|---|
//! | `client_for(origin) == None` | 连 client 都没有，一个字节没发 | **证明没发出去** ⇒ 可回落 |
//! | `Unsupported` | `call` 第一行 `if !self.accepts(cmd)`，早于 `next_id`/`register`/`send` | **证明没发出去** ⇒ 可回落（旧后端） |
//! | `TooManyPending` | `register(&id)` 失败，仍早于 `writes.send` | **证明没发出去** ⇒ 可回落 |
//! | `Disconnected` | **两个产地**：写队列 send 失败（没入队）**或** 等应答时 `rx` 掉了（已发出） | 分不开 ⇒ 按最坏算 ⇒ **不回落** |
//! | `Timeout` | **两个产地**：写入段超时（源码逐字写着「backend 没见过这条命令」）**或** 等应答段超时（可能已执行） | 分不开 ⇒ 按最坏算 ⇒ **不回落** |
//! | `Cancelled` / `Remote{..}` | backend 说过话了 | **不回落** |
//!
//! ⚠ **诚实边界**（F04b 记的，仍未变）：`Timeout` 与 `Disconnected` 各有两个产地，
//! 一个能证明没发出去、一个不能，而**类型上分不开**。这里只能按最坏的那个处理 ⇒
//! 一次写入段超时会让用户拿到错误而不是回落。要修得在 `inbound_client` 那边把两个产地
//! 分成两个变体 —— **那是它自己的活**。记在这里，别让下一个人以为是漏了。

use crate::inbound_client::CallError;

/// 一条走后端的控制命令的结局。**三态**，分界线见模块头注。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Routed {
    /// backend 确认做完了。
    Done,
    /// **证明**这条命令没发出去 ⇒ 调用方可以回落到过渡期的 SSH 路径（C7）。
    /// 带上原因只为诊断，**不参与分流判断**。
    NoChannel(String),
    /// backend 说了话（拒绝 / 失败），**或者**我们无法证明它没执行 ⇒
    /// **不许回落**，把这句话原样交给用户。
    Refused(String),
}

/// `CallError` → 三态。`refusal` 只负责把 `(code, message)` 翻成用户看的话 ——
/// **分流本身不许由调用方决定**，那就是本模块存在的全部意义。
pub(crate) fn route_call_error(e: &CallError, refusal: impl Fn(&str, &str) -> String) -> Routed {
    match e {
        // 以下三档都在 `call()` 真正写出去**之前**返回 —— 见模块头注那张表。
        CallError::Unsupported { cmd, offered } => Routed::NoChannel(format!(
            "远端后端没声明 `{cmd}` 能力（它声明的是 {offered:?}）—— 多半是旧版本"
        )),
        CallError::TooManyPending => {
            Routed::NoChannel("入方向同时在等的命令已达上限，这条没入队".into())
        }
        // 以下都不能证明「没发出去」⇒ 按最坏算，不回落。
        CallError::Disconnected | CallError::Timeout { .. } | CallError::Cancelled => {
            Routed::Refused(format!(
                "{e} —— ⚠ 无法确认远端是否已经执行过这条命令，因此**不**再用另一条路重做一次；\
                 请刷新会话列表后再决定"
            ))
        }
        CallError::Remote { code, message } => Routed::Refused(refusal(code, message)),
    }
}

/// 没有控制通道（`client_for` 回 `None`）—— **一个字节都没发出去**，可回落。
pub(crate) fn no_channel(origin: &str) -> Routed {
    Routed::NoChannel(format!(
        "[{origin}] 没有可用的控制通道（backend 未在场或长连接未握手）"
    ))
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/backend_route_tests.rs"]
mod tests;
