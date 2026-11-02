//! `platform-core` 的目录与进程那两族：只给本人的目录 · 目录锁 · `/proc/<pid>/stat` 一行怎么切。

use crate::dir::{hold, private_dir, private_dir_all, LockStage, PRIVATE_DIR_MODE};
use crate::proc::parse_stat;

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-platcore-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("建测试目录");
    d
}

#[cfg(unix)]
fn mode(p: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(p).expect("meta").permissions().mode() & 0o777
}

#[cfg(unix)]
#[test]
fn private_dir_builds_one_level_owner_only_and_leaves_an_existing_one_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = scratch("one");
    let fresh = base.join("fresh");
    private_dir(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    private_dir(&fresh).expect("再建一次（已在）不算错");
    let old = base.join("old");
    std::fs::create_dir(&old).expect("预置");
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    private_dir(&old).expect("已在");
    assert_eq!(mode(&old), 0o755, "已在的那一层被改了权限");
    assert!(
        private_dir(&base.join("no/such")).is_err(),
        "只建一层：缺上层就该失败"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn private_dir_all_builds_every_missing_level_owner_only() {
    let base = scratch("all");
    let deep = base.join("a").join("b");
    private_dir_all(&deep).expect("建");
    assert_eq!(mode(&base.join("a")), PRIVATE_DIR_MODE);
    assert_eq!(mode(&deep), PRIVATE_DIR_MODE);
    private_dir_all(&deep).expect("已在不算错");
    std::fs::remove_dir_all(&base).ok();
}

/// 拿着锁的时候，另开一次同一个目录抢（不等）抢不到；放了就抢得到 —— 与后端 · 壳各开一次进内核同一张表。
#[cfg(unix)]
#[test]
fn the_dir_lock_excludes_another_open_until_dropped() {
    use std::os::fd::AsRawFd as _;
    let base = scratch("lock");
    let try_other = || {
        let f = std::fs::File::open(&base).expect("开目录");
        // SAFETY：`f` 活着、描述有效。
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        rc == 0
    };
    let held = hold(&base).expect("拿锁");
    assert!(!try_other(), "拿着锁时别人还抢得到");
    drop(held);
    assert!(try_other(), "放了锁别人还抢不到");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_dir_lock_on_a_missing_dir_fails_at_open() {
    let base = scratch("lockmiss");
    let err = hold(&base.join("nope")).err().expect("目录不在还拿到了锁");
    assert_eq!(err.stage, LockStage::Open);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn stat_line_splits_around_the_last_paren() {
    // 名字里带空格与括号；右括号之后：state(3) ppid(4) pgrp(5) session(6) tty_nr(7) … starttime(22)。
    let line = "4242 (my (odd) proc) S 17 4242 4242 34816 4242 4194560 1 0 0 0 0 0 0 0 20 0 1 0 98765 1 1";
    let s = parse_stat(line).expect("切得开");
    assert_eq!(s.ppid, 17);
    assert_eq!(s.name, "my (odd) proc");
    assert_eq!(s.tty_nr, 34816);
    assert_eq!(s.start, 98765);
}

#[test]
fn stat_line_without_enough_fields_or_parens_is_unreadable() {
    assert!(parse_stat("").is_none());
    assert!(parse_stat("4242 no-parens S 1").is_none());
    assert!(parse_stat("123 (x) S 1 2 3").is_none(), "不够 22 栏");
    assert!(parse_stat("1 )x( S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20").is_none());
    let not_a_number = "1 (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 notanum 0";
    assert!(parse_stat(not_a_number).is_none(), "起始时刻那一栏不是数");
}
