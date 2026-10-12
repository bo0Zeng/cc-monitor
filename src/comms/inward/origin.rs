//! 「这一趟问的是哪台机器」：类型住契约 crate `cmd-wire`（[`cmd_wire::Machine`]，后端 · monitor 同一个）。
//! 通信层这一侧沿用旧名 `Origin`（壳里几十处签名还用着它），第 8 批第 5 刀铺开机器名时一起改成 `Machine`。

pub use cmd_wire::machine::{Machine as Origin, Route, LOCAL};

// 判据 `tests/comms/inward/origin_tests.rs` 挂在 monitor 的 `lib.rs`（它的棘轮扫的是 monitor 的命令面）。
