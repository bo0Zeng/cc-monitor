//! **失败时不说话的那一类**：丢掉一个 `Result` 只经 [`Discard::discard`]，理由是 [`Why`] 的一个变体，住在调用处。
//!
//! 与 [`crate::said::Said`] 是一对：`Said` 是失败了说什么，`Why` 是为什么不说。
//! 丢的那一下什么也不做；理由是闭集，拼错编不过。裸丢的两形 `let _ = …` / `….ok();` 是
//! `clippy::let_underscore_must_use` · `clippy::unused_result_ok` 两条认的形状，这一口是带着理由丢的那一形。
//!
//! 只给 `Result` 实现：`Option` 不是 `#[must_use]`，两条 lint 都不报，丢一个 `Option` 不用写理由。

/// 为什么这一处可以丢掉结果。**每一类一句理由**（写在变体上）；调用处可以再用行尾注释补一句。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// 收尸：杀 / 等子进程、关句柄（Windows `CloseHandle` / `LocalFree`）—— 收尾那一步本身失败，没有更好的补救，
    /// 而要报的主结局已经在别处定了。
    Reap,
    /// 对端已走：往一个接收端可能已经不在的通道里发（`send` / `try_send` / `emit`）。接收端不在 = 没人要这个结果。
    PeerGone,
    /// 写回一条正在出错 / 已断的流：拒绝行、错误帧、刷新、关写半边。写不进去说明对面已经走了，无处可报。
    DeadLink,
    /// 失败后收拾：主错误已经在回（或这一趟已经判失败），这一步只是清临时件 / 半成品；清不掉只剩一份垃圾，不改结局。
    CleanupAfterFailure,
    /// 窗口尽力而为：恢复 / 显示 / 聚焦 / 置顶。做不成不改语义（拉前有自己的失败报告那条路）。
    WindowBestEffort,
    /// 写进 `String`：`fmt::Write for String` 不会失败。
    InfallibleWrite,
    /// 等一个信号 / 一次性通知：只要「等到了」，结果本身没有信息量。
    Signal,
    /// 读完丢弃：排空管道 / 丢掉剩下的字节，只为让对面不堵。
    Drain,
    /// 诊断输出本身：往 stdout / stderr / 日志写失败时，没有第二个地方可以说。
    Diag,
    /// 结果是「有没有」而不是「成没成」（`Option`），`None` 是常态；该说的在被调的那一侧说了。
    NotAnError,
    /// 只设一次：`OnceLock::set` 第二次回 `Err` 是设计（第一次那个值留着）。
    SetOnce,
    /// 控制面让位：应答通道满了宁丢这一条应答也不等 —— 等就把读循环堵死、后面的 `cancel` 连解析都轮不到
    /// （`stream/inbound/` 里 `cancel` 那一臂的头注：「丢一条 cancel 应答，远比堵死读循环便宜」）；调用方按自己的期限收场。
    Backpressure,
    /// 别处在改：这一处由另一件改动收，那一件进来时这一行要随之删 / 改。
    OtherLane,
}

/// 丢掉一个结果，带上为什么可以丢。
pub trait Discard {
    /// 什么也不做；`why` 只为让理由住在调用处、由类型挡住拼错。
    fn discard(self, why: Why);
}

impl<T, E> Discard for Result<T, E> {
    #[inline]
    fn discard(self, _why: Why) {}
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/discard_tests.rs"]
mod tests;
