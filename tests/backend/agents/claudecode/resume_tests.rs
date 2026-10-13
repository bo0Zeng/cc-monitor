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
