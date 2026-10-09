//! 要求：「文件窗口成独立包 `src/frontend/filewin/`」：
//! 「`atomic_write_json` 与 `WorkArea` / `fit_into_work_area`（纯函数）收进 `src/common/host-core`（前端宿主原语新类，只许 monitor 与 filewin 链、后端不许）」。
//!
//! 两个前端进程（monitor 主界面 ＋ 文件窗口）都要、而只许有一份的宿主那几件：
//! - [`atomic_write_json`] —— 前端自己那几份状态文件（`auto-launch.json` · 书签 · 绑定表）的原子覆盖；
//! - [`local_offset_at`] —— 那一刻本机时区与 UTC 的差（文件窗口画修改时间 · 两边写「复制详情」的时刻）；
//! - [`WorkArea`] ＋ [`fit_into_work_area`] —— 一扇窗夹进它所在显示器的工作区（Tauri 那几扇窗与文件窗口同一个判定）。

mod atomic;
mod clock;
mod geometry;

pub use atomic::atomic_write_json;
pub use clock::local_offset_at;

/// 这一份编给的系统与架构（「复制详情」里写「本机」那一行用）。平台形态只许住宿主原语与 `platform/`。
pub const OS: &str = std::env::consts::OS;
/// 同上，架构。
pub const ARCH: &str = std::env::consts::ARCH;
pub use geometry::{center_in_work_area, fit_into_work_area, WorkArea};

#[cfg(test)]
#[path = "../../../../tests/common/host-core/lib_tests.rs"]
mod tests;
