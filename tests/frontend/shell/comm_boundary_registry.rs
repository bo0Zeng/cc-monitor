//! **通信层的边界判据**（`C1`–`C5` 铁律 ＋ `X1`–`X6` 签名判据）。
//!
//! # 成员怎么认：成员 ＝ 那两个 crate
//!
//! 通信层是两个独立的 crate：面 A `comms-inward`（`src/comms/inward/`，monitor 与文件窗口链它）·
//! 面 B `comms-outward`（`src/comms/outward/`，后端链它）。成员 ＝ 这两个 crate 目录下的源文件
//! （`.rs` ＋ 同住 `comms-inward` 目录的 `chan.ts`），从 `cargo metadata` 现取 —— 没有第二份名单，也没有文件头标记。
//!
//! 两条边界由此变成依赖图上的事：
//!
//! - **跨 crate**：通信层 crate 的普通依赖 ⊆ 只许的那几个（`C2`，按依赖图判，第一方业务 crate 出现即红）；
//!   同 crate 内的越界（`crate::stream_source::…` 那种写法）在通信层 crate 里根本编不过，由编译器挡。
//! - **成员只经依赖进来**：生产代码里 `#[path]` 指进 `src/comms/` 的零处 —— 挂进别人的模块树就不受那两个 crate 的依赖边界管了。
//!
//! 其余九条（`C1` · `C3`–`C5` · `X1`–`X6`）照旧逐份扫成员的生产段；每条的绿都来自一次相等 / 零命中断言，
//! 人群的反空真是「每个 crate 的 crate 根都在人群里 ＋ 人群里有 `.ts`」。
//!
//! # 进不来的那几份
//!
//! 中转的宿主（后端 `relay/listen.rs`：绑口 · 期限值）与面 A 的传输面候选（`stream_source/` · `sftp.rs` · `sftp_pool.rs`）
//! 不在通信层里；它们**为什么在外面**（被哪几条咬）逐份写在判据那两张表里，两向相等 —— 散文腐了当场红。
//!
//! # 买不到什么（如实登记）
//!
//! - **不买「这份文件真的该属于通信层」**：归属是人定的（放进哪个 crate），判据只管放进去之后它守不守十一条。
//! - **不买「通信层之外没有第二个通信层」**：一份做传输的代码住在别的包里，这里一个字都看不见。
//! - `C1`–`C5` / `X1`–`X6` 各自买到什么、买不到什么，写在 `tests/frontend/shell/comm_boundary_registry_tests.rs`
//!   每条判据自己的文档注释里。
//!
//! 本文件**整体在 `#[cfg(test)]` 内**，非测试构建为空。

#[cfg(test)]
#[path = "comm_boundary_registry_tests.rs"]
mod tests;
