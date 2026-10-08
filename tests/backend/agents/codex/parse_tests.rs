//! # 这族的家：（乙 · 不该有条 —— codex 专项的半成品，今天零生产消费者）
//!
//! 核原文：「`src/backend/agents/codex/parse.rs` 那一批**今天零生产消费者**」，留着是因为 codex 后面再做。
//! 本族钉的是 rollout 文件名取 sid 的行为（turn-end 识别那一半零读者，连同它的判据一起删了）。

use super::*;
use serde_json::json;

/// Phase D 审计修：sid 提取校验对齐 monitor（rollout- 前缀 + 末36 UUID 形），畸形名 → None（跳过、
/// 不吐幽灵行），合法名 → 末36 UUID。补后端侧此前缺的畸形名覆盖。
#[test]
fn codex_sid_from_path_validates_like_monitor() {
    let p = |n: &str| PathBuf::from(n);
    assert_eq!(
        codex_sid_from_path(&p(
            "rollout-2026-07-18T08-00-00-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl"
        ))
        .as_deref(),
        Some("019f75dd-875c-7c81-9eda-32f866b2c60f")
    );
    for bad in [
        "rollout-garbage.jsonl",                                 // 剥前缀后 <36
        "notrollout-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl", // 无 rollout- 前缀
        "rollout-2026-07-18T08-00-00-zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz.jsonl", // 末36 结构对但非 hex
    ] {
        assert!(codex_sid_from_path(&p(bad)).is_none(), "畸形名应跳: {bad}");
    }
}

#[test]
fn is_uuid_matches_8_4_4_4_12_hex() {
    assert!(is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60f"));
    assert!(!is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60")); // 末段 11 位
    assert!(!is_uuid("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz")); // 非 hex
    assert!(!is_uuid("019f75dd875c7c819eda32f866b2c60f")); // 无分隔
}
