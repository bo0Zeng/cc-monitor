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

#[test]
fn the_code_rides_through_every_reshape() {
    let s = Said::new("io_failed", "x".into()).wrap(|s| format!("外 · {s}"));
    assert_eq!(s.code.as_deref(), Some("io_failed"));
    assert_eq!(s.said, "外 · x");
    assert_eq!(Said::from("x").code, None);
}

#[test]
fn an_outer_layer_names_the_code_and_adds_the_raw_text() {
    let s = Said::from("x").coded("bad_args").and_raw(Some("原话"));
    assert_eq!(s.code.as_deref(), Some("bad_args"));
    assert_eq!(s.said, "x");
    assert_eq!(s.raw.as_deref(), Some("原话"));
    assert_eq!(
        s.clone().coded("io_failed").code.as_deref(),
        Some("io_failed")
    );
    assert_eq!(s.clone().and_raw(Some("  ")).raw.as_deref(), Some("原话"));
    assert_eq!(
        s.and_raw(Some("再一层")).raw.as_deref(),
        Some("原话\n再一层")
    );
}

#[test]
fn a_missing_code_answers_as_the_fallback() {
    assert_eq!(Said::from("x").code_or_fallback(), "failed");
    assert_eq!(
        Said::new("bad_args", "x".into()).code_or_fallback(),
        "bad_args"
    );
}

#[test]
fn retelling_moves_the_old_sentence_into_the_raw_text() {
    let s = Said::new("io_failed", "下层那句".into()).retold("那一句".into());
    assert_eq!(s.said, "那一句");
    assert_eq!(s.raw.as_deref(), Some("下层那句"));
    assert_eq!(s.code.as_deref(), Some("io_failed"));
    let kept = Said::with_raw("下层那句".into(), "原话").retold("那一句".into());
    assert_eq!(kept.raw.as_deref(), Some("原话"));
}

#[test]
fn data_rides_beside_the_code() {
    let s = Said::new("plan_conflict", "x".into())
        .with_data(serde_json::json!({"k": 1}))
        .wrap(|s| s.to_string());
    assert_eq!(s.data, Some(serde_json::json!({"k": 1})));
}

#[test]
fn new_names_the_code_before_the_sentence() {
    let s = Said::new("bad_args", "x".to_string());
    assert_eq!(s.code.as_deref(), Some("bad_args"));
    assert_eq!(s.said, "x");
    let s = Said::new("io_failed", "y".to_string());
    assert_eq!(s.code.as_deref(), Some("io_failed"));
    assert_eq!(s.said, "y");
}
