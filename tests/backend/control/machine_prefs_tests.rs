//! 这台的偏好（`prefs.json`）：读的三态 · 补丁只动点到的那几格 · 值域 · 读不懂不盖。家目录在临时目录里。
use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-prefs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set(home: &Path, req: serde_json::Value) -> Result<PrefsReply, PrefsSetErr> {
    let own = crate::platform::fs::Own::issue_at(home);
    let req: PrefsSet = serde_json::from_value(req).unwrap();
    answer_set_in(Some(home), &own, &req)
}

/// 没设过 ⇒ `absent`、缺省那一份；写一回 ⇒ `ok`、读回的就是写进去的；补丁只动点到的那几格（别的号 · 别的家不动，`null` 删掉那一条）。
#[test]
fn prefs_start_absent_and_a_patch_touches_only_what_it_names() {
    let home = scratch("patch");
    let r = answer_read_in(Some(&home));
    assert_eq!((r.state, r.prefs.clone()), ("absent", Prefs::default()));
    set(
        &home,
        serde_json::json!({"launchers": {"claude": "cc"}, "models": {"a": "opus", "b": "sonnet"}, "contextLimits": {"opus": 1000000}, "agentHomes": {"claude": "/srv/c"}}),
    )
    .unwrap();
    let r = set(
        &home,
        serde_json::json!({"models": {"a": null}, "resumeInTmux": true, "launchers": {"codex": "cx"}, "agentHomes": {"codex": "/srv/x"}}),
    )
    .unwrap();
    assert_eq!(r.state, "ok");
    assert_eq!(
        serde_json::to_value(&r.prefs).unwrap(),
        serde_json::json!({
            "launchers": {"claude": "cc", "codex": "cx"},
            "resumeInTmux": true,
            "models": {"b": "sonnet"},
            "contextLimits": {"opus": 1000000},
            "childTab": "hide",
            "bgTab": "show",
            "agentHomes": {"claude": "/srv/c", "codex": "/srv/x"},
        })
    );
    assert_eq!(
        answer_read_in(Some(&home)).prefs,
        r.prefs,
        "读回的与写的不同"
    );
    // 空串 ⇒ 那一家回到默认。
    let r = set(&home, serde_json::json!({"launchers": {"claude": ""}})).unwrap();
    assert_eq!(r.prefs.launcher_for("claude", "claude"), "claude");
    assert_eq!(r.prefs.launcher_for("codex", "codex"), "cx");
    let _ = std::fs::remove_dir_all(&home);
}

/// 值域在写口拒（`bad_args`，一个字节不落）：启动命令带壳元字符 · 模型名不像模型 · 上限为 0 · 数据目录不是绝对路径。
#[test]
fn prefs_set_refuses_values_a_launch_could_not_use() {
    let home = scratch("check");
    for bad in [
        serde_json::json!({"launchers": {"claude": "cc; rm -rf /"}}),
        serde_json::json!({"models": {"a": "op us"}}),
        serde_json::json!({"contextLimits": {"opus": 0}}),
        serde_json::json!({"agentHomes": {"claude": "relative/dir"}}),
    ] {
        let e = set(&home, bad.clone()).unwrap_err();
        assert!(matches!(e, PrefsSetErr::BadArgs(_)), "{bad} ⇒ {e:?}");
    }
    assert_eq!(answer_read_in(Some(&home)).state, "absent", "拒了还落了盘");
    let _ = std::fs::remove_dir_all(&home);
}

/// 盘上那份读不懂（不是 JSON · 版本不认 · 多一格）⇒ 读报 `unreadable`、照缺省用；写拒 `unreadable`、原文一个字节不动。
#[test]
fn an_unreadable_prefs_file_is_reported_and_never_overwritten() {
    let home = scratch("unreadable");
    let path = path_in(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for body in [
        "{not json".to_string(),
        r#"{"version":2,"launchers":{},"resumeInTmux":false,"models":{},"contextLimits":{}}"#.to_string(),
        r#"{"version":1,"launchers":{},"resumeInTmux":false,"models":{},"contextLimits":{},"more":1}"#.to_string(),
    ] {
        std::fs::write(&path, &body).unwrap();
        let r = answer_read_in(Some(&home));
        assert_eq!((r.state, r.prefs.clone()), ("unreadable", Prefs::default()), "{body}");
        assert!(r.reason.is_some(), "{body}");
        let e = set(&home, serde_json::json!({"resumeInTmux": true})).unwrap_err();
        assert!(matches!(e, PrefsSetErr::Unreadable(_)), "{body} ⇒ {e:?}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "读不懂的那份被盖了");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 子会话 · 分身会话那两格：缺省子会话隐藏、分身显示；盘上没写 ⇒ 缺省；`prefs-set` 只动给了的那一格。
#[test]
fn the_two_tab_rules_default_child_hidden_bg_shown_and_patch_one_at_a_time() {
    use cmd_wire::session::Tab;
    let d = Prefs::default();
    assert_eq!((d.child_tab, d.bg_tab), (Tab::Hide, Tab::Show));
    let old: OnDisk =
        serde_json::from_value(serde_json::json!({"version": 1, "resumeInTmux": true})).unwrap();
    assert_eq!(
        (old.prefs.child_tab, old.prefs.bg_tab),
        (Tab::Hide, Tab::Show)
    );
    let set: PrefsSet = serde_json::from_value(serde_json::json!({"bgTab": "hide"})).unwrap();
    let p = patched(d.clone(), &set);
    assert_eq!((p.child_tab, p.bg_tab), (Tab::Hide, Tab::Hide));
    let set: PrefsSet = serde_json::from_value(serde_json::json!({"childTab": "show"})).unwrap();
    assert_eq!(patched(p, &set).rules().child_tab, Tab::Show);
    assert!(serde_json::from_value::<PrefsSet>(serde_json::json!({"bgTab": "maybe"})).is_err());
}

/// 上限表交给会话事实那一形：子串小写。
#[test]
fn context_limits_come_out_lowercased_for_the_facts_scan() {
    let p = Prefs {
        context_limits: [("Opus".to_string(), 500_000)].into_iter().collect(),
        ..Prefs::default()
    };
    assert_eq!(p.limits(), vec![("opus".to_string(), 500_000)]);
}

/// 这台给哪家设了数据目录 ⇒ 起来时交成那一家的目录环境变量（名字问注册表）；注册表里没有 / 没有目录变量的那一家不交。
#[test]
fn the_agent_homes_become_each_familys_dir_env() {
    let kind = crate::agents::default_kind();
    let env = crate::agents::account_env_of(kind).expect("默认那一家有目录变量");
    let homes = BTreeMap::from([
        (kind.to_string(), PathBuf::from("/srv/one")),
        ("nobody".to_string(), PathBuf::from("/srv/two")),
    ]);
    assert_eq!(
        agent_home_envs(homes),
        vec![(env, PathBuf::from("/srv/one"))]
    );
    assert_eq!(agent_home_envs(BTreeMap::new()), vec![]);
}
