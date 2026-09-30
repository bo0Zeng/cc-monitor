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

/// 〔P1 · 第 4 件〕用户的 shell 集成 wrapper（含代理 / env）：探得到就先用它，探不到回退 [`DEFAULT_COMMAND`]。
/// 从前与下面两格一起住 monitor `adapter/claude_code.rs`〔散文墓碑〕，与后端 `control/ccm/` 各一份、靠金样对着。
pub(crate) const LAUNCHER_ALIAS: &str = "cc";

/// 〔P1〕resume 那个字面量：**flag 形**（`--` 开头）。[`resume_command`] 与画像表（`agents::LaunchFace`）同一份。
pub(crate) const RESUME_TOKEN: &str = "--resume";

/// 〔P1〕起会话 / resume 之前要清掉的嵌套会话标记（否则 claude 自认嵌套子会话、不注册 pidfile、不写 jsonl）。
/// ⚠ **顺序不是随手排的**：载荷按这个序 unset（`launch_render`），它直接决定送到那台的那条命令的字节。
pub(crate) const NESTED_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
];

/// resume 会话名前缀（Codex 是 `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "cc";

/// resume 命令：**flag 形**，与 Codex 的子命令形不同。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} {RESUME_TOKEN} {session_id}")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/resume_tests.rs"]
mod tests;
