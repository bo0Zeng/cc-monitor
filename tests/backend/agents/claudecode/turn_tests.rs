use super::*;
use serde_json::{json, Value};

/// 判词今天吃 [`Probe`]；下面各条仍按一条 JSON 记录写，经原文再读进探针（顺带走子串闸那一步不到的整条路）。
fn probe(v: &Value) -> Probe {
    serde_json::from_str(&v.to_string()).expect("探针读不进一条对象记录")
}
fn is_turn_end(v: &Value) -> bool {
    super::is_turn_end(&probe(v))
}
fn turn_end_uuid(v: &Value) -> Option<String> {
    super::turn_end_uuid(&probe(v)).map(str::to_string)
}

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
    assert_eq!(turn_end_uuid(&v).as_deref(), Some("u-1"));
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
    // 子运行那一轮的收尾不在本判词里排除：归属由 `runs::run_of` 答、通用 watcher 发 `TurnEnd` 之前排除
    //（判据住 `tests/backend/observe/watcher_tests.rs::a_sub_runs_turn_end_is_not_the_main_runs`）。
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

/// 子串闸只挡「不可能是 turn-end」的行，探针与原先整份 `Value` 同一判：
/// 闸放行的写法（冒号两侧有空白）照样判出；形状不对的旁格当缺、不把整行判畸形；原文里没有那个字面量 ⇒ 不解析、`None`。
#[test]
fn the_substring_gate_hides_no_turn_end_and_odd_shapes_count_as_missing() {
    let spaced =
        r#"{ "type" : "assistant", "uuid" : "u-2", "message" : { "stop_reason" : "end_turn" } }"#;
    assert_eq!(turn_end_uuid_of(spaced).as_deref(), Some("u-2"));
    let odd = r#"{"type":"assistant","uuid":"u-3","isSidechain":"no","isApiErrorMessage":1,"message":{"stop_reason":"end_turn","content":[{"type":"text","text":"x"}]}}"#;
    assert_eq!(
        turn_end_uuid_of(odd).as_deref(),
        Some("u-3"),
        "非 bool 的 error/sidechain 当缺（不排除），与 `as_bool().unwrap_or(false)` 同口径"
    );
    let not_str =
        r#"{"type":"assistant","uuid":"u-4","message":{"stop_reason":7},"note":"end_turn"}"#;
    assert_eq!(
        turn_end_uuid_of(not_str),
        None,
        "stop_reason 非字符串 ⇒ 非 turn-end"
    );
    assert_eq!(
        turn_end_uuid_of(
            r#"{"type":"assistant","uuid":"u-5","message":{"stop_reason":"tool_use"}}"#
        ),
        None
    );
    assert_eq!(turn_end_uuid_of("{torn \"end_turn\""), None, "畸形 ⇒ None");
}
