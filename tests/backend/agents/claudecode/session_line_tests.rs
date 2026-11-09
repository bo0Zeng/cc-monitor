use super::*;
use crate::agents::record::TitleBy;
use serde_json::json;

/// ★ 一行记录 ⇒ 会话级那几格：人说的 / 回话才给时刻与分叉来源；两种标题记录都认；后台标记 · cwd 任一行都给（造出来的最小几行）。
#[test]
fn one_line_becomes_the_session_cells() {
    let said = line_of(
        r#"{"type":"user","timestamp":"2026-10-01T08:00:00Z","cwd":"/w/p","sessionKind":"bg","forkedFrom":{"sessionId":"src","messageUuid":"m1"},"message":{"role":"user","content":"q"}}"#,
    )
    .expect("解得开");
    assert_eq!(
        said,
        SessionLine {
            role: Some(LineRole::Said),
            title: None,
            bg: true,
            at_ms: crate::common::time::parse_iso8601_ms("2026-10-01T08:00:00Z"),
            id: None,
            cwd: Some("/w/p".into()),
            forked: Some(("src".into(), "m1".into())),
        }
    );
    assert_eq!(
        line_of(r#"{"type":"assistant"}"#).unwrap().role,
        Some(LineRole::Replied)
    );
    // 两种标题记录都照实报，带上是谁起的（收不收是通用层 `search_rules::names_session` 一处判）。
    assert_eq!(
        line_of(r#"{"type":"ai-title","aiTitle":"甲"}"#)
            .unwrap()
            .title,
        Some(("甲".to_string(), TitleBy::Agent))
    );
    assert_eq!(
        line_of(r#"{"type":"custom-title","customTitle":"乙"}"#)
            .unwrap()
            .title,
        Some(("乙".to_string(), TitleBy::User))
    );
    // 不是人说的 / 回话那两类：时刻与分叉来源不给。
    let meta = line_of(
        r#"{"type":"system","timestamp":"2026-10-01T08:00:00Z","forkedFrom":{"sessionId":"s","messageUuid":"m"},"cwd":""}"#,
    )
    .unwrap();
    assert_eq!(
        (meta.role, meta.at_ms, meta.forked, meta.cwd),
        (None, None, None, None)
    );
    assert_eq!(line_of("{not json"), None);
}

/// 怪形状的口径与整条解成 `Value` 那一条相同：值不是串当没有 · 同名键取最后一个（快路解不出、退回那一条）。
#[test]
fn odd_shapes_read_the_same_as_the_whole_value() {
    let odd = [
        r#"{"type":7,"aiTitle":"类型不是串"}"#,
        r#"{"type":"ai-title","aiTitle":{"x":1}}"#,
        r#"{"type":"user","type":"ai-title","aiTitle":"同名键取最后一个"}"#,
        r#"{"type":"assistant","forkedFrom":"bad","sessionKind":["bg"]}"#,
        r#"[1,2,3]"#,
    ];
    for l in odd {
        let whole = serde_json::from_str::<Value>(l)
            .ok()
            .map(|v| line_of_value(&v));
        assert_eq!(line_of(l), whole, "{l}");
    }
    assert_eq!(
        line_of(odd[2]).unwrap().title,
        Some(("同名键取最后一个".to_string(), TitleBy::Agent))
    );
}

/// 正文的形状：串 · 块数组（正文块逐段 · 工具 / 思考 / 图片各算一个折叠单元）· 顶层 `content`；没有正文那一格 ⇒ 没有。
#[test]
fn body_shape_counts_prose_and_folded_units() {
    let v = json!({"type":"assistant","message":{"content":[
        {"type":"text","text":"一"},{"type":"tool_use","id":"t","name":"n","input":{}},
        {"type":"thinking","thinking":"x"},{"type":"text","text":"二"},{"type":"other"}
    ]}});
    assert_eq!(
        shape_of(&v),
        Some(BodyShape {
            prose: vec!["一", "二"],
            folded: 2
        })
    );
    assert_eq!(
        shape_of(&json!({"type":"system","content":"三"})),
        Some(BodyShape {
            prose: vec!["三"],
            folded: 0
        })
    );
    assert_eq!(shape_of(&json!({"type":"mode"})), None);
}
