//! 重连退避与心跳的数：上下界、站住要活多久、拨号期限、问活。
//! 两端同一条重连循环（[`crate::feed`]）用它：阈值只在这里。退避本身与「这一轮站住了没有」住循环里、模块私有：
//! 别处造不出一份退避（第二条重连循环写不出来）。

use std::time::Duration;

/// 重连退避下界：每次连接掉线后至少等这么久再重连（也是连上过之后的快速重连值）。
pub const RECONNECT_MIN: Duration = Duration::from_secs(2);
/// 重连退避上界：指数退避封顶，避免长断网时无意义地拉长重连间隔。
pub const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// **开一条链路（拨号 ＋ 在那台开出那条 exec）多久一步进展都没有就放弃、隔 [`RECONNECT_MIN`] 再拨**。
///
/// 进展由拨号核心报（`ssh-dial`，两端同一份）：TCP 连上 · 收到 SSH 头 · host key 过了 · 认证过 · 经跳板开出隧道 · 通道开了，
/// 每过一步重新计时（[`crate::feed::Progress`]）。慢链路（往返一秒以上、再隔跳板）合计远超它也接得上：每一步之间只隔一两个往返。
/// 静默断网（网卡还在、包全丢：电梯 · 地铁）里拨出去的那一拨，SYN 石沉大海，一步都过不了；内核按 1 · 3 · 7 · 15 · 31 · 63 秒补发，
/// 一拨要挂两分钟。网回来时正挂在后半段的那一拨要等到下一次补发才赶上（断 90 秒那一格量出来 56 秒）。
/// 10 秒放弃：前 7 秒里补发得勤（网回来至多再等 4 秒），之后丢下、隔下界重拨一拨新的，网回来到重拨上至多 5 秒。
/// 代价：一步要 10 秒以上的链路（单程就要好几秒的那种）接不上。
/// 到期限都没进展的那一拨没够到那一台（没有一次登录落到它身上），不涨退避（循环里那份退避的 `reached`）。
pub const DIAL_WITHIN: Duration = Duration::from_secs(10);

/// **连接自己的心跳**：流上这么久没来一个字节 ⇒ 问一句活（`ping`）。健康的空闲连接每这么久一个来回（与 SSH 保活同一个数，
/// 问活的回话把 SSH 保活的计时也续上，空闲时线上的包数不变）。
pub const HEARTBEAT_IDLE: Duration = Duration::from_secs(10);

/// 一问的期限下限：这条流还没量到往返、或量到的往返很短时，问活之后这么久一个字节都没来 ⇒ 这一问算没答。
/// 测试连接那一问也用它。
pub const PING_WITHIN: Duration = Duration::from_secs(5);

/// 一问的期限上限：往返量得再长，一问也至多等这么久。
pub const PING_CEIL: Duration = Duration::from_secs(30);

/// 连着这么多问都没来一个字节 ⇒ 判这条流死了、收掉重接。一问没答只再问一句：那台忙（负载高、写盘被堵）时回话晚到，
/// 不当成断。好链路上最后收到字节之后 [`HEARTBEAT_IDLE`] ＋ 两个 [`PING_WITHIN`]（20 秒）判死，与 SSH 保活同一个数；
/// 宿主报「网回来了 · 网变了 · 回前台」时连着 ⇒ 当场问一句（[`crate::feed::Feed::wake`]），之后同一个判法。
pub const PING_MISSES: u32 = 2;

/// **一问的期限照这条流最近的往返放宽**（RFC 6298 那一式：平滑往返 ＋ 4 倍抖动，夹在 [`PING_WITHIN`] 与 [`PING_CEIL`] 之间）。
/// 往返的一个样本是「问活发出去（没答再问的，从头一问算）到流上来下一行」。一条流一份，新连接从没量过开始。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PingWindow {
    /// 平滑往返 · 抖动；没量过 ⇒ `None`。
    est: Option<(Duration, Duration)>,
}

impl PingWindow {
    /// 记一个往返样本。
    pub(crate) fn sample(&mut self, r: Duration) {
        self.est = Some(match self.est {
            None => (r, r / 2),
            Some((srtt, var)) => {
                let dev = srtt.abs_diff(r);
                (srtt * 7 / 8 + r / 8, var * 3 / 4 + dev / 4)
            }
        });
    }

    /// 此刻一问等多久。
    pub(crate) fn window(&self) -> Duration {
        match self.est {
            None => PING_WITHIN,
            Some((srtt, var)) => (srtt + var * 4).clamp(PING_WITHIN, PING_CEIL),
        }
    }
}

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
