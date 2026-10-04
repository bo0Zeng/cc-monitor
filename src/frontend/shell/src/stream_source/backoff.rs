//! 重连退避：上下界、「这条连接算不算站住了」、一轮结束之后怎么办。

use std::time::Duration;

/// 重连退避下界：每次连接掉线后至少等这么久再重连（也是连上过之后的快速重连值）。
pub(super) const RECONNECT_MIN: Duration = Duration::from_secs(2);
/// 重连退避上界：指数退避封顶，避免长断网时无意义地拉长重连间隔。
const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// ★ **「本次连接算不算健康」的最短存活时长**〔audit-0805 F05 / 报告 I-1〕。
///
/// # 为什么不能拿「收到过 hello」当健康
///
/// `connected` 是在**收到 backend hello 的那一刻**置位的（`stream_loop` 里那句
/// `connected.store(true)`）。于是一个「发完 hello 就死」的 backend —— 比如小机器上
/// 整读 jsonl 触发 OOM 被杀 —— 每一轮都算「连上过」⇒ 退避每次都被重置回 2 秒
/// ⇒ **永远不增长**。而每次重连要付 3 次完整 SSH 登录（arch 探测 / SFTP 预检 / 起流），
/// 折算约 **90 次握手/分钟/台**，正好压在那台已经撑不住的机器上。
///
/// ⇒ 判据换成「**这条连接活过了多久**」：hello 只说明握手成功，活过 30 秒才说明它真站住了。
///
/// ⚠ 30 秒的取法：要明显长于「起流 + 首批帧」的正常耗时（冷启动实测约 0.9 s @30ms RTT、
/// 约 6 s @200ms RTT，见），又要短到不至于让一次真实的网络抖动被当成 flapping。
const MIN_HEALTHY_UPTIME: Duration = Duration::from_secs(30);

/// 纯函数：本轮连接结束后，退避该不该重置回 [`RECONNECT_MIN`]。
///
/// 两个条件**都要满足**：握手成功过（`saw_hello`）**且**这条连接活过 [`MIN_HEALTHY_UPTIME`]。
/// 只看前者就是 I-1 那个自激循环；只看后者会把「连了很久但从没握手成功」也当健康。
pub(super) fn should_reset_backoff(saw_hello: bool, lived: Duration) -> bool {
    saw_hello && lived >= MIN_HEALTHY_UPTIME
}

/// 一轮连接结束之后怎么办。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AfterRound {
    /// 按退避再连。
    RetryIn(Duration),
    /// 那台永久不支持（非 unix）⇒ 不再自动重连，这条流收工（带那句话）。
    Stop(String),
}

/// 纯函数：这一轮记下了「永久不支持」⇒ 停；否则按当前退避再连（非 unix 不按退避空转）。
pub(crate) fn after_round(unsupported: Option<String>, backoff: Duration) -> AfterRound {
    match unsupported {
        Some(why) => AfterRound::Stop(why),
        None => AfterRound::RetryIn(backoff),
    }
}

/// 纯函数：把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。run() 的重连循环在"仍未连上"时调用。
pub(super) fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/tier1_tests.rs"]
mod tier1_tests;
