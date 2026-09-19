use super::*;
use serde_json::json;

/// 基准正例：assistant + end_turn + 无 error + 无 sidechain → turn-end，uuid 抽出。
#[test]
fn plain_end_turn_is_turn_end() {
    let v = json!({
        "type": "assistant",
        "uuid": "u-1",
        "isSidechain": false,
        "message": {"stop_reason": "end_turn"}
    });
    assert!(is_turn_end(&v));
    assert_eq!(turn_end_uuid(&v), Some("u-1"));
}

/// 对拍 aterm 守卫四条，逐条证伪。
#[test]
fn guard_conditions_each_exclude() {
    // 非 assistant
    assert!(!is_turn_end(&json!({
        "type": "user", "message": {"stop_reason": "end_turn"}
    })));
    // stop_reason 非 end_turn（tool_use / max_tokens / null）
    for sr in ["tool_use", "max_tokens", "stop_sequence"] {
        assert!(
            !is_turn_end(&json!({"type":"assistant","message":{"stop_reason": sr}})),
            "stop_reason={sr} 不该是 turn-end"
        );
    }
    // isApiErrorMessage=true → 排除（API 错误不是完成一轮）
    assert!(!is_turn_end(&json!({
        "type":"assistant","isApiErrorMessage":true,"message":{"stop_reason":"end_turn"}
    })));
    // isSidechain=true → 排除（子代理轮不通知主链）
    assert!(!is_turn_end(&json!({
        "type":"assistant","isSidechain":true,"message":{"stop_reason":"end_turn"}
    })));
}

/// 字段坑：stop_reason 必须从 **message** 下取，top-level 的同名字段不算。
#[test]
fn stop_reason_must_be_nested_under_message() {
    // top-level stop_reason 是坑——不该被当成 end_turn。
    let top = json!({"type":"assistant","stop_reason":"end_turn","message":{}});
    assert!(
        !is_turn_end(&top),
        "top-level stop_reason 不算，须在 message 下"
    );
    // message 下缺 stop_reason → 非 turn-end。
    assert!(!is_turn_end(&json!({"type":"assistant","message":{}})));
}

/// 缺字段安全默认：无 isApiErrorMessage/isSidechain（缺失）→ 视为 false→不排除。
#[test]
fn missing_error_and_sidechain_default_to_not_excluded() {
    let v = json!({"type":"assistant","message":{"stop_reason":"end_turn"}});
    assert!(is_turn_end(&v), "缺 error/sidechain 字段 → 默认不排除");
    // 非 turn-end 记录 → uuid 不抽（即便有 uuid 字段）。
    let not = json!({"type":"assistant","uuid":"x","message":{"stop_reason":"tool_use"}});
    assert_eq!(turn_end_uuid(&not), None);
}
