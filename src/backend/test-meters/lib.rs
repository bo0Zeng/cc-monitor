//! 测试档的量具：各包的 `[dev-dependencies]` 引它，发布构建里没有它。
//!
//! - [`alloc_probe`]：线程级内存量具（带 `#[global_allocator]`：链了本包的测试二进制都换上它）
//! - [`golden`]：金样只比不写（现打的落 `.build/golden/`，`tests/scripts/bless-golden.sh` 拷回）
//! - [`heard`]：本线程上一段代码期间 `tracing` 说了什么
//!
//! 体住 `tests/backend/`（这里只挂）；三样都不引后端里任何一层。

// 测试档的量具可以直接问系统钟（同各包测试段）。
#![allow(clippy::disallowed_methods)]

use std::sync::{Arc, Mutex};

#[path = "../../../tests/backend/alloc_probe.rs"]
pub mod alloc_probe;

#[path = "../../../tests/backend/golden.rs"]
pub mod golden;

/// 本线程上 `f` 期间 `tracing` 说了什么（每行一条）。
pub fn heard(f: impl FnOnce()) -> Vec<String> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Sink(buf.clone());
    let sub = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .finish();
    // 另握一个空的 `Dispatch`：只剩一个登记的 dispatcher 时，tracing 按「碰到 callsite 的那条线程」的默认算 interest 并缓存 ——
    // 并行的别的测试先碰到就缓存成「没人要」，这里就听不见。进来先重算一遍缓存。
    let _spare = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    tracing::subscriber::with_default(sub, || {
        tracing::callsite::rebuild_interest_cache();
        f()
    });
    let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    text.lines().map(str::to_string).collect()
}

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
