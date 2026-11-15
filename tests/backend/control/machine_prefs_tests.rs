//! 这台的偏好（`prefs.json`）：读的三态 · 补丁只动点到的那几格 · 值域 · 读不懂不盖。家目录在临时目录里。
use super::*;
use crate::common::machine_prefs::bring_forward;

fn set(home: &Path, req: serde_json::Value) -> Result<PrefsReply, PrefsSetErr> {
    let own = crate::platform::fs::Own::issue_at(home);
    let req: PrefsSet = serde_json::from_value(req).unwrap();
    answer_set_in(Some(home), &own, &req)
}

/// 没设过 ⇒ `absent`、缺省那一份；写一回 ⇒ `ok`、读回的就是写进去的；补丁只动点到的那几格（别的号 · 别的家不动，`null` 删掉那一条）。
#[test]
fn prefs_start_absent_and_a_patch_touches_only_what_it_names() {
    let home = test_home::scratch("patch");
    let r = answer_read_in(Some(&home));
    assert_eq!(
        (r.state, r.prefs.clone()),
        (FileState::Absent, Prefs::default())
    );
    // 上下文上限那一段标题右侧「已改 N」由这里写好（界面照抄）；一条都没覆盖 ⇒ 不写。
    assert_eq!(r.context_limits_text, None);
    assert_eq!((r.reason, r.detail), (None, None), "没设过不说话");
    set(
        &home,
        serde_json::json!({"launchers": {"claude": "cc"}, "models": {"a": "opus", "b": "sonnet"}, "contextLimits": {"opus": 1000000}, "agentHomes": {"claude": "/srv/c"}}),
    )
    .unwrap();
    let r = set(
        &home,
        serde_json::json!({"models": {"a": null}, "resumeWhere": "as_started", "launchers": {"codex": "cx"}, "agentHomes": {"codex": "/srv/x"}}),
    )
    .unwrap();
    assert_eq!(r.state, FileState::Ok);
    assert_eq!(
        r.context_limits_text.as_deref(),
        Some(copy_core::copy_text("contextLimits.editor.count", &[("n", "1")]).as_str())
    );
    assert_eq!(
        serde_json::to_value(&r.prefs).unwrap(),
        serde_json::json!({
            "launchers": {"claude": "cc", "codex": "cx"},
            "resumeWhere": "as_started",
            "models": {"b": "sonnet"},
            "permissionModes": {},
            "contextLimits": {"opus": 1000000},
            "childTab": "hide",
            "bgTab": "show",
            "agentHomes": {"claude": "/srv/c", "codex": "/srv/x"},
            "pbDir": null,
            "planDirs": [],
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

/// 值域在写口拒（`bad_args`，一个字节不落）：启动命令带壳元字符 · 模型名不像模型 · 上限为 0 · 数据目录 / pb 路径不是绝对路径。
#[test]
fn prefs_set_refuses_values_a_launch_could_not_use() {
    let home = test_home::scratch("check");
    for bad in [
        serde_json::json!({"launchers": {"claude": "cc; rm -rf /"}}),
        serde_json::json!({"models": {"a": "op us"}}),
        serde_json::json!({"contextLimits": {"opus": 0}}),
        serde_json::json!({"agentHomes": {"claude": "relative/dir"}}),
        serde_json::json!({"pbDir": "relative/pb"}),
        serde_json::json!({"planDirs": ["relative/w"]}),
        serde_json::json!({"planDirs": [""]}),
        serde_json::json!({"planDirs": (0..=crate::common::machine_prefs::PREFS_ROWS_MAX).map(|i| format!("/w{i}")).collect::<Vec<_>>()}),
    ] {
        let e = set(&home, bad.clone()).unwrap_err();
        assert!(matches!(e, PrefsSetErr::BadArgs(_)), "{bad} ⇒ {e:?}");
    }
    assert_eq!(
        answer_read_in(Some(&home)).state,
        FileState::Absent,
        "拒了还落了盘"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 盘上那份读不懂（不是 JSON · 版本不认 · 多一格）⇒ 读报 `unreadable`、照缺省用；写拒 `unreadable`、原文一个字节不动。
#[test]
fn an_unreadable_prefs_file_is_reported_and_never_overwritten() {
    let home = test_home::scratch("unreadable");
    let path = path_in(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for body in [
        "{not json".to_string(),
        r#"{"version":3,"launchers":{},"resumeWhere":"tmux","models":{},"contextLimits":{}}"#.to_string(),
        r#"{"version":2,"launchers":{},"resumeWhere":"tmux","models":{},"contextLimits":{},"more":1}"#.to_string(),
        r#"{"version":2,"launchers":{},"resumeInTmux":true,"models":{},"contextLimits":{}}"#.to_string(),
        r#"{"version":2,"launchers":{},"resumeWhere":"tmux","models":{},"contextLimits":{},"planDirs":["rel/w"]}"#.to_string(),
    ] {
        std::fs::write(&path, &body).unwrap();
        let r = answer_read_in(Some(&home));
        assert_eq!((r.state, r.prefs.clone()), (FileState::Unreadable, Prefs::default()), "{body}");
        assert!(r.reason.is_some(), "{body}");
        let e = set(&home, serde_json::json!({"resumeWhere": "tmux"})).unwrap_err();
        assert!(matches!(e, PrefsSetErr::Unreadable(_)), "{body} ⇒ {e:?}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "读不懂的那份被盖了");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 盘上第 1 版（缺 `version` 也当 1）经那一步升级读：恢复那一格开关换成三选，别的格原样；读本身不动盘（读不变成写）。
#[test]
fn a_first_version_prefs_file_reads_upgraded_and_reading_never_writes() {
    let home = test_home::scratch("v1");
    let path = path_in(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for (v1, tmux) in [
        (
            r#"{"version":1,"launchers":{"claude":"cc"},"resumeInTmux":true,"models":{},"contextLimits":{}}"#,
            true,
        ),
        (
            r#"{"launchers":{"claude":"cc"},"models":{},"contextLimits":{}}"#,
            false,
        ),
    ] {
        std::fs::write(&path, v1).unwrap();
        let r = answer_read_in(Some(&home));
        assert_eq!((r.state, r.reason.clone()), (FileState::Ok, None), "{v1}");
        assert_eq!(
            r.prefs.launchers.get("claude").map(String::as_str),
            Some("cc"),
            "{v1}"
        );
        let want = if tmux {
            cmd_wire::session::ResumeWhere::Tmux
        } else {
            cmd_wire::session::ResumeWhere::Managed
        };
        assert_eq!(r.prefs.resume_where, want, "{v1}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), v1, "读把盘写了");
        assert!(!path.with_file_name("prefs.json.v1").exists(), "读留了 .v1");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 常驻起来那一下（[`bring_forward`]）把读得懂的旧版写回成这一版：原文照字节留成 `prefs.json.v1`；已是这一版 · 没有 ⇒ 不动盘。
#[test]
fn bringing_forward_writes_the_first_version_once_and_keeps_the_original() {
    let home = test_home::scratch("v1-forward");
    let own = crate::platform::fs::Own::issue_at(&home);
    let path = path_in(&home);
    assert!(!bring_forward(&own).unwrap(), "没有那一份 ⇒ 不写");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let v1 = r#"{"version":1,"launchers":{"claude":"cc"},"resumeInTmux":true,"models":{},"contextLimits":{}}"#;
    std::fs::write(&path, v1).unwrap();
    assert!(bring_forward(&own).unwrap(), "第 1 版 ⇒ 写回");
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], 2);
    assert_eq!(raw["resumeWhere"], "tmux");
    assert_eq!(raw["launchers"]["claude"], "cc");
    assert!(raw.get("resumeInTmux").is_none(), "旧键留下了：{raw}");
    assert_eq!(
        std::fs::read_to_string(path.with_file_name("prefs.json.v1")).unwrap(),
        v1,
        "第 1 版原文没留"
    );
    let now = std::fs::read_to_string(&path).unwrap();
    assert!(!bring_forward(&own).unwrap(), "已是这一版 ⇒ 不写");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), now);
    let _ = std::fs::remove_dir_all(&home);
}

/// 比下限还老（第 0 版）：当没有、照缺省办，回包带那一句（路径在里头）、复制详情码照 absent；同一份那一行日志只记一次。
#[test]
fn a_prefs_file_older_than_the_floor_is_said_and_logged_once() {
    let home = test_home::scratch("v0");
    let path = path_in(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"version":0,"resumeInTmux":true}"#).unwrap();
    let r = answer_read_in(Some(&home));
    assert_eq!(
        (r.state, r.prefs.clone()),
        (FileState::Absent, Prefs::default())
    );
    let want = copy_text(
        "beMachinePrefs.read.older",
        &[("path", &path.display().to_string()), ("mine", "1–2")],
    );
    assert_eq!(r.reason.as_deref(), Some(want.as_str()));
    let detail = r.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("absent") && !detail.contains("unreadable") && detail.contains("format 0"),
        "{detail}"
    );
    let lines = test_meters::heard(|| {
        for _ in 0..3 {
            assert!(matches!(read_at(&path), Loaded::Older(_)));
        }
    });
    let said = lines
        .iter()
        .filter(|l| l.contains("treated as absent") && l.contains(&*path.to_string_lossy()))
        .count();
    assert!(said <= 1, "{lines:#?}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 只有读口说「比下限老」才带那一句：prefs.json 是一条指向不在的目标的链接 ⇒ 就是没有，不说格式旧。
#[cfg(unix)]
#[test]
fn a_dangling_link_is_plain_absent_not_older() {
    let home = test_home::scratch("dangling");
    let path = path_in(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(home.join("nowhere.json"), &path).unwrap();
    assert!(
        matches!(read_at(&path), Loaded::Absent),
        "{:?}",
        read_at(&path)
    );
    let r = answer_read_in(Some(&home));
    assert_eq!((r.state, r.reason), (FileState::Absent, None));
    let _ = std::fs::remove_dir_all(&home);
}

/// 子会话 · 分身会话那两格：缺省子会话隐藏、分身显示；盘上没写 ⇒ 缺省；`prefs-set` 只动给了的那一格。
#[test]
fn the_two_tab_rules_default_child_hidden_bg_shown_and_patch_one_at_a_time() {
    use cmd_wire::session::Tab;
    let d = Prefs::default();
    assert_eq!((d.child_tab, d.bg_tab), (Tab::Hide, Tab::Show));
    let old: Prefs = serde_json::from_value(serde_json::json!({"resumeWhere": "tmux"})).unwrap();
    assert_eq!((old.child_tab, old.bg_tab), (Tab::Hide, Tab::Show));
    let set: PrefsSet = serde_json::from_value(serde_json::json!({"bgTab": "hide"})).unwrap();
    let p = patched(d.clone(), &set);
    assert_eq!((p.child_tab, p.bg_tab), (Tab::Hide, Tab::Hide));
    let set: PrefsSet = serde_json::from_value(serde_json::json!({"childTab": "show"})).unwrap();
    assert_eq!(patched(p, &set).rules().child_tab, Tab::Show);
    assert!(serde_json::from_value::<PrefsSet>(serde_json::json!({"bgTab": "maybe"})).is_err());
}

/// 恢复时放在哪：缺省托管；三个值都收；旧那一格开关（`resumeInTmux`）补丁里不认，别的值不认。
#[test]
fn resume_where_defaults_to_managed_and_takes_only_the_three_words() {
    use cmd_wire::session::ResumeWhere;
    assert_eq!(Prefs::default().resume_where, ResumeWhere::Managed);
    for (w, want) in [
        ("managed", ResumeWhere::Managed),
        ("tmux", ResumeWhere::Tmux),
        ("as_started", ResumeWhere::AsStarted),
    ] {
        let set: PrefsSet = serde_json::from_value(serde_json::json!({"resumeWhere": w})).unwrap();
        assert_eq!(patched(Prefs::default(), &set).resume_where, want, "{w}");
    }
    assert!(serde_json::from_value::<PrefsSet>(serde_json::json!({"resumeInTmux": true})).is_err());
    assert!(
        serde_json::from_value::<PrefsSet>(serde_json::json!({"resumeWhere": "direct"})).is_err()
    );
}

/// 恢复想不想进 tmux：三选 × 启动台账记着的宿主。照起时那一种只认记着 tmux 的；记着托管 / 记不着 ⇒ 不想。
#[test]
fn resume_wants_tmux_follows_the_choice_and_where_the_session_started() {
    use cmd_wire::frame::TerminalHost::{Managed, Tmux};
    use cmd_wire::session::ResumeWhere;
    let with = |w| Prefs {
        resume_where: w,
        ..Prefs::default()
    };
    for (w, started, want) in [
        (ResumeWhere::Managed, Some(Tmux), false),
        (ResumeWhere::Tmux, None, true),
        (ResumeWhere::Tmux, Some(Managed), true),
        (ResumeWhere::AsStarted, Some(Tmux), true),
        (ResumeWhere::AsStarted, Some(Managed), false),
        (ResumeWhere::AsStarted, None, false),
    ] {
        assert_eq!(
            with(w).resume_wants_tmux(started),
            want,
            "{w:?} × {started:?}"
        );
    }
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

/// pb 路径：设了读回的就是它（两头空白去掉）；空串 ⇒ 删掉；补丁里没提 ⇒ 不动。
#[test]
fn the_pb_dir_is_set_trimmed_kept_and_cleared_by_an_empty_string() {
    let home = test_home::scratch("pbdir");
    let r = set(&home, serde_json::json!({"pbDir": "  /opt/pb  "})).unwrap();
    assert_eq!(r.prefs.pb_dir.as_deref(), Some("/opt/pb"));
    let r = set(&home, serde_json::json!({"resumeWhere": "tmux"})).unwrap();
    assert_eq!(r.prefs.pb_dir.as_deref(), Some("/opt/pb"));
    let r = set(&home, serde_json::json!({"pbDir": ""})).unwrap();
    assert_eq!(r.prefs.pb_dir, None);
    let _ = std::fs::remove_dir_all(&home);
}

/// 计划页手加的目录：整节换掉、别的节不动就留着、空表清空；读回的就是写进去的。
#[test]
fn the_plan_dirs_are_replaced_as_a_whole_and_kept_by_other_patches() {
    let home = test_home::scratch("plandirs");
    let r = set(&home, serde_json::json!({"planDirs": ["/w/a", "/w/b"]})).unwrap();
    assert_eq!(r.prefs.plan_dirs, ["/w/a", "/w/b"]);
    let r = set(&home, serde_json::json!({"resumeWhere": "tmux"})).unwrap();
    assert_eq!(r.prefs.plan_dirs, ["/w/a", "/w/b"]);
    assert_eq!(
        answer_read_in(Some(&home)).prefs.plan_dirs,
        ["/w/a", "/w/b"]
    );
    let r = set(&home, serde_json::json!({"planDirs": []})).unwrap();
    assert!(r.prefs.plan_dirs.is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **P2** 新会话的权限档按「哪一家」存：收那一家清单里的档（`null` 删那一条）；不在清单里的 ⇒ `bad_args`、盘上那份逐字节不变。
#[test]
fn a_permission_mode_default_is_kept_per_agent_and_only_one_that_agent_offers() {
    let home = test_home::scratch("perm");
    let got = set(
        &home,
        serde_json::json!({"permissionModes": {"claude": "plan"}}),
    )
    .unwrap();
    assert_eq!(
        got.prefs.permission_modes.get("claude").map(String::as_str),
        Some("plan")
    );
    let file = path_in(&home);
    let before = std::fs::read(&file).unwrap();
    for bad in [
        serde_json::json!({"permissionModes": {"claude": "yolo"}}),
        serde_json::json!({"permissionModes": {"codex": "plan"}}),
        serde_json::json!({"permissionModes": {"nobody": "plan"}}),
    ] {
        let e = set(&home, bad.clone()).unwrap_err();
        assert!(matches!(e, PrefsSetErr::BadArgs(_)), "{bad} ⇒ {e:?}");
        assert_eq!(std::fs::read(&file).unwrap(), before, "{bad} 拒了还动了盘");
    }
    let got = set(
        &home,
        serde_json::json!({"permissionModes": {"claude": null}}),
    )
    .unwrap();
    assert!(got.prefs.permission_modes.is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 状态行那一格：没设 ⇒ 缺省模板、盘上不写那一格；设了读回同一条；空串 ⇒ 不留那一行；`null` ⇒ 回到缺省；超长拒、盘上不动。
#[test]
fn the_status_line_defaults_is_kept_can_be_turned_off_and_reset() {
    use crate::host::managed::status::{template_of, Template, DEFAULT_TEMPLATE};
    let tpl = |p: &Prefs| template_of(p.status_line.as_deref()).to_string();
    let home = test_home::scratch("status-line");
    assert_eq!(tpl(&Prefs::default()), DEFAULT_TEMPLATE);
    set(&home, serde_json::json!({"childTab": "show"})).unwrap();
    let disk = std::fs::read_to_string(path_in(&home)).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&disk).unwrap();
    assert!(doc.get("childTab").is_some(), "读的是那一层：{disk}");
    assert!(doc.get("statusLine").is_none(), "没设不写：{disk}");
    let r = set(&home, serde_json::json!({"statusLine": "{name}|{nope}"})).unwrap();
    assert_eq!(tpl(&r.prefs), "{name}|{nope}", "认不出的键照存");
    assert_eq!(tpl(&answer_read_in(Some(&home)).prefs), "{name}|{nope}");
    let r = set(&home, serde_json::json!({"childTab": "hide"})).unwrap();
    assert_eq!(tpl(&r.prefs), "{name}|{nope}", "别的补丁不动它");
    let r = set(&home, serde_json::json!({"statusLine": ""})).unwrap();
    assert!(
        !Template::parse(&tpl(&r.prefs)).shows(),
        "空串 ⇒ 不留那一行"
    );
    let r = set(&home, serde_json::json!({"statusLine": null})).unwrap();
    assert_eq!(tpl(&r.prefs), DEFAULT_TEMPLATE);
    let before = std::fs::read_to_string(path_in(&home)).unwrap();
    let long = "x".repeat(crate::common::machine_prefs::STATUS_LINE_MAX + 1);
    assert!(matches!(
        set(&home, serde_json::json!({"statusLine": long})),
        Err(PrefsSetErr::BadArgs(_))
    ));
    assert_eq!(
        std::fs::read_to_string(path_in(&home)).unwrap(),
        before,
        "拒了盘上不动"
    );
    let _ = std::fs::remove_dir_all(&home);
}
