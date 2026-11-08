//! **通信层的边界判据**（`C1`–`C3` 铁律 ＋ `X1` 签名判据 ＋ `M2` 两份禁用名单的比对）。
//!
//! # 成员怎么认：成员 ＝ 登在 `COMMS_CRATES` 的那几个 crate
//!
//! `src/comms/` 下五个包分两种：
//!
//! - **搬字节三包**：面 A `comms-inward`（`src/comms/inward/`，通道：monitor · 文件窗口 · 手机核心链它，后端几包也直接链它的有界读与通道那几句）·
//!   面 B `comms-outward`（`src/comms/outward/`，中转，后端链它）· 出网口 `egress`（`src/comms/egress/`，这台出站 TCP 只在这里开）。
//!   零判定、零自写句子，只许一种例外：通道写自己没成的那一句。
//! - **拨号端核心两件**：连常驻后端的客户端那一侧 `resident-client`（`src/comms/resident-client/`，桌面 · 手机核心 · 后端都链它；
//!   收尾怎么判、那几句话归它）· 拨号 `ssh-dial`（判定之家：机器表判准 · 阶段句子 · host key · 写围栏）。
//!
//! 本族管搬字节三包 ＋ resident-client；ssh-dial 不进（它住这个目录是因为共用上一级 `clippy.toml`、同属后端 workspace）。
//! 成员 ＝ 这几包目录下的源文件（`.rs` ＋ 同住 `comms-inward` 目录的 `chan.ts`），从壳与后端两棵 workspace 的 `cargo metadata` 现取并起来 ——
//! 没有第二份名单，也没有文件头标记。
//!
//! 两条边界由此变成依赖图上的事：
//!
//! - **跨 crate**：成员包的普通依赖 ⊆ 只许的那几个（`C2`，按依赖图判，第一方业务 crate 出现即红）；
//!   同 crate 内的越界（`crate::stream_source::…` 那种写法）在通信层 crate 里根本编不过，由编译器挡。
//! - **成员只经依赖进来**：生产代码里 `#[path]` 指进 `src/comms/` 的零处 —— 挂进别人的模块树就不受成员包的依赖边界管了。
//!
//! 其余几条（`C1` · `C3` · `X1` · 元判据）逐份扫成员的生产段；每条的绿都来自一次相等 / 零命中断言，
//! 人群的反空真是「每个 crate 的 crate 根都在人群里 ＋ 人群里有 `.ts`」。
//! 读盘 · 读环境 · 起进程 · 绑端口 · 期限字面量 · `try_send` 由两份 `clippy.toml` 挡（两份只差什么由 `M2` 比对）；
//! 丢一个 `Result` 由四包都开的 `let_underscore_must_use` · `unused_result_ok` 挡；`match` 列全变体由
//! `wildcard_enum_match_arm` 挡（只 inward · outward 开，其余归 `X1`）；期限只收紧、必给期限由 `Budget` 字段私有和 `call` 的签名挡。
//!
//! # 买不到什么（如实登记）
//!
//! - **不买「这份文件真的该属于通信层」**：归属是人定的（放进哪个 crate），判据只管放进去之后它守不守这几条。
//! - **不买「通信层之外没有第二个通信层」**：一份做传输的代码住在别的包里，这里一个字都看不见。
//! - **不买 ssh-dial 的依赖名单**：它不在成员里，后端那几族扫描判据扫得到它的源码，依赖白名单没有。
//! - 各条各自买到什么、买不到什么，写在 `tests/common/guard-core/rules/comm_boundary_registry_tests.rs`
//!   每条判据自己的文档注释里。
//!
//! 本文件**整体在 `#[cfg(test)]` 内**，非测试构建为空。

#[cfg(test)]
#[path = "comm_boundary_registry_tests.rs"]
mod tests;
