//! 远端那几台怎么连：壳里**没有**这一份了，连哪几台也不记（要哪几台问本机后端 `machine-read`）。
//! 地址 · 端口 · 用户 · 钥匙 · 跳板都在本机常驻后端的机器表里（读法 `src/backend/dial/machines.rs`，写口 `src/backend/control/machines.rs`），
//! 拨号请求由它按名字组（`src/backend/dial/mod.rs::request_named`）。本文件只剩下面那组判据的住址。

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/tier1_tests.rs"]
mod tier1_tests;
