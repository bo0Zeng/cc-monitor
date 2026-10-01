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
/// 「**这是本件已知没堵上的洞**，不是『测不了』而是『还没钉』」。
/// 本条把它钉上。
///
/// ⚠ needle 用 `reset_peak(` 而不是模块名：模块名在**本文件到处都是**（头注、函数名），
/// 而调用点必然带括号。这是 F23/F24 两族的教训 —— 匹配单位要对得上事实。
#[test]
fn every_test_that_uses_this_probe_stays_single_threaded() {
    // 🔴 〔步 7c 剖分 2026-09-19〕**人群必须含测试树。**
    //
    // 本条的人群按定义是**测试代码**（「每一个用这个量具的测试都得单线程」），
    // 而测试段这一轮整批搬进了 `<repo>/tests/backend/`：`reset_peak(` 的调用点今天
    // 全住那儿（`common/fs_tests.rs` · `inbound_tests.rs` · `relay/http1_tests.rs`）。
    // 只扫 `src_root()` ⇒ `users` 掉到 0，自检逐字报「抽取器坏了或量具没人用了」——
    // 红得对，而它红的正是「一棵树掉出全部扫描面」（`§5.4b` 的第四种形状）。
    // ⇒ 两棵根，互不包含；排除**明写**（本文件的代码里有 `reset_peak(` 当 needle 的字面量，
    //   而 `scan_tree_excluding` 摘不到就 panic ⇒ 本文件改名会出声）。
    let src_root = crate::guard_support::src_root();
    let tests_root = crate::guard_support::tests_root();
    let mut files = guard_core::scan_tree_excluding(&src_root, &["rs"], &[]);
    files.extend(guard_core::scan_tree_excluding(
        &tests_root,
        &["rs"],
        &["alloc_probe_tests.rs"],
    ));
    // 面 B 成员的单测镜像住 `tests/comms/outward/`（`http1_tests.rs` 用这个量具）。
    files.extend(guard_core::scan_tree_excluding(
        &crate::guard_support::comms_tests_root(),
        &["rs"],
        &[],
    ));
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
