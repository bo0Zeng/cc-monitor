//! 文件窗口反查的回包 → 按名字查的表（期望手写，回包是合成的中性例子）。

use super::*;
use serde_json::json;

#[test]
fn a_dir_outside_every_slice_adds_nothing() {
    assert_eq!(
        from_reply(
            &json!({"workspace": null, "slice": null, "unreadable": null, "entries": [], "unowned": null})
        ),
        None
    );
}

#[test]
fn entries_become_owners_by_name_with_status_block_sign_and_double_claims() {
    let v = json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": null, "entries": [
        {"name": "read.txt", "id": "A1-1", "title": "甲的读入", "statusCode": "done", "block": "甲功能",
         "signAtText": "02:08", "fileState": "ok", "fileNote": "12 字", "dup": [{"id": "A1-2", "title": "甲的写出"}]},
        {"name": "write.txt", "id": "A1-2", "title": null, "statusCode": "open", "block": null,
         "signAtText": null, "fileState": "broken", "fileNote": "不是 UTF-8", "dup": []},
        {"name": "gone.txt", "id": "A2", "title": "乙", "statusCode": "dropped", "block": null,
         "signAtText": null, "fileState": "missing", "fileNote": null, "dup": []}
    ]});
    let d = from_reply(&v).unwrap();
    assert_eq!((d.workspace.as_str(), d.slice.as_str()), ("/w", "alpha"));
    assert_eq!(d.unreadable, None);
    let r = d.owner("read.txt").unwrap();
    assert_eq!(
        (r.id.as_str(), r.title.as_str(), r.status),
        ("A1-1", "甲的读入", Status::Done)
    );
    assert_eq!(r.block.as_deref(), Some("甲功能"));
    assert_eq!(r.signed.as_deref(), Some("02:08"));
    assert_eq!(r.broken, None);
    assert_eq!(r.dup, vec!["甲的写出".to_string()]);
    // 标题没给 ⇒ 写编号；对账「坏」⇒ 带原因。
    let w = d.owner("write.txt").unwrap();
    assert_eq!((w.title.as_str(), w.status), ("A1-2", Status::Open));
    assert_eq!(w.broken.as_deref(), Some("不是 UTF-8"));
    assert_eq!(d.owner("gone.txt").unwrap().status, Status::Dropped);
    assert_eq!(d.owner("nobody.txt"), None);
}

#[test]
fn an_unreadable_slice_keeps_its_said_and_no_owners() {
    let d = from_reply(&json!({"workspace": "/w", "slice": "alpha", "unreadable": "图.md 第 3 行：元行缺 id", "entries": [], "unowned": null})).unwrap();
    assert_eq!(d.unreadable.as_deref(), Some("图.md 第 3 行：元行缺 id"));
    assert!(d.owners.is_empty());
}
