use super::*;

#[test]
fn jsonl_stem_basics() {
    assert_eq!(
        jsonl_stem("/home/pi/.claude/projects/p/abc-123.jsonl").as_deref(),
        Some("abc-123")
    );
    assert_eq!(jsonl_stem("abc.jsonl").as_deref(), Some("abc"));
    assert_eq!(jsonl_stem("/x/y/note.txt"), None);
    assert_eq!(jsonl_stem(""), None);
}

#[test]
fn old_daemon_hello_detected() {
    assert!(is_old_daemon_hello(
        r#"{"kind":"hello","v":1,"build_id":"phase0-proto","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#
    ));
    // 查询模式的正常输出不含 kind
    assert!(!is_old_daemon_hello(
        r#"{"dirName":"-home-pi-proj","projectPath":"/home/pi/proj","sessionCount":3,"lastActivityMs":1}"#
    ));
    // jsonl 正文里聊到 hello 不该误判（必须是 kind 字段形态）
    assert!(!is_old_daemon_hello(
        r#"{"type":"user","message":{"content":"say hello"}}"#
    ));
}

#[test]
fn shell_quote_via_ssh_source() {
    assert_eq!(
        crate::ssh_source::shell_quote("/a/b c.jsonl"),
        "'/a/b c.jsonl'"
    );
    assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
}
