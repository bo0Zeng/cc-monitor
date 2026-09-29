//! 量具：**从 egui 这一帧真的画出来的东西里，把文字读回来。**
//!
//! # 🔴 它为什么必须存在（而不是拿 `grep` 顶上）
//!
//! `真相源/99 §8.5`／`§9.1` 记过同一条教训两层：
//! 「`grep -q show_rows` 只证明盘上有」·「调了那个看起来对的 egui API 也只证明盘上有」。
//! 「退路要在界面上出声」这一条同族 —— 一条
//! `assert!(src.contains("colored_label"))` 在**那一行被 `if false` 包住**时照样绿。
//!
//! ⇒ 判据走这条路：**真跑一帧生产那个 `ui()`，再从 `FullOutput::shapes` 里
//! 把 egui 这一帧交给文字排版的每一段文字抠出来**，按内容相等/包含判。
//!
//! # ⚠ 它买不到什么（逐条）
//!
//! - **「那几个像素在屏幕上没被裁掉」买不到。** `Galley::text()` 的文档逐字是
//!   「the full, non-elided text of the input job」⇒ 一段被列宽省略号截断的文字，
//!   这里读回来仍是全文。本量具证明的是「这一帧**真的把这句话拿去排版了**」。
//! - **颜色买不到**（`Shape::Text` 上那个「覆盖文字颜色」的字段这里没读）。
//!   「警告档」那一维由 [`super::outcome_notice`] 的相等断言钉，不由这把尺子钉。
//! - **真机上看得见买不到** —— 本机 `XDG_SESSION_TYPE=tty`，没有图形会话。

use super::*;

/// 这一帧画出来的一段文字：内容 ＋ 它的矩形。
pub type PaintedText = (String, egui::Rect);

/// 从一帧 `FullOutput` 里把**真的交给文字排版的**每一段文字连同矩形抠出来。
///
/// ⚠ `Shape::Vec` 要递归（egui 里嵌套 shape 是常见的）——
/// 不递归的话某些控件的文字会**整段看不见**，那会让这把尺子在「没画出来」与
/// 「画了但我没找着」之间分不开。
pub fn text_in_frame(out: &egui::FullOutput) -> Vec<PaintedText> {
    let mut found = Vec::new();
    for cs in &out.shapes {
        collect_text(&cs.shape, &mut found);
    }
    found
}

fn collect_text(s: &egui::Shape, out: &mut Vec<PaintedText>) {
    match s {
        egui::Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            egui::Rect::from_min_size(t.pos, t.galley.size()),
        )),
        egui::Shape::Vec(v) => {
            for one in v {
                collect_text(one, out);
            }
        }
        _ => {}
    }
}

/// 跑一帧 egui（可喂合成事件），把这一帧画出来的文字收回来。
pub fn painted_text(
    ctx: &egui::Context,
    screen: egui::Vec2,
    time: f64,
    events: Vec<egui::Event>,
    build: impl FnMut(&mut egui::Ui),
) -> Vec<PaintedText> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        time: Some(time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, build);
    let found = text_in_frame(&out);
    out.drop_without_applying_deltas();
    found
}

/// 这一帧画出来的文字里，有没有一段**包含** `needle`。
pub fn painted_contains(painted: &[PaintedText], needle: &str) -> bool {
    painted.iter().any(|(t, _)| t.contains(needle))
}

/// 这一帧里内容**正好等于** `label` 的那几段文字的矩形（按出现顺序）。
pub fn rects_of(painted: &[PaintedText], label: &str) -> Vec<egui::Rect> {
    painted
        .iter()
        .filter(|(t, _)| t == label)
        .map(|(_, r)| *r)
        .collect()
}
