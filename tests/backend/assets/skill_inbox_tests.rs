//! 设计/99 §2.1 ⑬ · F03b：收件箱编辑读写经那台后端自己的文件管理面，写带 CAS（打开之后被改过 ⇒ `stale`、一个字节不写）。
use super::*;
use crate::stream::inbound::LocalFiles;

/// 读回打开时那一份；照那一份写 ⇒ 成；盘上已被改过 ⇒ `stale`、盘上一个字节不动。
#[test]
fn the_inbox_is_read_and_written_with_cas_through_the_file_face() {
    let d = std::env::temp_dir().join(format!("mig3a-inbox-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".claude/planned-build")).unwrap();
    let inbox = d.join(".claude/planned-build/INBOX.txt");
    std::fs::write(&inbox, "one\n").unwrap();
    let args = |extra: Value| {
        let mut a = json!({ "cwd": d.to_string_lossy(), "skillId": "planned-build", "path": inbox.to_string_lossy() });
        a.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        a
    };
    let got = answer_read(&LocalFiles, &args(json!({}))).expect("读不出来");
    assert_eq!(got["text"], "one\n");
    answer_write(
        &LocalFiles,
        &args(json!({ "content": "two\n", "expected": "one\n" })),
    )
    .expect("照读到的那一份写被拒了");
    assert_eq!(std::fs::read_to_string(&inbox).unwrap(), "two\n");
    let (code, _) = answer_write(
        &LocalFiles,
        &args(json!({ "content": "three\n", "expected": "one\n" })),
    )
    .expect_err("过期的那一份也写了");
    assert_eq!(code, "stale");
    assert_eq!(std::fs::read_to_string(&inbox).unwrap(), "two\n");
    let _ = std::fs::remove_dir_all(&d);
}
