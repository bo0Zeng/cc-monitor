//! `rotation.json` 第三版的存取：读不懂（连比这一版新的）不覆盖、写一律拒 · 第一版 · 第二版当没有 · 新会话 · 清旧会话。临时家目录，不碰真文件。

use super::*;
use serde_json::{json, Value};
use crate::accounts::quota::rules::book;

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotstore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        Self(root)
    }
    fn own(&self) -> Own {
        Own::issue_data_at(&self.0)
    }
    fn file(&self) -> PathBuf {
        self.0.join(FILE_NAME)
    }
    fn store(&self) -> RotationStore {
        RotationStore::at(Some(self.file()))
    }
}

/// ★ 读不懂的那一份：写不覆盖；读的一侧照上一份读懂的（没有过 ⇒ 新的那一份）接着用、说得出为什么。
#[test]
fn an_unreadable_book_is_never_overwritten() {
    let home = Home::new("unread");
    std::fs::write(home.file(), "{not json").expect("w");
    let store = home.store();
    assert!(face_change(&store, &home.own(), |b| b.saw(
        "s",
        "claude-code",
        "a",
        1,
        || None
    ))
    .is_err());
    assert_eq!(
        std::fs::read_to_string(home.file()).expect("r"),
        "{not json"
    );
    assert!(store.now().sessions.is_empty());
    assert!(store.unreadable().is_some());
    let mut v = json!({"version": 3, "sessions": {}, "extra": 1});
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(
        matches!(read_at(&home.file()), Disk::Unreadable(_)),
        "多一格该读不懂"
    );
    v.as_object_mut().expect("obj").remove("extra");
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(matches!(read_at(&home.file()), Disk::Now(_)));
}

/// ★ 比这一版新的：读不懂、写一律拒、盘上字节不变。
#[test]
fn a_newer_book_is_unreadable_and_left_as_is() {
    let home = Home::new("newer");
    let v = json!({"version": 4, "sessions": {}});
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(matches!(read_at(&home.file()), Disk::Unreadable(_)));
    let store = home.store();
    assert!(store.unreadable().is_some());
    assert!(face_change(&store, &home.own(), |b| b.saw("s", "claude-code", "a", 1, || None)).is_err());
    assert_eq!(std::fs::read_to_string(home.file()).expect("r"), v.to_string());
}

/// ★ 第一版（没有 `version`）· 第二版不升级：当没有；写一回 ⇒ 盖成这一版，旧原文照字节留成 `rotation.json.v<N>`。
#[test]
fn an_old_book_is_absent_and_kept_beside_on_write() {
    for (v, n) in [
        (json!({"rules": {}, "sessions": {}}), 1),
        (json!({"version": 2, "configs": {}, "sessions": {}}), 2),
    ] {
        let home = Home::new(&format!("old{n}"));
        std::fs::write(home.file(), v.to_string()).expect("w");
        assert!(matches!(read_at(&home.file()), Disk::Absent));
        let store = home.store();
        assert!(store.now().sessions.is_empty() && store.unreadable().is_none());
        face_change(&store, &home.own(), |b| b.saw("s", "claude-code", "a", 1, || None))
            .expect("盖成这一版");
        let kept = home.0.join(format!("{FILE_NAME}.v{n}"));
        assert_eq!(std::fs::read_to_string(kept).expect("留了旧原文"), v.to_string());
        let now: Value = serde_json::from_str(&std::fs::read_to_string(home.file()).expect("r"))
            .expect("json");
        assert_eq!(now["version"], 3);
    }
}

/// ★ 新会话用哪份（`forNew`）住在 `rotation.json` 里：读写往返；缺 ⇒ 空（不轮换）。
#[test]
fn for_new_lives_in_the_book_and_round_trips() {
    let home = Home::new("fornew");
    std::fs::write(home.file(), json!({"version": 3, "sessions": {}}).to_string()).expect("w");
    let store = home.store();
    assert_eq!(store.now().for_new, "");
    face_change(&store, &home.own(), |b| b.for_new = "mine".into()).expect("写");
    let raw: Value =
        serde_json::from_str(&std::fs::read_to_string(home.file()).expect("r")).expect("json");
    assert_eq!((raw["version"].clone(), raw["forNew"].clone()), (json!(3), json!("mine")));
    let Disk::Now(b) = read_at(&home.file()) else {
        panic!("读不回")
    };
    assert_eq!(b.for_new, "mine");
}

/// 换了开它的号 ⇒ 在用的号回到它、拖出来的顺序 · memo 清掉；没拖 · 没换过号、七天没见的清掉。
#[test]
fn new_sessions_are_kept_and_stale_ones_are_dropped() {
    let mut b = Book::fresh();
    assert!(b.saw("s1", "claude-code", "a", 100, || None));
    b.sessions.get_mut("s1").expect("s1").temp = vec!["b".into()];
    b.sessions.get_mut("s1").expect("s1").current = "b".into();
    assert!(b.saw("s1", "claude-code", "c", 200, || None));
    let s = &b.sessions["s1"];
    assert_eq!((s.current.as_str(), s.temp.len()), ("c", 0));
    assert!(
        !b.saw("s1", "claude-code", "c", 300, || None),
        "同一天再看见不该改"
    );
    b.saw("s2", "claude-code", "a", 300, || None);
    let later = 300 + book::DROP_AFTER + 1;
    b.saw("s3", "claude-code", "a", later, || None);
    assert!(!b.sessions.contains_key("s1") && !b.sessions.contains_key("s2"));
    assert!(b.sessions.contains_key("s3"));
}
