//! 各族登记一行 typed 命令经这里（`super::typed::blocking::<X>(…)`）：本体就是 `wire::typed` 的那四样（阻塞 · 异步 · 帧面硬臂 · 硬臂进阻塞池）；
//! 测试档在这里给那一行补上形状（入参 · 回包的 TS 声明 ＋ 码表，形状表 · 界面命令表 · 协议参考都读它）。
//!
//! 补形状住主 crate 而不住 wire 包：形状要 TS，命令的类型只在各自那一包的测试档派生 TS；
//! 把那道界写进 wire 包的特性，dev 侧开的特性会在 `cargo test` 那一趟并进主 crate 的普通编（编 bin · bench 那一份），
//! 那一份的类型没有 TS ⇒ 编不过。

#[cfg(not(test))]
pub(super) use crate::wire::typed::{asynchronous, blocking, frame, frame_blocking};

#[cfg(test)]
pub(super) use shaped::{asynchronous, blocking, frame, frame_blocking};

#[cfg(test)]
mod shaped {
    use crate::wire::spec::CommandSpec;
    use crate::wire::typed::test_support::{shape_of, shaped, DataTys};
    use crate::wire::typed::{self, Codes, Command, Handle};

    /// 阻塞档的一行登记（补上形状）。
    pub(crate) const fn blocking<C: Handle>(summary: &'static str) -> CommandSpec
    where
        C::Req: ts_rs::TS,
        C::Reply: ts_rs::TS,
        <C::Err as Codes>::Data: DataTys,
    {
        shaped(typed::blocking::<C>(summary), shape_of::<C>)
    }

    /// 异步档的一行登记（补上形状）。
    pub(crate) const fn asynchronous<C: Handle>(summary: &'static str) -> CommandSpec
    where
        C::Req: ts_rs::TS,
        C::Reply: ts_rs::TS,
        <C::Err as Codes>::Data: DataTys,
    {
        shaped(typed::asynchronous::<C>(summary), shape_of::<C>)
    }

    /// 帧面硬臂那一档的一行登记（补上形状）。
    pub(crate) const fn frame<C: Command>(summary: &'static str) -> CommandSpec
    where
        C::Req: ts_rs::TS,
        C::Reply: ts_rs::TS,
        <C::Err as Codes>::Data: DataTys,
    {
        shaped(typed::frame::<C>(summary), shape_of::<C>)
    }

    /// 硬臂进阻塞池那一档的一行登记（补上形状）。
    pub(crate) const fn frame_blocking<C: Command>(summary: &'static str) -> CommandSpec
    where
        C::Req: ts_rs::TS,
        C::Reply: ts_rs::TS,
        <C::Err as Codes>::Data: DataTys,
    {
        shaped(typed::frame_blocking::<C>(summary), shape_of::<C>)
    }
}
