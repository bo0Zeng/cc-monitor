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
