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

/// 第一句等就绪那一下的期限归发起方（B4）：等到请求期限换来的截止时刻为止；发起方放手（撤单 · 断线：持着的那一头丢了）⇒ 当场停、不送字。
/// 没等到 ⇒ `refused` ＋ `not_ready`；送字那一下一次都不叫。
#[test]
fn the_first_prompt_waits_only_as_long_as_the_caller_gives_and_never_goes_in_after_a_cancel() {
    use crate::platform::child::{Deadline, Until};
    use std::time::{Duration, Instant};
    let home = std::env::temp_dir().join(format!("ccm-first-say-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(crate::observe::watcher::pidfile_dir(&home)).unwrap();
    let sid = "bbbbbbbb-1111-2222-3333-444444444444";
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
        .expect("起运行时");
    let sent = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let run = |until: Until, gone: tokio::sync::oneshot::Receiver<()>| {
        let (home, sent) = (home.clone(), sent.clone());
        rt.spawn_blocking(move || {
            let ready = crate::observe::one_wait::session_ready(&home, sid);
            first_say_with(ready, until, gone, || {
                sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                session_new::FirstSay {
                    result: "delivered".into(),
                    why: None,
                    said: None,
                }
            })
        })
    };
    // 到期：发起方给 300 毫秒 ⇒ 等满再回，不早退、不拖。
    let (stay, gone) = tokio::sync::oneshot::channel::<()>();
    let t0 = Instant::now();
    let got = rt
        .block_on(run(Until::after(Deadline::millis(300)).unwrap(), gone))
        .unwrap();
    let took = t0.elapsed();
    drop(stay);
    assert_eq!(got.result, "refused");
    assert_eq!(got.why.as_deref(), Some("not_ready"));
    assert!(took >= Duration::from_millis(250), "没到期就回了：{took:?}");
    assert!(
        took < Duration::from_secs(5),
        "过了发起方的期限还在等：{took:?}"
    );
    // 撤单：期限还远（30 秒），持着的那一头一丢 ⇒ 当场停。
    let (stay, gone) = tokio::sync::oneshot::channel::<()>();
    let t0 = Instant::now();
    let h = run(Until::after(Deadline::millis(30_000)).unwrap(), gone);
    std::thread::sleep(Duration::from_millis(100));
    drop(stay);
    let got = rt.block_on(h).unwrap();
    let took = t0.elapsed();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(got.result, "refused");
    assert_eq!(got.why.as_deref(), Some("not_ready"));
    assert!(took < Duration::from_secs(5), "撤了单还在等：{took:?}");
    assert_eq!(
        sent.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "没就绪 · 撤了单还送了字"
    );
}

/// 带第一句却没给请求期限（`within_ms`）⇒ `bad_args`，什么都不起：等就绪的期限只由发起方给，后端不替它定一个。
#[test]
fn a_first_prompt_without_the_callers_deadline_is_refused_before_anything_starts() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("起运行时");
    let args = serde_json::json!({"agent": "claude", "cwd": "/nowhere", "place": "tmux", "local": false, "prompt": "你好"});
    let e = rt
        .block_on(answer(args, None, &crate::stream::inbound::LocalFiles))
        .expect_err("不收");
    assert_eq!(e.0, "bad_args", "{e:?}");
}

/// 后端不替发起方定等多久：这一份生产段里没有自定的期限常量（`…_MS` · `…WITHIN…`）。
#[test]
fn the_host_keeps_no_deadline_of_its_own() {
    let path = crate::guard_support::src_root().join("faces/session_new_face.rs");
    let src = std::fs::read_to_string(&path).unwrap();
    let prod = crate::guard_support::production_side_of(&path, &src);
    let own: Vec<&str> = prod
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            (l.starts_with("const ")
                || l.starts_with("pub(crate) const ")
                || l.starts_with("pub const "))
                && (l.contains("_MS") || l.contains("WITHIN"))
        })
        .collect();
    assert!(own.is_empty(), "起会话那一份自定了期限：{own:?}");
}
