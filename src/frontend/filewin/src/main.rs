//! 文件管理窗口那个**独立进程**的入口（本包的 `[[bin]] cc-monitor-filewin`）。躯体住本包的库面，这里只转调。
//!
//! 🔴 为什么这个文件只有一行：逻辑住库面（`lib.rs` 的 [`cc_monitor_filewin::run`]），判据在 `cargo test --lib` 的射程里；
//! 写在 bin 的 crate 根里就不在了（「判据不在执行链上就等于不存在」）。
//! size2（10-11）从 monitor 包搬来：monitor 包的每个 `[[bin]]` 都会被打包器装进安装包，而开窗只用 monitor 内嵌的那份
//! ⇒ 住在那里就是安装包里多一份纯重复的 10 MB。

// 与主二进制同一条理由（`src/main.rs` 逐字）：release 档不要多一个控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::process::exit(cc_monitor_filewin::run())
}
