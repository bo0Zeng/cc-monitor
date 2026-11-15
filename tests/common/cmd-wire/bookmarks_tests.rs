//! [`super`] 的判据：两条命令的线上形（目录归一的判据在后端 `tests/backend/control/bookmarks_tests.rs`）。

use super::*;

#[test]
fn the_two_commands_have_the_wire_shape_both_ends_share() {
    use crate::Call;
    assert_eq!(BookmarksReadReq::NAME, "bookmarks-read");
    assert_eq!(BookmarksEditReq::NAME, "bookmarks-edit");
    let edit = BookmarksEditReq {
        op: BookmarkOp::Toggle,
        dirs: vec!["/srv".into()],
        at: Vec::new(),
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        serde_json::json!({ "op": "toggle", "dirs": ["/srv"] }),
        "没带此刻的目录 ⇒ 线上不出那一格"
    );
    assert!(
        serde_json::from_value::<BookmarksReadReq>(serde_json::json!({ "x": 1 })).is_err(),
        "多一个键 ⇒ 拒"
    );
    let r: Bookmarks = serde_json::from_value(serde_json::json!({ "dirs": ["/a"] })).unwrap();
    assert_eq!(r.dirs, vec!["/a".to_string()]);
    assert!(r.here.is_empty());
}

/// 各栏此刻的目录（`at`）两条都可缺；回包的 `here` 可缺、有就是「问的那一串 ⇒ 下标」。
#[test]
fn the_current_dirs_and_their_answers_are_optional_cells() {
    let read: BookmarksReadReq = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(read.at.is_empty());
    let read: BookmarksReadReq =
        serde_json::from_value(serde_json::json!({ "at": ["/a//b/", "/c"] })).unwrap();
    assert_eq!(read.at, ["/a//b/", "/c"]);
    let edit = BookmarksEditReq {
        op: BookmarkOp::Remove,
        dirs: vec!["/x".into()],
        at: vec!["/y".into()],
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        serde_json::json!({ "op": "remove", "dirs": ["/x"], "at": ["/y"] })
    );
    let here = Bookmarks {
        dirs: vec!["/a".into()],
        here: [("/a/".to_string(), 0)].into_iter().collect(),
    };
    assert_eq!(
        serde_json::to_value(&here).unwrap(),
        serde_json::json!({ "dirs": ["/a"], "here": { "/a/": 0 } })
    );
    assert_eq!(
        serde_json::to_value(Bookmarks::default()).unwrap(),
        serde_json::json!({ "dirs": [] }),
        "没答 ⇒ 线上不出那一格（存盘那一份也不带它）"
    );
}
