//! 起新会话框要的这台事实（最近用过的目录怎么排 · 这台能起哪几家）· 恢复读记录那一处（终端没了时换的模型 / 权限档在记录没长时盖过记录里的）。

use super::*;
use crate::history::history_annotations::kept_for_resume;

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
                last_text: "00:00".into(),
            },
            RecentDir {
                cwd: "/b".into(),
                last_ms: 30,
                last_text: "00:00".into(),
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
    let claude = agent_facts("claude", &Default::default(), &|_| false);
    assert_eq!(claude.kind, "claude");
    assert_eq!(claude.text, "Claude Code");
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let models: Vec<(&str, String)> = claude
        .models
        .iter()
        .map(|c| (c.value.as_str(), c.text.clone()))
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
        .map(|c| (c.value.as_str(), c.text.clone()))
        .collect();
    assert_eq!(
        modes,
        vec![
            ("default", t("beAsk.choice.planManual")),
            ("acceptEdits", t("beAsk.choice.planEdits")),
            ("plan", t("beSessionNew.mode.plan")),
            ("auto", t("beAsk.choice.planAuto")),
        ]
    );
    // 每一个值都是 `session-new` 收的那一格：模型过全仓模型名判，权限档是那一家起会话认的档。
    for c in &claude.models {
        assert!(shell_quote_core::model_name_ok(&c.value), "{}", c.value);
    }
    let codex = agent_facts("codex", &Default::default(), &|_| false);
    assert_eq!(codex.text, "Codex");
    assert!(codex.models.is_empty() && codex.permission_modes.is_empty());
}

/// ★ 换模型抽屉的第一项「默认」：回到那一家缺省模型的那个值 ＋ 写好的字；抽屉底那一句（换模型的副作用，那一家自己的行为）也由后端写。
/// 没声明的那一家 ⇒ 两格都 `null`（界面不画那一项、那一行）。
#[test]
fn each_agent_says_how_to_go_back_to_its_default_model_and_what_a_switch_also_changes() {
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let claude = agent_facts("claude", &Default::default(), &|_| false);
    let d = claude.model_default.expect("Claude 有「默认」那一项");
    assert_eq!(
        (d.value.as_str(), d.text),
        ("default", t("beSessionNew.model.default"))
    );
    assert!(
        claude.models.iter().all(|c| c.value != "default"),
        "「默认」不混进可选的模型里（起会话不选 ＝ 默认，不交这个值）"
    );
    assert_eq!(
        claude.model_note,
        Some(t("beSessionModel.note.savesDefault"))
    );
    // 那份设置各号共用（问的就是换模型写的那一份）⇒ 那一句说「各号」。
    let asked = std::cell::RefCell::new(Vec::new());
    let shared = agent_facts("claude", &Default::default(), &|f: &str| {
        asked.borrow_mut().push(f.to_string());
        true
    });
    assert_eq!(
        shared.model_note,
        Some(t("beSessionModel.note.savesDefaultShared"))
    );
    assert_eq!(*asked.borrow(), vec!["settings.json".to_string()]);
    let codex = agent_facts("codex", &Default::default(), &|_| false);
    assert!(codex.model_default.is_none() && codex.model_note.is_none());
}

/// ★ 线上那一形：`agents` 每项 `{kind, text, models, modelDefault, modelNote, permissionModes, permissionDefault}`，`recent` 每项 `{cwd, lastMs, lastText}`。
#[test]
fn the_new_session_facts_wire_names_its_cells() {
    let v = serde_json::to_value(agent_facts("claude", &Default::default(), &|_| false)).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec![
            "kind",
            "modelDefault",
            "modelNote",
            "models",
            "permissionDefault",
            "permissionModes",
            "text"
        ]
    );
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

/// ★ **P3** 每家的 `permissionDefault`：这台偏好里设了、且还在那一家清单里 ⇒ 那一项；没设 · 清单里没了 · 那一家没有权限档 ⇒ `null`。
#[test]
fn the_permission_default_follows_the_preference_while_the_agent_still_offers_it() {
    let prefs = |kind: &str, mode: &str| crate::control::machine_prefs::Prefs {
        permission_modes: [(kind.to_string(), mode.to_string())].into_iter().collect(),
        ..Default::default()
    };
    let got = agent_facts("claude", &prefs("claude", "plan"), &|_| false).permission_default;
    assert_eq!(got.map(|p| p.value), Some("plan".to_string()));
    assert_eq!(
        agent_facts("claude", &Default::default(), &|_| false).permission_default,
        None
    );
    assert_eq!(
        agent_facts("claude", &prefs("claude", "yolo"), &|_| false).permission_default,
        None,
        "清单里没了 ⇒ 当没设"
    );
    assert_eq!(
        agent_facts("codex", &prefs("codex", "plan"), &|_| false).permission_default,
        None
    );
}

fn of_record() -> ResumeOf {
    ResumeOf {
        agent: "claude".into(),
        cwd: "/w".into(),
        model: Some("claude-opus-4-5-20251101".into()),
        permission_mode: Some("acceptEdits".into()),
    }
}

fn pick(model: Option<&str>, mode: Option<&str>, end: u64) -> ResumeWith {
    ResumeWith {
        model: model.map(str::to_string),
        permission_mode: mode.map(str::to_string),
        end,
    }
}

/// ★ 记下 `opus` 且记录没长 ⇒ 恢复带 `opus`；记录追加一行 ⇒ 那一份作废、照记录；记下「默认」⇒ 不带模型；
/// 只记了权限档 ⇒ 模型照记录。
#[test]
fn a_kept_pick_wins_until_the_record_grows() {
    let d = std::env::temp_dir().join(format!("ccm-open-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let rec = d.join("s.jsonl");
    std::fs::write(&rec, "{\"i\":0}\n").unwrap();
    let len = std::fs::metadata(&rec).unwrap().len();

    let kept = kept_for_resume(Some(pick(Some("opus"), Some("plan"), len)), &rec);
    let r = with_kept(of_record(), kept);
    assert_eq!(r.model.as_deref(), Some("opus"));
    assert_eq!(r.permission_mode.as_deref(), Some("plan"));

    let r = with_kept(
        of_record(),
        kept_for_resume(Some(pick(Some("default"), None, len)), &rec),
    );
    assert_eq!(r.model, None, "默认那一项不带模型");
    assert_eq!(r.permission_mode.as_deref(), Some("acceptEdits"));

    let r = with_kept(
        of_record(),
        kept_for_resume(Some(pick(None, Some("plan"), len)), &rec),
    );
    assert_eq!(r.model.as_deref(), Some("claude-opus-4-5-20251101"));

    std::fs::write(&rec, "{\"i\":0}\n{\"i\":1}\n").unwrap();
    let grown = kept_for_resume(Some(pick(Some("opus"), Some("plan"), len)), &rec);
    assert_eq!(grown, None);
    let r = with_kept(of_record(), grown);
    assert_eq!(r.model.as_deref(), Some("claude-opus-4-5-20251101"));
    assert_eq!(r.permission_mode.as_deref(), Some("acceptEdits"));
    let _ = std::fs::remove_dir_all(&d);
}
