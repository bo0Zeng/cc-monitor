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
