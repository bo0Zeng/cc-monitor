//! 桌面旧书签文件的一次性迁移：只读那台那一格 · 答 ok 才删那一格 · 删空了整份删 · 读不懂一个字节不动。
//! ⚠ 一律落在临时目录（句柄的根经 `CCM_DATA_DIR` 喂成它）；真数据目录一个字节都不碰。

use super::*;
use std::path::PathBuf;

/// 这一条判据自己的数据目录与写它的句柄。
fn scratch(tag: &str) -> (OwnState, PathBuf) {
    let d = test_home::scratch(tag);
    let own = OwnState::issue_with(&|k| {
        (k == crate::config::DATA_DIR_ENV).then(|| d.clone().into_os_string())
    })
    .expect("临时目录是绝对路径");
    (own, d.join(OLD_FILE))
}

fn write(file: &Path, v: serde_json::Value) {
    std::fs::write(file, serde_json::to_vec(&v).unwrap()).unwrap();
}

#[test]
fn only_that_machines_cell_is_pending_and_settling_it_keeps_the_others() {
    let (own, f) = scratch("two");
    assert_eq!(pending_at(&own, "devbox"), None, "旧文件不在 ⇒ 没东西要迁");
    write(
        &f,
        serde_json::json!({ "devbox": ["/srv", "/home/u"], "<local>": ["/tmp/a"], "gone": [] }),
    );
    assert_eq!(
        pending_at(&own, "devbox"),
        Some(vec!["/srv".to_string(), "/home/u".to_string()])
    );
    assert_eq!(pending_at(&own, "gone"), None, "空的那一格不迁");
    settled_at(&own, "devbox").unwrap();
    let left: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(left, serde_json::json!({ "<local>": ["/tmp/a"] }));
    // 最后一格也迁好 ⇒ 整份连锁旁件都没了。
    settled_at(&own, "<local>").unwrap();
    assert!(!f.exists(), "删空了旧文件还在");
    assert!(!f.with_file_name(LOCK_FILE).exists(), "锁旁件还在");
}

#[test]
fn an_unreadable_old_file_is_neither_migrated_nor_touched() {
    let (own, f) = scratch("bad");
    std::fs::write(&f, b"{ not json").unwrap();
    assert_eq!(pending_at(&own, "devbox"), None);
    assert!(settled_at(&own, "devbox").is_err());
    assert_eq!(
        std::fs::read(&f).unwrap(),
        b"{ not json",
        "读不懂 ⇒ 一个字节不动"
    );
}
