//! **连远端常驻后端的客户端那一侧**：桌面 monitor 与手机嵌的 Rust 核心（`src/mobile/core-rs`）链同一份。
//!
//! 下面接任何 `AsyncRead + AsyncWrite`：桌面是本机常驻后端给的那条链路（`dial_host`），手机是拨号核心开的长 exec（`use: stream`）。两端的整条重连循环都是 [`feed`]。
//!
//! - [`client`]：帧客户端 —— 「hello 之前不许写」做成类型 · 一问一答（期限 · 超时补发撤单 · 放弃等待也补发）· 应答路由；
//! - [`attach`]：接常驻流那一段握手的零件（小中继的拒绝行 · hello · attach 行与回话）；
//! - [`backoff`]：重连退避（2 → 30 s，活过 30 s 才清零：F05 / I-1 那个自激循环；拨出去没人回话的那几轮不涨）· 拨号期限 · 心跳的数；
//! - [`frames`]：帧门（握手那一刻比出哪几种帧形状对不上，那几种的行不解、不交）；
//! - [`gate`]：版本门（那台旧 ⇒ 换 / 说「要更新」；同或新 ⇒ 接；能不能用逐条命令由形状指纹判，在 [`client`]）；
//! - [`watch`]：`stream-watch` 名单（整份换 · 新连接重报 · `from[]` 补段）；
//! - [`feed`]：一台的常驻流断了自己接回去（两端同一条重连循环；全仓只这一处，monitor `one_loop_guard` 守着）。
//!
//! 有计时器（期限 · 退避），属客户端那一侧：不进后端的判据人群（`no_timer_guard`）。

pub mod attach;
pub mod backoff;
pub mod client;
pub mod feed;
pub mod frames;
pub mod gate;
pub mod line;
pub mod watch;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
