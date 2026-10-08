use super::*;

/// 行数涨 64 倍，每帧的活**不许跟着涨**。
///
/// # 口径 10-07 换过：原来量帧时毫秒，今天数每帧物化的行数
///
/// 原来判的是**帧时中位的比值**（闸 3 倍，现打 1.00）与**首屏毫秒的比值**（闸 10 倍）。
/// 本意是复杂度：虚拟滚动下一帧的代价与总行数无关。毫秒在门禁并发时会飘。
/// 今天数的是**这一帧 row-painter 真被调用了几次**（生产那条画列表的路自己记的 `rows_materialized`）：
/// 一帧的布局 ＋ 文字整形 ＋ 生成绘制命令正比于画了几行（每行同一套控件），
/// ⇒ 「画的行数与总行数无关」就是「帧时与总行数无关」的原因，而且是整数、确定。
/// 首屏那一帧与滚动扫过全程的每一帧都数（滚动那几帧取最多的一帧）。毫秒照旧印出来，只当读数。
#[test]
fn rows_painted_per_frame_stay_flat_from_ten_thousand_rows_to_six_hundred_thousand() {
    let small = corpus::synth_rows(10_000, 0xF1);
    let big = corpus::synth_rows(640_413, 0xF1);
    let (a, b) = (measure(&small, 60), measure(&big, 60));
    println!("  F1 · 虚拟滚动");
    println!("    {a} · 滚动一帧最多画 {} 行", a.scroll_rows_max);
    println!("    {b} · 滚动一帧最多画 {} 行", b.scroll_rows_max);

    // ① 首屏那一帧：两档物化的行数相等。
    assert_eq!(
        a.rows_materialized, b.rows_materialized,
        "两档首屏物化的行数不等 —— 虚拟滚动没生效，或者有人把 show_rows 换掉了"
    );
    // ② 滚动扫过全程：每帧最多画的行数相等（判的是斜率：行数 ×64，每帧的活 ×1）。
    assert_eq!(
        a.scroll_rows_max, b.scroll_rows_max,
        "行数涨 64 倍、滚动时一帧最多画的行数从 {} 变成 {} —— 每帧的活跟着总行数涨了",
        a.scroll_rows_max, b.scroll_rows_max
    );
    // ③ 反空真：真的画了东西，而且一帧只画一屏上下（远小于小档的总行数）。
    assert!(a.rows_materialized > 0 && a.scroll_rows_max > 0);
    assert!(
        a.scroll_rows_max * 10 < small.len(),
        "一帧画了 {} 行，小档总共才 {} 行 —— 这不是虚拟滚动",
        a.scroll_rows_max,
        small.len()
    );
}

/// 🔴 **对照组：证明上面那把尺子量得出斜率。**
///
/// 没有这一条，「两档行数相等」可能在任何实现上都成立（比如计数器根本没在数）
/// ⇒ 那就是一条空真判据。这里用**不虚拟**的那条路跑同样形状的跨度，
/// 断言它每帧画的行数**真的跟着总行数涨**（原来比的是帧时毫秒的比值 > 3，10-07 起数行数，理由同上一条）。
#[test]
fn the_flatness_meter_can_actually_see_a_slope() {
    use super::super::rows::testing::render_headless_nonvirtual;
    let screen = egui::vec2(SCREEN.0, SCREEN.1);
    let sizes = [1_000usize, 20_000];
    let painted: Vec<usize> = sizes
        .iter()
        .map(|&n| {
            let rows = corpus::synth_rows(n, 0xF1);
            let ctx = egui::Context::default();
            render_headless_nonvirtual(&ctx, &rows, screen).rows_materialized
        })
        .collect();
    let ratio = painted[1] as f64 / painted[0].max(1) as f64;
    println!("  F1 · 对照组（不虚拟 ScrollArea::show）");
    println!(
        "    1 000 行画 {} 行 → 20 000 行画 {} 行，比值 {ratio:.2}",
        painted[0], painted[1]
    );
    assert!(
        ratio > 3.0,
        "不虚拟的那条路在 20 倍数据上每帧只多画了 {ratio:.2} 倍 —— \
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

/// 🔴 **第二刀补上那条欠账：「滚久了会不会越来越胖」。**
///
/// # 上一条为什么买不到它（原文逐字留在它的头注里）
///
/// `egui_itself_does_not_keep_per_row_state` 比的是**两档行数**（1 千 / 64 万），
/// 而两档在固定 20 帧里**访问的行数差不多一样多**（20 帧 × 约 40 行）
/// ⇒ 「状态随已访问行数增长」这一维在那条判据里**是恒等的**，它看不见。
/// 那是现打出来的：换轴那一拍第一刀注的就是「给可见的那几十行留状态」，**没红**。
///
/// # 本条换的那一轴
///
/// **行数固定、帧数固定，只改「扫过多少行」**：
/// · 甲：偏移**钉死**在顶上 ⇒ 每帧访问同一批约 40 行；
/// · 乙：偏移**扫过整张列表** ⇒ 把 20 000 行全访问一遍。
/// 两边**同一档行数、同一个帧数** ⇒ 两张状态表的条数必须**相等**。
///
/// 这才是「滚久了会不会越来越胖」那个形状：唯一的自变量是已访问行数。
///
/// # 阳性对照（没有它这条同样可能恒真）
///
/// 第三趟**照乙的偏移序列**跑，但每访问一行就往那张表里按 `Id` 塞一条
/// ⇒ 条数必须涨到**已访问行数**的量级。它证明这把尺子认得出「按已访问行留状态」。
///
/// # ⚠ 买不到
///
/// · 仍然只看 `Memory::data` 那一张表（同上一条的边界）：galley 缓存 / 纹理图集
///   若按行涨，本条看不见。⚠ 那一维**还是欠着**，缺的是一份「egui 内部还有哪些
///   按行增长的容器」的独立读数。
/// · 不是「跑一整天不会胖」：帧数是 40，扫过的是 2 万行。它买的是**斜率为 0**，
///   不是「绝对值有上限」。
#[test]
fn egui_does_not_keep_state_per_row_we_have_scrolled_past() {
    const ROWS: usize = 20_000;
    const FRAMES: usize = 40;
    let rows = corpus::synth_rows(ROWS, 0xF1);
    let screen = egui::vec2(SCREEN.0, SCREEN.1);
    let row_h = crate::theme::metrics(&egui::Context::default()).row_h;
    let total_h = ROWS as f32 * (row_h + 4.0);
    let span = (total_h - SCREEN.1).max(1.0);

    // 偏移序列：甲恒 0，乙扫全程。**帧数一模一样。**
    let pinned: Vec<f32> = vec![0.0; FRAMES];
    let sweeping: Vec<f32> = (0..FRAMES)
        .map(|i| span * (i as f32 / (FRAMES - 1) as f32))
        .collect();

    // 跑一趟并回 (状态表条数, 这一趟一共访问了多少行)。
    let run = |offsets: &[f32]| -> (usize, usize) {
        let ctx = egui::Context::default();
        let mut visited = 0usize;
        for off in offsets {
            let t = render_headless(&ctx, &rows, screen, *off);
            visited += t.rows_materialized;
        }
        (ctx.memory(|m| m.data.len()), visited)
    };

    let (kept_pinned, seen_pinned) = run(&pinned);
    let (kept_sweep, seen_sweep) = run(&sweeping);

    println!("  F1 · 内存（换轴：行数固定 {ROWS}、帧数固定 {FRAMES}，只改扫过多少行）");
    println!("    偏移钉死 ⇒ 访问 {seen_pinned} 行次 · 状态表 {kept_pinned} 条");
    println!("    扫过全程 ⇒ 访问 {seen_sweep} 行次 · 状态表 {kept_sweep} 条");

    // 反空真：两趟访问的行数必须**真的**差开，否则「相等」什么都没说。
    // 钉死那趟访问的是同一批约 40 行；扫全程那趟每帧都是新行。
    assert!(
        seen_sweep >= seen_pinned,
        "扫全程访问的行次（{seen_sweep}）不该少于钉死那趟（{seen_pinned}）"
    );
    let distinct_pinned = seen_pinned / FRAMES; // 每帧同一批 ⇒ 不同行数就是一帧的量
    assert!(
        seen_sweep >= distinct_pinned * 20,
        "扫全程只访问了 {seen_sweep} 行次，而钉死那趟每帧约 {distinct_pinned} 行 —— \
         两趟的「已访问行数」没差开，这条判据此刻在空转（那正是上一条栽过的形状）"
    );

    assert_eq!(
        kept_pinned, kept_sweep,
        "同一档 {ROWS} 行、同样 {FRAMES} 帧：偏移钉死时状态表 {kept_pinned} 条，\
         扫过整张列表之后 {kept_sweep} 条 —— egui 在按**已访问的行**留状态，\
         也就是说滚久了会越来越胖。虚拟滚动省的是绘制，不是这个。"
    );

    // ── 阳性对照：照乙的偏移跑，但每访问一行就按 `Id` 塞一条 ────────────────
    let probe = egui::Context::default();
    let mut planted = 0usize;
    for off in &sweeping {
        let t = render_headless(&probe, &rows, screen, *off);
        probe.memory_mut(|m| {
            for i in t.first_row..t.last_row {
                m.data.insert_temp(egui::Id::new(("per-visited-row", i)), i);
                planted += 1;
            }
        });
    }
    let probe_kept = probe.memory(|m| m.data.len());
    println!("    阳性对照：按**已访问行**塞（塞了 {planted} 次）⇒ 表里 {probe_kept} 条");
    assert!(
        probe_kept >= seen_sweep / 2,
        "阳性对照只数到 {probe_kept} 条，而这一趟访问了 {seen_sweep} 行次 —— \
         这个计数器看不见「按已访问行留状态」，那么上面那条相等断言证明不了任何东西"
    );
    // 而且它必须比「钉死」那趟明显多 —— 否则对照组与被测组分不开。
    assert!(
        probe_kept > kept_pinned * 10,
        "阳性对照 {probe_kept} 条 vs 钉死那趟 {kept_pinned} 条 —— 差得不够，尺子可疑"
    );
}
