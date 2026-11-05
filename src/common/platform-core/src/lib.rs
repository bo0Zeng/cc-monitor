//! 后端（`backend-platform`）与壳（monitor · 经 `host-core` 的文件窗口）都要、只该有一份的平台原语。
//!
//! - [`dir`]：只给本人的目录（[`dir::private_dir`] · [`dir::private_dir_all`]）· 目录的跨进程锁（[`dir::hold`]）。
//! - [`atomic`]：自家状态文件的原子写（[`atomic::write_atomic`]）。
//! - [`proc`]：`/proc/<pid>/stat` 一行怎么切（[`proc::parse_stat`]）· Windows 进程表（`proc::processes`）· `CREATE_NO_WINDOW`。
//!
//! 本包不说话：失败回 `io::Error` 或带阶段的结局，给人看的那一句由各家在自己那一层照文案表取（后端造 `Said`）。
//! 判定不进来：拿这些事实去判「是不是那个进程」「该不该收紧」的规则在调用方。
//! 本人通道那把「抢不到就算」的门锁不在这里：那是 `own-chan` 的（听的那一个抢门、不等，Windows 上按互斥量已在判）。

// 测试段的夹具收尾时删临时目录、丢结果不写理由 ⇒ 测试段放开吞结果那两条；这一行是整包唯一许写 `allow` 的地方。
#![cfg_attr(
    test,
    allow(
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开吞结果那两条：夹具收尾时删临时目录、丢结果"
    )
)]
pub mod atomic;
pub mod dir;
pub mod proc;

#[cfg(test)]
#[path = "../../../../tests/common/platform-core/lib_tests.rs"]
mod lib_tests;

#[cfg(test)]
#[path = "../../../../tests/common/platform-core/atomic_tests.rs"]
mod atomic_tests;

#[cfg(test)]
#[path = "../../../../tests/common/platform-core/single_home_tests.rs"]
mod single_home_tests;

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
