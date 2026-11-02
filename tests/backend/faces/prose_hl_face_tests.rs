//! 拉一段高亮（`prose-hl`）：交上来几块代码就回几份、次序对得上；认得的语言带类，没写语言的空；
//! 同样的内容第二次不再算；超了上限一个字节不算、回 `too_large`。夹具只造结构（几行占位代码）。

use super::*;
use cmd_wire::md::HlCode;

fn code(lang: Option<&str>, x: &str) -> HlCode {
    HlCode {
        lang: lang.map(str::to_string),
        x: x.to_string(),
    }
}

/// 三元组 `[起点差, 长, 类]…` 里出现过的类。
fn classes(hl: &[u32]) -> Vec<u32> {
    hl.chunks(3).filter(|t| t.len() == 3).map(|t| t[2]).collect()
}

#[test]
fn two_blocks_come_back_in_order_and_only_the_known_language_is_coloured() {
    let mut h = md_core::Highlighter::new(1 << 20);
    let req = ProseHlReq {
        codes: vec![code(Some("rust"), "fn main() { let x = 1; }"), code(None, "plain words")],
    };
    let got = seg_hl(&mut h, req).expect("两块代码在上限之内");
    assert_eq!(got.hl.len(), 2, "交两块回两份");
    assert!(
        classes(&got.hl[0]).contains(&(cmd_wire::md::HlClass::Keyword as u32)),
        "rust 那一块带关键字类：{:?}",
        got.hl[0]
    );
    assert!(got.hl[1].is_empty(), "没写语言的那一块不高亮");
}

#[test]
fn the_same_content_is_not_computed_twice() {
    let mut h = md_core::Highlighter::new(1 << 20);
    let req = || ProseHlReq {
        codes: vec![code(Some("python"), "def f():\n    return 1\n")],
    };
    let a = seg_hl(&mut h, req()).unwrap();
    let after_first = h.computed();
    let b = seg_hl(&mut h, req()).unwrap();
    assert_eq!(a, b);
    assert_eq!(h.computed(), after_first, "第二次命中缓存，不再算");
}

#[test]
fn over_the_cap_is_refused_without_computing() {
    let mut h = md_core::Highlighter::new(1 << 20);
    let big = "x".repeat(PROSE_HL_REQ_CAP.bytes as usize + 1);
    let err = seg_hl(
        &mut h,
        ProseHlReq {
            codes: vec![code(Some("rust"), &big)],
        },
    )
    .expect_err("超了上限要拒");
    assert!(matches!(err, ProseHlErr::TooLarge(_)));
    assert_eq!(h.computed(), 0, "拒收的那一趟一块都不算");
}
