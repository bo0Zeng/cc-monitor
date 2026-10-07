//! 通道 —— 通信层面 A 的进程外客户端那一条路。文件管理窗口是独立进程（`filewin/proc.rs`），在构造上够不着后端（客户端登记表是进程级的 ·
//! 本机后端那个口一次只服务一条流 · 远端后端住在 monitor 手里那条 SSH 流里）⇒ 窗口是又一个前端，它经这条路说 `call` / `subscribe`。
//!
//! | 文件 | 住哪一侧 | 通信层成员？ | 干什么 |
//! |---|---|---|---|
//! | `wire.rs` | 两端共用 | ✅ | 全部类型 ＋ 拆帧 |
//! | `router.rs` | monitor 进程 | ✅ | 认证 ＋ 按 `origin` 转给注入的句柄 ＋ 撤单 ＋ credit |
//! | `client.rs` | 外部前端进程 | ✅ | `Comms` 的实现：`call` / `subscribe` |
//! | `host.rs` | monitor 进程 | ❌ 刻意不是 | 绑回环 · 造钥匙 · `accept` · 生产句柄（`C4`/`C5` 不许成员做的那几件） |
//! | `dial.rs` | 外部前端进程 | ✅ | 按交接件拨号（连出去、出示钥匙；不绑口） |
//! | `webview.rs` | monitor 进程 | ❌ 刻意不是 | 主界面（webview）说 `call` 的那一跳：Tauri 命令 `chan_call` ＋ 注入生产句柄 |
//!
//! 成员（线上词汇 · 路由器 · 客户端 · 拨号 · 交接件的形状）住通信层 crate `comms-inward`，宿主住本目录 ——「绑口在外、`serve` 在内」这条分界就是 crate 的边界。
//! 本模块在 `lib.rs` 里是 `pub mod`：外部前端那个 `[[bin]]` 经 `monitor_lib::chan` 够得着它。
//! 路由器身上零业务、零读盘、零起进程、零绑口、零期限常量（`C1`–`C5` 的判据对它成立）。
//! 用上的：文件窗口进程（`filewin/proc.rs::dial_back` 拨回，`filewin/source.rs::ask` 说 `call`）；订阅只有传输进度 `transfer/<id>` 一条流（理由住 `host.rs` 头注）。
//! 不买重连（`client.rs` 头注）· 不买对端撤活（`host.rs` 头注）· 不买协议版本协商（`wire.rs` 头注）。

// 线上词汇 · 客户端 · 路由器 · 拨号 · 交接件的形状住通信层 crate `comms-inward`（`src/comms/inward/chan/`，文件窗口进程链同一份）；
//   壳里只留 monitor 自己的宿主那两份（host 的绑口 · 生产入口与句柄 · webview 那一跳），其余经再导出、模块路径不变。
pub use comms_inward::chan::{client, dial, router, wire};
pub mod host;
pub mod webview;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/chan/chan_tests.rs"]
mod tests;
