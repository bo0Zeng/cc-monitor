use super::*;

/// 行数涨 64 倍，帧时**不许跟着涨**。
///
/// 🔴 判的是**比值**，不是绝对毫秒 —— 绝对阈值换台机器就得改，
/// 改着改着就变成调宽了凑绿。这里的闸是 **3 倍**，而
/// `真相源/99 §2.1` 现打的真实比值是 **1.00**（0.90 ms → 0.90 ms）
/// ⇒ 3 倍是给机器噪声留的余量，不是给退化留的：
/// 用不虚拟的那条路，同样的跨度会是 **64 倍**量级（见下一条对照）。
#[test]
fn frame_time_stays_flat_from_ten_thousand_rows_to_six_hundred_thousand() {
    let small = corpus::synth_rows(10_000, 0xF1);
    let big = corpus::synth_rows(640_413, 0xF1);

    let a = measure(&small, 60);
    let b = measure(&big, 60);
    println!("  F1 · 虚拟滚动");
    println!("    {a}");
    println!("    {b}");

    // ① 平坦性 —— 本条是承重的。
    let ratio = b.frame_median_ms / a.frame_median_ms.max(1e-6);
    println!("    帧时比值 640413/10000 = {ratio:.2}（闸 ≤ 3.0；99 现打 1.00）");
    assert!(
        ratio <= 3.0,
        "行数涨 64 倍、帧时涨了 {ratio:.2} 倍 —— 虚拟滚动没生效，或者有人把 show_rows 换掉了"
    );

    // ② 物化行数与行数无关（与 rows.rs 那条同源，这里再钉一次是因为
    //    帧时平坦也可能来自「两档都慢得一样」）。
    assert_eq!(
        a.rows_materialized, b.rows_materialized,
        "两档物化的行数不等 —— 平坦是假的"
    );

    // ③ 反空真：真的量到了东西。
    assert!(a.frame_median_ms > 0.0 && b.frame_median_ms > 0.0);
    assert!(a.rows_materialized > 0);

    // ④ 首屏：64 倍数据，首屏不许涨成另一个量级。
    //    ⚠ 首屏含建字体图集，噪声比帧时大 ⇒ 闸放到 10 倍。
    let fp = b.first_paint_ms / a.first_paint_ms.max(1e-6);
    println!("    首屏比值 = {fp:.2}（闸 ≤ 10.0）");
    assert!(fp <= 10.0, "首屏时延涨了 {fp:.2} 倍");
}

/// 🔴 **对照组：证明上面那把尺子量得出斜率。**
///
/// 没有这一条，「比值 ≤ 3」可能在任何实现上都成立（比如两档都被别的开销淹掉）
/// ⇒ 那就是一条空真判据。这里用**不虚拟**的那条路跑同样的跨度，
/// 断言它**真的会超过那道闸**。
///
/// ⚠ 跨度只取 1 000 → 20 000（20 倍）：`真相源/99 §2.3` 现打 10 万行是 83.6 ms/帧，
/// 再往上跑完要几分钟而结论不变 —— 同一条「刻意不跑」的理由。
#[test]
fn the_flatness_meter_can_actually_see_a_slope() {
    use super::super::rows::testing::render_headless_nonvirtual;
    let screen = egui::vec2(SCREEN.0, SCREEN.1);

    let mut t = |n: usize| -> f64 {
        let rows = corpus::synth_rows(n, 0xF1);
        let ctx = egui::Context::default();
        let _ = render_headless_nonvirtual(&ctx, &rows[..64.min(rows.len())], screen);
        let mut s: Vec<f64> = Vec::new();
        for _ in 0..5 {
            let t0 = std::time::Instant::now();
            let _ = render_headless_nonvirtual(&ctx, &rows, screen);
            s.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[s.len() / 2]
    };

    let a = t(1_000);
    let b = t(20_000);
    let ratio = b / a.max(1e-6);
    println!("  F1 · 对照组（不虚拟 ScrollArea::show）");
    println!("    1 000 行 {a:.3} ms → 20 000 行 {b:.3} ms，比值 {ratio:.2}");
    assert!(
        ratio > 3.0,
        "不虚拟的那条路在 20 倍数据上只涨了 {ratio:.2} 倍 —— \
             那说明这把尺子量不出斜率，上面那条平坦性判据是空的"
    );
}

/// 内存：egui **自己**留的状态不许随行数涨。
///
/// # 🔴 这一条 2026-09-20 整条重写 —— 上一版量错了对象
///
/// 上一版量的是**进程级 RSS**（`/proc/self/status` 的 `VmRSS`）并把闸放在 64 MiB。
/// 立它的那一路**自己报过它「结构上噪声敏感」**，而它**当趟就在门禁上红了**：
/// `渲染 64 万行带来 129056 KiB 的 RSS 增量`。
///
/// 🔴 **那 126 MiB 里绝大部分不是 egui 的。** `cargo test` 把 1500+ 条测试跑在
/// **同一个进程**的多条线程上 ⇒ `VmRSS` 是**全进程共享**的读数，它在构造上
/// 分不开「egui 分配的」与「隔壁那条测试分配的」。
/// ⇒ 这不是「闸放窄了」，是**量具与被测性质不同轴**：
///   放宽闸只会让它在更晚的某一趟再红一次，而且红的时候仍然说不出是谁的内存。
///
/// # 换成什么
///
/// 要钉的性质是「**egui 不按行留状态**」。而那个性质有一个**确定的、进程内的**量：
/// egui 自己那张按 `Id` 存控件状态的表（`Memory::data`，`IdTypeMap`）的**条数**。
/// · 它只属于这一个 `Context` ⇒ 隔壁测试碰不到它
/// · 它是**整数**且确定 ⇒ 判法从「小于某个阈值」变成**相等**
/// · 它正是「按行留状态」会涨的那个东西 ⇒ 与性质同轴
///
/// ⇒ 1 千行与 64 万行各起一个新 `Context`、各渲 20 帧，**两张表的条数必须相等**。
///
/// # 阳性对照（没有它，这条判据可能根本看不见增长）
///
/// 第三趟**故意按行往那张表里塞状态**（每行一个 `Id`），断言条数**真的涨到行数量级**。
/// 它证明这个计数器认得出「按行留状态」那一形 —— 前两趟的相等才有意义。
///
/// # ⚠ 买不到
///
/// · 它只看 `Memory::data` 那一张表。egui 若在**别处**按行留东西（纹理图集、
///   galley 缓存），本条看不见。⇒ 那一维今天**判不了**，缺的是一份「egui 内部
///   还有哪些按 Id 增长的容器」的独立读数。
/// · RSS 那个读数**保留为读数**（每趟印出来），但**不再当闸** —— 见上。
/// · 🔴 **它只钉「状态随列表长度增长」，钉不住「状态随已访问行数增长」。**
///   这一条是现打出来的，不是想出来的：换判据那一拍我第一刀注的是
///   「给**可见的**那几十行留状态」，**那一刀没红** —— 因为虚拟滚动下
///   两档（1 千行 / 64 万行）在固定 20 帧里访问的行数**差不多一样多**
///   （20 帧 × 约 40 行；1 千行那档整张列表就在这个跨度里）。
///   ⇒ 相等照样成立，而判据什么都没看见。
///   重切成「给**整张列表**每一行留状态」之后当场红（1001 条 vs 640414 条）。
///   ⇒ **「滚久了会不会越来越胖」是另一条真性质，本条买不到**，
///     它要的是「同一档行数、跑很多帧、扫过整张列表」那种形状的秤。欠着，如实记。
/// · ⚠ 顺带一条读数自证了为什么不能用 RSS：换轴那一趟现打，
///   **1 千行那档的 RSS 增量（13 028 KiB）比 64 万行那档（1 564 KiB）还大**。
///   在旧判据的口径下这读数根本不自洽 —— 它全是隔壁测试的噪声。
#[test]
fn egui_itself_does_not_keep_per_row_state() {
    fn retained(n: usize) -> (usize, u64) {
        let rows = corpus::synth_rows(n, 0xF1);
        let before = rss_kib();
        let ctx = egui::Context::default();
        let screen = egui::vec2(SCREEN.0, SCREEN.1);
        for i in 0..20 {
            let _ = render_headless(&ctx, &rows, screen, i as f32 * 1000.0);
        }
        let kept = ctx.memory(|m| m.data.len());
        (kept, rss_kib().saturating_sub(before))
    }

    let (small_kept, small_rss) = retained(1_000);
    let (big_kept, big_rss) = retained(640_413);

    println!("  F1 · 内存（egui 自己那张按 Id 的状态表，条数）");
    println!("    1 000 行   ⇒ {small_kept} 条");
    println!("    640 413 行 ⇒ {big_kept} 条");
    println!(
        "    ⚠ 顺带读数（**不是闸**，进程级 RSS 在并行测试进程里量不准）：\
         增量 {small_rss} KiB / {big_rss} KiB"
    );

    assert_eq!(
        small_kept, big_kept,
        "行数从 1 000 涨到 640 413，egui 那张按 `Id` 的状态表从 {small_kept} 条涨到 {big_kept} 条 \
         —— 它在按行留状态。虚拟滚动只省了绘制，没省状态，滚久了会越来越胖"
    );

    // ── 阳性对照：这个计数器认不认得出「按行留状态」 ──────────────────────
    let probe = egui::Context::default();
    let rows = corpus::synth_rows(5_000, 0xF1);
    probe.memory_mut(|m| {
        for (i, _) in rows.iter().enumerate() {
            m.data.insert_temp(egui::Id::new(("per-row-probe", i)), i);
        }
    });
    let probe_kept = probe.memory(|m| m.data.len());
    println!("    阳性对照：故意按行塞 5 000 条 ⇒ 表里 {probe_kept} 条");
    assert!(
        probe_kept >= 5_000,
        "阳性对照只数到 {probe_kept} 条（应当 ≥ 5 000）—— 这个计数器看不见「按行留状态」，\
         那么上面那条相等断言证明不了任何东西"
    );
}
