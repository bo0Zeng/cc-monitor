//! 界面进程零 SSH：拨号与端口转发只住本机常驻后端的拨号代理（`src/backend/dial/`）。
//!
//! 两条判据（住 `tests/frontend/shell/dial_home_registry_tests.rs`）：
//! - monitor 生产段里一处拨号锚点都没有（握手 · 开隧道那几个调用；零命中，带正控）；
//! - 界面 crate 的 manifest 里零 `russh` 家族直接依赖（终点那面二值旗，回弹即红）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/dial_home_registry_tests.rs"]
mod tests;
