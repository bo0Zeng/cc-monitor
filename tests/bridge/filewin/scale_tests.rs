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

/// 内存：egui **自己**的增量不许随行数涨。
///
/// `真相源/99 §2.4` 现打：语料本身 107.9 MiB（64 万个 `String`），
/// egui 自己约 **3.5 MiB** ⇒ 内存基本全在语料上，那是「持有 64 万条路径」的
/// 固有成本，不是框架的。这里钉的就是那句话：**渲染带来的 RSS 增量**
/// 与行数**不成比例**。
#[test]
fn egui_itself_does_not_keep_per_row_state() {
    if rss_kib() == 0 {
        println!("  F1 · 内存：这台机器读不到 RSS（非 Linux）⇒ 判不了，跳过（不是绿）");
        return;
    }
    let rows = corpus::synth_rows(640_413, 0xF1);
    let corpus_bytes: usize = rows.iter().map(|r| r.path.len() + r.name.len()).sum();
    let before = rss_kib();
    let ctx = egui::Context::default();
    let screen = egui::vec2(SCREEN.0, SCREEN.1);
    for i in 0..20 {
        let _ = render_headless(&ctx, &rows, screen, i as f32 * 1000.0);
    }
    let after = rss_kib();
    let delta_kib = after.saturating_sub(before);
    println!(
        "  F1 · 内存：语料 {:.1} MiB | 渲染前 {} KiB → 渲染后 {} KiB | 增量 {} KiB",
        corpus_bytes as f64 / 1048576.0,
        before,
        after,
        delta_kib
    );
    // 若 egui 给每行留了状态，64 万行的增量会是**几百 MiB** 量级。
    // 闸放在 64 MiB —— 99 现打是 3.5 MiB，余量 18 倍，但仍远低于「每行一份」的量级。
    assert!(
        delta_kib < 64 * 1024,
        "渲染 64 万行带来 {} KiB 的 RSS 增量 —— egui 像是在按行留状态",
        delta_kib
    );
}
