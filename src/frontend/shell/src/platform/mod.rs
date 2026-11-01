//! 壳（monitor）这一侧的**系统适配层**〔收尾重排 ·  ＋  阶段 H 的第一格〕。
//!
//! 与后端 `src/backend/platform/` 同一条纪律（`backend-split` C10）：平台原语与平台 cfg 的唯一住址。
//! 今天有 [`fs`]（原 `platform_fs.rs`，只挪住址）等。
//! 判据立了：壳源码人群里平台形态只许住这里（与 `host-core`），还没收的逐份点名在待收名单里、只许变短
//! （`tests/frontend/shell/platform/platform_home_guard.rs`）。住这里的只是「读法」；判定规则留在调用方。

// 别的进程的控制台显示在哪个窗口（↗ 认窗口）· Linux 往终端设备写改标题序列。
pub mod console;
// X11 上找 / 验 / 拉前窗口的 EWMH 读法（`hwnd` 的 Linux 那一臂）。
#[cfg(target_os = "linux")]
pub mod ewmh;
// Win32 FILETIME（Windows 起始时刻戳那一件）。
pub mod filetime;
pub mod fs;
// 壳自己的东西往盘上写的唯一一层（自己的状态 · 自己的部署物 · 终端设备 · 日志目录，各一组函数）。
#[expect(
    dead_code,
    reason = "写盘落点逐份改经这一层；全部改完这条期望不再满足，随之删掉"
)]
pub mod own_fs;
// 界面要按系统分的那几件事（↗ · shell 方言 · ccm 缓存），起页时注入给界面。
pub mod host_facts;
pub mod hwnd;
pub mod pid;
pub mod proc;
// 只开一个 cc-monitor（Linux 自己占会话总线上的名字、转交激活令牌；别处经插件）。
pub mod single_instance;
pub mod spawn;
// 开终端窗口的平台那一半（另一半在 `launch.rs`）。
pub mod terminal;
// Windows Terminal 窗口里的标签页（UI Automation：列名字 · 选中一个；只给借控制台的小助手用）。
pub mod wt_tabs;
// 窗口那一族（单实例 · WebView2 错位修复 · 主窗口拉前）。
pub mod window;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/platform_home_guard.rs"]
mod platform_home_guard;
