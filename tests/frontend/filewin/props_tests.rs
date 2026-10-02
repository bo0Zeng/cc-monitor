//! 右键「属性」：后端那一份解得对、框里列得对、菜单那一下真的摆出框并去问。

use super::*;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};
use crate::select::Action;

/// `files-stat` 的应答 → 框里那几格：属主与链接指向（字符串 / 字节形）都认，`null` 当没有。
#[test]
fn a_stat_reply_is_read_with_owner_and_link_target() {
    let d = serde_json::json!({
        "path": "/srv/ln", "kind": "file", "size": 3, "mtime_secs": 0, "mode": 493,
        "owner": "zbl", "link_target": { "b16": "612e62696e" }
    });
    let s = stat_from_reply(&d).unwrap();
    assert_eq!(s.owner.as_deref(), Some("zbl"));
    assert_eq!(s.link_target.as_deref(), Some("a.bin"));
    assert_eq!(s.mode, Some(0o755));
    let plain =
        serde_json::json!({ "path": "/srv/a", "size": 3, "owner": null, "link_target": null });
    let s = stat_from_reply(&plain).unwrap();
    assert_eq!((s.owner, s.link_target, s.mode), (None, None, None));
    assert!(
        stat_from_reply(&serde_json::json!({ "size": 1 })).is_err(),
        "没有路径就是错"
    );
    assert_eq!(mode_text(0o755), "rwxr-xr-x (755)");
    assert_eq!(mode_text(0o4640), "rw-r----- (4640)");
}

/// 菜单上「属性」那一下：框摆出来、真去问了 `files-stat`，答回来的那几格逐格列在框里。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_menu_item_puts_up_the_box_and_asks_the_backend() {
    let dir = std::env::temp_dir().join(format!("ccm-props-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"abc").unwrap();
    let d = dir.to_string_lossy().to_string();
    let wired = wire_up(
        "props",
        FakeBackend::new(&["files-stat", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, &d);
    *w.listing.rows.lock().unwrap() = crate::source::list_local(&dir).unwrap();
    // 目录里只有这一份 ⇒ 全选就是选中它。
    assert!(w.apply_intent(crate::select::Intent::SelectAll, 0.0, None));
    assert!(w.perform(Action::Properties, None), "菜单那一下该摆出框");
    for _ in 0..400 {
        if !matches!(*w.props().unwrap().state.lock().unwrap(), State::Loading) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(wired.count("files-stat"), 1, "该恰好问一次后端");
    let lines = w.props().unwrap().lines();
    let path_line = lines
        .iter()
        .find(|(k, _)| *k == copy_text("rsFilewinProps.label.path", &[]))
        .unwrap();
    assert!(path_line.1.ends_with("a.txt"), "{lines:?}");
    std::fs::remove_dir_all(&dir).ok();
}
