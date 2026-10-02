//! 秤 **F1**：首屏时延 · 滚动帧时 · 内存 —— 从一次性原型**变成执行链上的判据**。
//!
//! 那三个数是 scratchpad 里一个原型打出来的，跑完就没了。
//! 本模块把它接到 `cargo test --lib` 上 ⇒ 落在门禁 `cargo` 那一格里
//! （`gate.sh` 里 `run_gate_sum cargo 10` 那一行）。**判据不在执行链上就等于不存在。**
//!
//! # 🔴 一、为什么只量 CPU 段（照，理由与射程一字不改）
//!
//! 本机**没有图形会话** —— `XDG_SESSION_TYPE=tty`，`DISPLAY` 与 `WAYLAND_DISPLAY` 都空。
//! 🔴 **刻意不走 Xvfb ＋ 软渲染**：那样量出来的光栅化速度既不像真机、也没法解释。
//!
//! ⇒ 只量 CPU 段，而这个选择是有道理的、不是将就：
//!
//! - egui 把一帧分成「布局＋生成绘制命令＋文字整形」（CPU）与「把三角形交给 GPU」两段；
//! - **64 万行压的是前一段**；后一段的代价只跟**屏幕上真有多少像素**有关，**与总行数无关**；
//! - 旁证（现打）：虚拟滚动那一档，三角形数从 1 千行到 64 万行
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
//! 这条在 `ScrollArea::show` 上结构上不成立（10 万行 83.6 ms/帧）
//! ⇒ 它同时也是「有没有用对 API」的第二道门。
//! 对照组就在判据里（[`tests::the_flatness_meter_can_actually_see_a_slope`]），
//! 证明这把尺子量得出斜率，不是两边都恒真。
//!
//! # 🔴 三、**本秤印的绝对毫秒数不许拿去和比**
//!
//! `99` 那趟是 `cargo build --release` 的原型；本秤住 `cargo test`，**是 debug 档**。
//! 同一台机器（laptop）上现打的差距摆在这儿：
//!
//! | | （release 原型） | 本秤（debug，`cargo test`） |
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
use super::source::Listed;

/// 视口：1280×800，与那趟原型同值。
pub const SCREEN: (f32, f32) = (1280.0, 800.0);

/// 一趟 F1 读数。
#[derive(Clone, Copy, Debug)]
pub struct F1 {
    pub rows: usize,
    /// 首屏：从空 ctx 到第一帧画完（含建字体图集那一次）。
    pub first_paint_ms: f64,
    /// 滚动帧时中位 —— **滚动着量**，不是静止态。
    /// 现打过：偏移钉死是 0.06 ms、真滚是 0.90 ms，
    /// 差的那 15 倍就是「每帧给新出现的约 44 行整形文字」⇒ **该用的是滚动那个数**。
    pub frame_median_ms: f64,
    pub frame_p99_ms: f64,
    /// 这一帧物化了多少行（应当与行数无关）。
    pub rows_materialized: usize,
    /// 进程 RSS（KiB）。拿不到就是 0（非 Linux）。
    pub rss_kib: u64,
}

// 读进程 RSS（Linux `/proc/self/statm` · 别处 0）住 `platform.rs::rss_kib`。
pub use crate::platform::rss_kib;

/// 量一趟。`frames` 帧里每帧把滚动偏移往下推一屏，**真的在滚**。
pub fn measure(rows: &[Listed], frames: usize) -> F1 {
    let screen = egui::vec2(SCREEN.0, SCREEN.1);
    let ctx = egui::Context::default();

    // ── 首屏：全新 ctx 的第一帧（字体图集在这一帧建起来）
    let t0 = std::time::Instant::now();
    let first: RenderTally = render_headless(&ctx, rows, screen, 0.0);
    let first_paint_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // ── 滚动帧时：偏移扫过全程
    let total_h = rows.len() as f32 * super::rows::ROW_HEIGHT;
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
#[path = "../../../../tests/frontend/filewin/scale_tests.rs"]
mod tests;
