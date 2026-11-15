//! **自家状态的写句柄**（`Own` · `OwnPath` · `OwnLock`）—— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | W1 | `OwnPath` 只从根往下拼：绝对路径 · `..` · 空名都拒 | 纯函数 |
//! | W2 | 建目录：新建 0700；已在的不动；父目录不在照实报错 | 真目录 |
//! | W3 | 原子写要那个目录的锁：写完读回逐字同、0600；拿的是别的目录的锁 ⇒ 拒、一个字节不落 | 真文件 |
//! | W4 | 删文件 · 改名都只在根底下 | 真文件 |
//! | W5 | 家目录那一根只给 `.cc-monitor` 底下：别的打头 · 家目录本身 · 往上跳都拒 | 纯函数 |
//! | D1 | 自家目录（`own_dir`）：新建 0700；已在且是本人的 0700 真目录 ⇒ 过；组 / 他人有权 ⇒ 收回 0700；链接 · 不是目录 ⇒ 拒、出声、一个字节不改 | 真目录 |

use super::*;
use std::path::Path;

#[test]
fn w1_paths_only_go_down_from_the_root() {
    let base = test_home::scratch("w1");
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
    let base = test_home::scratch("w2");
    let own = Own::issue_data_at(&base);
    let fresh = own.path("fresh").expect("名");
    own.ensure_dir(&fresh).expect("建");
    assert_eq!(mode(fresh.as_path()), 0o700);
    own.ensure_dir(&fresh).expect("已在不算错");
    std::fs::create_dir(base.join("old")).expect("预置");
    test_home::set_dir_mode(&base.join("old"), 0o755);
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
    let base = test_home::scratch("w3");
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
    let base = test_home::scratch("w4");
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
    let base = test_home::scratch("w5");
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
    let base = test_home::scratch("w6");
    let acct = base.join("acct");
    std::fs::create_dir_all(&acct).unwrap();
    let own = Own::for_tests();
    for bad in ["", "../x", "a/b", ".."] {
        assert!(
            own.account_creds(&acct, bad, Some(".lock"), None, None)
                .is_err(),
            "文件名 {bad:?} 收下了"
        );
        assert!(
            own.account_creds(&acct, ".credentials.json", Some(bad), None, None)
                .is_err(),
            "锁名 {bad:?} 收下了"
        );
    }
    assert!(
        own.account_creds(&acct, ".credentials.json", Some(".lock"), Some("/x"), None)
            .is_err(),
        "旁边那把的后缀带分隔符也收下了"
    );
    for bad in ["", "../x", "a/b"] {
        assert!(
            own.account_creds(
                &acct,
                ".credentials.json",
                Some(".lock"),
                None,
                Some((bad, WriteLockForm::Dir))
            )
            .is_err(),
            "写锁名 {bad:?} 收下了"
        );
    }
    let c = own
        .account_creds(
            &acct,
            ".credentials.json",
            Some(".lock"),
            Some(".lock"),
            Some((".w.lock", WriteLockForm::Dir)),
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
    assert!(base.join("acct.lock").is_dir() && acct.join(".lock").is_dir());
    assert!(acct.join(".w.lock").is_dir());
    for w in [RenewLock::Inside, RenewLock::Beside, RenewLock::Write] {
        c.drop_lock(&w).unwrap();
    }
    assert!(!base.join("acct.lock").exists() && !acct.join(".lock").exists());
    assert!(!acct.join(".w.lock").exists());
    // 那一家续期不拿锁（里面那把没登记）⇒ 那一把无处可建、无处可删。
    let bare = own
        .account_creds(&acct, ".credentials.json", None, None, None)
        .expect("不登记续期锁的那一形");
    assert_eq!(bare.lock_path(&RenewLock::Inside), None);
    assert!(
        !bare.make_lock(&RenewLock::Inside).unwrap(),
        "没登记 ⇒ 没建"
    );
    assert!(side_files(&acct).is_empty(), "号目录里留了旁名");
    std::fs::remove_dir_all(&base).ok();
}

/// MCP 登录文件的窄句柄：文件名只认一段名字；锁在铸的那一下登记（`mkdir` 那一形只认一段名字，文件锁认一段名字或
/// 「一层子目录 / 一段名字」），之后只按登记的序号点、形不对就拒；文件锁的子目录不在就建；同一份锁文件第二个人拿 ⇒ 没拿到；
/// 写只落那一份文件。
#[cfg(unix)]
#[test]
fn the_mcp_logins_handle_reaches_one_file_and_its_declared_locks() {
    use crate::platform::fs::LoginLockForm::{Dir, File};
    let base = test_home::scratch("mcp-logins-handle");
    let acct = base.join("acct");
    std::fs::create_dir_all(&acct).unwrap();
    let own = Own::for_tests();
    for bad in ["", "../x", "a/b", ".."] {
        assert!(
            own.mcp_logins_file(&acct, bad, &[]).is_err(),
            "文件名 {bad:?} 收下了"
        );
    }
    for bad in ["../x", "a/b", ""] {
        assert!(
            own.mcp_logins_file(&acct, "l.conf", &[(bad.into(), Dir)])
                .is_err(),
            "mkdir 锁名 {bad:?} 收下了"
        );
    }
    for bad in ["../x", "a/b/c", "", "a/../b"] {
        assert!(
            own.mcp_logins_file(&acct, "l.conf", &[(bad.into(), File)])
                .is_err(),
            "文件锁名 {bad:?} 收下了"
        );
    }
    let f = own
        .mcp_logins_file(
            &acct,
            "logins.conf",
            &[("w.lock".into(), Dir), ("locks/k.lock".into(), File)],
        )
        .expect("合法的那一形");
    let held = f.lock().expect("目录锁");
    f.write(&held, b"login k {}\n").expect("写");
    assert_eq!(
        std::fs::read(acct.join("logins.conf")).unwrap(),
        b"login k {}\n"
    );
    assert!(f.make_dir_lock(0).unwrap(), "mkdir 锁建成");
    assert!(!f.make_dir_lock(0).unwrap(), "已在 ⇒ 没建");
    assert!(f.make_dir_lock(1).is_err(), "拿文件锁那一把当 mkdir 建了");
    assert!(f.make_dir_lock(2).is_err(), "没登记的那一把也建了");
    f.drop_dir_lock(0).unwrap();
    assert!(!acct.join("w.lock").exists());
    assert!(f.try_file_lock(0).is_err(), "拿 mkdir 那一把当文件锁开了");
    let first = f.try_file_lock(1).unwrap();
    assert!(first.is_some(), "文件锁拿到");
    assert!(acct.join("locks/k.lock").is_file(), "子目录与锁文件建好");
    assert!(
        f.try_file_lock(1).unwrap().is_none(),
        "我方持着再拿还拿到了"
    );
    drop(first);
    // 本进程放了之后马上再拿不判：旁边别的判据线程 fork 出的子进程在 fork 到 exec 之间还攥着那份 fd（锁跟着它）。
    // 「放了 ⇒ 拿到」由真子进程来持、来放（它 `flock` 是等着拿的，本进程漏出去的那几份关了才拿上）。
    let forks = test_home::forking(4);
    for i in 0..300 {
        let theirs = test_home::held_elsewhere(&acct, "locks/k.lock");
        assert!(
            f.try_file_lock(1).unwrap().is_none(),
            "第 {i} 次：别人持着还拿到了"
        );
        test_home::let_go(theirs);
        assert!(
            f.try_file_lock(1).unwrap().is_some(),
            "第 {i} 次：放了还拿不到"
        );
    }
    drop(forks);
    drop(held);
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
    let base = test_home::scratch("a2");
    let p = base.join("x.json");
    // 编码那一半（一行 JSON ＋ 结尾换行）住上层，它的判据在后端库根的 `platform/cross_tests.rs`。
    atomic_write(&p, b"{\"a\":1}\n").expect("write");
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
    let base = test_home::scratch("a3");
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
    let base = test_home::scratch("a4");
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
    let base = test_home::scratch("a5");
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
    let base = test_home::scratch("owndir");
    let fresh = base.join(".cc-monitor");
    create_dir_private(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    create_dir_private(&fresh).expect("再建一次（已在）不算错");
    let old = base.join("old");
    std::fs::create_dir(&old).expect("预置");
    test_home::set_dir_mode(&old, 0o755);
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
    let base = test_home::scratch("d1");
    let fresh = base.join("fresh");
    own_dir(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    own_dir(&fresh).expect("已在、本人、0700 ⇒ 过");
    // 不合的两形：各拒、各说清是哪一形，盘上一个字节不改。
    let real = base.join("real");
    std::fs::create_dir(&real).unwrap();
    test_home::set_dir_mode(&real, 0o700);
    let link = base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let file = base.join("file");
    std::fs::write(&file, b"x").unwrap();
    for (p, key) in [
        (&link, "beOwnState.dir.link"),
        (&file, "beOwnState.dir.notDir"),
    ] {
        let e = own_dir(p).expect_err(&format!("{} 该拒", p.display()));
        assert!(
            copy_core::copy_matches(key, &e.said),
            "{}：{e:?}",
            p.display()
        );
    }
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(mode(&real), 0o700, "链接那头被改了");
    // 放宽了的本人目录 ⇒ 收回 0700、过。
    let wide = base.join("wide");
    std::fs::create_dir(&wide).unwrap();
    test_home::set_dir_mode(&wide, 0o755);
    own_dir(&wide).expect("放宽了的本人目录该收回、过");
    assert_eq!(mode(&wide), PRIVATE_DIR_MODE, "放宽着没收回");
    assert!(
        own_dir(&base.join("no/such")).is_err(),
        "父目录不在也建出来了（只建一层）"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn diag_log_created_file_is_born_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = test_home::scratch("diag-perm");
    let log_path = base.join("stderr.log");
    let diag = DiagLog::issue(log_path.clone());
    let file = diag.create().expect("create");
    drop(file);
    assert_eq!(
        mode(&log_path),
        0o600,
        "新建的诊断日志权限位不是 0600（按 umask 落了）"
    );

    let append_path = base.join("append.log");
    let diag_append = DiagLog::issue(append_path.clone());
    diag_append.append_line(b"hello\n").expect("append");
    assert_eq!(
        mode(&append_path),
        0o600,
        "追加新建的诊断日志权限位不是 0600"
    );

    std::fs::remove_dir_all(&base).ok();
}

/// 整份写锁的文件锁那一形：别人持着那份锁文件的排他锁 ⇒ 拿不到（不等）；放了 ⇒ 拿得到；这一形不建锁目录。
/// 「别人」是一个真子进程，这一段排在本进程第一次拿锁之前：本进程持过锁的那份 fd，旁边别的判据线程 fork 出的子进程在
/// fork 到 exec 之间也攥着（flock 跟着打开的那份文件），本进程放了、锁还在。开着旁边不停 fork 循环 300 次，撞上一次当场红。
#[cfg(unix)]
#[test]
fn a_file_write_lock_is_taken_without_waiting_and_never_made_a_directory() {
    let base = test_home::scratch("filelock");
    let acct = base.join("acct");
    std::fs::create_dir_all(&acct).unwrap();
    let own = Own::for_tests();
    let c = own
        .account_creds(
            &acct,
            "auth.json",
            Some(".renew"),
            None,
            Some(("auth.lock", WriteLockForm::File)),
        )
        .expect("合法的那一形");
    assert_eq!(c.write_lock_form(), Some(WriteLockForm::File));
    let forks = test_home::forking(4);
    for i in 0..300 {
        let theirs = test_home::held_elsewhere(&acct, "auth.lock");
        assert!(
            c.try_file_lock().expect("拿得动").is_none(),
            "第 {i} 次：别人持着 ⇒ 不等、拿不到"
        );
        test_home::let_go(theirs);
        assert!(
            c.try_file_lock().expect("拿得动").is_some(),
            "第 {i} 次：放了 ⇒ 拿到"
        );
    }
    drop(forks);
    // 循环最后一下本进程刚拿过、放过这一份：删掉它，下面「不在就建」建的是一个新 inode，漏出去的旧 fd 碍不着。
    std::fs::remove_file(acct.join("auth.lock")).unwrap();
    let held = c.try_file_lock().expect("拿得动").expect("没人持着 ⇒ 拿到");
    assert!(acct.join("auth.lock").is_file(), "锁文件不在就建");
    let other = std::fs::OpenOptions::new()
        .write(true)
        .open(acct.join("auth.lock"))
        .unwrap();
    assert!(
        other.try_lock().is_err(),
        "我方持着时别人拿不到（同一份文件锁）"
    );
    drop(other);
    drop(held);
    assert!(
        c.make_lock(&RenewLock::Write).is_err(),
        "文件锁那一形不许建成目录"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// 🔴 家说不出（没有家 · 家是空串）⇒ 家那一根两形都拒、拒的是同一句（`beOwnState.path.noHome`），不拼成根上的 `/.cc-monitor/…`；
/// 判法不在本文件另写一份：家那一格只交给 `platform::paths::under_home`。
#[test]
fn w7_an_unsaid_home_is_refused_alike_and_judged_by_under_home() {
    let base = test_home::scratch("w7");
    let want = Said::from(copy_core::copy_text("beOwnState.path.noHome", &[]));
    for own in [Own::issue_data_at(&base), Own::issue_at(Path::new(""))] {
        assert_eq!(
            own.at_home(".cc-monitor/x")
                .map(|p| p.as_path().to_path_buf()),
            Err(want.clone()),
            "家说不出给出了路径"
        );
        assert_eq!(
            own.home_dir().map(|p| p.as_path().to_path_buf()),
            Err(want.clone()),
            "家说不出给出了 `.cc-monitor` 那一层"
        );
    }
    let me =
        guard_core::production_code(include_str!("../../../../src/backend/platform/fs/own.rs"));
    // 按词数 `self.home`（后面接的不是名字字符）：另起的同前缀字段不算。
    let reads = me
        .match_indices("self.home")
        .filter(|(i, s)| {
            !me[i + s.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .count();
    assert_eq!(reads, 1, "家那一格只该递给 `under_home` 一处");
    assert_eq!(
        me.matches("under_home(self.home.as_deref()").count(),
        1,
        "家那一格不再交给 `platform::paths::under_home` 判"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 凭据文件 · MCP 登录文件是符号链接（号目录里链到别处那一份）⇒ 两只窄句柄的整份写都写到链接指着的那一份：
/// 链接还是链接、指向不变；那一份换成新内容；号目录与那一份旁边都不留旁名。链到链的照一串解到底；相对链接照链接所在目录解。
#[cfg(unix)]
#[test]
fn w8_a_linked_creds_file_is_written_through_to_what_it_points_at() {
    let base = test_home::scratch("w8");
    let acct = base.join("acct");
    let shared = base.join("shared");
    std::fs::create_dir_all(&acct).unwrap();
    std::fs::create_dir_all(&shared).unwrap();
    let real = shared.join("real.json");
    std::fs::write(&real, b"old").unwrap();
    // 号目录里：creds.json → ../shared/real.json（相对）；logins.json → hop.json → 绝对的 real.json（两跳）。
    std::os::unix::fs::symlink("../shared/real.json", acct.join("creds.json")).unwrap();
    std::os::unix::fs::symlink(&real, acct.join("hop.json")).unwrap();
    std::os::unix::fs::symlink("hop.json", acct.join("logins.json")).unwrap();
    let own = Own::for_tests();

    let c = own
        .account_creds(&acct, "creds.json", None, None, None)
        .expect("句柄");
    let held = c.lock().expect("目录锁");
    c.write(&held, b"via-creds").expect("写");
    drop(held);
    let link = acct.join("creds.json");
    assert!(
        std::fs::symlink_metadata(&link).unwrap().is_symlink(),
        "凭据链接被换成了普通文件"
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        Path::new("../shared/real.json")
    );
    assert_eq!(std::fs::read(&real).unwrap(), b"via-creds");

    let f = own
        .mcp_logins_file(&acct, "logins.json", &[])
        .expect("句柄");
    let held = f.lock().expect("目录锁");
    f.write(&held, b"via-logins").expect("写");
    drop(held);
    for l in ["logins.json", "hop.json"] {
        assert!(
            std::fs::symlink_metadata(acct.join(l))
                .unwrap()
                .is_symlink(),
            "{l} 被换成了普通文件"
        );
    }
    assert_eq!(std::fs::read(&real).unwrap(), b"via-logins");
    assert!(side_files(&acct).is_empty(), "号目录里留了旁名");
    assert!(side_files(&shared).is_empty(), "那一份旁边留了旁名");
    std::fs::remove_dir_all(&base).ok();
}
