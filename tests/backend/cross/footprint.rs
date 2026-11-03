//! 足迹判据里引到上层（命令表 · 进后端的口）的那几条：那一层是独立的包（`backend-footprint`）、够不到上层 ⇒ 挂在主 crate。

// 帧面那一跳：入参多一个键 ⇒ `bad_args`（五条小命令 · 两条大命令）。
#[path = "../footprint/on_upper_tests.rs"]
mod on_upper_tests;
