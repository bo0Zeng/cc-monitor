//! 远端那几台怎么连：壳里**没有**这一份了。一台远端在壳里只剩它的名字（`machine_table::Row`：名字 · 连不连），
//! 地址 · 端口 · 用户 · 钥匙 · 跳板都在本机常驻后端的机器表里（`src/backend/control/machines.rs`），
//! 拨号请求由它按名字组（`src/backend/dial/mod.rs::request_named`）。本文件只剩下面那组判据的住址。

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/tier1_tests.rs"]
mod tier1_tests;
