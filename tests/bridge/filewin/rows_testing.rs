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
