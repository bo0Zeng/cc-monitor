//! U4a（2026-08-01）：**判活的纯判定表** —— 与「怎么读到那些事实」分开。
//!
//! # 为什么从 `proc.rs` 上提到这里
//!
//! U3 的 Phase D 审计留了一条 U4 伏笔：这张表是**跨平台共用的那一半** ——
//! Windows 侧（U4b）读事实的方式完全不同（`OpenProcess` 而不是 `/proc`），
//! 但「exists / captured / current 三者怎么组合出存活判定」这套规则**一模一样**。
//!
//! 留在按 `/proc` 命名的模块里，U4b 要么把它复制一份（两份判定表迟早漂），
//! 要么从一个名字说它是 `/proc` 的模块里 import 一段与 `/proc` 无关的逻辑。两个都不对。
//!
//! 它是**纯函数**：不碰 `/proc`、不碰 Win32、不做 I/O。所以它在**任何平台**都能被直接喂参数测。

/// Pure liveness decision (testable without a real `/proc`), given whether the
/// PID currently **exists**, the procStart **captured** at add-time, and the
/// procStart **read now**.
///
/// Key correctness rule (#34): a PID reuse only ever shows up as a
/// *successfully-read, DIFFERENT* current start. So the only case that declares
/// "dead by reuse" is `(Some(captured), Some(current))` with `captured != current`.
/// Every other arm where the PID still exists returns alive — in particular a
/// **transient `/proc/<pid>/stat` read failure** (`current == None`) must NOT
/// false-archive a process that demonstrably still exists (that would be a
/// regression vs. the Phase-0 existence-only check). If the process is truly
/// gone, `exists` is already `false` and we return dead.
pub(crate) fn is_same_live_process(
    exists: bool,
    expected_start: Option<u64>,
    current_start: Option<u64>,
) -> bool {
    if !exists {
        return false;
    }
    match (expected_start, current_start) {
        // Baseline captured AND current readable: same process iff equal.
        (Some(captured), Some(current)) => captured == current,
        // No baseline, or current unreadable right now: existence is all we can
        // assert. Do not archive a still-existing PID on missing start info.
        _ => true,
    }
}
