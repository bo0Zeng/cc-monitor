//! 自家状态文件的原子写：一组行为夹具（首写 · 覆盖 · 写到一半失败 · 旁名名字被别人占着 · 换不上去 · 新文件只给本人）。
//! 每一格都看两样：目标是不是原样 / 新的那一份，旁名有没有留下。

use crate::atomic::{create_new_private, write_atomic, AtomicFail, Stage};
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-platcore-atomic-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("建测试目录");
    d
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("列")
        .map(|e| e.expect("项").file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn put(path: &Path, tmp: &Path, body: &[u8]) -> Result<(), AtomicFail> {
    write_atomic(path, tmp, create_new_private, |f| f.write_all(body))
}

#[test]
fn first_write_creates_the_file_and_leaves_no_side_file() {
    let d = scratch("first");
    let at = d.join("state.json");
    put(&at, &d.join(".state.json.1.tmp"), b"{\"v\":1}").expect("写");
    assert_eq!(std::fs::read(&at).expect("读"), b"{\"v\":1}");
    assert_eq!(names(&d), vec!["state.json"]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn overwrite_replaces_the_whole_file() {
    let d = scratch("over");
    let at = d.join("state.json");
    std::fs::write(&at, b"old old old old").expect("预置");
    put(&at, &d.join(".state.json.2.tmp"), b"new").expect("写");
    assert_eq!(std::fs::read(&at).expect("读"), b"new");
    assert_eq!(names(&d), vec!["state.json"]);
    std::fs::remove_dir_all(&d).ok();
}

/// 写到一半失败（正文交了一截就出错，像写一半崩）：目标原样、旁名删掉。
#[test]
fn a_fill_that_fails_halfway_leaves_the_target_and_no_side_file() {
    let d = scratch("half");
    let at = d.join("state.json");
    std::fs::write(&at, b"intact").expect("预置");
    let err = write_atomic(&at, &d.join(".state.json.3.tmp"), create_new_private, |f| {
        f.write_all(b"half of it")?;
        Err(std::io::Error::other("崩在一半"))
    })
    .err()
    .expect("该失败");
    assert_eq!(err.stage, Stage::WriteTmp);
    assert_eq!(std::fs::read(&at).expect("读"), b"intact");
    assert_eq!(names(&d), vec!["state.json"]);
    std::fs::remove_dir_all(&d).ok();
}

/// 旁名那个名字上已经有一份（不是这一趟建的）：不碰它、不碰目标。
#[test]
fn a_side_name_already_taken_is_not_ours_and_is_left_alone() {
    let d = scratch("taken");
    let at = d.join("state.json");
    let tmp = d.join(".state.json.4.tmp");
    std::fs::write(&at, b"intact").expect("预置");
    std::fs::write(&tmp, b"someone else").expect("预置旁名");
    let err = put(&at, &tmp, b"new").err().expect("该失败");
    assert_eq!(err.stage, Stage::CreateTmp);
    assert_eq!(std::fs::read(&tmp).expect("读"), b"someone else");
    assert_eq!(std::fs::read(&at).expect("读"), b"intact");
    std::fs::remove_dir_all(&d).ok();
}

/// 换不上去（目标是一个非空目录）：旁名删掉。
#[test]
fn a_replace_that_fails_removes_the_side_file() {
    let d = scratch("noreplace");
    let at = d.join("state.json");
    std::fs::create_dir(&at).expect("预置目录");
    std::fs::write(at.join("inside"), b"x").expect("预置");
    let err = put(&at, &d.join(".state.json.5.tmp"), b"new")
        .err()
        .expect("该失败");
    assert_eq!(err.stage, Stage::Replace);
    assert_eq!(names(&d), vec!["state.json"]);
    std::fs::remove_dir_all(&d).ok();
}

/// 新文件只给本人（unix：旁名出生即 0600，换名上位后就是这份的权限）。
#[cfg(unix)]
#[test]
fn the_new_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let d = scratch("mode");
    let at = d.join("state.json");
    put(&at, &d.join(".state.json.6.tmp"), b"x").expect("写");
    let m = std::fs::metadata(&at).expect("meta").permissions().mode() & 0o777;
    assert_eq!(m, 0o600);
    std::fs::remove_dir_all(&d).ok();
}
