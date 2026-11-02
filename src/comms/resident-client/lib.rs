//! **连远端常驻后端的客户端那一侧**：桌面 monitor 与手机嵌的 Rust 核心（`src/mobile/core-rs`）链同一份。
//!
//! 下面接任何 `AsyncRead + AsyncWrite`：桌面是本机常驻后端替各台开的那条长 exec（`control/feeds.rs`），手机是拨号核心开的长 exec（`use: stream`）。两端的整条重连循环都是 [`feed`]。
//!
//! - [`client`]：帧客户端 —— 「hello 之前不许写」做成类型 · 一问一答（期限 · 超时补发撤单 · 放弃等待也补发）· 应答路由；
//! - [`attach`]：接常驻流那一段握手的零件（attach 行 · 小中继的拒绝行 · hello · 回话里那台相对这一版的新旧）；
//! - [`backoff`]：重连退避（2 → 30 s，活过 30 s 才清零：F05 / I-1 那个自激循环；拨出去没人回话的那几轮不涨）· 拨号期限 · 心跳的数；
//! - [`frames`]：帧门（握手那一刻比出哪几种帧形状对不上，那几种的行不解、不交）；
//! - [`watch`]：`stream-watch` 名单（整份换 · 新连接重报 · `from[]` 补段）；
//! - [`feed`]：一台的常驻流断了自己接回去（两端同一条重连循环；退避住它里面、模块私有 ⇒ 全仓写不出第二条）；
//! - [`book`]：一台的会话簿（会话帧折成整份；两端只调这一份折法）；
//! - [`live`]：一台的活卡板（`live_card` 帧折成某会话某运行整份的卡；两端只调这一份折法）；
//! - [`listing_copy`]：远端那几台的会话清单副本（副本表 · 照答换 · 落盘那一形；两端同一份）；
//! - [`column`]：几台的会话清单并成一列（本机后端历史页与手机会话屏同一份并法）。
//!
//! 有计时器（期限 · 退避），属客户端那一侧：不进后端的判据人群（`no_timer_guard`）。

// 生产段的禁用名单住上一层 `src/comms/clippy.toml`，放行逐处写在调用点；测试段的夹具要自己定期限 ⇒ 测试段整段放开。
// 这一行是整包唯一许写 `allow` 的地方：测试段用不用得到名单上的调用随判据增减，写成 `expect` 会在用不到时报。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::disallowed_methods,
        reason = "测试段放开禁用名单：夹具自己定期限"
    )
)]

pub mod attach;
pub mod backoff;
pub mod book;
pub mod client;
pub mod column;
pub mod feed;
pub mod frames;
pub mod line;
pub mod listing_copy;
pub mod live;
pub mod watch;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
