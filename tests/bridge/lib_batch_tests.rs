use super::*;
use std::path::PathBuf;

fn jline(session_id: &str, seq: u64, raw: &str) -> watcher::JsonlLine {
    watcher::JsonlLine {
        session_id: session_id.to_string(),
        path: PathBuf::from("/tmp/projects/proj/s-abc.jsonl"),
        seq,
        raw: raw.to_string(),
    }
}

/// batch_to_payloads 必须：只保留 displayable 行、透传 seq/session_id、
/// 在遇到 malformed 行时 warn-then-continue 不 panic，并对非 displayable 行静默丢弃。
#[test]
fn filters_non_displayable_and_survives_malformed() {
    // (a) 真实 displayable user 行（copy 自 messages.rs golden sample），seq 5
    let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/me/proj"
        }"#;
    // (b) 非 displayable 记录：permission-mode 的 is_displayable() 返回 false
    let non_displayable = r#"{"type":"permission-mode"}"#;
    // (c) 无法解析的行 → parse_line 返回 Err，必须被 warn 后跳过、不 panic
    let malformed = "not json at all";

    let lines = vec![
        jline("s-abc", 5, displayable_user),
        jline("s-abc", 6, non_displayable),
        jline("s-abc", 7, malformed),
    ];

    let payloads = batch_to_payloads(lines, None);

    // 只有 displayable user 行进 payload
    assert_eq!(payloads.len(), 1, "只应保留 1 条 displayable 记录");
    assert_eq!(payloads[0].seq, 5, "seq 必须原样透传");
    assert_eq!(payloads[0].session_id, "s-abc", "session_id 必须原样透传");
    assert_eq!(
        payloads[0].cwd.as_deref(),
        Some("/home/me/proj"),
        "extract_cwd 应取出 user.cwd"
    );
    // 本地（origin=None）行不带 origin。
    assert_eq!(payloads[0].origin, None, "本地行 origin 应为 None");
}

/// origin=Some(host) 时每条 payload 都带上该标签（远端 Tab 标题前缀用）。
#[test]
fn origin_is_propagated_to_payloads() {
    let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/pi/proj"
        }"#;
    let lines = vec![jline("s-remote", 0, displayable_user)];
    let payloads = batch_to_payloads(lines, Some("pi".to_string()));
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        payloads[0].origin.as_deref(),
        Some("pi"),
        "远端行 origin 必须透传 host 标签"
    );
}
