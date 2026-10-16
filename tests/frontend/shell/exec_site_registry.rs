//! ★ **monitor 里每一处「经拨号链路跑 shell」都要写清为什么它不是那台后端的一条具名命令**。
//!
//! 人群从源码派生（`connect_and_exec_capture(` 的调用点，「文件::外层函数」），与理由表两向相等：
//! 多一处 = 又长出一条自拼 shell 的路；少一处 = 那一处换掉了，删行。
//!
//! **不守**：命令串里的自由文本有没有过引用（`shell_quote` 的判据与各调用点的行为判据）；
//! 走本机 `Command` 的路（`write_site_registry::spawn_sites` 接）；SFTP（monitor 的依赖清单里没有 SSH crate，`dial_home_registry`）。

#[cfg(test)]
#[path = "exec_site_registry_tests.rs"]
mod tests;
