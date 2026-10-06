//! 判据用：一个目录视图连同它的框（工具条 · 命令栏 · 状态栏）跑一帧 —— 摆法与 `Workspace::chrome_ui` 同一套面板。

use crate::shell::FileWindow;
use crate::theme::metrics;

/// 一帧：工具条 · 命令栏 · 状态栏 ＋ 正文（列表）。
pub fn pane_with_chrome(ui: &mut egui::Ui, w: &mut FileWindow) {
    let bar_h = metrics(ui.ctx()).bar_h;
    egui::Panel::top("t-toolbar")
        .exact_size(bar_h)
        .show(ui, |ui| w.toolbar_ui(ui));
    egui::Panel::top("t-commands")
        .exact_size(bar_h - 4.0)
        .show(ui, |ui| {
            if let Some(c) = w.command_bar_ui(ui, crate::chrome::ViewState::default()) {
                let ctx = ui.ctx().clone();
                w.run_command(c, Some(ctx));
            }
        });
    egui::Panel::bottom("t-status")
        .exact_size(28.0)
        .show(ui, |ui| w.status_ui(ui));
    w.frame_body(ui);
}

/// 跑一帧 [`pane_with_chrome`]，交回这一帧画出来的字 ＋ 位置。
pub fn frame(
    ctx: &egui::Context,
    w: &mut FileWindow,
    events: Vec<egui::Event>,
) -> Vec<crate::copy::testing::PaintedText> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| pane_with_chrome(ui, w));
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted
}

/// 跑一帧生产那个 `Workspace::frame`，交回这一帧画出来的字 ＋ 位置。
pub fn ws_frame(
    ctx: &egui::Context,
    ws: &mut crate::workspace::Workspace,
    events: Vec<egui::Event>,
) -> Vec<crate::copy::testing::PaintedText> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| ws.frame(ui));
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted
}

/// 找到 `label` 那一段字（恰好一处），点它一下（先画一帧找位置，再喂一次点击）。
pub fn click(
    ctx: &egui::Context,
    w: &mut FileWindow,
    label: &str,
) -> Vec<crate::copy::testing::PaintedText> {
    let painted = frame(ctx, w, Vec::new());
    let at = crate::copy::testing::rects_of(&painted, label);
    assert_eq!(at.len(), 1, "这一帧上「{label}」该恰好一处：{painted:?}");
    frame(ctx, w, crate::rows::testing::click_at(at[0].center()))
}

/// 同 [`click`]，跑的是整扇窗（`Workspace::frame`）。
pub fn ws_click(
    ctx: &egui::Context,
    ws: &mut crate::workspace::Workspace,
    label: &str,
) -> Vec<crate::copy::testing::PaintedText> {
    let painted = ws_frame(ctx, ws, Vec::new());
    let at = crate::copy::testing::rects_of(&painted, label);
    assert_eq!(at.len(), 1, "这一帧上「{label}」该恰好一处：{painted:?}");
    ws_frame(ctx, ws, crate::rows::testing::click_at(at[0].center()))
}
