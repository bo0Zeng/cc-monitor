//! 桌面旧书签文件的一次性迁移：只读那台那一格 · 答 ok 才删那一格 · 删空了整份删 · 读不懂一个字节不动。
//! ⚠ 一律落在临时目录；真数据目录一个字节都不碰。

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-bm-migrate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join(OLD_FILE)
}

fn write(file: &Path, v: serde_json::Value) {
    std::fs::write(file, serde_json::to_vec(&v).unwrap()).unwrap();
}

#[test]
fn only_that_machines_cell_is_pending_and_settling_it_keeps_the_others() {
    let f = scratch("two");
    assert_eq!(pending_at(&f, "devbox"), None, "旧文件不在 ⇒ 没东西要迁");
    write(
        &f,
        serde_json::json!({ "devbox": ["/srv", "/home/u"], "<local>": ["/tmp/a"], "gone": [] }),
    );
    assert_eq!(
        pending_at(&f, "devbox"),
        Some(vec!["/srv".to_string(), "/home/u".to_string()])
    );
    assert_eq!(pending_at(&f, "gone"), None, "空的那一格不迁");
    settled_at(&f, "devbox").unwrap();
    let left: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(left, serde_json::json!({ "<local>": ["/tmp/a"] }));
    // 最后一格也迁好 ⇒ 整份连锁旁件都没了。
    settled_at(&f, "<local>").unwrap();
    assert!(!f.exists(), "删空了旧文件还在");
    let mut lock = f.as_os_str().to_os_string();
    lock.push(".lock");
    assert!(!PathBuf::from(lock).exists(), "锁旁件还在");
}

#[test]
fn an_unreadable_old_file_is_neither_migrated_nor_touched() {
    let f = scratch("bad");
    std::fs::write(&f, b"{ not json").unwrap();
    assert_eq!(pending_at(&f, "devbox"), None);
    assert!(settled_at(&f, "devbox").is_err());
    assert_eq!(
        std::fs::read(&f).unwrap(),
        b"{ not json",
        "读不懂 ⇒ 一个字节不动"
    );
}
