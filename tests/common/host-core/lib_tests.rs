//! （文件窗口成独立包）：原子写与窗口几何收进 `host-core`，两个前端共用一份。
//! 下面几条原住 `tests/frontend/shell/utils_tests.rs`（原子写）与 `lib_window_lifecycle_tests.rs`（夹进工作区），随代码逐字搬来。

use super::*;

#[test]
fn atomic_write_json_first_write_creates_file() {
    let tmp = std::env::temp_dir().join(format!(
        "ccm-utils-test-first-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&tmp);
    let v = serde_json::json!({ "a": 1, "b": "hi" });
    atomic_write_json(&tmp, &v).unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
    assert_eq!(on_disk["a"], 1);
    assert_eq!(on_disk["b"], "hi");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn atomic_write_json_replace_keeps_content() {
    let tmp = std::env::temp_dir().join(format!(
        "ccm-utils-test-replace-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    atomic_write_json(&tmp, &serde_json::json!({ "v": 1 })).unwrap();
    atomic_write_json(&tmp, &serde_json::json!({ "v": 2, "extra": "y" })).unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
    assert_eq!(on_disk["v"], 2);
    assert_eq!(on_disk["extra"], "y");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn atomic_write_json_no_stray_tmp() {
    // 写完 dst 父目录里不应该有任何 ccm-tmp-* 残留
    let dir = std::env::temp_dir().join(format!("ccm-utils-test-tmpcheck-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("a.json");
    let _ = std::fs::remove_file(&dst);
    atomic_write_json(&dst, &serde_json::json!({ "k": 1 })).unwrap();
    let stray = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".ccm-tmp-"));
    assert!(!stray, "ccm-tmp- 残留未清理");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── 要求：「窗口初始高 780 > 工作区 712 …… 结果最后一行被任务栏压住」──

/// 真机那一格原样（期望手算）：工作区 1280×712（屏 760、任务栏 48），主窗外框 1116×780 @ (156,0)、边框 + 标题栏 16×39 ⇒
/// 外框高夹到 712、内框跟着缩到 673、左上不动；放得下的那一扇 ⇒ `None`（不碰）；跑出右下角的 ⇒ 只挪位置、尺寸不动；
/// 副屏在主屏左边（负坐标）⇒ 照样夹进那一块。
#[test]
fn a_window_taller_than_the_work_area_is_shrunk_and_moved_inside_it() {
    let work = WorkArea {
        x: 0,
        y: 0,
        w: 1280,
        h: 712,
    };
    assert_eq!(
        fit_into_work_area((156, 0), (1116, 780), (1100, 741), work),
        Some(((1100, 673), (156, 0)))
    );
    assert_eq!(
        fit_into_work_area((10, 10), (800, 600), (784, 561), work),
        None
    );
    assert_eq!(
        fit_into_work_area((900, 500), (800, 600), (784, 561), work),
        Some(((784, 561), (480, 112)))
    );
    let left = WorkArea {
        x: -1920,
        y: 0,
        w: 1920,
        h: 1040,
    };
    assert_eq!(
        fit_into_work_area((-100, 0), (800, 1200), (784, 1161), left),
        Some(((784, 1001), (-800, 0)))
    );
}

/// ★ monitor 自己的东西只给本人：缺的父目录建成 700、文件 600（换名上位后就是临时件的权限）；已在的目录不动。
#[cfg(unix)]
#[test]
fn atomic_write_json_creates_dirs_and_file_only_for_the_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = std::env::temp_dir().join(format!("ccm-utils-test-private-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dst = root.join("a").join("b").join("x.json");
    atomic_write_json(&dst, &serde_json::json!({ "k": 1 })).unwrap();
    atomic_write_json(&dst, &serde_json::json!({ "k": 2 })).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        [
            mode(&root),
            mode(&root.join("a")),
            mode(&root.join("a/b")),
            mode(&dst)
        ],
        [0o755, 0o700, 0o700, 0o600]
    );
    let _ = std::fs::remove_dir_all(&root);
}
