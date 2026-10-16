/// 形状锁：带 `--resume` flag。`resolve_query` 那边还有端到端的
///（`resolve_defaults_to_claude_when_no_candidates` 等），`S3` 搬迁时一个字没改。
#[test]
fn resume_command_is_a_flag_not_a_subcommand() {
    let c = super::resume_command("claude", "sid_123");
    assert_eq!(c, "claude --resume sid_123");
    // 与 Codex 那半的差别就在这里：那边是 `codex resume <sid>`，没有 flag。
    assert_ne!(
        c,
        crate::agents::codex::resume::resume_command("claude", "sid_123")
    );
}

fn screen(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

/// ★ 认档：屏底那一行写的是哪一档就是哪一档（几种写法都认、大小写不论）；自下而上找，正文里引了同样的字盖不过屏底；
/// 一行档标都没有 ⇒ 认不出（不按「没有标就是默认」猜）。
#[test]
fn the_mode_is_read_from_the_bottom_line_and_never_guessed() {
    let read = super::mode_on_screen;
    assert_eq!(
        read(&screen(&[
            "> ",
            "  ⏵⏵ accept edits on (shift+tab to cycle)"
        ])),
        Some("acceptEdits")
    );
    assert_eq!(
        read(&screen(&["> ", "  ⏸ plan mode on (shift+tab to cycle)"])),
        Some("plan")
    );
    assert_eq!(
        read(&screen(&["> ", "  ⏸ manual mode on"])),
        Some("default")
    );
    assert_eq!(read(&screen(&["> ", "  ⏵⏵ Auto mode on"])), Some("auto"));
    assert_eq!(
        read(&screen(&["> ", "  ⏵⏵ bypass permissions on"])),
        Some("bypassPermissions")
    );
    assert_eq!(
        read(&screen(&[
            "正文里说 accept edits on",
            "> ",
            "  ⏸ plan mode on"
        ])),
        Some("plan"),
        "正文里那一句盖过了屏底"
    );
    assert_eq!(read(&screen(&["> ", "  ? for shortcuts"])), None);
    assert_eq!(read(&[]), None);
}

/// 一条本地命令的输出记录（只放那一句；别的格合成）。
fn stdout_line(said: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "system",
        "subtype": "local_command",
        "content": format!("<local-command-stdout>{said}</local-command-stdout>"),
        "uuid": "u-1"
    })
}

/// ★ 换模型读回：认 `/model` 之后那条本地命令输出里的 `Set model to …` / `Kept model as …`，取反引号里的显示名（带括号的照原样），
/// effort 那一词有就取；别的记录、别的输出 ⇒ 认不出（不猜）。原文照 Claude Code 实际写的那几句。
#[test]
fn the_model_is_read_back_from_the_local_command_output() {
    let read = super::model_said;
    let set = read(&stdout_line(
        "Set model to `Opus 5.5` and saved as your default for new sessions",
    ))
    .unwrap();
    assert_eq!(
        (set.name.as_str(), set.kept, set.effort.as_deref()),
        ("Opus 5.5", false, None)
    );
    let set = read(&stdout_line(
        "Set model to `Opus 5.5 (1M context)` and saved as your default for new sessions with `xhigh` effort",
    ))
    .unwrap();
    assert_eq!(
        (set.name.as_str(), set.effort.as_deref()),
        ("Opus 5.5 (1M context)", Some("xhigh"))
    );
    let set = read(&stdout_line(
        "Set model to `Opus 5.5 (1M context) (default)` and saved as your default for new sessions with `high` effort",
    ))
    .unwrap();
    assert_eq!(set.name, "Opus 5.5 (1M context) (default)");
    assert_eq!(
        read(&stdout_line(
            "Set model to `Fable 5.1` and saved as your default for new sessions"
        ))
        .unwrap()
        .name,
        "Fable 5.1"
    );
    let kept = read(&stdout_line("Kept model as `Opus 5 (1M context)`")).unwrap();
    assert_eq!(
        (kept.name.as_str(), kept.kept),
        ("Opus 5 (1M context)", true)
    );
    // 别的本地命令输出 · 不是本地命令输出的记录 ⇒ 认不出。
    assert!(read(&stdout_line("Conversation compacted")).is_none());
    assert!(read(&serde_json::json!({"type": "user", "message": {"role": "user", "content": "Set model to `X`"}})).is_none());
    assert!(read(&stdout_line("Set model to Opus")).is_none());
}

/// ★ 换模型送的那一行：`/model <值>`。
#[test]
fn the_model_command_is_slash_model_and_the_value() {
    assert_eq!(super::model_command("opus"), "/model opus");
    assert_eq!(super::model_command("sonnet[1m]"), "/model sonnet[1m]");
}

/// ★ 每一档都有字：记录里会写的那几档（`recognize::facts::tests` 那七个值：六个真值 ＋ 别名 `manual`）· 屏底认得出的那几档，
/// 经同一处（[`crate::agents::permission_mode_text`]）都给字，字取自文案表；可选的那三档的字就是起会话框里那一句。认不出的档不猜字。
#[test]
fn every_mode_the_session_can_be_in_has_its_words() {
    let text = |m: &str| crate::agents::permission_mode_text("claude", m);
    for m in [
        "default",
        "plan",
        "acceptEdits",
        "bypassPermissions",
        "dontAsk",
        "auto",
        "manual",
    ] {
        let t = text(m).unwrap_or_else(|| panic!("`{m}` 没有字"));
        assert!(
            !t.is_empty() && !t.contains(m),
            "`{m}` 的字不像文案表里的：{t}"
        );
    }
    for (_, m) in super::MODE_MARKS {
        assert!(text(m).is_some(), "屏底认得出的 `{m}` 没有字");
    }
    for c in super::PERMISSION_MODES {
        assert_eq!(
            text(c.value),
            Some((c.text)()),
            "`{}` 的字与可选的那一句不同",
            c.value
        );
    }
    assert_eq!(
        text("manual"),
        text("default"),
        "别名 `manual` 与 `default` 该是同一句"
    );
    assert_eq!(text("noSuchMode"), None);
    assert_eq!(
        crate::agents::permission_mode_text("codex", "default"),
        None
    );
}

/// 记录里的模型原值 ⇒ 写给人看的名字：系列名同起会话那张表的字，版本取一两位的数段（日期段不算）；认不出 ⇒ 没有字，不猜。
#[test]
fn the_model_written_in_the_record_reads_as_its_name() {
    let family = |v: &str| {
        super::MODEL_ALIASES
            .iter()
            .find(|c| c.value == v)
            .map(|c| (c.text)())
            .unwrap()
    };
    let text = |m: &str| crate::agents::model_text("claude", m);
    assert_eq!(
        text("claude-opus-4-5-20251101"),
        Some(format!("{} 4.5", family("opus")))
    );
    assert_eq!(
        text("claude-sonnet-4-20250514"),
        Some(format!("{} 4", family("sonnet")))
    );
    assert_eq!(
        text("claude-3-5-haiku-20241022"),
        Some(format!("{} 3.5", family("haiku")))
    );
    for m in [
        "<synthetic>",
        "claude-opus",
        "claude-mystery-4-5",
        "gpt-5",
        "",
    ] {
        assert_eq!(text(m), None, "`{m}` 不该有字");
    }
    assert_eq!(
        crate::agents::model_text("codex", "claude-opus-4-5"),
        None,
        "没考据过模型名的那一家不给字"
    );
}
