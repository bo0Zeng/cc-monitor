//! **通信层的边界判据**（`C1`–`C3` 铁律 ＋ `X1` · `X4` 签名判据）。
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
//! 其余五条（`C1` · `C3` · `X1` · `X4` · 元判据）逐份扫成员的生产段；每条的绿都来自一次相等 / 零命中断言，
//! 人群的反空真是「每个 crate 的 crate 根都在人群里 ＋ 人群里有 `.ts`」。
//! 读盘 · 读环境 · 起进程 · 绑端口 · 期限字面量 · `try_send` 由两包 `clippy.toml` 挡，`match` 列全变体由
//! `wildcard_enum_match_arm` 挡，期限只收紧、必给期限由 `Budget` 字段私有和 `call` 的签名挡。
//!
//! # 买不到什么（如实登记）
//!
//! - **不买「这份文件真的该属于通信层」**：归属是人定的（放进哪个 crate），判据只管放进去之后它守不守这几条。
//! - **不买「通信层之外没有第二个通信层」**：一份做传输的代码住在别的包里，这里一个字都看不见。
//! - 各条各自买到什么、买不到什么，写在 `tests/common/guard-core/rules/comm_boundary_registry_tests.rs`
//!   每条判据自己的文档注释里。
//!
//! 本文件**整体在 `#[cfg(test)]` 内**，非测试构建为空。

#[cfg(test)]
#[path = "comm_boundary_registry_tests.rs"]
mod tests;
