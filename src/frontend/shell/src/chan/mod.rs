//! **通道** —— 通信层面 A 的第一个**进程外**客户端那一条路（末尾「面 A 的第一个外部客户端：通道」）。
//!
//! # 为什么要有它（用户原话）
//!
//! 「**甲, 窗口变成独立前端. 我说了后端要模块化, 即原生后端+文件管理后端. 现在先解耦清楚.
//! 然后 monitor 可以打开文件管理器的前端**」
//!
//! 文件管理窗口已经是独立进程了（`filewin/proc.rs`），而它**在构造上够不着后端**：
//! 客户端登记表是进程级的（新进程里是空的）· 本机后端那个口一次只服务一条流 ·
//! 远端后端住在 monitor 手里那条 SSH 流里。⇒ 窗口今天只能走退路，按构造违反 `D11`。
//! ⇒ **窗口是又一个前端**，它该说的正是那两个动作。本模块就是那两个动作跨进程的那条路。
//!
//! # 五份，两侧（＋ webview 那一侧一份宿主）
//!
//! | 文件 | 住哪一侧 | 通信层成员？ | 干什么 |
//! |---|---|---|---|
//! | `wire.rs` | 两端共用 | ✅ | 全部类型 ＋ 拆帧 |
//! | `router.rs` | monitor 进程 | ✅ | 认证 ＋ 按 `origin` 转给注入的句柄 ＋ 撤单 ＋ credit |
//! | `client.rs` | 外部前端进程 | ✅ | `Comms` 的实现：`call` / `subscribe` |
//! | `host.rs` | monitor 进程 | ❌ 刻意不是 | 绑回环 · 造钥匙 · `accept` · 生产句柄（`C4`/`C5` 不许成员做的那几件） |
//! | `dial.rs` | 外部前端进程 | ✅ | 按交接件拨号（连出去、出示钥匙；不绑口） |
//! | `webview.rs` | monitor 进程 | ❌ 刻意不是 | **主界面**（webview）说 `call` 的那一跳：Tauri 命令 `chan_call` ＋ 注入生产句柄；期限执行与回环那条共用 `router::settle`。成员那一半是 TS 的 `src/comms/inward/chan.ts` |
//!
//! # 分两处住
//!
//! 成员（线上词汇 · 路由器 · 客户端 · 拨号 · 交接件的形状）住通信层 crate `comms-inward`，宿主（绑口 · 造钥匙 · 生产句柄 ·
//! webview 那一跳）住本目录 ——「绑口在外、`serve` 在内」这条 `C5` 的分界就是 crate 的边界，编译器挡着。
//! 本模块在 `lib.rs` 里是 `pub mod`：外部前端那个 `[[bin]]` 经 `monitor_lib::chan` 够得着它。
//!
//! # 买到什么
//!
//! - 一个进程外前端**第一次**能用 `call` / `subscribe` 走到后端，而且只能经一把钥匙进来。
//! - 路由器（成员）身上零业务、零读盘、零起进程、零绑口、零期限常量 —— `C1`–`C5` 十一条当场对它成立。
//!
//! # 买不到什么（逐条，别读宽）
//!
//! - ✅**第一个真前端接上了**：文件窗口进程（`filewin/proc.rs::dial_back` 拨回，`filewin/source.rs::ask` 说 `call`）。
//! - ✅**生产上的 `subscribe` 有了第一条流**：传输进度 `transfer/<id>`；其余 `kind` 照旧没有（理由住 `host.rs` 头注）。
//! - **不买重连**（`client.rs` 头注）· **不买对端撤活**（`host.rs` 头注）· **不买协议版本协商**（`wire.rs` 头注）。

// 线上词汇 · 客户端 · 路由器 · 拨号 · 交接件的形状住通信层 crate `comms-inward`（`src/comms/inward/chan/`，文件窗口进程链同一份）；
//   壳里只留 monitor 自己的宿主那两份（host 的绑口 · 生产入口与句柄 · webview 那一跳），其余经再导出、模块路径不变。
pub use comms_inward::chan::{client, dial, router, wire};
pub mod host;
pub mod webview;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/chan/chan_tests.rs"]
mod tests;
