//! 〔TL3 · 审计 F 🔴-2〕**同步 IPC 命令里零 `block_on` / 零同步连后端**（整体 `#[cfg(test)]`）。
//!
//! # 守的要求（住址）
//!
//! `src/doc/INVARIANTS.md §10`，逐字：「Tauri 的 `#[tauri::command] fn`（非 async）跑在 IPC 派发线程上。
//! 一个慢命令阻塞期间，其他 IPC 全部排队 → 整个 UI 没反应」；「**实施口诀**：IPC 命令默认写 `pub async fn`，
//! 函数体包 `tokio::task::spawn_blocking(move || { ... }).await.map_err(...)?`」。
//!
//! # 为什么要有它
//!
//! `§10` 此前**一条判据都没有**。US1（第四波 4D）给本机起会话那两条同步命令
//! （`resume_history_session` · `new_local_session`〔散文墓碑〕，〔MIG-2〕已搬进本机后端 `launch-local`）的链路里加了一次 `block_on` 等本机后端
//! （最长 10 s）—— 设计篇写着、没人量，于是它合进了主线（审计 F 🔴-2 · 🟠-1）。
//!
//! # 它量什么（判据住 `tests/frontend/shell/sync_command_registry_tests.rs`，头注写全）
//!
//! 人群 = 本 crate 生产段里**全部**同步 `#[tauri::command]`（现扫）。每一条的名字级调用闭包里，
//! 在 IPC 派发线程上「等外面」的两种写法（`block_on(` · `TcpStream::connect`）零处；
//! 例外逐条登记、逐条写「偏离 `§10`、报备待裁」—— 登记不是认可。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/sync_command_registry_tests.rs"]
mod tests;
