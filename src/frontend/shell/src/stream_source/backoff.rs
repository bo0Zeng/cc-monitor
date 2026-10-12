//! 重连退避：本体住 `resident-client`（手机同一份，两端的阈值只在那里）；这里只转用。

pub(crate) use resident_client::backoff::*;
// 判据（`tier1_tests.rs`）沿用本体那一个词。
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/tier1_tests.rs"]
mod tier1_tests;
