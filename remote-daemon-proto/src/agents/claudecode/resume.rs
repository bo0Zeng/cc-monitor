//! Claude 的 **resume 调用形状** —— 与 [`crate::agents::codex::resume`] 严格对称。
//!
//! 两边摆在一起才看得出差别（这也是 `D4` 要求"接口由现有能力反推"的用意）：
//!
//! | | Claude | Codex |
//! |---|---|---|
//! | 默认命令 | `claude` | `codex` |
//! | resume 形 | `<base> --resume <sid>`（**flag**） | `<base> resume <sid>`（**子命令**） |
//! | 会话名前缀 | `cc-` | `cx-` |
//!
//! golden-parity aterm。⚠ 两边**只有这三件事不同** —— sid 校验、base 的 shell-safe 校验、
//! `CommandPlan` 骨架、错误出口全都共用，留在 `control/resolve_query.rs`。

/// 无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "claude";

/// resume 会话名前缀（Codex 是 `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "cc";

/// resume 命令：**flag 形**，与 Codex 的子命令形不同。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} --resume {session_id}")
}

#[cfg(test)]
mod tests {
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
}
