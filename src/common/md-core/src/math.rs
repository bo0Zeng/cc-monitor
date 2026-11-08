//! 公式排版：一条 TeX ⇒ [`MathBox`]（排好的画单），两端只照着画。
//!
//! 排版器是 RaTeX（KaTeX 语法、KaTeX 的字体度量）：解析 ⇒ 排版 ⇒ 一串定位好的字形 · 线 · 矩形 · 路径。
//! 这里只做两件事：把它的浮点 em 换成千分之一 em 的整数，字体名只留 KaTeX 认得的那十九族（[`KATEX_FONTS`]）。
//! 排不出（语法错 · 超出它的资源上限 · 原文太长 · 排版 panic）⇒ 空，两端照原文等宽排。

use crate::md::{MathBox, MathItem, MathPath};
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
    std::panic::catch_unwind(|| lay_unchecked(tex, display))
        .ok()
        .flatten()
}

fn lay_unchecked(tex: &str, display: bool) -> Option<MathBox> {
    let nodes = ratex_parser::parse(tex).ok()?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
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
            f: font_stem(font),
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
            x,
            y,
            width,
            height,
            ..
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

/// KaTeX 那一套字体（两端各带同一份文件，`src/shared/fonts/katex/KaTeX_<名>.ttf`）：RaTeX 的字体名就是文件名主干。
const KATEX_FONTS: [&str; 19] = [
    "AMS-Regular",
    "Caligraphic-Regular",
    "Fraktur-Regular",
    "Fraktur-Bold",
    "Main-Regular",
    "Main-Bold",
    "Main-Italic",
    "Main-BoldItalic",
    "Math-Italic",
    "Math-BoldItalic",
    "SansSerif-Regular",
    "SansSerif-Bold",
    "SansSerif-Italic",
    "Script-Regular",
    "Size1-Regular",
    "Size2-Regular",
    "Size3-Regular",
    "Size4-Regular",
    "Typewriter-Regular",
];

/// RaTeX 的字体名 ⇒ 线上那一格：KaTeX 的一族 ⇒ 原样（文件名主干）；别的（汉字 · 表情的兜底）⇒ 缺格（系统字体）。
fn font_stem(name: &str) -> Option<String> {
    KATEX_FONTS.contains(&name).then(|| name.to_owned())
}

#[cfg(test)]
#[path = "../../../../tests/common/md-core/math_tests.rs"]
mod tests;
