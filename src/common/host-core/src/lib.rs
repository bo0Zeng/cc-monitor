//! 要求：「文件窗口成独立包 `src/frontend/filewin/`」：
//! 「`atomic_write_json` 与 `WorkArea` / `fit_into_work_area`（纯函数）收进 `src/common/host-core`（前端宿主原语新类，只许 monitor 与 filewin 链、后端不许）」。
//!
//! 两个前端进程（monitor 主界面 ＋ 文件窗口）都要、而只许有一份的宿主那几件：
//! - [`atomic_write_json`] —— 前端自己那几份状态文件（`auto-launch.json` · 书签 · 绑定表）的原子覆盖；
//!   [`read_capped`] · [`read_capped_bytes`] —— 两个前端整读一份文件都经它们，先过上限（[`Cap`]：调用方声明的那一道）；
//! - [`fit_into_work_area`] —— 一扇窗夹进它所在显示器的工作区（Tauri 那几扇窗与文件窗口同一个判定；工作区的形状 `filewin_contract::WorkArea`）；
//!   [`work_area_at`] —— 文件窗口改缩放之后问此刻那块屏的工作区（只 Windows）。

mod atomic;
mod geometry;

pub use atomic::{atomic_write_json, read_capped, read_capped_bytes, Capped};
/// 读上限那个类型（住 `cap_core`）：文件窗口包只依赖本包、不依赖 `cap-core`，声明它自己的读上限从这里拿。
pub use cap_core::{Cap, Over};

/// 这一份编给的系统与架构（「复制详情」里写「本机」那一行用）。平台形态只许住宿主原语与 `platform/`。
pub const OS: &str = std::env::consts::OS;
/// 同上，架构。
pub const ARCH: &str = std::env::consts::ARCH;
/// 工作区那个形状住开窗契约（随种子过线）；本包的几何判定收它，不另起一份、不重导出。
use filewin_contract::WorkArea;
pub use geometry::{center_in_work_area, fit_into_work_area, work_area_at};

#[cfg(test)]
#[path = "../../../../tests/common/host-core/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
