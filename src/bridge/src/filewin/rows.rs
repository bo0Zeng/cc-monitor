//! `24e` 的列表：**虚拟滚动**的文件行。
//!
//! # 🔴 这个模块存在的全部理由，是那一个 API 名字
//!
//! `真相源/99 §2.3` 那组对照读数（**同一趟、同一台机器、同一份语料**）：
//!
//! | 行数 | `ScrollArea::show_rows`（虚拟） | `ScrollArea::show`（不虚拟） |
//! |---:|---:|---:|
//! | 10 000 | 0.90 ms | 4.16 ms |
//! | 100 000 | **0.90 ms** | **83.63 ms（12 fps）** |
//! | 640 413 | **0.90 ms** | 刻意没跑（按斜率是秒级/帧） |
//!
//! ⇒ `设计/60 §4 戊` 逐字：**「egui 扛得住」这句话的主语是 `show_rows`，不是 egui。
//! 用错 API，egui 一样死。**
//!
//! # ⚠ 所以判据必须钉住「用的是虚拟滚动」，而不是「盘上写着 show_rows」
//!
//! 一条 `grep -q show_rows` 只证明**盘上有**，不证明**被走到**
//! （本仓自己的说法：`K-R18` 语料八「盘上有 ≠ 被走到」）。
//! ⇒ [`tests`] 里那几条**真跑一趟 egui**（headless，`Context::run_ui`），
//! 数**这一趟到底物化了多少行**，并且：
//!
//! 1. **恒等**：1 000 行那档与 640 413 行那档物化的行数**相等**
//!    —— 这条性质 `show` 结构上给不出来（它必然是 O(n)）。
//! 2. **对照组就在判据里**：同一个测试里放一份**故意不虚拟**的实现，
//!    断言它在 100 000 行上物化 **100 000** 行 —— 证明这把尺子量得出差别，
//!    不是两边都恒真。

use egui::{ScrollArea, Ui};

use super::source::Row;

/// 一行的高度（不含 item spacing）。与 `真相源/99` 那趟原型同值，
/// 那趟的「一屏约 44 行 @ 1280×800」就是按这个数算的。
pub const ROW_HEIGHT: f32 = 18.0;

/// 这一趟画了什么 —— 判据靠它说话，生产也靠它做诊断。
///
/// 🔴 `rows_materialized` 是**这一趟 row-painter 真的被调用的次数**，
/// 不是「应该是多少」的推算值。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderTally {
    pub rows_materialized: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub total_rows: usize,
}

/// 画一屏文件行。**这里是 `show_rows`，改成 `show` 会被判据当场逮住。**
///
/// 🔴 **这是画列表的唯一一条路** —— 生产（[`super::shell::FileWindow::ui`]）与
/// 判据（[`render_headless`]）调的是同一个函数。
/// ⚠ 这一条是**刻意的、承重的**：一开始 `render_headless` 自己抄了一份
/// `ScrollArea` 调用，于是「虚拟滚动」那条判据钉的是**判据自己那份副本**，
/// 生产那份改成 `show` 一条都不会红（死值验第 1 刀就是这么露出来的）。
/// **别再为了省一个参数把它抄回去。**
///
/// `scroll_offset_y`：`None` = 由 egui 自己管（生产）；`Some(y)` = 钉死偏移（量帧时用）。
pub fn show_file_rows(
    ui: &mut Ui,
    rows: &[Row],
    tally: &mut RenderTally,
    scroll_offset_y: Option<f32>,
) {
    tally.total_rows = rows.len();
    let mut area = ScrollArea::vertical().auto_shrink([false; 2]);
    if let Some(y) = scroll_offset_y {
        area = area.vertical_scroll_offset(y);
    }
    area.show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
        tally.first_row = range.start;
        tally.last_row = range.end;
        for i in range {
            let r = &rows[i];
            tally.rows_materialized += 1;
            paint_one_row(ui, r);
        }
    });
}

/// 一行的长相。**刻意抽出来**：虚拟与不虚拟两条路要画的是同一样东西，
/// 否则对照组比的就不是「虚不虚拟」而是「画得多不多」。
fn paint_one_row(ui: &mut Ui, r: &Row) {
    ui.horizontal(|ui| {
        ui.label(if r.is_dir { "📁" } else { "📄" });
        ui.label(&r.name);
        if !r.is_dir {
            ui.label(human_size(r.size));
        }
        if r.lossy_name {
            // 非 UTF-8 名：SFTP 那侧寻址不到真字节 ⇒ 写操作要灰置。
            // 这一刀只做标记，灰置逻辑等有写操作那一刀。
            ui.label("⚠");
        }
    });
}

/// 人读的大小。**不是** `format!("{size}")` —— 列表里一列宽度有限。
pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Headless 跑一趟 egui 并收 [`RenderTally`]。
///
/// ⚠ **这是 CPU 段**：布局 ＋ 生成绘制命令 ＋ 文字整形，不含把三角形交给 GPU。
/// 理由与射程照 `真相源/99 §一`（本机 `XDG_SESSION_TYPE=tty`，没有图形会话）。
/// 那一节的旁证也照抄下来：虚拟滚动那一档三角形数与总行数无关
/// ⇒ 交给 GPU 的活不随行数涨，**它不是 64 万行的风险点**。
pub fn render_headless(
    ctx: &egui::Context,
    rows: &[Row],
    screen: egui::Vec2,
    scroll_offset_y: f32,
) -> RenderTally {
    let mut tally = RenderTally::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        // 🔴 调的是**生产那个函数**，不是它的副本 —— 见 `show_file_rows` 的注释。
        let mut t = RenderTally::default();
        show_file_rows(ui, rows, &mut t, Some(scroll_offset_y));
        tally = t;
    });
    out.drop_without_applying_deltas();
    tally
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// 🔴 **对照组**：故意**不虚拟**的同一个列表（`ScrollArea::show`）。
    /// 它只住 `cfg(test)` —— 生产里不许有第二条画列表的路。
    /// 它的用处是证明上面那把尺子**量得出差别**：
    /// 没有它，「物化行数恒等」在两个实现上都可能恒真，那就是一条空真判据。
    pub fn render_headless_nonvirtual(
        ctx: &egui::Context,
        rows: &[Row],
        screen: egui::Vec2,
    ) -> RenderTally {
        let mut tally = RenderTally::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| {
            let mut t = RenderTally::default();
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for (i, r) in rows.iter().enumerate() {
                        t.rows_materialized += 1;
                        t.last_row = i + 1;
                        paint_one_row(ui, r);
                    }
                });
            t.total_rows = rows.len();
            tally = t;
        });
        out.drop_without_applying_deltas();
        tally
    }
}

#[cfg(test)]
mod tests {
    use super::testing::render_headless_nonvirtual;
    use super::*;
    use crate::filewin::corpus;

    /// 1280×800，与 `真相源/99` 那趟原型同一个视口。
    fn screen() -> egui::Vec2 {
        egui::vec2(1280.0, 800.0)
    }

    fn rows(n: usize) -> Vec<Row> {
        corpus::synth_rows(n, 0xC0FFEE)
    }

    /// 🔴 **本刀的核心判据**：物化的行数与总行数**无关**。
    ///
    /// 这是**恒等**断言（不是「小于某个上限」那种在「变少」方向瞎掉的地板）：
    /// 1 000 行与 640 413 行两档，物化行数必须**一模一样**。
    /// `ScrollArea::show` 结构上做不到这件事 —— 见下面那条对照。
    #[test]
    fn the_row_count_we_materialize_does_not_depend_on_how_many_rows_there_are() {
        let ctx = egui::Context::default();
        // 先跑一趟把字体图集建起来，免得第一趟的开销混进来。
        let _ = render_headless(&ctx, &rows(64), screen(), 0.0);

        let small = render_headless(&ctx, &rows(1_000), screen(), 0.0);
        let mid = render_headless(&ctx, &rows(100_000), screen(), 0.0);
        let big = render_headless(&ctx, &rows(640_413), screen(), 0.0);

        assert_eq!(
            small.rows_materialized, mid.rows_materialized,
            "1 000 行与 100 000 行物化的行数必须相等 —— 不相等就说明没走虚拟滚动"
        );
        assert_eq!(
            small.rows_materialized, big.rows_materialized,
            "1 000 行与 640 413 行物化的行数必须相等 —— 不相等就说明没走虚拟滚动"
        );
        // 反空真：它真的画了东西，而且远少于总数。
        assert!(
            small.rows_materialized > 0,
            "一行都没画 —— 0 不是绿（那也会让上面三个数恒等）"
        );
        assert!(
            big.rows_materialized < 640_413 / 1_000,
            "物化了 {} 行；虚拟滚动应当只物化一屏的量级",
            big.rows_materialized
        );
        // 总行数确实喂进去了（否则「无关」是因为压根没数据）。
        assert_eq!(big.total_rows, 640_413);
    }

    /// 🔴 **对照组**：证明上面那把尺子量得出差别。
    ///
    /// 同一个视口、同一份语料、同一个 `paint_one_row`，只把 `show_rows` 换成 `show`
    /// ⇒ 物化行数**等于总行数**。没有这一条，上面那条恒等可能两边都成立 ⇒ 空真。
    #[test]
    fn a_non_virtual_scroll_area_materializes_every_single_row() {
        let ctx = egui::Context::default();
        let _ = render_headless(&ctx, &rows(64), screen(), 0.0);

        // 10 万行走不虚拟的那条路，`真相源/99` 量到 83.6 ms/帧 —— 这里只数行数，不计时。
        let n = 100_000;
        let ctrl = render_headless_nonvirtual(&ctx, &rows(n), screen());
        assert_eq!(
            ctrl.rows_materialized, n,
            "不虚拟的那条路必须物化全部 {n} 行；它要是也只画一屏，这个对照组就废了"
        );

        let virt = render_headless(&ctx, &rows(n), screen(), 0.0);
        assert_eq!(virt.total_rows, ctrl.total_rows);
        // 两条路在**同一个 n** 上的差距：至少三个数量级。
        assert!(
            ctrl.rows_materialized >= virt.rows_materialized * 1_000,
            "虚拟 {} 行 vs 不虚拟 {} 行 —— 差距不到 1000 倍，尺子可疑",
            virt.rows_materialized,
            ctrl.rows_materialized
        );
    }

    /// 滚到中间时，物化的是**中间那一段**，不是永远的头部。
    /// （只断言「窗口移动了」与「宽度不变」，不锚死具体行号 —— 那由 egui 的取整决定。）
    #[test]
    fn scrolling_moves_the_materialized_window_without_widening_it() {
        let ctx = egui::Context::default();
        let rows = rows(640_413);
        let _ = render_headless(&ctx, &rows, screen(), 0.0);

        let top = render_headless(&ctx, &rows, screen(), 0.0);
        let deep = render_headless(&ctx, &rows, screen(), 500_000.0);

        assert_eq!(
            top.rows_materialized, deep.rows_materialized,
            "滚到深处物化的行数应当与顶部一样宽"
        );
        assert!(
            deep.first_row > top.first_row,
            "滚了 50 万像素，起始行还是 {} —— 视口没动",
            deep.first_row
        );
        assert!(
            deep.last_row <= 640_413,
            "越界了：last_row={}",
            deep.last_row
        );
    }

    #[test]
    fn human_size_is_short_enough_for_a_column() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 K");
        assert_eq!(human_size(1024 * 1024), "1.0 M");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 G");
    }
}
