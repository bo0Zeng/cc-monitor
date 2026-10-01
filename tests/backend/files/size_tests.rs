//! `files/size.rs` 的判据 —— **算目录大小**。
//!
//! 要求：「复制目录 · 批量复制 · 跨机复制 · **算目录大小** · 解压」＋ `§3.7`「不跨文件系统边界那一档没做
//! （设备号要走 `platform/`）」＋ 用户「我能连 ssh 对机器文件进行什么操作，后端就应该能进行什么操作」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_planted_tree_measures_to_hand_counted_numbers`] | 一棵铺好的树 ⇒ 七个数逐格等于手数的（链接记数不算字节） | 期望手写 |
//! | [`a_directory_on_another_device_is_counted_as_skipped_and_not_entered`] | 注入「这个子目录在另一个设备上」⇒ 不进去、`skipped_mounts == 1`；正控：同一棵不注入 ⇒ 进去 | 期望手写；真挂载点造不出来（如实） |
//! | [`a_file_measures_as_itself`] | 顶上是文件 ⇒ `files: 1`、`bytes` == 它的长度 | 期望手写 |
//! | [`the_device_of_two_paths_on_one_filesystem_is_the_same`] | `platform::paths::device_of` 同一文件系统上两条路径相等且非空（unix） | 两次独立读 |
//! | [`the_command_face_answers_exactly_the_declared_fields`] | 线上回的键 == 声明的 `fields`；读不到 ⇒ `unreadable`；没给 ⇒ `bad_path` | 声明住 `CAPABILITIES` |

use super::*;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-size-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时根");
    p
}

/// `t/{a.bin(3), d/{b.bin(5), e/{c.bin(7)}}, m/{z.bin(11)}}` ＋（unix）一条链接 `t/ln -> a.bin`。
fn plant(base: &std::path::Path) -> std::path::PathBuf {
    let t = base.join("t");
    std::fs::create_dir_all(t.join("d/e")).expect("铺");
    std::fs::create_dir_all(t.join("m")).expect("铺");
    std::fs::write(t.join("a.bin"), b"abc").expect("铺");
    std::fs::write(t.join("d/b.bin"), b"12345").expect("铺");
    std::fs::write(t.join("d/e/c.bin"), b"1234567").expect("铺");
    std::fs::write(t.join("m/z.bin"), b"12345678901").expect("铺");
    #[cfg(unix)]
    std::os::unix::fs::symlink(t.join("a.bin"), t.join("ln")).expect("铺链接");
    t
}

#[test]
#[cfg(unix)]
fn a_planted_tree_measures_to_hand_counted_numbers() {
    let base = temp_root("tree");
    let t = plant(&base);
    let m = measure(&t).expect("量不出来");
    assert_eq!(
        m,
        Measured {
            bytes: 3 + 5 + 7 + 11,
            files: 4,
            dirs: 4,
            links: 1,
            other: 0,
            skipped_mounts: 0,
            unreadable_dirs: 0,
        }
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_directory_on_another_device_is_counted_as_skipped_and_not_entered() {
    let base = temp_root("mnt");
    let t = plant(&base);
    let mounted = t.join("m");
    let dev = |p: &std::path::Path| Some(if p == mounted { 2 } else { 1 });
    let m = measure_with(&t, dev).expect("量不出来");
    assert_eq!(
        (m.skipped_mounts, m.dirs, m.files, m.bytes),
        (1, 3, 3, 3 + 5 + 7),
        "挂着的那一格被走进去了（或者没记数）"
    );
    // 正控：不注入 ⇒ 同一棵全走进去。
    let all = measure_with(&t, |_| Some(1)).expect("量不出来");
    assert_eq!((all.skipped_mounts, all.dirs, all.files), (0, 4, 4));
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_file_measures_as_itself() {
    let base = temp_root("file");
    let t = plant(&base);
    let m = measure(&t.join("d/b.bin")).expect("量不出来");
    assert_eq!(
        m,
        Measured {
            bytes: 5,
            files: 1,
            ..Measured::default()
        }
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn the_device_of_two_paths_on_one_filesystem_is_the_same() {
    let base = temp_root("dev");
    let t = plant(&base);
    let a = crate::platform::paths::device_of(&t);
    let b = crate::platform::paths::device_of(&t.join("d/e"));
    assert!(a.is_some(), "unix 上问不出设备号");
    assert_eq!(a, b, "同一个文件系统上两条路径的设备号不等");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_command_face_answers_exactly_the_declared_fields() {
    let base = temp_root("face");
    let t = plant(&base);
    let v = crate::files::answer_wire(
        "files-size",
        &serde_json::json!({ "path": t.to_string_lossy() }),
    )
    .expect("一趟干净的算大小被拒了");
    let got: std::collections::BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> = crate::files::CAPABILITIES
        .iter()
        .find(|c| c.name == "files.size")
        .expect("声明里没有 files.size")
        .fields
        .iter()
        .copied()
        .collect();
    assert_eq!(got, declared);
    assert_eq!(v["bytes"], 3 + 5 + 7 + 11);
    let e = crate::files::answer_wire(
        "files-size",
        &serde_json::json!({ "path": base.join("没有").to_string_lossy() }),
    )
    .expect_err("不存在的路径算出了大小");
    assert_eq!(e.0, "unreadable");
    // 原因说成人话，不把 `ErrorKind` 的调试名（`NotFound`）原样上屏。
    let not_found = copy_core::copy_text("beFilesRead.ioKind.notFound", &[]);
    assert_eq!(
        e.1,
        copy_core::copy_text("beFilesRead.size.unreadable", &[("kind", &not_found)])
    );
    let e = crate::files::answer_wire("files-size", &serde_json::json!({}))
        .expect_err("没给 path 竟然收了");
    assert_eq!(e.0, "bad_path");
    std::fs::remove_dir_all(&base).ok();
}

/// 要求：「不跨文件系统边界那一档没做（设备号要走 `platform/`）」。
///
/// 索引那一侧同一个判法：注入「这个子目录在另一个设备上」⇒ 那个目录本身在索引里、它底下的不在、`skipped_mounts == 1`；
/// 正控：不注入 ⇒ 底下的在、`skipped_mounts == 0`。期望手写（真挂载点造不出来，如实）。
#[test]
fn the_index_does_not_walk_into_another_filesystem() {
    let base = temp_root("idxmnt");
    let t = plant(&base);
    let mounted = t.join("m");
    let has = |snap: &crate::files::index::Snapshot, p: &std::path::Path| {
        let want = crate::files::raw::path_bytes(p).to_vec();
        snap.iter().any(|e| e == want.as_slice())
    };
    let snap = crate::files::index::build_with(&t, |p| Some(if p == mounted { 2 } else { 1 }));
    assert_eq!(snap.stats().skipped_mounts, 1);
    assert!(has(&snap, &mounted), "挂载点那个目录本身该在索引里");
    assert!(!has(&snap, &mounted.join("z.bin")), "走进了另一个文件系统");
    assert!(has(&snap, &t.join("d/e/c.bin")), "同一个文件系统里的丢了");
    let all = crate::files::index::build_with(&t, |_| Some(1));
    assert_eq!(all.stats().skipped_mounts, 0);
    assert!(
        has(&all, &mounted.join("z.bin")),
        "正控：不注入时底下的该在"
    );
    std::fs::remove_dir_all(&base).ok();
}
