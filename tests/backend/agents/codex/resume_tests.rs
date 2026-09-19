/// 形状锁：子命令、无 flag、三段。`resolve_query` 那边还有一条端到端的
/// （`resolve_codex_builds_resume_subcommand_and_cx_name`，`S2` 搬迁时一个字没改）。
#[test]
fn resume_command_is_a_subcommand_not_a_flag() {
    let c = super::resume_command("codex", "019f75dd-875c-7c81-9eda-32f866b2c60f");
    assert_eq!(c, "codex resume 019f75dd-875c-7c81-9eda-32f866b2c60f");
    assert!(!c.contains("--"), "Codex 的 resume 不带 flag：{c}");
}
