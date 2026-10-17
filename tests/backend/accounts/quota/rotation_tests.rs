//! `rotation.json` 第三版的存取：旧版没升级前当没有、写一律拒 · 读不懂不覆盖 · 新会话 · 清旧会话。临时家目录，不碰真文件。

use super::*;
use serde_json::json;

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

/// ★ 读不懂的那一份：升级不动它、写也不覆盖；读的一侧照上一份读懂的（没有过 ⇒ 新的那一份）接着用、说得出为什么。
#[test]
fn an_unreadable_book_is_never_overwritten() {
    let home = Home::new("unread");
    std::fs::write(home.file(), "{not json").expect("w");
    let store = home.store();
    assert!(face_change(&store, &home.own(), |b| b.saw("s", "claude-code", "a", 1)).is_err());
    assert_eq!(
        std::fs::read_to_string(home.file()).expect("r"),
        "{not json"
    );
    assert!(store.now().sessions.is_empty());
    assert!(store.unreadable().is_some());
    let mut v = json!({"version": 3, "sessions": {}, "extra": 1});
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(
        matches!(read_at::<Book>(&home.file()), Disk::Unreadable(_)),
        "多一格该读不懂"
    );
    v.as_object_mut().expect("obj").remove("extra");
    std::fs::write(home.file(), v.to_string()).expect("w");
    assert!(matches!(read_at::<Book>(&home.file()), Disk::Now(_)));
}

/// ★ 旧版（第一版没有 `version` · 第二版）没升级写回前：读的一侧当没有，写一律拒、不碰原文。
#[test]
fn an_old_book_reads_as_none_and_is_not_written() {
    for v in [
        json!({"rules": {}, "sessions": {}}),
        json!({"version": 2, "configs": {}, "sessions": {}}),
    ] {
        let home = Home::new("old");
        std::fs::write(home.file(), v.to_string()).expect("w");
        assert!(matches!(read_at::<Book>(&home.file()), Disk::Old));
        let store = home.store();
        assert!(store.now().sessions.is_empty());
        assert!(face_change(&store, &home.own(), |b| b.saw("s", "claude-code", "a", 1)).is_err());
        assert_eq!(
            std::fs::read_to_string(home.file()).expect("r"),
            v.to_string()
        );
    }
}

/// 换了开它的号 ⇒ 在用的号回到它、拖出来的顺序 · memo 清掉；没拖 · 没换过号、七天没见的清掉。
#[test]
fn new_sessions_are_kept_and_stale_ones_are_dropped() {
    let mut b = Book::fresh();
    assert!(b.saw("s1", "claude-code", "a", 100));
    b.sessions.get_mut("s1").expect("s1").temp = vec!["b".into()];
    b.sessions.get_mut("s1").expect("s1").current = "b".into();
    assert!(b.saw("s1", "claude-code", "c", 200));
    let s = &b.sessions["s1"];
    assert_eq!((s.current.as_str(), s.temp.len()), ("c", 0));
    assert!(!b.saw("s1", "claude-code", "c", 300), "同一天再看见不该改");
    b.saw("s2", "claude-code", "a", 300);
    let later = 300 + book::DROP_AFTER + 1;
    b.saw("s3", "claude-code", "a", later);
    assert!(!b.sessions.contains_key("s1") && !b.sessions.contains_key("s2"));
    assert!(b.sessions.contains_key("s3"));
}
