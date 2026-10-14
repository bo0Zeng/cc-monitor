//! 重连退避：上下界、「这条连接算不算站住了」、一轮结束之后怎么办。
//! 两端同一条重连循环（[`crate::feed`]）用它：阈值只在这里。

use std::time::Duration;

/// 重连退避下界：每次连接掉线后至少等这么久再重连（也是连上过之后的快速重连值）。
pub const RECONNECT_MIN: Duration = Duration::from_secs(2);
/// 重连退避上界：指数退避封顶，避免长断网时无意义地拉长重连间隔。
pub const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// **开一条链路（拨号 ＋ 在那台开出那条 exec）等多久没回话就放弃、隔 [`RECONNECT_MIN`] 再拨**。
///
/// 静默断网（网卡还在、包全丢：电梯 · 地铁）里拨出去的那一拨，SYN 石沉大海；内核按 1 · 3 · 7 · 15 · 31 · 63 秒补发，
/// 一拨要挂两分钟。网回来时正挂在后半段的那一拨要等到下一次补发才赶上（断 90 秒那一格量出来 56 秒）。
/// 10 秒放弃：前 7 秒里补发得勤（网回来至多再等 4 秒），7 秒之后丢下、隔下界重拨一拨新的，网回来到重拨上至多 5 秒。
/// 代价：登录 ＋ 开 exec 要 10 秒以上的链路（往返一秒以上、再隔跳板）一拨也接不上。
/// 到期限都没回话的那一拨没够到那一台（没有一次登录落到它身上），不涨退避（[`Backoff::after_wait`] 的 `reached`）。
pub const DIAL_WITHIN: Duration = Duration::from_secs(10);

/// **连接自己的心跳**：流上这么久没来一个字节 ⇒ 问一句活（`ping`）。健康的空闲连接每这么久一个来回（与 SSH 保活同一个数，
/// 问活的回话把 SSH 保活的计时也续上，空闲时线上的包数不变）。
pub const HEARTBEAT_IDLE: Duration = Duration::from_secs(10);

/// 问活之后这么久一个字节都没来 ⇒ 判这条流死了、收掉重接。最后收到字节之后至多 [`HEARTBEAT_IDLE`] ＋ 它（15 秒）判死，
/// 不再等 SSH 保活（最后收包之后 20 秒）；判死之前网就回来了的那种，旧连接上补发的包要等 TCP 的重传退避（断得越久等得越久），
/// 判死重拨一条新的反而快。宿主报「网回来了 · 网变了 · 回前台」时连着 ⇒ 当场问一句（[`crate::feed::Feed::wake`]），这一问也按它判。
pub const PING_WITHIN: Duration = Duration::from_secs(5);

/// ★ **「本次连接算不算健康」的最短存活时长**〔audit-0805 F05 / 报告 I-1〕。
///
/// # 为什么不能拿「收到过 hello」当健康
///
/// 「握上手」是在**收到 backend hello 的那一刻**定的（[`crate::feed`] 接上那一步）。于是一个「发完 hello 就死」的 backend —— 比如小机器上
/// 整读 jsonl 触发 OOM 被杀 —— 每一轮都算「连上过」⇒ 退避每次都被重置回 2 秒
/// ⇒ **永远不增长**。而每次重连要付 3 次完整 SSH 登录（arch 探测 / SFTP 预检 / 起流），
/// 折算约 **90 次握手/分钟/台**，正好压在那台已经撑不住的机器上。
///
/// ⇒ 判据换成「**这条连接活过了多久**」：hello 只说明握手成功，活过 30 秒才说明它真站住了。
///
/// ⚠ 30 秒的取法：要明显长于「起流 + 首批帧」的正常耗时（冷启动实测约 0.9 s @30ms RTT、
/// 约 6 s @200ms RTT，见），又要短到不至于让一次真实的网络抖动被当成 flapping。
pub const MIN_HEALTHY_UPTIME: Duration = Duration::from_secs(30);

/// 纯函数：这一轮连接结束后，退避该不该重置回 [`RECONNECT_MIN`]。
///
/// 两个条件**都要满足**：握手成功过（`saw_hello`）**且**这条连接活过 [`MIN_HEALTHY_UPTIME`]。
/// 只看前者就是 I-1 那个自激循环；只看后者会把「连了很久但从没握手成功」也当健康。
pub fn should_reset_backoff(saw_hello: bool, lived: Duration) -> bool {
    saw_hello && lived >= MIN_HEALTHY_UPTIME
}

/// 纯函数：把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。[`Backoff::after_wait`] 在这一轮没站住时调用。
pub fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

/// 一台重连循环手里的那一份退避（[`crate::feed`] 只经它动）。两段式：
/// 一轮结束先 [`Backoff::before_wait`]（这一轮站住了 ⇒ 回下界）按它睡，睡完 [`Backoff::after_wait`]：
/// 被叫醒（［重新连接］· 换了一条 SSH）⇒ 回下界；这一轮没站住（没握上手，或握上手却活不过 [`MIN_HEALTHY_UPTIME`]）⇒ 翻倍。
/// 「握上手却活不过」也要翻倍：对端发完 hello 就死的话，每一轮都「连上过」，只看握手退避就永远停在下界（F05 / I-1 的自激循环）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    cur: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Backoff { cur: RECONNECT_MIN }
    }

    /// 这一轮结束：`healthy` = [`should_reset_backoff`] 的结论。回这一次要睡多久。
    pub fn before_wait(&mut self, healthy: bool) -> Duration {
        if healthy {
            self.cur = RECONNECT_MIN;
        }
        self.cur
    }

    /// 睡完（或被叫醒）之后。`reached` = 这一轮够到了那一台（拨出去有回话）；拨到 [`DIAL_WITHIN`] 都没回话 ⇒ `false`：
    /// 没有一次登录落到那台身上，涨退避护不了谁，只会让网回来之后多等 ⇒ 下一轮照样从下界来。
    pub fn after_wait(&mut self, kicked: bool, healthy: bool, reached: bool) {
        if kicked || !reached {
            self.cur = RECONNECT_MIN;
        } else if !healthy {
            self.cur = next_backoff(self.cur);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/backoff_tests.rs"]
mod tests;
