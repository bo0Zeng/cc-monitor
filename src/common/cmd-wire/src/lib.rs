//! 帧命令两端必须对上的那几样（契约类：只放形状，不放判定）。
//!
//! - [`files`]：文件管理那一族（`files-*`）的入参与回包，路径走原始字节（[`files::WirePath`]）；
//! - [`machine`]：「这一趟问的是哪台机器」的唯一类型 [`Machine`]（线上就是一个字符串，本机逐字 `"<local>"`）；
//! - [`accounts`]：账号库那几条命令（`accounts-*`）的入参与成品；
//! - [`session`]：会话族手机问的那几条的回包，连同客户端那一侧按类型解回包的那一份（[`session::decode_reply`]）；
//! - [`transfer`]：传输台开单那两条（`transfer-upload` · `-download`）：窗口说给 monitor 的 · monitor 补上拨号转给本机后端的，两层入参各一个类型；
//! - [`shapes`]：每条 typed 命令（入参 · 回包 · 错误码都从 Rust 类型来的那几条）的形状指纹。
//!   后端在 hello 里交出它自己那一份，客户端拿自己编进来的这一份比：对得上就直接按类型用，对不上就说「那台后端版本不同」。
//!   表由后端的判据从类型现算、写进 `src/shapes.rs`（过期即红），这里只是那份表本身。

pub mod accounts;
pub mod files;
pub mod machine;
pub mod session;
pub mod shapes;
pub mod transfer;

pub use machine::{Machine, Route, LOCAL};

/// 两端形状指纹对不上的那条 op 当「这台做不到」时用的码（客户端事前就拒、一个字节不发；给人看的那一句 `peerVersion.said.differs`）。
pub const SHAPE_DIFFERS: &str = "backend_differs";

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
