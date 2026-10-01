//! 文件管理窗口那个**独立进程**的入口。真正的躯体住独立包 `cc_monitor_filewin`（`src/frontend/filewin/`，原住 `monitor_lib::filewin::proc`）。
//!
//! 🔴 为什么这个文件只有一行：它是一个 `[[bin]]` 的 crate 根，**不是**库里的一个模块
//! （`filewin/mod.rs` 刻意不 `mod` 它）。把躯体写在这里的代价是它从此
//! 不在 `cargo test -p monitor --lib` 的射程里 —— 那正是本仓反复栽过的那一形
//! （「判据不在执行链上就等于不存在」）⇒ 逻辑一律住库面，这里只转调。
//!
//! ⚠ 它**刻意住在 `src/filewin/` 底下**（而不是 `src/bin/` 或包根的 `bin/`）：
//! `write_site_registry::spawn_sites` 那张「谁在本机起进程」的表、以及别的几张
//! 源码扫描型登记表，语料面都是 `src/` 这棵树 —— 放到树外就是给自己开一个扫描盲区，
//! 而那张表的头注逐字记着 `build.rs` 当年就是这么漏掉的。

// 与主二进制同一条理由（`src/main.rs` 逐字）：release 档不要多一个控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::process::exit(cc_monitor_filewin::run())
}
