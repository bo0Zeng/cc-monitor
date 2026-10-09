//! `copy_core::said`：那一句与原话分开带；原话空 ⇒ 不带；拼进别的句子时只出那一句。

use super::*;

#[test]
fn the_raw_text_rides_beside_the_sentence_and_never_inside_it() {
    let s = Said::with_raw("写入远端失败 · 原因不明".into(), "夹具原话");
    assert_eq!(s.raw.as_deref(), Some("夹具原话"));
    assert_eq!(s.to_string(), "写入远端失败 · 原因不明");
    assert_eq!(Said::with_raw("x".into(), "  ").raw, None);
    assert_eq!(Said::from("x".to_string()).raw, None);
}
