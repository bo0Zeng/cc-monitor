//! 公式排版的判据：仓里夹具那几条都排得出 · 分数有横线 · 行内比块级矮 · 排不出的给空 · 解析出的公式都带画单。

use super::*;
use crate::md::{Inl, InlNode, MathFont, MathItem, MdBlock};
use crate::parse;

/// 仓里正文夹具（金样 · 两端判据）里出现过的公式。
const FIXTURES: [&str; 5] = [
    "E=mc^2",
    "a+b",
    "\\sum_i x_i",
    "x^2",
    "\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}",
];

fn glyphs(m: &MathBox) -> usize {
    m.items
        .iter()
        .filter(|i| matches!(i, MathItem::G { .. }))
        .count()
}

#[test]
fn fixtures_lay_out_into_non_empty_boxes() {
    for tex in FIXTURES {
        for display in [true, false] {
            let m = lay(tex, display).unwrap_or_else(|| panic!("{tex:?} 排不出"));
            assert!(m.w > 0 && m.h + m.d > 0, "{tex:?} 的框是空的：{m:?}");
            assert!(glyphs(&m) > 0, "{tex:?} 一个字形都没有：{m:?}");
        }
    }
}

#[test]
fn letters_use_katex_math_italic() {
    let m = lay("x", false).unwrap();
    assert!(
        m.items.iter().any(|i| matches!(
            i,
            MathItem::G {
                f: MathFont::MathItalic,
                u: 0x78,
                ..
            }
        )),
        "x 应当是 Math-Italic 的 x：{m:?}"
    );
}

#[test]
fn a_fraction_has_a_bar() {
    let m = lay("\\frac{a}{b}", true).unwrap();
    assert!(
        m.items
            .iter()
            .any(|i| matches!(i, MathItem::L { w, th, .. } if *w > 0 && *th > 0)),
        "分数没有横线：{m:?}"
    );
}

#[test]
fn inline_style_is_shorter_than_display() {
    let tex = "\\sum_{i=1}^{n} \\frac{1}{i}";
    let (d, t) = (lay(tex, true).unwrap(), lay(tex, false).unwrap());
    assert!(
        t.h + t.d < d.h + d.d,
        "行内应当比块级矮：行内 {}，块级 {}",
        t.h + t.d,
        d.h + d.d
    );
}

#[test]
fn unparsable_tex_gives_none() {
    for tex in ["\\frac{a", "\\nosuchcommand{x}", ""] {
        assert_eq!(lay(tex, true), None, "{tex:?} 应当排不出");
    }
}

#[test]
fn oversized_tex_is_not_laid_out() {
    let tex = "x+".repeat(MAX_TEX_BYTES / 2 + 1);
    assert_eq!(lay(&tex, true), None);
}

#[test]
fn parsed_math_carries_its_box() {
    let doc = parse("行内 $E=mc^2$ 与块级：\n\n$$\n\\sum_i x_i\n$$\n");
    let mut inline = None;
    let mut block = None;
    for b in &doc.blocks {
        match b {
            MdBlock::P { k } => {
                for i in k {
                    if let Inl::N(InlNode::M { tex, m }) = i {
                        inline = Some((tex.clone(), m.clone()));
                    }
                }
            }
            MdBlock::Math { tex, m } => block = Some((tex.clone(), m.clone())),
            _ => {}
        }
    }
    let (tex, m) = inline.expect("没解析出行内公式");
    assert_eq!(tex, "E=mc^2");
    assert_eq!(m, lay("E=mc^2", false), "行内公式的画单应当照行内样式排");
    let (tex, m) = block.expect("没解析出块级公式");
    assert_eq!(tex, "\\sum_i x_i");
    assert_eq!(
        m,
        lay("\\sum_i x_i", true),
        "块级公式的画单应当照块级样式排"
    );
    assert!(m.is_some());
}
