//! 公式排版：一条 TeX ⇒ [`MathBox`]（排好的画单），两端只照着画。
//!
//! 排版器是 RaTeX（KaTeX 语法、KaTeX 的字体度量）：解析 ⇒ 排版 ⇒ 一串定位好的字形 · 线 · 矩形 · 路径。
//! 这里只做两件事：把它的浮点 em 换成千分之一 em 的整数，把字体名收成 [`MathFont`]。
//! 排不出（语法错 · 超出它的资源上限 · 原文太长 · 排版 panic）⇒ 空，两端照原文等宽排。

use crate::md::{MathBox, MathFont, MathItem, MathPath};
use ratex_layout::{layout, to_display_list, validate_layout, LayoutOptions};
use ratex_types::{DisplayItem, MathStyle, PathCommand};

/// 原文超过这么多字节就不排（照原文排）：排版耗时 · 画单大小都跟着原文长。
pub(crate) const MAX_TEX_BYTES: usize = 4096;

/// 排一条公式；`display` ⇒ 块级样式，否则行内样式。排不出 ⇒ 空。
pub(crate) fn lay(tex: &str, display: bool) -> Option<MathBox> {
    if tex.trim().is_empty() || tex.len() > MAX_TEX_BYTES {
        return None;
    }
    // 排版器是第三方的：哪条式子让它 panic 了，也只是这一条照原文排，不带倒整段正文。
    std::panic::catch_unwind(|| lay_unchecked(tex, display)).ok().flatten()
}

fn lay_unchecked(tex: &str, display: bool) -> Option<MathBox> {
    let nodes = ratex_parser::parse(tex).ok()?;
    let style = if display { MathStyle::Display } else { MathStyle::Text };
    let laid = layout(&nodes, &LayoutOptions::default().with_style(style));
    validate_layout(&laid).ok()?;
    let list = to_display_list(&laid);
    let items: Vec<MathItem> = list.items.iter().filter_map(item).collect();
    if items.is_empty() {
        return None;
    }
    Some(MathBox {
        w: milli(list.width),
        h: milli(list.height),
        d: milli(list.depth),
        items,
    })
}

/// em ⇒ 千分之一 em，取整；非有限值记 0。
fn milli(v: f64) -> i32 {
    if v.is_finite() {
        (v * 1000.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    } else {
        0
    }
}

fn item(i: &DisplayItem) -> Option<MathItem> {
    Some(match i {
        DisplayItem::GlyphPath {
            x,
            y,
            scale,
            font,
            char_code,
            ..
        } => MathItem::G {
            f: font_of(font),
            u: *char_code,
            x: milli(*x),
            y: milli(*y),
            s: milli(*scale),
        },
        DisplayItem::Line {
            x,
            y,
            width,
            thickness,
            dashed,
            ..
        } => MathItem::L {
            x: milli(*x),
            y: milli(*y),
            w: milli(*width),
            th: milli(*thickness),
            dash: *dashed,
        },
        DisplayItem::Rect {
            x, y, width, height, ..
        } => MathItem::R {
            x: milli(*x),
            y: milli(*y),
            w: milli(*width),
            h: milli(*height),
        },
        DisplayItem::Path {
            x,
            y,
            commands,
            fill,
            ..
        } => {
            if commands.is_empty() {
                return None;
            }
            MathItem::P {
                x: milli(*x),
                y: milli(*y),
                fill: *fill,
                d: commands.iter().map(path_cmd).collect(),
            }
        }
    })
}

fn path_cmd(c: &PathCommand) -> MathPath {
    match *c {
        PathCommand::MoveTo { x, y } => MathPath::M {
            x: milli(x),
            y: milli(y),
        },
        PathCommand::LineTo { x, y } => MathPath::L {
            x: milli(x),
            y: milli(y),
        },
        PathCommand::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => MathPath::C {
            x1: milli(x1),
            y1: milli(y1),
            x2: milli(x2),
            y2: milli(y2),
            x: milli(x),
            y: milli(y),
        },
        PathCommand::QuadTo { x1, y1, x, y } => MathPath::Q {
            x1: milli(x1),
            y1: milli(y1),
            x: milli(x),
            y: milli(y),
        },
        PathCommand::Close => MathPath::Z,
    }
}

/// RaTeX 的字体名（KaTeX 的 `Main-Regular` 那一套）⇒ [`MathFont`]；KaTeX 里没有的（汉字 · 表情的兜底）⇒ `Sys`。
fn font_of(name: &str) -> MathFont {
    match name {
        "AMS-Regular" => MathFont::Ams,
        "Caligraphic-Regular" => MathFont::CaligraphicRegular,
        "Fraktur-Regular" => MathFont::FrakturRegular,
        "Fraktur-Bold" => MathFont::FrakturBold,
        "Main-Regular" => MathFont::MainRegular,
        "Main-Bold" => MathFont::MainBold,
        "Main-Italic" => MathFont::MainItalic,
        "Main-BoldItalic" => MathFont::MainBoldItalic,
        "Math-Italic" => MathFont::MathItalic,
        "Math-BoldItalic" => MathFont::MathBoldItalic,
        "SansSerif-Regular" => MathFont::SansSerifRegular,
        "SansSerif-Bold" => MathFont::SansSerifBold,
        "SansSerif-Italic" => MathFont::SansSerifItalic,
        "Script-Regular" => MathFont::ScriptRegular,
        "Size1-Regular" => MathFont::Size1,
        "Size2-Regular" => MathFont::Size2,
        "Size3-Regular" => MathFont::Size3,
        "Size4-Regular" => MathFont::Size4,
        "Typewriter-Regular" => MathFont::Typewriter,
        _ => MathFont::Sys,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/md-core/math_tests.rs"]
mod tests;
