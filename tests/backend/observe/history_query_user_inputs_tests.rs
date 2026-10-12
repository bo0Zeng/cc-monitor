//! 大纲清单（帧命令 `history-user-inputs`）的打开口那一层（纯核的判据在 `user_inputs_tests.rs`）。

use super::*;

/// `from` 超过文件长度 ⇒ 报错（文件被截断/重写），不回一份空清单假装「没有新的」。
/// 对照：恰好等于文件长度（「读到头了、没有新的」）是合法的。路径守卫照 `history-read`。
#[test]
fn from_past_eof_is_an_error_not_an_empty_list() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-ui-{}", std::process::id()));
    let dir = tmp.join("projects").join("proj-ui");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(&p, "{\"type\":\"system\"}\n").unwrap();
    let len = std::fs::metadata(&p).unwrap().len();
    let ps = p.to_string_lossy().into_owned();
    let err = open_user_inputs_at(&tmp, &ps, len + 1).expect_err("越过 EOF 必须报错");
    assert!(err.contains("past EOF"), "{err}");
    assert!(open_user_inputs_at(&tmp, &ps, len).is_ok());
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "x\n").unwrap();
    assert!(open_user_inputs_at(&tmp, &outside.to_string_lossy(), 0).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}
