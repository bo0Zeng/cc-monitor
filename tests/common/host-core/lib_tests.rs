//! （文件窗口成独立包）：原子写与窗口几何收进 `host-core`，两个前端共用一份。

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
    // 写完 dst 父目录里只有 dst，没有 ccm-tmp-* 残留
    let dir = test_home::scratch("tmpcheck");
    let dst = dir.join("a.json");
    atomic_write_json(&dst, &serde_json::json!({ "k": 1 })).unwrap();
    assert_eq!(test_home::list(&dir), ["a.json"], "ccm-tmp- 残留未清理");
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

// ── 要求：「连着重启 monitor 三次，主窗外框左上 x 78 → 52 → 26，每起一次左移 26 px」──

/// 主窗 / 设置窗每次起都摆在工作区正中：左上只由尺寸与工作区定（入参里没有「它原来在哪」）
/// ⇒ 重启 N 次位置不漂（期望手算：工作区 1280×712，外框 1116×780 ⇒ 高夹到 712、内框 1100×673，左上 (82, 0)）。
#[test]
fn restarting_n_times_lands_the_window_in_the_same_place() {
    let work = WorkArea {
        x: 0,
        y: 0,
        w: 1280,
        h: 712,
    };
    let landed = center_in_work_area((1116, 780), (1100, 741), work);
    assert_eq!(landed, ((1100, 673), (82, 0)));
    // 放得下的设置窗：尺寸不动、正中；副屏在左边（负坐标）照样摆进那一块正中。
    assert_eq!(
        center_in_work_area((776, 600), (760, 561), work),
        ((760, 561), (252, 56))
    );
    let left = WorkArea {
        x: -1920,
        y: 0,
        w: 1920,
        h: 1040,
    };
    assert_eq!(
        center_in_work_area((800, 600), (784, 561), left),
        ((784, 561), (-1360, 220))
    );
}

/// 开窗那一刻问到的外框比内框还小（Linux GTK 上窗口还没映射时的占位尺寸）⇒ 不许把窗「夹」成那个占位尺寸。
#[test]
fn a_placeholder_outer_size_smaller_than_the_inner_does_not_shrink_the_window() {
    let work = WorkArea {
        x: 0,
        y: 0,
        w: 1400,
        h: 900,
    };
    let (inner, pos) = center_in_work_area((149, 35), (1100, 800), work);
    assert_eq!(inner, (1100, 800));
    assert_eq!(pos, (150, 50));
    assert_eq!(
        fit_into_work_area((10, 10), (149, 35), (1100, 800), work),
        None
    );
}

// ── 有上限的读：前端自有状态文件读回来先过上限（上限从声明表来，见 `relay_route_core::CONFIG_FILE`）──

fn capped_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-capped-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 判据用的一道读上限（超了怎么办这几条判据不看，只看字节数）。
fn cap(bytes: u64) -> Cap {
    Cap {
        bytes,
        over: Over::Reject,
    }
}

#[test]
fn a_capped_read_says_absent_whole_or_too_big_at_the_exact_boundary() {
    let d = capped_dir("edge");
    let f = d.join("s.json");
    assert_eq!(read_capped(&f, cap(8)).unwrap(), Capped::Absent);
    std::fs::write(&f, "12345678").unwrap();
    assert_eq!(
        read_capped(&f, cap(8)).unwrap(),
        Capped::Whole("12345678".into())
    );
    std::fs::write(&f, "123456789").unwrap();
    assert_eq!(read_capped(&f, cap(8)).unwrap(), Capped::TooBig);
    assert_eq!(
        read_capped(&f, cap(9)).unwrap(),
        Capped::Whole("123456789".into())
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_cap_of_four_bytes_turns_a_five_byte_file_away() {
    let d = capped_dir("four");
    let f = d.join("s.json");
    std::fs::write(&f, "12345").unwrap();
    let four = Cap {
        bytes: 4,
        over: Over::Reject,
    };
    assert_eq!(read_capped(&f, four).unwrap(), Capped::TooBig);
    assert_eq!(read_capped_bytes(&f, four).unwrap(), Capped::TooBig);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_capped_byte_read_hands_back_bytes_that_are_not_utf8() {
    let d = capped_dir("bytes");
    let f = d.join("s.bin");
    assert_eq!(read_capped_bytes(&f, cap(2)).unwrap(), Capped::Absent);
    std::fs::write(&f, [0xffu8, 0xfe]).unwrap();
    assert_eq!(
        read_capped_bytes(&f, cap(2)).unwrap(),
        Capped::Whole(vec![0xff, 0xfe])
    );
    assert_eq!(read_capped_bytes(&f, cap(1)).unwrap(), Capped::TooBig);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_capped_read_of_bytes_that_are_not_utf8_is_a_read_error() {
    let d = capped_dir("utf8");
    let f = d.join("s.json");
    std::fs::write(&f, [0xffu8, 0xfe]).unwrap();
    let e = read_capped(&f, cap(64)).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
    let _ = std::fs::remove_dir_all(&d);
}
