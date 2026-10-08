//! 起新会话框要的这台事实：最近用过的目录怎么排 · 这台能起哪几家。

use super::*;

#[test]
fn recent_dirs_are_newest_first_once_each_and_skip_the_hidden_one() {
    let items: Vec<(String, i64)> = vec![
        ("/a".into(), 10),
        ("/b".into(), 30),
        ("/a".into(), 40),
        ("".into(), 99),
        ("/hidden".into(), 100),
    ];
    let got = recent_dirs(&items, &|c| c == "/hidden");
    assert_eq!(
        got,
        vec![
            json!({ "cwd": "/a", "lastMs": 40 }),
            json!({ "cwd": "/b", "lastMs": 30 })
        ]
    );
    let many: Vec<(String, i64)> = (0..20).map(|i| (format!("/d{i}"), i)).collect();
    let got = recent_dirs(&many, &|_| false);
    assert_eq!(got.len(), RECENT_MAX);
    assert_eq!(got[0]["cwd"], "/d19");
}

#[test]
fn only_the_families_whose_launcher_this_machine_finds_are_offered() {
    let all = crate::agents::launchable_kinds();
    assert!(all.len() > 1, "注册表里由我们起的不止一家");
    assert_eq!(launchable_here(&|_| true), all);
    assert!(launchable_here(&|_| false).is_empty());
    let first = all[0];
    let launcher = crate::agents::pick_kind(Some(first))
        .unwrap()
        .1
        .default_launcher;
    assert_eq!(launchable_here(&|l| l == launcher), vec![first]);
}

/// ★ 「这台能起」按起会话那个 shell 的 `PATH` 判（与足迹里「装没装」同一个查法）：
/// 那里找得到 ⇒ 能起；那里也没有 ⇒ 不能起；问不出那个 shell 的 `PATH` ⇒ 不藏（判不了不当成没装）。
#[test]
fn a_launcher_is_found_in_the_session_shell_path() {
    let d = std::env::temp_dir().join(format!("ccm-launcher-found-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let user_bin = d.join("user-bin");
    let sys_bin = d.join("sys-bin");
    std::fs::create_dir_all(&user_bin).unwrap();
    std::fs::create_dir_all(&sys_bin).unwrap();
    let exe = user_bin.join(if cfg!(windows) {
        "tool-x.exe"
    } else {
        "tool-x"
    });
    std::fs::write(&exe, "x").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let joined = std::env::join_paths([&sys_bin, &user_bin]).unwrap();
    let login = joined.to_string_lossy().into_owned();
    let bare = sys_bin.display().to_string();
    assert!(launcher_found_in("tool-x", Some(&login)));
    assert!(!launcher_found_in("tool-x", Some(&bare)));
    assert!(launcher_found_in("tool-x", None), "判不了被当成了没装");
    let _ = std::fs::remove_dir_all(&d);
}
