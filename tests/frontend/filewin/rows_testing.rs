use super::*;

/// 🔴 **对照组**：故意**不虚拟**的同一个列表（`ScrollArea::show`）。
/// 它只住 `cfg(test)` —— 生产里不许有第二条画列表的路。
/// 它的用处是证明上面那把尺子**量得出差别**：
/// 没有它，「物化行数恒等」在两个实现上都可能恒真，那就是一条空真判据。
pub fn render_headless_nonvirtual(
    ctx: &egui::Context,
    rows: &[Listed],
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
                    paint_one_row(ui, i, r, false, Mark::default());
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
    rows: &[Listed],
    screen: egui::Vec2,
    time: f64,
    events: Vec<egui::Event>,
) -> RenderTally {
    render_headless_with_events_and_text(ctx, rows, screen, time, events).0
}

/// 同 [`render_headless_with_events`]，**另外**把这一帧真的画出来的文字带回来。
///
/// 🔴它存在的理由只有一个：行上那颗「复制」按钮在屏幕上的位置，
/// 判据自己算不出来（由字体宽度与布局决定）。⇒ 从**画出来的东西**里找它 ——
/// 内容正好是 `copy::COPY_LABEL` 的那一段文字，它的矩形就是那颗按钮的位置。
///
/// ⚠ 这样判据就**不需要**在生产里开一个「把按钮矩形吐出来」的测试专用出口
/// （那正是「把测试形状写进生产签名」那一形）。
pub fn render_headless_with_events_and_text(
    ctx: &egui::Context,
    rows: &[Listed],
    screen: egui::Vec2,
    time: f64,
    events: Vec<egui::Event>,
) -> (RenderTally, Vec<crate::copy::testing::PaintedText>) {
    let mut tally = RenderTally::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        time: Some(time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        let mut t = RenderTally::default();
        show_file_rows(ui, rows, &mut t, Some(0.0), None, None);
        tally = t;
    });
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    (tally, painted)
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

/// 实景台架（`Xvfb` ＋ `xdotool`）—— 三格此前记着「判不了」的判据靠它。
///
/// # 🔴 住址说明（为什么挂在这儿，而不是自己一个模块）
///
/// 它是一台**跨模块**的台架：`shell` 那一格（窗口起没起来）与 `rows` 那一格
/// （真事件点不点得动）都用它。而这一路的写区只有 `tests/` ——
/// 生产那棵树 `src/frontend/shell/src/filewin/` 归另一路，**只许调不许改**
/// ⇒ 往生产文件里挂一个新的 `#[path]` 模块这条路是关着的。
///
/// ⇒ 它挂在本文件下面：本文件是 `filewin/` 下**唯一**一个 `pub(crate)` 的测试模块，
/// 挂在这儿全 crate 的测试段都够得着，而**生产段一个字节没动**。
/// ⚠ 如实记成一处「住址不理想但没有第二条路」，别读成「台架属于 rows」。
#[cfg(not(windows))]
#[path = "xvfb_rig.rs"]
pub mod xvfb;
