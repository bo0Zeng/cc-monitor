//! 一条会话能用的命令与 Skill：内置（适配层声明）在前，本项目 · 号的配置目录在后；每格的字由后端写好。

use super::*;

fn put(root: &std::path::Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// ★ 内置那几条在前（`/clear` 先确认、`/model` 要参数），再本项目、用户级；范围写好字；Skill 带后端写好的那一句。
#[test]
fn the_list_puts_builtins_first_and_writes_every_word() {
    let d = std::env::temp_dir().join(format!("ccm-session-cmds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (proj, cfg) = (d.join("proj"), d.join("acct"));
    put(
        &proj,
        ".claude/commands/ship.md",
        "---\ndescription: 发版前核一遍\n---\n",
    );
    put(&cfg, "commands/ask.md", "回答：$ARGUMENTS\n");
    put(
        &cfg,
        "skills/plot/SKILL.md",
        "---\ndescription: 画图\n---\n",
    );
    let face = crate::agents::asset_face("claude").expect("Claude 的资产面");
    let got = commands_of(&face, Some(&proj), &cfg);
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let rows: Vec<(&str, CommandScope, &str, bool, bool)> = got
        .commands
        .iter()
        .map(|c| {
            (
                c.name.as_str(),
                c.scope,
                c.scope_text.as_str(),
                c.needs_args,
                c.confirm,
            )
        })
        .collect();
    let (b, p, u) = (
        t("beSessionCommands.scope.builtin"),
        t("beSessionCommands.scope.project"),
        t("beSessionCommands.scope.user"),
    );
    assert_eq!(
        rows,
        vec![
            ("clear", CommandScope::Builtin, b.as_str(), false, true),
            ("compact", CommandScope::Builtin, b.as_str(), false, false),
            ("init", CommandScope::Builtin, b.as_str(), false, false),
            ("model", CommandScope::Builtin, b.as_str(), true, false),
            ("ship", CommandScope::Project, p.as_str(), false, false),
            ("ask", CommandScope::User, u.as_str(), true, false),
        ]
    );
    assert_eq!(
        got.commands[0].text.as_deref(),
        Some(t("beSessionCommands.builtin.clear").as_str())
    );
    assert_eq!(got.commands[4].text.as_deref(), Some("发版前核一遍"));
    assert_eq!(got.skills.len(), 1);
    assert_eq!(got.skills[0].name, "plot");
    assert_eq!(
        got.skills[0].say,
        copy_core::copy_text("beSessionCommands.skill.say", &[("name", "plot")])
    );
    // 线上那一形。
    let v = serde_json::to_value(&got.commands[0]).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec![
            "confirm",
            "confirmText",
            "name",
            "needsArgs",
            "needsArgsText",
            "scope",
            "scopeText",
            "text"
        ]
    );
    assert_eq!(v["scope"], "builtin");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 号：这条会话上次用的号 → 这台默认号 → 账号 0（那一家自己的家）；号在库里却没有配置目录 ⇒ 往下一级退。
#[test]
fn the_account_follows_last_then_default_then_account_zero() {
    let lib = |default: Option<&str>| crate::control::launch_account::Library {
        known: true,
        accounts: vec![
            crate::control::launch_account::Candidate {
                name: "work".into(),
                config_dir: Some("/h/work".into()),
                usable: true,
            },
            crate::control::launch_account::Candidate {
                name: "lab".into(),
                config_dir: None,
                usable: false,
            },
        ],
        default: default.map(str::to_string),
    };
    let zero = std::path::Path::new("/h/zero");
    let pick = |last: Option<&str>, default| config_root_of(last, &lib(default), Some(zero));
    assert_eq!(pick(Some("work"), None), Some("/h/work".into()));
    assert_eq!(pick(Some("gone"), Some("work")), Some("/h/work".into()));
    assert_eq!(pick(Some("lab"), None), Some(zero.to_path_buf()));
    assert_eq!(pick(None, None), Some(zero.to_path_buf()));
    assert_eq!(config_root_of(None, &lib(None), None), None);
}

/// ★ 每条命令「带参数 · 先确认」的字由后端写：为真才有那一句，为假 ⇒ `null`（界面照抄，不按真假自己写字）。
#[test]
fn each_command_carries_the_words_for_its_marks() {
    let face = crate::agents::asset_face("claude").expect("Claude 的资产面");
    let got = commands_of(&face, None, std::path::Path::new(""));
    let t = |k: &str| copy_core::copy_text(k, &[]);
    for c in &got.commands {
        assert_eq!(
            c.needs_args_text.clone(),
            c.needs_args.then(|| t("beSessionCommands.mark.needsArgs")),
            "{}",
            c.name
        );
        assert_eq!(
            c.confirm_text.clone(),
            c.confirm.then(|| t("beSessionCommands.mark.confirm")),
            "{}",
            c.name
        );
    }
    assert!(got.commands.iter().any(|c| c.needs_args) && got.commands.iter().any(|c| c.confirm));
}

/// ★ ［+］面板分隔线下那两个一次性动作：送进会话的那一句与先不先确认由后端给（压缩不确认 · 清空先确认）；
/// 那一句取注册表那一家声明的（压缩 ＝ `compact_request`），界面不按命令名认。这一家没有 ⇒ `null`（那一行不画）。
#[test]
fn compact_and_clear_come_with_the_line_to_say() {
    let (compact, clear) = one_shots("claude");
    let compact = compact.expect("Claude 能压缩");
    assert_eq!(
        Some(compact.say.as_str()),
        crate::agents::compact_request_of("claude")
    );
    assert!(!compact.confirm);
    let clear = clear.expect("Claude 能清空");
    assert_eq!(
        Some(clear.say.as_str()),
        crate::agents::clear_request_of("claude")
    );
    assert!(clear.confirm);
    assert!(compact.say != clear.say);
    let (c, k) = one_shots("codex");
    assert!(c.is_none() && k.is_none());
    let v = serde_json::to_value(&clear).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["confirm", "say"]);
}

/// ★ 回包逐字节（线上形挪进 `cmd-wire` 前后同一份字节）：顶层键序 · 每条命令的键序 · 为假的标字照出 `null`；
/// 手机按 `cmd_wire::replies::decode_reply` 解得开、按类型重写出来是同一份。
#[test]
fn the_reply_bytes_are_pinned_and_the_phone_reads_them_back() {
    let face = crate::agents::asset_face("claude").expect("Claude 的资产面");
    let mut got = commands_of(&face, None, std::path::Path::new(""));
    got.commands.truncate(1);
    (got.compact, got.clear) = one_shots("claude");
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let want = format!(
        r#"{{"commands":[{{"name":"clear","text":"{text}","scope":"builtin","scopeText":"{scope}","needsArgs":false,"confirm":true,"needsArgsText":null,"confirmText":"{confirm}"}}],"skills":[],"problems":[],"compact":{{"say":"/compact","confirm":false}},"clear":{{"say":"/clear","confirm":true}}}}"#,
        text = t("beSessionCommands.builtin.clear"),
        scope = t("beSessionCommands.scope.builtin"),
        confirm = t("beSessionCommands.mark.confirm"),
    );
    let bytes = serde_json::to_string(&got).unwrap();
    assert_eq!(bytes, want);
    let back =
        cmd_wire::replies::decode_reply("session-commands", serde_json::from_str(&bytes).unwrap())
            .expect("表里有 session-commands")
            .expect("解得开");
    assert_eq!(
        back,
        serde_json::from_str::<serde_json::Value>(&want).unwrap()
    );
}
