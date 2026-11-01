//! 本线程上一段代码期间 `tracing` 说了什么。量具包 `test-meters` 与纯工具那一层的 `test-support`（契约错的诊断）各挂这一份；
//! 只用 `tracing` · `tracing-subscriber`，不引后端里任何一层。

use std::sync::{Arc, Mutex};

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
