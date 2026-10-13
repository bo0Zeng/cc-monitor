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
    // 此刻 ＝ 1970-01-01 00:01（UTC）：40 ms、30 ms 都在当天 ⇒ 时刻字只写 `HH:MM`。
    let clock = crate::common::time::TextClock::new(60, &crate::Tz::default());
    let got = recent_dirs(&items, &|c| c == "/hidden", &clock);
    assert_eq!(
        got,
        vec![
            RecentDir {
                cwd: "/a".into(),
                last_ms: 40,
                last_text: crate::common::cells::Words("00:00".into()),
            },
            RecentDir {
                cwd: "/b".into(),
                last_ms: 30,
                last_text: crate::common::cells::Words("00:00".into()),
            }
        ]
    );
    let many: Vec<(String, i64)> = (0..20).map(|i| (format!("/d{i}"), i)).collect();
    let got = recent_dirs(&many, &|_| false, &clock);
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

/// ★ 起会话框里每一家带写好的名字，和那一家可选的模型 · 权限档（值 ＋ 写好的字）：值就是交回 `session-new` 的那一格，
/// 字由后端写（界面零拼）；没声明的那一家 ⇒ 两张表都空（不是 `null`：界面照空表不画那颗 chip）。
#[test]
fn each_agent_comes_with_its_name_and_its_models_and_modes_in_words() {
    let claude = agent_facts("claude");
    assert_eq!(claude.kind, "claude");
    assert_eq!(claude.text.0, "Claude Code");
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let models: Vec<(&str, String)> = claude
        .models
        .iter()
        .map(|c| (c.value, c.text.0.clone()))
        .collect();
    assert_eq!(
        models,
        vec![
            ("sonnet", t("beSessionNew.model.sonnet")),
            ("opus", t("beSessionNew.model.opus")),
            ("haiku", t("beSessionNew.model.haiku")),
            ("sonnet[1m]", t("beSessionNew.model.sonnetLong")),
            ("opusplan", t("beSessionNew.model.opusPlan")),
        ]
    );
    // 权限档的字与答批计划那一组同字（同一件事一个说法），「规划模式」是新字。
    let modes: Vec<(&str, String)> = claude
        .permission_modes
        .iter()
        .map(|c| (c.value, c.text.0.clone()))
        .collect();
    assert_eq!(
        modes,
        vec![
            ("default", t("beAsk.choice.planManual")),
            ("acceptEdits", t("beAsk.choice.planEdits")),
            ("plan", t("beSessionNew.mode.plan")),
        ]
    );
    // 每一个值都是 `session-new` 收的那一格：模型过全仓模型名判，权限档是那一家起会话认的档。
    for c in &claude.models {
        assert!(shell_quote_core::model_name_ok(c.value), "{}", c.value);
    }
    let codex = agent_facts("codex");
    assert_eq!(codex.text.0, "Codex");
    assert!(codex.models.is_empty() && codex.permission_modes.is_empty());
}

/// ★ 线上那一形：`agents` 每项 `{kind, text, models, permissionModes}`，`recent` 每项 `{cwd, lastMs, lastText}`。
#[test]
fn the_new_session_facts_wire_names_its_cells() {
    let v = serde_json::to_value(agent_facts("claude")).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["kind", "models", "permissionModes", "text"]);
    assert_eq!(
        v["models"][0],
        serde_json::json!({"value": "sonnet", "text": copy_core::copy_text("beSessionNew.model.sonnet", &[])})
    );
    let clock = crate::common::time::TextClock::new(60, &crate::Tz::default());
    let r = serde_json::to_value(&recent_dirs(&[("/a".into(), 1)], &|_| false, &clock)[0]).unwrap();
    assert_eq!(
        r,
        serde_json::json!({"cwd": "/a", "lastMs": 1, "lastText": "00:00"})
    );
}

impl crate::guard_support::Shaped for NewFacts {
    fn samples() -> Vec<Self> {
        vec![NewFacts {
            recent: vec![RecentDir {
                cwd: "/home/u/proj".into(),
                last_ms: 1,
                last_text: crate::common::cells::Words("00:00".into()),
            }],
            tmux: true,
            agents: vec![agent_facts("claude")],
            fork: Some(ForkFacts {
                agent: "claude",
                launch: crate::control::fork_launch::infer(&crate::control::fork_launch::Source {
                    live: true,
                    cwd: Some("/home/u/proj".into()),
                    account: Some(crate::control::fork_launch::AccountFact::Base),
                    terminal: Some("tmux-3-7".into()),
                }),
                turn: Some(3),
                start_text: Some("09:30".into()),
            }),
        }]
    }
}
