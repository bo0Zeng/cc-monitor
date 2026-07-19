//! turn-end 判词（daemon 侧 · phase② TurnEnd 帧的检测核）。
//!
//! **契约 = aterm `TurnDetector.kt:29` 逐字对拍**（golden-parity，同 `usage_query` 套路）：
//! turn-end ⟺ `type=="assistant" && message.stop_reason=="end_turn" && !isApiError && !isSidechain`。
//! 字段坑（master plan §0）：`isApiErrorMessage`→isApiError、`stop_reason` 嵌在 **message** 下、
//! `isSidechain` 是 **top-level**。在 `serde_json::Value` 上抽取（daemon 无 `parse_line`/typed model）。
//!
//! ★ **本轮仅落纯判词 + golden 测**；**帧发射 + dedup 视界**（per-session 末结算 uuid + 客户端重启
//! 首轮吞历史不通知）待 aterm 加占位 `JsonlFrame.TurnEnd` 变体、两端对齐边沿/去重语义后再接线
//! （见 cc-bus #daemon）。故 `#[allow(dead_code)]`——判词就绪、等接线那个 commit 摘掉。
//!
//! §2.1 不变量并存：daemon 仍逐行 raw 转发**每一条** Line（不因分类丢行）；turn-end 是在 raw 之外
//! **额外**从解析内容算的边沿信号，不替代、不过滤 Line。

#![allow(dead_code)] // 判词已就绪；帧发射接线在后续 commit（待 aterm TurnEnd 变体 + dedup 对齐）

use serde_json::Value;

/// 一条已解析的 jsonl 记录是否为 turn-end 边沿。**逐字对拍 aterm `TurnDetector`**：
/// `assistant` && `message.stop_reason=="end_turn"` && !`isApiErrorMessage` && !`isSidechain`。
/// 缺字段一律安全默认（stop_reason 缺→非 end_turn→false；error/sidechain 缺→false→不排除）。
pub fn is_turn_end(v: &Value) -> bool {
    is_assistant(v) && stop_reason(v) == Some("end_turn") && !is_api_error(v) && !is_sidechain(v)
}

/// turn-end 边沿的 uuid（= 完成 assistant 记录 uuid），供 TurnEnd 帧 + 客户端幂等去重。
/// 非 turn-end / 无 uuid → None。
pub fn turn_end_uuid(v: &Value) -> Option<&str> {
    if is_turn_end(v) {
        v.get("uuid").and_then(Value::as_str)
    } else {
        None
    }
}

fn is_assistant(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("assistant")
}

/// `message.stop_reason`（嵌在 message 下——字段坑）。
fn stop_reason(v: &Value) -> Option<&str> {
    v.get("message")
        .and_then(|m| m.get("stop_reason"))
        .and_then(Value::as_str)
}

/// `isApiErrorMessage`（→ isApiError；缺/非 bool → false）。
fn is_api_error(v: &Value) -> bool {
    v.get("isApiErrorMessage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// top-level `isSidechain`（缺/非 bool → false）。
fn is_sidechain(v: &Value) -> bool {
    v.get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
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
}
