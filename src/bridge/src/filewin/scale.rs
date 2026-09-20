//! 秤 **F1**：首屏时延 · 滚动帧时 · 内存 —— 从一次性原型**变成执行链上的判据**。
//!
//! `真相源/99` 那三个数是 scratchpad 里一个原型打出来的，跑完就没了。
//! 本模块把它接到 `cargo test --lib` 上 ⇒ 落在门禁 `cargo` 那一格里
//! （`tests/scripts/gate.sh:1351`）。**判据不在执行链上就等于不存在。**
//!
//! # 🔴 一、为什么只量 CPU 段（照 `真相源/99 §一`，理由与射程一字不改）
//!
//! 本机**没有图形会话** —— `XDG_SESSION_TYPE=tty`，`DISPLAY` 与 `WAYLAND_DISPLAY` 都空。
//! 🔴 **刻意不走 Xvfb ＋ 软渲染**：那样量出来的光栅化速度既不像真机、也没法解释。
//!
//! ⇒ 只量 CPU 段，而这个选择是有道理的、不是将就：
//!
//! - egui 把一帧分成「布局＋生成绘制命令＋文字整形」（CPU）与「把三角形交给 GPU」两段；
//! - **64 万行压的是前一段**；后一段的代价只跟**屏幕上真有多少像素**有关，**与总行数无关**；
//! - 旁证（`真相源/99 §一` 现打）：虚拟滚动那一档，三角形数从 1 千行到 64 万行
//!   基本不变（9 978 → 10 852）⇒ 交给 GPU 的活确实与行数无关，**它不是 64 万行的风险点**。
//!
//! ⚠ **射程**：本秤买「同一台机器上，行数涨 64 倍时这三个数涨不涨」；
//! **不买**「在你所有机器上都流畅」，**不买** GPU 那一段，**不买**真机字体回落 / DPI / 合成器差异。
//!
//! # 🔴 二、判的是**关系**，不是绝对毫秒数
//!
//! 绝对阈值换台机器就得改，改着改着就变成「调宽了凑绿」。
//! 本秤钉的是**虚拟滚动买到的那条性质本身**：
//!
//! > 行数从 10 000 涨到 640 413（**64 倍**），帧时**不许跟着涨**。
//!
//! 这条在 `ScrollArea::show` 上结构上不成立（`真相源/99 §2.3`：10 万行 83.6 ms/帧）
//! ⇒ 它同时也是「有没有用对 API」的第二道门。
//! 对照组就在判据里（[`tests::the_flatness_meter_can_actually_see_a_slope`]），
//! 证明这把尺子量得出斜率，不是两边都恒真。
//!
//! # 🔴 三、**本秤印的绝对毫秒数不许拿去和 `真相源/99` 比**
//!
//! `99` 那趟是 `cargo build --release` 的原型；本秤住 `cargo test`，**是 debug 档**。
//! 同一台机器（laptop）上现打的差距摆在这儿：
//!
//! | | `真相源/99`（release 原型） | 本秤（debug，`cargo test`） |
//! |---|---:|---:|
//! | 64 万行滚动帧时中位 | **0.90 ms** | **约 11 ms** |
//! | 物化行数 | 一屏约 44 行 | **40 行** |
//!
//! ⇒ 两个分母不同，**别相减、也别说「变慢了 12 倍」**。
//! 物化行数那一行两边对得上（44 vs 40，视口与行高同值、取整差异）——
//! 那说明**被测的结构是同一个**，差的只有优化档。
//! ⚠ 所以本秤的判据一条都不钉绝对毫秒：**钉的全是同一档内部的比值**。
//! 「这个面板在真机上流不流畅」**本秤买不到**，那要 release 档 ＋ 真图形会话。

use super::corpus;
use super::rows::{render_headless, RenderTally};
use super::source::Row;

/// 视口：1280×800，与 `真相源/99` 那趟原型同值。
pub const SCREEN: (f32, f32) = (1280.0, 800.0);

/// 一趟 F1 读数。
#[derive(Clone, Copy, Debug)]
pub struct F1 {
    pub rows: usize,
    /// 首屏：从空 ctx 到第一帧画完（含建字体图集那一次）。
    pub first_paint_ms: f64,
    /// 滚动帧时中位 —— **滚动着量**，不是静止态。
    /// `真相源/99 §2.2` 现打过：偏移钉死是 0.06 ms、真滚是 0.90 ms，
    /// 差的那 15 倍就是「每帧给新出现的约 44 行整形文字」⇒ **该用的是滚动那个数**。
    pub frame_median_ms: f64,
    pub frame_p99_ms: f64,
    /// 这一帧物化了多少行（应当与行数无关）。
    pub rows_materialized: usize,
    /// 进程 RSS（KiB）。拿不到就是 0（非 Linux）。
    pub rss_kib: u64,
}

/// 读进程 RSS。Linux 走 `/proc/self/statm`；别的平台返回 0（**不是假装 0 字节**，
/// 是「这台机器上量不到」—— 判据那边按 0 跳过，并且把「跳过了」印出来）。
pub fn rss_kib() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let s = match std::fs::read_to_string("/proc/self/statm") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let pages: u64 = s
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        pages.saturating_mul(4) // 4 KiB/page
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// 量一趟。`frames` 帧里每帧把滚动偏移往下推一屏，**真的在滚**。
pub fn measure(rows: &[Row], frames: usize) -> F1 {
    let screen = egui::vec2(SCREEN.0, SCREEN.1);
    let ctx = egui::Context::default();

    // ── 首屏：全新 ctx 的第一帧（字体图集在这一帧建起来）
    let t0 = std::time::Instant::now();
    let first: RenderTally = render_headless(&ctx, rows, screen, 0.0);
    let first_paint_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // ── 滚动帧时：偏移扫过全程
    let total_h = rows.len() as f32 * (super::rows::ROW_HEIGHT + 4.0);
    let span = (total_h - SCREEN.1).max(1.0);
    let mut samples: Vec<f64> = Vec::with_capacity(frames);
    for i in 0..frames {
        let off = span * (i as f32 / frames.max(1) as f32);
        let t = std::time::Instant::now();
        let _ = render_headless(&ctx, rows, screen, off);
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[samples.len() / 2];
    let p99 = samples[((samples.len() as f64 * 0.99) as usize).min(samples.len() - 1)];

    F1 {
        rows: rows.len(),
        first_paint_ms,
        frame_median_ms: median,
        frame_p99_ms: p99,
        rows_materialized: first.rows_materialized,
        rss_kib: rss_kib(),
    }
}

impl std::fmt::Display for F1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "行 {:>7} | 首屏 {:>7.2} ms | 帧时中位 {:>6.3} ms | p99 {:>6.3} ms | 物化 {:>3} 行 | RSS {:>7} KiB",
            self.rows,
            self.first_paint_ms,
            self.frame_median_ms,
            self.frame_p99_ms,
            self.rows_materialized,
            self.rss_kib
        )
    }
}

#[cfg(test)]
mod tests {
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
}
