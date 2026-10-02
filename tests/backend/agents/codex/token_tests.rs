//! # 这族的家：（乙 · 不该有条 —— codex 专项之前停放着的半成品）
//!
//! 核原文：「留着**不是因为有人在用**，是因为 codex 后面再做」。
//! 本族三条判 `codex_delta` 的减法（input 不含 cached）· 缺字段当 0 · 饱和减 —— 今天零生产消费者，口径哪篇都没写成要求；
//! 把一件停放着的半成品的口径升格成红线等于替 codex 专项提前做决定 ⇒ 等专项立项时再定。

use super::*;

#[test]
fn codex_delta_subtracts_cached_from_input_no_double_count() {
    // 真机数：input_tokens 含 cached ⇒ input+cache_read 必等于 input_tokens。
    // 反例（不减）：input=12599 且 cache_read=10496 ⇒ 总 prompt 虚报成 23095。
    let d = codex_delta(&serde_json::json!({
        "input_tokens": 12599, "cached_input_tokens": 10496, "output_tokens": 565,
    }));
    assert_eq!(d.input, 2103, "input = 未命中缓存部分");
    assert_eq!(d.cache_read, 10496);
    assert_eq!(d.output, 565);
    assert_eq!(
        d.input + d.cache_read,
        12599,
        "合起来等于总 prompt，无重复计"
    );
    assert!(!d.is_noop());
}

#[test]
fn codex_delta_missing_fields_are_zero_and_all_zero_is_noop() {
    assert!(codex_delta(&serde_json::json!({})).is_noop());
    assert!(codex_delta(&serde_json::json!({
        "input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0
    }))
    .is_noop());
    // 只有 output 也算真事件 —— is_noop 不许退化成「只看 input」。
    let only_out = codex_delta(&serde_json::json!({"output_tokens": 7}));
    assert!(!only_out.is_noop());
    assert_eq!(only_out.output, 7);
}

#[test]
fn codex_delta_cached_exceeding_input_saturates_instead_of_panicking() {
    // 畸形/未来形态：cached > input_tokens。u64 直接减会 panic（debug）或绕回天文数字。
    let d = codex_delta(&serde_json::json!({
        "input_tokens": 5, "cached_input_tokens": 9, "output_tokens": 1,
    }));
    assert_eq!(d.input, 0, "饱和减，不 panic 不绕回");
    assert_eq!(d.cache_read, 9);
}
