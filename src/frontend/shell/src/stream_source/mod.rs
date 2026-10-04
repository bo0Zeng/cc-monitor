//! 会话流从哪来：远端那台后端的流（经本机常驻后端的链路）与本机后端的流，进同一条收口。
//!
//! 各份分工：`config` 远端配置 · `backoff` 重连退避 · `version` 能力与版本 · `snapshot` 旁路快照 ·
//! `exec` 一次性远端 exec 与限长读行 · `frame` 帧类型与解析 · `batch` 攒批与收行 ·
//! `local` 本机那条流 · `run` 远端流主循环。外面一律写 `crate::stream_source::X`。
//!
//! 本模块不拨 SSH：链路只经宿主 `dial_host`，读代理应答的是通信层成员 `ssh_link`
//! （所以它不登记为通信层成员，理由住 `comm_boundary_registry_tests::TRANSPORT_LEFT_OUTSIDE`）。

mod backoff;
mod batch;
mod config;
mod exec;
mod frame;
mod local;
mod run;
mod snapshot;
mod version;

pub(crate) use backoff::*;
pub(crate) use batch::*;
pub(crate) use config::*;
pub(crate) use exec::*;
pub(crate) use frame::*;
pub(crate) use local::*;
pub(crate) use run::*;
pub(crate) use snapshot::*;
pub(crate) use version::*;

// 🔴 **这个模块的 `pub(crate)` 是 `K-R74` 的承重件，别顺手收回私有**：
// `dial_home_registry`（另一份文件）那条递减棘轮拿 `DIAL_SITES` 里 `moved == false` 的
// **处数合计**当今天的读数；收回成私有那一边就编不过，抄一份数字过去则是「同一个值两个家」。
//
// **这里原先多一道绕道，现在拆掉了**：模块写成私有 `mod`，再在文件顶层
// 加一行 `pub(crate) use dial_move_judge::DIAL_SITES;` 重导出一次。那道绕道**不是品味**，
// 它当时有一个真理由：`guard_core::test_module_ranges` 按**字面前缀**认 `mod ` ⇒
// 写成 `pub(crate) mod` 那一刻本文件整个测试段**不再被剥掉**（`K-R74` 09-12 实打：
// `cargo test -p monitor --lib` 从 `1403 passed; 0 failed` 变成 `1399 passed; 10 failed`，
// 红的是本文件的 `six_of_the_seven_dial_sites_are_still_in_this_process` 与
// `write_half_guard` 三条，外加 `structural_scan` · `cross_half_edge_registry` ·
// `exec_site_registry` · `local_read_surface_registry` · `byte_cap_registry`
// 那几份里「扫生产段」的判据 —— 它们那一刻扫的是测试代码）。
// `K-R75`（09-12）把那一步换成**按形状**剥可见性修饰（`guard_core::strip_visibility`）之后
// **那个理由不再成立** ⇒ 绕道拆掉，模块写回它本来的样子。
// ⚠ 「今天剥法真的接得住 `pub(crate) mod`」这句话**由机检守着，不靠这段散文**：
//   住 `local_backend_host.rs` 的 `the_strip_rule_this_file_leans_on_is_still_on_disk`
//   （`K-R76` `KR76D2`，拿一段逐字校验位去核 `guard-core` 那一行）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/dial_move_judge.rs"]
pub(crate) mod dial_move_judge;
