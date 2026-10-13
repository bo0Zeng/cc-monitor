//! 读盘那一半：三个根的判法（纯）· 临时家目录里真建一套账号库、读成快照逐格对（只在 unix：账号库靠符号链接与权限位）。
//! 夹具全是合成的（假邮箱 · 假号名），家目录是这一份测试自己的临时目录。
use super::*;

fn roots(home: &str, shared: &str, accts: &str) -> Roots {
    Roots {
        home: home.into(),
        shared: shared.into(),
        accts: accts.into(),
    }
}

/// 按路径段判包含，不按字符前缀；`join` 去掉左边的尾部 `/`。
#[test]
fn is_under_goes_by_path_segments() {
    assert!(is_under("/h/a", "/h"));
    assert!(is_under("/h", "/h/"));
    assert!(is_under("/h/a/b", "/h/a"));
    assert!(!is_under("/hx", "/h"), "字符前缀不算");
    assert!(!is_under("/h", "/h/a"));
    assert_eq!(join("/h/", "x"), "/h/x");
    assert_eq!(join("/h", "x/y"), "/h/x/y");
    assert_eq!(
        accts_root("/h/"),
        join("/h", relay_route_core::ACCOUNTS_DIR_REL)
    );
}

/// 三个根能不能动：共享库与账号库都在家目录底下、不是家目录本身、互不包含。
#[test]
fn roots_are_refused_outside_home_or_nested() {
    let ok = roots("/h", "/h/.claude", "/h/.cc-monitor/accounts");
    assert_eq!(ok.check(), Ok(()));
    assert_eq!(
        ok.manifest(),
        join("/h/.cc-monitor/accounts", MANIFEST_FILE)
    );
    for bad in [
        roots("/h", "/elsewhere/.claude", "/h/.cc-monitor/accounts"),
        roots("/h", "/h", "/h/.cc-monitor/accounts"),
        roots("/h", "/h/.claude", "/other/accounts"),
        roots("/h", "/h/.claude", "/h/.claude/accounts"),
        roots("/h", "/h/x/.claude", "/h/x"),
    ] {
        assert!(bad.check().is_err(), "{bad:?}");
    }
}

/// 归本模块动的号目录：账号库里面的（不含账号库本身），或者就是共享库本身（旧的 in-place 号）。
#[test]
fn controls_covers_dirs_inside_the_store_and_the_shared_root_itself() {
    let r = roots("/h", "/h/.claude", "/h/.cc-monitor/accounts");
    assert!(r.controls("/h/.cc-monitor/accounts/z"));
    assert!(r.controls("/h/.claude"));
    assert!(!r.controls("/h/.cc-monitor/accounts"));
    assert!(!r.controls("/h/.claude/sub"));
    assert!(!r.controls("/h/other"));
}

/// 快照的几个便捷问：没登记的目录 ⇒ 空的样子；没登记的子项 ⇒ 不在。
#[test]
fn snapshot_lookups_default_to_absent() {
    let s = Snapshot::default();
    assert_eq!(s.dir("/nope"), DirState::default());
    assert!(!s.dir("/nope").exists());
    assert_eq!(s.dir("/nope").get("x"), &Item::Absent);
    assert_eq!(s.email_of("/nope"), None);
    assert!(s.manifest().is_none());
}

#[cfg(unix)]
mod on_disk {
    use super::super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    struct Home(PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn home(tag: &str) -> Home {
        let p = std::env::temp_dir().join(format!(
            "acct-scan-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Home(p)
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn s(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    /// 什么都没建：清单不在 ⇒ `None`（不是错）；共享库落默认那一处；三个根齐。
    #[test]
    fn an_empty_home_scans_to_an_absent_store() {
        let h = home("empty");
        let hs = s(&h.0);
        let snap = scan(&hs, &[], &[]);
        assert!(snap.manifest.is_none() && snap.manifest_text.is_none());
        assert!(!snap.accts.exists());
        let r = snap.roots();
        assert_eq!(r.home, hs);
        assert_eq!(r.accts, accts_root(&hs));
        assert_eq!(r.shared, super::super::super::layout::shared_root_in(&hs));
        assert!(snap.backups.is_empty() && snap.dirs.is_empty());
    }

    /// 建好一套：清单里那个号的目录与子项（不跟链接）· 共享库换成清单点名的那一处 · 邮箱 ·
    /// 另点名的文件 · 备份（撤销清单读出 · 还原过的标记 · 名字对但不是目录的那一项不算）。
    #[test]
    fn a_built_store_scans_to_every_cell() {
        let h = home("built");
        let hs = s(&h.0);
        let accts = PathBuf::from(accts_root(&hs));
        let shared = h.0.join("store");
        let z = accts.join("z");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(z.join("projects")).unwrap();
        std::fs::set_permissions(&z, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink(shared.join("skills"), z.join("skills")).unwrap();
        write(
            &z.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"z@example.test"}}"#,
        );
        let manifest = format!(
            r#"{{"version":1,"sharedStore":"{}/","accounts":[{{"name":"z","email":"","configDir":"{}","isDefault":true}}]}}"#,
            s(&shared),
            s(&z)
        );
        write(&accts.join(MANIFEST_FILE), &manifest);
        write(
            &accts
                .join(format!("{BACKUP_PREFIX}20260101-000000"))
                .join(UNDO_NAME),
            "DELETE\t/x\n",
        );
        let rolled = accts.join(format!("{BACKUP_PREFIX}20260102-000000"));
        write(&rolled.join(ROLLED_BACK_NAME), "");
        write(&accts.join(format!("{BACKUP_PREFIX}not-a-dir")), "");
        let cred = s(&h.0.join("cred.json"));
        write(Path::new(&cred), "{}");

        let snap = scan(&hs, &["/nowhere/extra".into()], &[cred.clone()]);
        assert_eq!(snap.manifest_text.as_deref(), Some(manifest.as_str()));
        assert_eq!(snap.manifest().unwrap().managed().count(), 1);
        assert_eq!(
            snap.roots().shared,
            s(&shared),
            "清单点名的共享库（尾部 `/` 去掉）"
        );
        assert!(snap.shared.is_dir());
        let zd = snap.dir(&s(&z));
        assert!(zd.is_dir());
        assert_eq!(zd.mode().map(|m| m & 0o777), Some(0o700));
        assert!(matches!(zd.get("projects"), Item::Dir { .. }));
        assert!(
            matches!(zd.get("skills"), Item::Link { dangling: true, .. }),
            "链接不跟过去：{:?}",
            zd.get("skills")
        );
        assert!(
            !snap.dir("/nowhere/extra").exists(),
            "另点名的目录照样登记，不在就是不在"
        );
        assert!(snap.dirs.contains_key("/nowhere/extra"));
        assert_eq!(snap.email_of(&s(&z)), Some("z@example.test"));
        assert_eq!(snap.email_of(&hs), None);
        assert!(snap.files[&cred].exists());

        let backups: Vec<_> = snap
            .backups
            .iter()
            .map(|b| (b.id.as_str(), b.undo.as_deref(), b.rolled_back))
            .collect();
        assert_eq!(
            backups,
            [
                ("20260101-000000", Some("DELETE\t/x\n"), false),
                ("20260102-000000", None, true)
            ]
        );
    }

    /// 清单在却读不动（不是 UTF-8）⇒ 记成 `Some(Err)`、原文 `None`，由用的那一步报；共享库落默认那一处。
    #[test]
    fn an_unreadable_manifest_is_recorded_not_raised() {
        let h = home("bad");
        let hs = s(&h.0);
        let accts = PathBuf::from(accts_root(&hs));
        std::fs::create_dir_all(&accts).unwrap();
        std::fs::write(accts.join(MANIFEST_FILE), [0xff, 0xfe, 0x00]).unwrap();
        let snap = scan(&hs, &[], &[]);
        assert!(snap.manifest_text.is_none());
        assert!(matches!(snap.manifest, Some(Err(_))));
        assert!(snap.manifest().is_none());
        assert_eq!(
            snap.roots().shared,
            super::super::super::layout::shared_root_in(&hs)
        );
        assert!(manifest_text_at(&accts.join(MANIFEST_FILE)).is_err());
    }
}
