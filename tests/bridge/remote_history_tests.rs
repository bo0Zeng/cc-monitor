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

// 〔C4d · 第四波 4B〕「首行是 hello ⇒ 老后端」那条识别（`is_old_backend_hello`〔散文墓碑〕）随逐次拨号那条路删了：
// 帧面的老后端由能力协商说「还不认」（`frame_query::call` 发之前先问 `accepts`）。

#[test]
fn shell_quote_via_ssh_source() {
    assert_eq!(
        crate::ssh_source::shell_quote("/a/b c.jsonl"),
        "'/a/b c.jsonl'"
    );
    assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
}
