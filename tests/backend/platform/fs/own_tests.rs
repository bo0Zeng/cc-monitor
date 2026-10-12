//! **自家状态的写句柄**（`Own` · `OwnPath` · `OwnLock`）—— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | W1 | `OwnPath` 只从根往下拼：绝对路径 · `..` · 空名都拒 | 纯函数 |
//! | W2 | 建目录：新建 0700；已在的不动；父目录不在照实报错 | 真目录 |
//! | W3 | 原子写要那个目录的锁：写完读回逐字同、0600；拿的是别的目录的锁 ⇒ 拒、一个字节不落 | 真文件 |
//! | W4 | 删文件 · 改名都只在根底下 | 真文件 |
//! | W5 | 家目录那一根只给 `.cc-monitor` 底下：别的打头 · 家目录本身 · 往上跳都拒 | 纯函数 |

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
fn w4_remove_and_rename_stay_under_the_root() {
    let base = sandbox("w4");
    let own = Own::issue_data_at(&base);
    let a = own.path("a").expect("名");
    let b = own.path("b").expect("名");
    std::fs::write(a.as_path(), b"1").expect("夹具");
    own.move_file(&a, &b).expect("改名");
    assert!(!a.as_path().exists() && b.as_path().exists());
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
