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
                    paint_one_row(ui, i, r);
                }
            });
        t.total_rows = rows.len();
        tally = t;
    });
    out.drop_without_applying_deltas();
    tally
}

/// 喂**真输入事件**跑一帧，仍然调**生产那个** [`show_file_rows`]。
///
/// 🔴 与第一刀那条教训的关系要说准：那一刀栽的是**画列表的那段代码被抄了一份**
/// （判据钉的是副本）。本函数抄的是 `RawInput` 的**装配**，
/// 而被判的对象（`show_file_rows` ／ `paint_one_row` 里那个 `interact`）**仍然只有一份**。
/// ⇒ 把 `show_rows` 换成 `show`、或者把那个 `interact` 摘掉，本函数照样当场红。
///
/// ⚠ 不塞进 `render_headless`（生产文件里那个）的理由：生产不需要「喂假事件」这件事，
/// 给它加一个只有判据用的参数，就是把测试形状写进生产签名。
pub fn render_headless_with_events(
    ctx: &egui::Context,
    rows: &[Row],
    screen: egui::Vec2,
    time: f64,
    events: Vec<egui::Event>,
) -> RenderTally {
    let mut tally = RenderTally::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        time: Some(time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        let mut t = RenderTally::default();
        show_file_rows(ui, rows, &mut t, Some(0.0));
        tally = t;
    });
    out.drop_without_applying_deltas();
    tally
}

/// 在 `pos` 按一下再松开（一次 click 的事件序列）。
pub fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ]
}
