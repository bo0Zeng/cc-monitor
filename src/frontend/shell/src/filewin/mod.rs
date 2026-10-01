//! monitor **开文件窗口那一侧**（窗口本体独立成包 `src/frontend/filewin/`，crate `cc_monitor_filewin`）：
//!
//! - [`entry`] —— 开窗入口（Tauri 命令 `open_file_window`，界面唯一的开口）：算好种子（那台的名字 · 落点 · 书签全路径 · 工作区 · 通道交接件）；
//! - [`proc`] —— 起窗口进程、把种子写进它的 stdin、读它说的就绪那一行、收尸（进程形态与为什么住那份头注）；
//! - `win_main.rs` —— 本包第二个 `[[bin]] cc-monitor-filewin` 的 crate 根，只转调 `cc_monitor_filewin::run`（打包路线不变，K-R124 ⑭）。
//!
//! 两边对上的形状住契约 crate `filewin-contract`；窗口够到 monitor 只经通道（`call` / `subscribe`），边界判据挂在这里（`boundary_tests`）。

pub mod entry;
pub mod proc;

// 🔴解耦那条边界的判据（窗口本体搬成独立包之后，它判的是 app 侧只有一条门、窗口包的依赖只许契约类）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/boundary_tests.rs"]
mod boundary_tests;

// 跨两半的几条（一侧窗口包、异源一侧 monitor）：只有 monitor 的测试档两边都够得着。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/cross_half_tests.rs"]
mod cross_half_tests;
