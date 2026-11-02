//! [`crate::files::index::test_support`] 的体 —— 判据专用的那几个口（本包的测试与上层的测试共用，只用本包 `[dependencies]` 里有的 crate）。
//!
//! 🔴 **它住这儿而不是生产树里**：要求生产树里只留三行桩
//! （`#[cfg(…)]` ＋ `#[path]` ＋ `mod X;`），测试体住 `<repo>/tests/`。
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

/// ★ 常驻那一份是**进程级的一个 static** ⇒ 同一进程里并行跑的判据会互相踩。
///
/// 🔴 不加这把锁会怎样（这不是假想，是这一族的结构性事实）：`cargo test` 默认多线程，
/// 两条判据各自 [`super::rebuild_once`] 一棵不同的树 ⇒ 后一条把前一条的索引整份换掉，
/// 前一条的条目数断言拿到的是**别人那棵树**的数。而那种红是**间歇性**的，
/// 一次绿一次红，最省事的消法是把断言调松 —— 正是本仓反复治的那条路。
///
/// ⇒ 凡是碰常驻那一份的判据，**第一行就拿这把锁**。
///
/// ⚠ 它顺手把**两份**常驻状态都清空（索引 ＋ 浏览目录的 overlay）：
/// 那两份一起构成「后端此刻手上的那份真相」，只清一半会让上一条判据挂的 watch
/// 在下一条判据的查询里冒出来 —— 那种红同样是间歇性的。
pub fn resident_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // 中毒了也要能接着跑：一条判据 panic 不该让其余的全部变成「毒锁」红。
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(mut r) = super::RESIDENT.write() {
        *r = None;
    }
    if let Ok(mut w) = crate::files::browse_watch::WATCHED.write() {
        w.clear();
    }
    g
}
