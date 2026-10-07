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
            self_paste: Some("/h/.bashrc".into())
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
