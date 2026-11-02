//! 中转宿主判据里引到上层的那几条：那一层是独立的包（`backend-relay`）、够不到上层 ⇒ 挂在主 crate。
//! 组合判据：真中转 ＋ 生产段的上游选择 / tap / 内存探针 / 面（纯中转的单测在 `comms_outward` 里）。

// fd 用尽时接受循环回 503、只说一次、缓过来照常
#[path = "../relay/exhaust_tests.rs"]
mod exhaust_tests;
// 进程内中转：`host` 的四种结局 ＋ 生产接线在真子进程里 stdout 零 tee
#[path = "../relay/host_tests.rs"]
mod host_tests;
#[path = "../relay/observe_retry_tests.rs"]
mod observe_retry_tests;
#[path = "../relay/server_tests.rs"]
mod server_tests;
// 只许直通的那把钥匙 · 先试协议升级、被拒再发普通请求
#[path = "../relay/two_form_tests.rs"]
mod two_form_tests;
