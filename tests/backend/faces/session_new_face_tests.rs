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
            RecentDir {
                cwd: "/a".into(),
                last_ms: 40
            },
            RecentDir {
                cwd: "/b".into(),
                last_ms: 30
            }
        ]
    );
    let many: Vec<(String, i64)> = (0..20).map(|i| (format!("/d{i}"), i)).collect();
    let got = recent_dirs(&many, &|_| false);
    assert_eq!(got.len(), RECENT_MAX);
    assert_eq!(got[0].cwd, "/d19");
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
    let here = cfg!(windows);
    assert!(launcher_found_in("tool-x", Some(&login), here));
    assert!(!launcher_found_in("tool-x", Some(&bare), here));
    assert!(
        launcher_found_in("tool-x", None, here),
        "判不了被当成了没装"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 缺陷（10-10）：起新会话框在 Windows 上只认 `.exe`（npm 全局的 `.cmd` / `.ps1` 认不出）。
/// 它与足迹「装没装」问同一个函数（`platform::shell::find_session_command`）⇒ Windows 那一形下 `.ps1` 也算能起，unix 那一形不算。
#[test]
fn a_windows_launcher_counts_the_powershell_forms() {
    let d = std::env::temp_dir().join(format!("ccm-launcher-win-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("tool-z.ps1"), "x").unwrap();
    let path = d.display().to_string();
    assert!(
        launcher_found_in("tool-z", Some(&path), true),
        "Windows 那一形下 .ps1 没认出来"
    );
    assert!(
        !launcher_found_in("tool-z", Some(&path), false),
        "unix 那一形下认了带后缀的"
    );
    let _ = std::fs::remove_dir_all(&d);
}

impl crate::guard_support::Shaped for NewFacts {
    fn samples() -> Vec<Self> {
        vec![NewFacts {
            recent: vec![RecentDir {
                cwd: "/home/u/proj".into(),
                last_ms: 1,
            }],
            tmux: true,
            agents: vec!["claude"],
            fork: Some(ForkFacts {
                agent: "claude",
                launch: serde_json::json!({ "cwd": { "kind": "unknown", "why": "gone" } }),
                turn: Some(3),
                start_text: Some("09:30".into()),
            }),
        }]
    }
}
