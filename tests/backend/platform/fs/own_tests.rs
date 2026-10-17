//! **自家状态的写句柄**（`Own` · `OwnPath` · `OwnLock`）—— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | W1 | `OwnPath` 只从根往下拼：绝对路径 · `..` · 空名都拒 | 纯函数 |
//! | W2 | 建目录：新建 0700；已在的不动；父目录不在照实报错 | 真目录 |
//! | W3 | 原子写要那个目录的锁：写完读回逐字同、0600；拿的是别的目录的锁 ⇒ 拒、一个字节不落 | 真文件 |
//! | W4 | 删文件 · 改名都只在根底下 | 真文件 |
//! | W5 | 家目录那一根只给 `.cc-monitor` 底下：别的打头 · 家目录本身 · 往上跳都拒 | 纯函数 |
//! | D1 | 自家目录（`own_dir`）：新建 0700；已在且是本人的 0700 真目录 ⇒ 过；链接 · 不是目录 · 组 / 他人有权 ⇒ 拒、出声、一个字节不改 | 真目录 |

use super::*;
use std::path::{Path, PathBuf};

/// 判据用的几种造法（只在测试构建里有：住测试模块里，生产段一个都看不见）。
impl Own {
    /// 判据用：家目录指到一个临时目录（数据目录随之是它底下的 `.cc-monitor`，同没设 `CCM_DATA_DIR` 那一形）。
    pub(crate) fn issue_at(home: &Path) -> Own {
        Own {
            home: Some(home.to_path_buf()),
            data: Some(home.join(HOME_DIR_NAME)),
        }
    }

    /// 判据用：只给数据目录（指到一个临时目录；家目录那一根没有）。
    pub(crate) fn issue_data_at(data: &Path) -> Own {
        Own {
            home: None,
            data: Some(data.to_path_buf()),
        }
    }

    /// 判据用：两个根各指一处（家目录 · 数据目录，同设了 `CCM_DATA_DIR` 那一形）。
    pub(crate) fn issue_roots(home: &Path, data: &Path) -> Own {
        Own {
            home: Some(home.to_path_buf()),
            data: Some(data.to_path_buf()),
        }
    }

    /// 判据用：根指到这个测试进程自己的一个临时目录（不碰真家目录；只是要递一份句柄、并不关心写到哪的那些判据用它）。
    pub(crate) fn for_tests() -> Own {
        Own::issue_at(
            &std::env::temp_dir().join(format!("ccm-own-for-tests-{}", std::process::id())),
        )
    }
}

fn sandbox(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "ccm-own-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("sandbox");
    base
}

#[test]
fn w1_paths_only_go_down_from_the_root() {
    let base = sandbox("w1");
    let own = Own::issue_data_at(&base);
    let root = own.root().expect("根");
    assert_eq!(root.as_path(), base.as_path());
    let p = own.path("a/b.json").expect("相对名");
    assert_eq!(p.as_path(), base.join("a/b.json").as_path());
    for bad in ["", "/etc/passwd", "..", "a/../../x", "./../x"] {
        assert!(own.path(bad).is_err(), "该拒没拒：{bad:?}");
        assert!(root.join(bad).is_err(), "该拒没拒（join）：{bad:?}");
    }
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn w2_dirs_are_born_private_and_existing_ones_are_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = sandbox("w2");
    let own = Own::issue_data_at(&base);
    let fresh = own.path("fresh").expect("名");
    own.ensure_dir(&fresh).expect("建");
    assert_eq!(mode(fresh.as_path()), 0o700);
    own.ensure_dir(&fresh).expect("已在不算错");
    std::fs::create_dir(base.join("old")).expect("预置");
    std::fs::set_permissions(base.join("old"), std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    own.ensure_dir(&own.path("old").expect("名")).expect("已在");
    assert_eq!(mode(&base.join("old")), 0o755, "已在的那一层被改了权限");
    assert!(
        own.ensure_dir(&own.path("no/such").expect("名")).is_err(),
        "父目录不在也建出来了（只建一层）"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn w3_atomic_write_needs_the_lock_of_that_very_dir() {
    let base = sandbox("w3");
    let own = Own::issue_data_at(&base);
    let dir = own.root().expect("根");
    let sub = own.path("sub").expect("名");
    own.ensure_dir(&sub).expect("建");
    let file = dir.join("x.json").expect("名");
    let held = own.lock(&dir).expect("锁");
    own.write(&file, &held, b"{\"a\":1}\n").expect("写");
    assert_eq!(std::fs::read(file.as_path()).expect("读回"), b"{\"a\":1}\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let m = std::fs::metadata(file.as_path())
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(m & 0o777, 0o600, "新文件不是只给本人");
    }
    drop(held);
    let other = own.lock(&sub).expect("别的目录的锁");
    let elsewhere = dir.join("y.json").expect("名");
    assert!(
        own.write(&elsewhere, &other, b"z").is_err(),
        "拿着别的目录的锁也写进去了"
    );
    assert!(!elsewhere.as_path().exists(), "拒了却落了盘");
    let leftovers: Vec<_> = guard_core::scan_tree!(&base, &["tmp"])
        .into_iter()
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(leftovers.is_empty(), "留下了旁名：{leftovers:?}");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn w4_remove_stays_under_the_root() {
    let base = sandbox("w4");
    let own = Own::issue_data_at(&base);
    let b = own.path("b").expect("名");
    std::fs::write(b.as_path(), b"1").expect("夹具");
    own.delete_file(&b).expect("删");
    assert!(!b.as_path().exists());
    assert!(own.delete_file(&b).is_err(), "删一个不在的该照实报错");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn w5_the_home_root_only_hands_out_the_backend_dir() {
    let base = sandbox("w5");
    let own = Own::issue_at(&base);
    assert_eq!(
        own.at_home(".cc-monitor/chores.json")
            .expect("表里那一形")
            .as_path(),
        base.join(".cc-monitor/chores.json").as_path()
    );
    assert_eq!(
        own.home_dir().expect("那一层").as_path(),
        base.join(".cc-monitor").as_path()
    );
    assert_eq!(
        own.root().expect("数据目录").as_path(),
        base.join(".cc-monitor").as_path()
    );
    for bad in [
        "",
        ".bashrc",
        "x/.cc-monitor/a",
        ".cc-monitor/../.bashrc",
        "/abs/.cc-monitor",
        "../.cc-monitor/a",
    ] {
        assert!(own.at_home(bad).is_err(), "该拒没拒：{bad:?}");
    }
    assert!(
        Own::issue_data_at(&base).at_home(".cc-monitor/x").is_err(),
        "没有家目录那一根也给出了路径"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 订阅号凭据的窄句柄：只认「一段名字」的文件名与锁名（带分隔符 · `..` · 空的都拒）；写只落那一份文件、锁只建删那两把目录。
#[test]
fn w6_the_account_creds_handle_reaches_one_file_and_its_lock_dirs() {
    let base = sandbox("w6");
    let acct = base.join("acct");
    std::fs::create_dir_all(&acct).unwrap();
    let own = Own::for_tests();
    for bad in ["", "../x", "a/b", ".."] {
        assert!(
            own.account_creds(&acct, bad, ".lock", None, None).is_err(),
            "文件名 {bad:?} 收下了"
        );
        assert!(
            own.account_creds(&acct, ".credentials.json", bad, None, None)
                .is_err(),
            "锁名 {bad:?} 收下了"
        );
    }
    assert!(
        own.account_creds(&acct, ".credentials.json", ".lock", Some("/x"), None)
            .is_err(),
        "旁边那把的后缀带分隔符也收下了"
    );
    for bad in ["", "../x", "a/b"] {
        assert!(
            own.account_creds(&acct, ".credentials.json", ".lock", None, Some(bad))
                .is_err(),
            "写锁名 {bad:?} 收下了"
        );
    }
    let c = own
        .account_creds(
            &acct,
            ".credentials.json",
            ".lock",
            Some(".lock"),
            Some(".w.lock"),
        )
        .expect("合法的那一形");
    let held = c.lock().expect("目录锁");
    c.write(&held, b"{}").expect("写");
    assert_eq!(
        std::fs::read(acct.join(".credentials.json")).unwrap(),
        b"{}"
    );
    assert!(c.make_lock(&RenewLock::Inside).unwrap(), "里面那把建成");
    assert!(!c.make_lock(&RenewLock::Inside).unwrap(), "已在 ⇒ 没建");
    assert!(c.make_lock(&RenewLock::Beside).unwrap(), "旁边那把建成");
    assert!(c.make_lock(&RenewLock::Write).unwrap(), "写锁建成");
    let entry = RenewLock::OneLogin("mcp-x.lock.lock".into());
    assert!(c.make_lock(&entry).unwrap(), "按条起名的那把建成");
    for bad in ["../x", "a/b", ""] {
        assert!(
            c.make_lock(&RenewLock::OneLogin(bad.into())).is_err(),
            "条锁名 {bad:?} 收下了"
        );
    }
    assert!(base.join("acct.lock").is_dir() && acct.join(".lock").is_dir());
    assert!(acct.join(".w.lock").is_dir() && acct.join("mcp-x.lock.lock").is_dir());
    for w in [
        RenewLock::Inside,
        RenewLock::Beside,
        RenewLock::Write,
        entry,
    ] {
        c.drop_lock(&w).unwrap();
    }
    assert!(!base.join("acct.lock").exists() && !acct.join(".lock").exists());
    assert!(!acct.join(".w.lock").exists() && !acct.join("mcp-x.lock.lock").exists());
    assert!(side_files(&acct).is_empty(), "号目录里留了旁名");
    std::fs::remove_dir_all(&base).ok();
}

// ── 原子写本体（从 `common/own_state` 那一份测试搬来：写那一半随转调一起退了，本体只住这里）──────────────

fn side_files(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = guard_core::scan_tree!(dir, &["tmp"])
        .into_iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn a2_round_trip_newline_and_private() {
    let base = sandbox("a2");
    let p = base.join("x.json");
    atomic_write(
        &p,
        &crate::common::own_state::json_body(&p, &serde_json::json!({"a": 1})).expect("编码"),
    )
    .expect("write");
    assert_eq!(std::fs::read(&p).expect("read"), b"{\"a\":1}\n");
    atomic_write(&p, b"raw").expect("overwrite");
    assert_eq!(std::fs::read(&p).expect("read"), b"raw");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "后端自有状态文件不是只给本人");
    }
    assert!(side_files(&base).is_empty(), "留了旁名");
    assert!(p.is_file());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a3_concurrent_writers_never_collide_on_the_side_name() {
    let base = sandbox("a3");
    let p = base.join("x.json");
    let fails: Vec<String> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..16)
            .map(|i| {
                let p = p.clone();
                s.spawn(move || {
                    (0..20)
                        .filter_map(|j| {
                            atomic_write(&p, format!("{i}-{j}").as_bytes())
                                .err()
                                .map(|e| e.said)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("join"))
            .collect()
    });
    assert!(
        fails.is_empty(),
        "并发写撞了旁名 {} 次，头一条：{:?}",
        fails.len(),
        fails.first()
    );
    assert!(side_files(&base).is_empty(), "留了旁名");
    assert!(p.is_file());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a4_failed_rename_removes_only_its_own_side_file() {
    let base = sandbox("a4");
    let target = base.join("x.json");
    std::fs::create_dir(&target).expect("target is a dir");
    std::fs::write(target.join("keep"), b"k").expect("non-empty");
    std::fs::write(base.join("x.json.other.tmp"), b"theirs").expect("foreign");
    assert!(
        atomic_write(&target, b"mine").is_err(),
        "挪到非空目录上居然成了"
    );
    assert_eq!(
        side_files(&base),
        vec!["x.json.other.tmp".to_string()],
        "旁名没删，或删了别人的文件"
    );
    assert!(target.join("keep").is_file(), "挪的目标被动了");
    std::fs::remove_dir_all(&base).ok();
}

/// 写不进的那一句只带路径与原因词，系统原话进 `raw`（复制详情）。
#[test]
fn a5_a_failed_write_carries_the_reason_word_and_the_raw_words_go_to_the_detail() {
    let base = sandbox("a5");
    let gone = base.join("no-such-dir").join("y.json");
    let e = atomic_write(&gone, b"x").expect_err("父目录不在也写成了");
    assert!(
        e.said
            .ends_with(&copy_core::io_reason(std::io::ErrorKind::NotFound)),
        "写不进那一句没带原因词：{e:?}"
    );
    assert!(e.raw.is_some(), "系统原话没进详情：{e:?}");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn o1_born_private_existing_left_alone_missing_parent_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = std::env::temp_dir().join(format!("ccm-hx1-owndir-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("base");
    let fresh = base.join(".cc-monitor");
    create_dir_private(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    create_dir_private(&fresh).expect("再建一次（已在）不算错");
    let old = base.join("old");
    std::fs::create_dir(&old).expect("预置");
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    create_dir_private(&old).expect("已在");
    assert_eq!(mode(&old), 0o755, "已在的那一层被改了权限");
    assert!(
        create_dir_private(&base.join("no/such")).is_err(),
        "父目录不在也建出来了（只建一层）"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn d1_an_own_dir_is_a_private_real_dir_of_mine_or_it_is_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = sandbox("d1");
    let fresh = base.join("fresh");
    own_dir(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    own_dir(&fresh).expect("已在、本人、0700 ⇒ 过");
    // 不合的三形：各拒、各说清是哪一形，盘上一个字节不改。
    let real = base.join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
    let link = base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let file = base.join("file");
    std::fs::write(&file, b"x").unwrap();
    let wide = base.join("wide");
    std::fs::create_dir(&wide).unwrap();
    std::fs::set_permissions(&wide, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (p, key) in [
        (&link, "beOwnState.dir.link"),
        (&file, "beOwnState.dir.notDir"),
        (&wide, "beOwnState.dir.tooWide"),
    ] {
        let e = own_dir(p).expect_err(&format!("{} 该拒", p.display()));
        assert!(
            copy_core::copy_matches(key, &e.said),
            "{}：{e:?}",
            p.display()
        );
    }
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(mode(&wide), 0o755, "不合的那一层被改了权限");
    assert!(
        own_dir(&base.join("no/such")).is_err(),
        "父目录不在也建出来了（只建一层）"
    );
    std::fs::remove_dir_all(&base).ok();
}
