//! 通信层面 A（前端 ↔ 后端）：只搬字节、零业务判断（边界判据 `C1`–`C5`）。
//!
//! monitor（宿主那一侧）与文件窗口进程（外部前端）链同一份：
//! - [`origin`]：「这一趟问的是哪台机器」的唯一类型；
//! - [`chan`]：通道的线上词汇与帧（`wire`）· 客户端（`client`）· 路由器（`router`）· 外部前端按交接件拨号（`dial`）· 交接件的形状（`handoff`）；
//! - [`backend_route`]：问后端失败了怎么分流（`CallError` 那一套词）；
//! - [`ssh_link`]：拨号应答的读法（SSH 链路那一段）。
//!
//! 绑口、造钥匙、起进程、读盘、读环境都不在这里：那是宿主的事（壳里 `chan/host.rs` · `dial_host.rs`）。
//! `chan.ts`（主界面那一侧的通道入口）与这几份同住这个目录，同归通信层管。

pub mod backend_route;
pub mod origin;
pub mod ssh_link;

/// 通道：线上词汇 · 帧 · 客户端 · 路由器 · 拨号 · 交接件的形状。逐份的分工住壳里 `chan/mod.rs` 那张表。
pub mod chan {
    pub mod client;
    pub mod dial;
    pub mod handoff;
    pub mod router;
    pub mod wire;
}
