//! [`crate::files::index::testing`] 的体 —— 判据专用的那个口。
//!
//! 🔴 **它住这儿而不是生产树里**：`设计/16 §3.1` 逐字要求生产树里只留三行桩
//! （`#[cfg(test)]` ＋ `#[path]` ＋ `mod X;`），测试体住 `<repo>/tests/`。
//! 2026-09-21 我第一版把它内联写在 `index.rs` 里，
//! `structural_scan::the_split_stays_done_and_p9_is_blocked_for_a_reason_that_says_itself`
//! 当场红了（`内联 #[cfg(test)] mod X {}` 1 处），**而它红对了**。
//! ⚠ 那条判据的判词值得抄一句：步 7c 之前那棵树上有过 **533** 个测试属性，
//! 「0 个」那个状态是用**两轮尺子重瞄**换回来的 ⇒ **别把它改松。**

/// 把「正在重走」那个位按成给定值。
pub fn hold_rebuilding(on: bool) {
    super::REBUILDING.store(on, std::sync::atomic::Ordering::Release);
}
