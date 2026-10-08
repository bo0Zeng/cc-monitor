//! 「要你动手」记下的选择：不用了 / 还是要做 · 我自己贴 / 改回让 cc-monitor 接上；读不懂的那份不覆盖。

use super::*;
use serde_json::json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "st8-marks-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn 记下_撤回_读回() {
    let d = tmp("rw");
    let f = d.join("chores.json");
    assert_eq!(current(Some(&f)), Marks::default());
    mark_at(&f, &json!({"op": "decline", "id": "relay"})).unwrap();
    mark_at(&f, &json!({"op": "decline", "id": "relay"})).unwrap();
    mark_at(&f, &json!({"op": "selfPaste", "rc": "/h/.bashrc"})).unwrap();
    assert_eq!(
        current(Some(&f)),
        Marks {
            declined: vec!["relay".into()],
            self_paste: Some("/h/.bashrc".into()),
            start_skipped: false,
        }
    );
    mark_at(&f, &json!({"op": "undecline", "id": "relay"})).unwrap();
    mark_at(&f, &json!({"op": "unselfPaste"})).unwrap();
    assert_eq!(current(Some(&f)), Marks::default());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 读不懂的那份_不覆盖_回错() {
    let d = tmp("bad");
    let f = d.join("chores.json");
    std::fs::write(&f, "{oops").unwrap();
    assert_eq!(
        mark_at(&f, &json!({"op": "decline", "id": "x"}))
            .unwrap_err()
            .0,
        "marks_unreadable"
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{oops");
    assert_eq!(current(Some(&f)), Marks::default(), "判的时候照没记算");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 不认的_op_或缺参数_拒() {
    let d = tmp("args");
    let f = d.join("chores.json");
    for a in [
        json!({}),
        json!({"op": "nope"}),
        json!({"op": "decline"}),
        json!({"op": "selfPaste"}),
    ] {
        assert_eq!(mark_at(&f, &a).unwrap_err().0, "bad_args", "{a}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 「开始用」那一块点过「跳过」：记在同一份文件里，读回来带着；撤回就没了。
#[test]
fn skipping_the_start_block_is_remembered_and_can_be_undone() {
    let d = std::env::temp_dir().join(format!("ccm-marks-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let path = d.join(".cc-monitor").join("chores.json");
    let got = mark_at(&path, &serde_json::json!({"op": "skipStart"})).unwrap();
    assert_eq!(got["startSkipped"], serde_json::json!(true));
    assert!(current(Some(&path)).start_skipped);
    mark_at(&path, &serde_json::json!({"op": "unskipStart"})).unwrap();
    assert!(!current(Some(&path)).start_skipped);
    let _ = std::fs::remove_dir_all(&d);
}
