use super::*;

fn parse(line: &str) -> JsonlRecord {
    serde_json::from_str(line).unwrap_or_else(|e| {
        panic!("parse failed for {line}: {e}");
    })
}

/// Batch10-F31 (issue #36)：queue-operation 解析（真实样本行）+ displayable。
#[test]
fn queue_operation_parses_and_is_displayable() {
    let r = parse(
        r#"{"type": "queue-operation", "operation": "enqueue", "timestamp": "2026-07-05T06:12:29.248Z", "sessionId": "0cbbdbae", "content": "这是登录门户"}"#,
    );
    match &r {
        JsonlRecord::QueueOperation {
            operation,
            content,
            timestamp,
        } => {
            assert_eq!(operation.as_deref(), Some("enqueue"));
            assert_eq!(content.as_deref(), Some("这是登录门户"));
            // P0c：时间戳原文里一直有，只是此前没收。`remove` 那一支要拿它建卡。
            assert_eq!(timestamp.as_deref(), Some("2026-07-05T06:12:29.248Z"));
        }
        other => panic!("expected QueueOperation, got {other:?}"),
    }
    assert!(r.is_displayable(), "要 emit 给前端做折叠豁免");
    // dequeue（无 content）也能安全解析
    let r = parse(
        r#"{"type": "queue-operation", "operation": "dequeue", "timestamp": "t", "sessionId": "s"}"#,
    );
    assert!(matches!(
        r,
        JsonlRecord::QueueOperation { content: None, .. }
    ));
}

/// Batch14-F42：assistant 终结记录的 stop_reason 解析 + 老记录无字段兼容。
#[test]
fn assistant_stop_reason_roundtrip() {
    let r = parse(
        r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-07-09T12:00:00.000Z",
                "message":{"role":"assistant","content":[],"stop_reason":"end_turn"}}"#,
    );
    match &r {
        JsonlRecord::Assistant { message, .. } => {
            assert_eq!(message.stop_reason.as_deref(), Some("end_turn"));
            // 序列化保留字段（前端判定依赖）
            let out = serde_json::to_string(&r).unwrap();
            assert!(out.contains(r#""stop_reason":"end_turn""#));
        }
        other => panic!("expected Assistant, got {other:?}"),
    }
    // 老记录 / 流式中间记录无 stop_reason → None 且不序列化
    let r = parse(
        r#"{"type":"assistant","uuid":"a-2","timestamp":"t",
                "message":{"role":"assistant","content":[]}}"#,
    );
    match &r {
        JsonlRecord::Assistant { message, .. } => {
            assert!(message.stop_reason.is_none());
            let out = serde_json::to_string(&r).unwrap();
            assert!(!out.contains("stop_reason"));
        }
        other => panic!("expected Assistant, got {other:?}"),
    }
}

#[test]
fn user_minimal_golden_sample_parses() {
    // 仅含必填字段（uuid / timestamp / message）的 user 行也应反序列化成功；
    // 其余字段（cwd / sessionId / isSidechain / parentUuid / forkedFrom）走 default
    let line = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"}
        }"#;
    let r = parse(line);
    assert!(r.is_displayable());
    match r {
        JsonlRecord::User {
            uuid,
            is_sidechain,
            is_meta,
            forked_from,
            ..
        } => {
            assert_eq!(uuid, "u-1");
            assert!(!is_sidechain, "isSidechain 缺省应 false");
            assert!(!is_meta, "isMeta 缺省应 false");
            assert!(forked_from.is_none(), "forkedFrom 缺省应 None");
        }
        other => panic!("expected User, got {other:?}"),
    }
}

#[test]
fn assistant_api_error_message_fields_parse() {
    // issue #21：API 最终失败的合成 assistant 消息。isApiErrorMessage 是前端
    // 渲染红色报错卡的判定主键，error/apiErrorStatus 是辅助展示字段。
    let line = r#"{
            "type":"assistant",
            "uuid":"a-err",
            "timestamp":"2026-06-12T01:00:00Z",
            "parentUuid":"prev",
            "isApiErrorMessage":true,
            "error":"authentication_failed",
            "apiErrorStatus":403,
            "message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"Please run /login · API Error: 403 Request not allowed"}]}
        }"#;
    let r = parse(line);
    assert!(r.is_displayable());
    match r {
        JsonlRecord::Assistant {
            is_api_error_message,
            error,
            api_error_status,
            ..
        } => {
            assert!(
                is_api_error_message,
                "isApiErrorMessage:true 必须解析为 true"
            );
            assert_eq!(error, Some(serde_json::json!("authentication_failed")));
            assert_eq!(api_error_status, Some(403));
        }
        other => panic!("expected Assistant, got {other:?}"),
    }
    // §18 类型漂移容忍：error 写成对象（同 system 侧 shape）整行仍须可解析——
    // 钉死 String 会让这条报错消息本身被 serde 吞掉。
    let drifted = parse(
        r#"{"type":"assistant","uuid":"a-2","timestamp":"2026-06-12T01:00:00Z","isApiErrorMessage":true,"error":{"status":500},"message":{"role":"assistant","content":[{"type":"text","text":"API Error: 500"}]}}"#,
    );
    match drifted {
        JsonlRecord::Assistant {
            is_api_error_message,
            error,
            ..
        } => {
            assert!(is_api_error_message);
            assert!(error.is_some(), "对象形态的 error 应透传不丢行");
        }
        other => panic!("expected Assistant, got {other:?}"),
    }
    // 普通 assistant 缺省应 false/None（不误判成报错卡）
    let normal = parse(
        r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-06-12T01:00:00Z","message":{"role":"assistant","content":"hi"}}"#,
    );
    match normal {
        JsonlRecord::Assistant {
            is_api_error_message,
            error,
            ..
        } => {
            assert!(!is_api_error_message);
            assert!(error.is_none());
        }
        other => panic!("expected Assistant, got {other:?}"),
    }
}

#[test]
fn system_api_error_retry_fields_parse() {
    // issue #21：API 调用失败将重试的中间态。retryAttempt/maxRetries 给前端
    // 渲染「重试 N/M」，error 对象 shape 随 CLI 版本变化 → Value 透传。
    let line = r#"{
            "type":"system",
            "subtype":"api_error",
            "level":"error",
            "retryAttempt":2,
            "maxRetries":10,
            "retryInMs":521.4,
            "timestamp":"2026-06-12T01:00:00Z",
            "uuid":"sys-err",
            "parentUuid":"prev",
            "error":{"formatted":"529 Overloaded","status":529}
        }"#;
    let r = parse(line);
    assert!(r.is_displayable());
    match r {
        JsonlRecord::System {
            subtype,
            level,
            retry_attempt,
            max_retries,
            error,
            ..
        } => {
            assert_eq!(subtype.as_deref(), Some("api_error"));
            assert_eq!(level.as_deref(), Some("error"));
            assert_eq!(retry_attempt, Some(2));
            assert_eq!(max_retries, Some(10));
            let e = error.expect("error 对象应透传");
            assert_eq!(e["formatted"], "529 Overloaded");
        }
        other => panic!("expected System, got {other:?}"),
    }
}

#[test]
fn user_is_meta_flag_parses_and_stays_displayable() {
    // Claude Code 注入的 meta user 消息（skill/command 展开 prompt、system-reminder、
    // caveat 等）带 isMeta:true —— 不是用户真输入。必须解析出 is_meta，前端据此跳过
    // 建卡（否则 /code-review 等 skill 的整段 prompt 会当用户气泡渲染）。仍 displayable：
    // 它含 uuid+parentUuid 是 parent 链一环，漏 emit 会断链（同 attachment #8）。
    let line = r#"{
            "type":"user",
            "uuid":"u-meta",
            "timestamp":"2026-06-08T01:00:00Z",
            "isMeta":true,
            "parentUuid":"prev-uuid",
            "message":{"role":"user","content":[{"type":"text","text":"You are reviewing for recall..."}]}
        }"#;
    let r = parse(line);
    assert!(r.is_displayable(), "meta user 仍须 emit 保 parent 链完整");
    match r {
        JsonlRecord::User {
            is_meta,
            uuid,
            parent_uuid,
            ..
        } => {
            assert!(is_meta, "isMeta:true 必须解析为 true");
            assert_eq!(uuid, "u-meta");
            assert_eq!(parent_uuid.as_deref(), Some("prev-uuid"));
        }
        other => panic!("expected User, got {other:?}"),
    }
}

#[test]
fn custom_title_v21_schema_hits_custom_title_variant() {
    // v2.4.3 真实事故回归测试：Claude Code v2.1.x 把 ai-title → custom-title /
    // customTitle。messages.rs 若漏 CustomTitle 变体，整个 type 走 Unknown →
    // 不 emit → 前端拿不到标题（Tab 永远只显示项目名）。
    let line = r#"{"type":"custom-title","customTitle":"我的会话","sessionId":"s-42"}"#;
    let r = parse(line);
    assert!(r.is_displayable());
    match r {
        JsonlRecord::CustomTitle {
            custom_title,
            session_id,
        } => {
            assert_eq!(custom_title, "我的会话");
            assert_eq!(session_id, "s-42");
        }
        other => panic!("v2.1.x custom-title 未命中 CustomTitle 变体，got {other:?}"),
    }
}

#[test]
fn ai_title_legacy_schema_still_works() {
    // 历史 jsonl 仍可能有 ai-title（v2.0 及之前）。两个 schema 必须共存兼容。
    let line = r#"{"type":"ai-title","aiTitle":"old","sessionId":"s-1"}"#;
    let r = parse(line);
    assert!(matches!(r, JsonlRecord::AiTitle { .. }));
    assert!(r.is_displayable());
}

#[test]
fn attachment_preserves_uuid_chain_and_is_displayable() {
    // issue #8: attachment 不渲染但**必须 emit**——前端 BranchFolder 需要
    // attachment 的 uuid+parentUuid 才能完整跟 parent 链。漏 emit → ESC 回退
    // 误判 → 整段消息被错误折叠到"已被回退"。
    let line = r#"{
            "type":"attachment",
            "uuid":"att-1",
            "timestamp":"2026-05-20T01:00:00Z",
            "parentUuid":"prev-msg-uuid"
        }"#;
    let r = parse(line);
    // 先校验 displayable（不 move r），再 destructure 取字段
    assert!(r.is_displayable(), "attachment 必须 emit 保 parent 链完整");
    match r {
        JsonlRecord::Attachment {
            uuid, parent_uuid, ..
        } => {
            assert_eq!(uuid, "att-1");
            assert_eq!(parent_uuid.as_deref(), Some("prev-msg-uuid"));
        }
        other => panic!("expected Attachment, got {other:?}"),
    }
}

#[test]
fn unknown_type_does_not_panic_and_not_displayable() {
    // 注意：`parse` = 裸 `serde_json::from_str`，测的是 **serde 层**（Unknown 是
    // serde 落点，此层它确实非 displayable）。**生产走 `parser::parse_line`**，
    // 那里 Unknown 会被抢救成 `Unrecognized`（F63）—— 见 parser.rs 的护栏测试。
    let r = parse(r#"{"type":"future-unknown-type","x":1}"#);
    assert!(matches!(r, JsonlRecord::Unknown));
    assert!(!r.is_displayable());
}

/// F63 wire 契约：`Unrecognized` 序列化出的 `type` 必须是 `"cc-monitor-unrecognized"`，
/// 且身份字段用 camelCase。前端 `branching.ts` 白名单 + `cards/index.ts` 镜像
/// **硬编码同一字符串**；改这边的 rename 就得同步改前端，否则 unrecognized
/// 全部认不出 → 又开始丢链，且现有测试全绿（漂移无声）。此测试守 Rust 侧，
/// 前端侧由 branching.test.ts 的白名单用例守（改前端白名单则那边挂）。
#[test]
fn unrecognized_wire_type_and_camel_case_contract() {
    let rec = JsonlRecord::Unrecognized {
        uuid: Some("u1".into()),
        parent_uuid: Some("u0".into()),
        timestamp: Some("t1".into()),
        original_type: Some("mode".into()),
        raw: "{\"type\":\"mode\"}".into(),
        reason: "unknown-type".into(),
    };
    let v = serde_json::to_value(&rec).unwrap();
    assert_eq!(
        v.get("type").and_then(|t| t.as_str()),
        Some("cc-monitor-unrecognized"),
        "改 rename 必须同步前端 branching.ts / cards/index.ts 白名单"
    );
    // camelCase：前端镜像按此取（parentUuid / originalType）
    assert!(v.get("parentUuid").is_some(), "parentUuid 必须 camelCase");
    assert!(
        v.get("originalType").is_some(),
        "originalType 必须 camelCase"
    );
    // 回环：抢救出的记录序列化后能被 serde 读回同一变体（不因 rename 撞其他分支）
    let back: JsonlRecord = serde_json::from_value(v).unwrap();
    assert!(matches!(back, JsonlRecord::Unrecognized { .. }));
}

#[test]
fn system_record_keeps_uuid_for_branch_detection() {
    // issue #8 配套：system 大多有 uuid+parentUuid 参与 parent 链。
    let line = r#"{
            "type":"system",
            "subtype":"turn_duration",
            "durationMs":1234,
            "timestamp":"2026-05-20T01:00:00Z",
            "uuid":"sys-1",
            "parentUuid":"prev"
        }"#;
    let r = parse(line);
    match r {
        JsonlRecord::System {
            uuid,
            parent_uuid,
            duration_ms,
            ..
        } => {
            assert_eq!(uuid.as_deref(), Some("sys-1"));
            assert_eq!(parent_uuid.as_deref(), Some("prev"));
            assert_eq!(duration_ms, Some(1234));
        }
        _ => panic!("expected System, got {r:?}"),
    }
}
