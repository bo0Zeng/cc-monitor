//! 壳取家目录的唯一住址（`clippy.toml` 取家目录那一组只在这一处放行）。
//!
//! 数据位置那几行（monitor 数据目录里界面自己那几份 · 网页引擎的数据目录 · 本机后端家里那几样）不在壳里列：
//! 本机后端 `data-report` 照契约包 `relay_route_core::MONITOR_DATA_ENTRIES` / `OWN_HOME_ENTRIES` 逐样 stat（`footprint/data.rs`），界面照它画；
//! 网页引擎的落点只有壳推得出，经 `footprint_client_facts` 交过去。

use std::path::PathBuf;

/// 本用户的家目录：壳里只在这里取，别处拿它给的路径。
/// 取法同全仓（`relay_route_core::home_dir`，monitor 与后端同一家）；问不到 ⇒ `None`，不猜一个出来。
#[expect(
    clippy::disallowed_methods,
    reason = "壳取家目录的唯一住址：别处都调本函数"
)]
pub(crate) fn home() -> Option<PathBuf> {
    relay_route_core::home_dir()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/data_paths_tests.rs"]
mod tests;
