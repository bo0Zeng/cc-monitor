//! 测试档的量具：各包的 `[dev-dependencies]` 引它，发布构建里没有它。
//!
//! - [`alloc_probe`]：线程级内存量具（带 `#[global_allocator]`：链了本包的测试二进制都换上它）
//! - [`golden`]：金样只比不写（现打的落 `.build/golden/`，`tests/scripts/bless-golden.sh` 拷回）
//! - [`heard`]：本线程上一段代码期间 `tracing` 说了什么
//!
//! 体住 `tests/backend/`（这里只挂）；三样都不引后端里任何一层。

// 测试档的量具可以直接问系统钟、收尾时丢结果不写理由（同各包测试段）。
#![allow(
    clippy::disallowed_methods,
    clippy::let_underscore_must_use,
    clippy::unused_result_ok
)]

#[path = "../../../tests/backend/alloc_probe.rs"]
pub mod alloc_probe;

#[path = "../../../tests/backend/golden.rs"]
pub mod golden;

// 纯工具那一层的 `test-support` 也挂同一份（契约错的诊断要听日志；本包只许在 dev 侧）。
#[path = "../../../tests/backend/heard.rs"]
mod heard;
pub use heard::heard;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
