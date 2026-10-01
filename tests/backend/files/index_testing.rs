//! [`crate::files::index::testing`] 的体 —— 判据专用的那两个口。
//!
//! 🔴 **它住这儿而不是生产树里**：要求生产树里只留三行桩
//! （`#[cfg(test)]` ＋ `#[path]` ＋ `mod X;`），测试体住 `<repo>/tests/`。
//! 2026-09-21 我第一版把它内联写在 `index.rs` 里，
//! `structural_scan::the_split_stays_done_and_p9_is_blocked_for_a_reason_that_says_itself`
//! 当场红了（`内联 #[cfg(test)] mod X {}` 1 处），**而它红对了**。
//! ⚠ 那条判据的判词值得抄一句：步 7c 之前那棵树上有过 **533** 个测试属性，
//! 「0 个」那个状态是用**两轮尺子重瞄**换回来的 ⇒ **别把它改松。**
//!
//! # 🔴 为什么有一把锁（2026-09-21 现打逼出来的）
//!
//! `index::rebuild_once` 的那道非阻塞互斥用的是一个**进程级**的位。
//! 而 `cargo test` **默认并行** ⇒ 一旦有一条判据**按住那个位**去验「第二趟被拒」，
//! 同时在跑的别的判据调 `rebuild_once` 就会拿到 `None`，
//! 它们那句「本格独占跑」的断言当场炸。
//!
//! **现打的读数**（我第一版就是这么栽的）：
//!
//! | 怎么跑 | 结果 |
//! |---|---|
//! | `cargo test -- --test-threads=1` | **781 绿** |
//! | `cargo test`（并行，＝门禁那一格）× 3 趟 | **红 2 趟** |
//!
//! ⚠ 我第一版手跑 `cargo test` 拿到 781 绿就报了绿 —— **那一趟只是运气好**。
//! 这是本仓那条「**单独跑绿不算验过**」的又一个实例，而这次写 bug 的是我。
//!
//! ⇒ 处置：一把**只串行冲突那几个**的锁。凡是碰 `rebuild_once` 或那个位的判据都先拿它。
//! 🔴 **它刻意不是「让 `rebuild_once` 自己变可重入」** —— 那会改掉被测的那条性质本身。
//! ⚠ 代价如实记：拿了这把锁的那几条判据**从此串行**，那一族的墙钟会变长
//! （现打 `--test-threads=1` 12.95 秒 vs 并行 2.7 秒，而只串行这几条远没那么贵）。

/// 串行那几条会撞那个进程级位的判据。
///
/// ⚠ **不用 `Mutex<()>` 的 `lock()` 直接返回** —— 中毒（别的判据在持锁时 panic）
/// 之后要能继续，否则一条真红会把它后面每一条都变成「锁中毒」这种**说不出原因**的红。
pub fn serial() -> impl Drop {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct Guard(Option<std::sync::MutexGuard<'static, ()>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.take();
        }
    }
    Guard(Some(match LOCK.lock() {
        Ok(g) => g,
        // 中毒 ⇒ 拿走那把锁继续跑。**出声**，别静默。
        Err(poisoned) => {
            eprintln!(
                "⚠ `files::index` 那把串行锁中毒了（前一条判据持锁时 panic）——\
                 本条继续跑，但**前面那条红才是正题**"
            );
            poisoned.into_inner()
        }
    }))
}

/// 把「正在重走」那个位按成给定值。
///
/// 🔴 **调用方必须先拿 [`serial`]** —— 这个位是进程级的。
pub fn hold_rebuilding(on: bool) {
    super::REBUILDING.store(on, std::sync::atomic::Ordering::Release);
}
