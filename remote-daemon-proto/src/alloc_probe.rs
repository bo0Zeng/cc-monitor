//! U-2：**线程级**内存量具（仅测试构建）。
//!
//! # 它为什么存在〔audit-0805 F22〕
//!
//! `inbound::tests::an_oversized_line_does_not_grow_memory` 要判「一整行没有进内存」。
//! 它原本读 `/proc/self/status` 的 `VmHWM`，而 `VmHWM` 由内核按**整个进程**维护，
//! `cargo test` 又在**同一进程内并行**跑测试 ⇒ **邻居测试的一次性大分配会被算进来**。
//! 实测代价：它在 F07 / F09 / F11 / F18 四件里各制造过一次假红，每次都逼人停下来
//! 重跑一遍确认「不是我弄坏的」—— 一条会随机说谎的判据，比没有判据更消耗注意力。
//!
//! ★ **走过的弯路，记在这里免得下一个人再走一遍**：
//! 试过「跑两遍洪流、只判第二遍的增量」，想法是「真泄漏每跑一次都会再顶高，
//! 邻居的一次性分配不会」。**不成立** —— `VmHWM` 是单调高水位，而本条要防的缺陷
//! （整行进 `buf`）是**用完即释放的瞬时峰值**：第一遍已经把水位顶到 256 MiB，
//! 第二遍同样的峰值**不会再顶高** ⇒ 第二遍增量恒约等于 0 ⇒ **判据变哑**。
//! 变异实测（在 flood 里注入一块 256 MiB 的瞬时峰值、用完即释放）：
//! 单次法 `FAILED`（涨 257 MiB），两遍法 `ok` —— **一模一样的缺陷，两遍法看不见。**
//!
//! ⚠ 第一次做这个变异时我用了 `vec![0u8; N]` 只 touch 首尾两页：`alloc_zeroed` 拿的是
//! 内核零页，**RSS 根本不涨**，两个版本都绿。`grep` 证明源码落地了，但**语义没落地**。
//! ⇒ 「变异要连诊断文案一起读」这条纪律，在内存类判据上还要多一句：
//! **确认变异真的改变了被测的那个量**，源码落地不等于效果落地。
//!
//! # 修法
//!
//! 换量具，不换判据：把「进程 RSS 高水位」换成「**本线程**分配中未释放字节数的高水位」。
//! 邻居测试跑在别的线程上，动不到本线程的计数器 ⇒ 假红的成因被拆掉，而
//! 「整行进内存」照样被抓住（那块 `buf` 就分配在本线程上）。
//!
//! 附带两个好处：
//! - **不再依赖 `/proc`** ⇒ 这条判据在 Windows / macOS 上同样有效（原来只能在 Linux 上跑）；
//! - 量的是分配器看到的字节，不是内核页 ⇒ 不受 `alloc_zeroed`、glibc 是否把内存还给系统影响。
//!
//! # 诚实边界
//!
//! - 只覆盖**本线程**。被测代码若把大块内存的分配挪到别的线程上，本量具看不见。
//!   `inbound` 的 reader 在 `#[tokio::test]`（current-thread 运行时）下与测试同线程，
//!   ⚠⚠ **这一点今天没有判据**〔devbench F04, 08-10 订正〕。本行原写「由
//!   `the_probe_sees_allocations_made_by_the_reader_task` 钉着」—— 而那个名字**全仓只出现在
//!   这一行自己身上**，那条判据从未存在过。
//!   ★ **指向一个不存在的判据比没有注释更坏**：它让读者以为这一层有人守着，于是不会去查。
//!   ⇒ 如实登记为**无判据**。要真钉它得让探针在一个受控的分配序列上跑一遍并断言计数，
//!   而那要能从测试里驱动 reader task —— 今天做不到（那条路径要真实的 stdin 流）。
//! - 只在 `cfg(test)` 下接管全局分配器，**生产构建里这个文件整体为空**。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// 本线程「已分配未释放」的字节数。
    ///
    /// ⚠ 必须 `const` 初始化：带惰性初始化的 TLS 首次访问自己会分配，
    /// 而我们正在分配器里 —— 那是无限递归。`Cell<isize>` 无 `Drop`，
    /// 也不会注册析构器（注册析构器同样会分配）。
    static LIVE: Cell<isize> = const { Cell::new(0) };
    /// 自上次 `reset_peak` 以来 `LIVE` 达到过的最大值。
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

#[inline]
fn bump(delta: isize) {
    // `try_with` 而不是 `with`：线程析构阶段 TLS 已经拿不到，`with` 会 panic。
    // 那时候的分配不关我们的事，直接丢掉计数。
    let _ = LIVE.try_with(|live| {
        let now = live.get() + delta;
        live.set(now);
        if delta > 0 {
            let _ = PEAK.try_with(|peak| {
                if now > peak.get() {
                    peak.set(now);
                }
            });
        }
    });
}

struct Tracking;

// SAFETY: 每个方法都把实际分配转交给 `System`，只在前后维护两个 thread-local
// `Cell<isize>` 计数器。计数器是 `const` 初始化、无 `Drop`，其读写不会再分配，
// 因此不存在分配器重入。
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            bump(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        bump(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            bump(layout.size() as isize);
        }
        p
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            bump(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[global_allocator]
static TRACKING: Tracking = Tracking;

/// 把峰值拉回当前水位，返回当前水位。判据用法：`reset_peak()` → 跑被测代码 → `peak_since()`。
pub fn reset_peak() -> isize {
    LIVE.with(|live| {
        let now = live.get();
        PEAK.with(|peak| peak.set(now));
        now
    })
}

/// 自 `reset_peak` 以来，本线程「已分配未释放」字节数的**最大增量**（非负）。
pub fn peak_since(base: isize) -> usize {
    PEAK.with(|peak| (peak.get() - base).max(0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 量具自检①：它**看得见**一块用完即释放的瞬时峰值。
    ///
    /// 这正是 `VmHWM` 两遍法看不见的那个形状，也是被测缺陷的真实形状。
    #[test]
    fn a_transient_peak_is_visible_after_it_has_been_freed() {
        let base = reset_peak();
        {
            let peak = vec![7u8; 8 * 1024 * 1024];
            std::hint::black_box(&peak);
        } // ← 已释放
        let seen = peak_since(base);
        assert!(
            seen >= 8 * 1024 * 1024,
            "8 MiB 的瞬时峰值只量到 {seen} 字节 —— 量具没看见「用完即释放」的分配，\n\
             而那正是本量具存在的理由（`VmHWM` 的两遍法就是栽在这里）"
        );
    }

    /// 量具自检②：**邻居线程的分配不算在我头上** —— 这条是 F22 的本职。
    ///
    /// ⚠ 这条必须存在：如果哪天有人把 `LIVE`/`PEAK` 从 thread-local 改成 `static AtomicIsize`，
    /// 上面那条自检**照样绿**，而假红会原样回来。
    #[test]
    fn a_neighbour_threads_allocation_does_not_count_against_me() {
        let base = reset_peak();
        std::thread::spawn(|| {
            let hog = vec![7u8; 64 * 1024 * 1024];
            std::hint::black_box(&hog);
        })
        .join()
        .expect("邻居线程");
        let seen = peak_since(base);
        assert!(
            seen < 1024 * 1024,
            "邻居线程分配 64 MiB，本线程却量到 {seen} 字节 —— 计数器不是线程私有的，\n\
             `an_oversized_line_does_not_grow_memory` 的偶发假红会原样回来"
        );
    }

    /// 量具自检③：释放要**真的**把水位降回去，否则 `LIVE` 只增不减，
    /// 判据会随测试跑的顺序漂。
    #[test]
    fn freeing_brings_the_live_count_back_down() {
        let before = LIVE.with(|l| l.get());
        {
            let v = vec![7u8; 4 * 1024 * 1024];
            std::hint::black_box(&v);
        }
        let after = LIVE.with(|l| l.get());
        assert!(
            (after - before).abs() < 1024 * 1024,
            "分配 4 MiB 再释放，水位从 {before} 变成 {after} —— dealloc 没有回冲，\n\
             说明 `Tracking` 的某个方法漏了记账（realloc 最容易漏）"
        );
    }

    /// ★ **用这个量具的测试，一律不许是 multi_thread**〔audit-0805 §5 1k，08-06 结案〕。
    ///
    /// # 它治的是一次「静默变哑」
    ///
    /// 本量具是 **thread-local** 的（`LIVE`/`PEAK` 都在 `thread_local!` 里）。
    /// 被测代码若跑在别的线程上，`peak_since` 量到的是**测试线程自己**的峰值 ——
    /// 也就是**几乎为零**，于是「内存没涨」这个断言**恒真**。
    /// ⚠ 它不会报错、不会 panic，只会**永远绿**。
    ///
    /// 风险不是假设的：`inbound.rs` 那条内存判据是 `#[tokio::test]`（current-thread），
    /// 而**同一个文件里**就有 `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`。
    /// 照抄邻居的属性 = 把量具关掉，而**没有任何东西会红**。
    ///
    /// `ROADMAP §5` 的 1k 逐字写着「**这是本件已知没堵上的洞**，不是『测不了』而是『还没钉』」。
    /// 本条把它钉上。
    ///
    /// ⚠ needle 用 `reset_peak(` 而不是模块名：模块名在**本文件到处都是**（头注、函数名），
    /// 而调用点必然带括号。这是 F23/F24 两族的教训 —— 匹配单位要对得上事实。
    #[test]
    fn every_test_that_uses_this_probe_stays_single_threaded() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = guard_core::scan_tree!(&root, &["rs"]);
        assert!(
            files.len() >= 10,
            "只扫到 {} 个 .rs —— 遍历坏了，本条此刻是空转的",
            files.len()
        );
        let mut users = 0usize;
        let mut bad = Vec::new();
        for (path, raw) in &files {
            // ★ **先剥注释再扫**〔本条第一次跑就栽在这里〕。
            //
            // `common/fs.rs` 的头注里逐字写着「改成 `#[tokio::test(flavor = "multi_thread")]`
            // 会让它静默变哑」—— 那是一句**警告**，而本条把它当成了真属性、当场误报。
            // ⚠ 「判据数到注释」在本区已是第三次（F12 跨语言对拍 · F24 的裸 contains 计数 ·
            // 本条）。**写下来提醒自己无效**：剥注释要写进代码，不是写进注释。
            let src: String = raw
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let src = &src;
            let mut from = 0usize;
            while let Some(rel) = src[from..].find("reset_peak(") {
                let at = from + rel;
                from = at + 1;
                // 定义处不算（`pub fn reset_peak(`）。
                if src[..at].ends_with("pub fn ") {
                    continue;
                }
                users += 1;
                // 往前找最近的测试属性行。
                let head = &src[..at];
                let Some(a) = head
                    .rfind("#[tokio::test")
                    .or_else(|| head.rfind("#[test]"))
                else {
                    bad.push(format!("  {}：找不到它所在测试的属性行", path.display()));
                    continue;
                };
                let line_end = src[a..].find('\n').map_or(src.len(), |k| a + k);
                let attr = &src[a..line_end];
                if attr.contains("multi_thread") {
                    bad.push(format!("  {}：{attr}", path.display()));
                }
            }
        }
        // 抽取器自检：一个用户都没扫到时，下面那条会零命中地绿。
        assert!(
            users >= 2,
            "只扫到 {users} 处 `reset_peak(` 调用（08-06 实测：`inbound.rs` 与 `common/fs.rs` 各一处）\
             —— 抽取器坏了或量具没人用了，两种都要人来看"
        );
        assert!(
            bad.is_empty(),
            "这些用本量具的测试跑在 **multi_thread** 运行时上：\n{}\n\n\
             ★ 量具是 **thread-local** 的 —— 被测代码跑在别的线程时，`peak_since` 量到的是\n\
             测试线程自己的峰值（≈0），于是「内存没涨」**恒真**。\n\
             它不会报错、不会 panic，**只会永远绿**。\n\
             ⚠ 真要在多线程下量，得先给量具加跨线程聚合 —— 那是另一件事，别先改属性。",
            bad.join("\n")
        );
    }
}
