//! Codex 的 resume 调用形状。`resolve_query` 留着两个 agent 一样的那部分（sid 校验 · base 的 shell-safe 校验 · `CommandPlan` 骨架 · 错误出口）；
//! 这里只有三件 Codex 独有的事实：默认命令叫什么、resume 怎么写、会话名什么前缀。
//!
//! 与 aterm `CodexInvocation` 对拍：`resumeInvocation` = `<base> resume <sid>`（子命令、无 `--resume` flag、无 unset）；`resumeSessionName` = `cx-<sid8>`。
//!
//! 钉住它的是：resume 规格只按注册表里那一家的起会话事实（`agents::LaunchFace`）拼，而通用层里一个 agent 名字面量都不许有
//! （`agent_locality_guard` 判据②）—— 谁想在别处再开一个 Codex 分支，得先写出 `"codex"`，当场红。
//! 有人在通用层直接写 `format!("{base} resume …")`（不提名字）仍然不会红。

/// 无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "codex";

/// resume 那个字面量：**子命令形**（不以 `--` 开头）。[`resume_command`] 与画像表同一份。
pub(crate) const RESUME_TOKEN: &str = "resume";

/// `ccm` 起它（新起与 resume）一律不连共享后台：共享后台沿用第一个拉起它的进程的环境，
/// 这一趟给的账号与中转地址可能不生效，也认不出哪个窗格是哪个会话。
pub(crate) const LAUNCH_ARGS: &[&str] = &["--no-daemon"];

/// 起之前要清掉的嵌套会话标记：Codex 今天没人考据出有（空 ＝ 考据过、确实没有要清的）。
pub(crate) const NESTED_ENV: &[&str] = &[];

/// resume 会话名前缀（Claude 是 `cc`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "cx";

/// resume 命令：**子命令形**，与 Claude 的 `--resume` flag 形不同。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} {RESUME_TOKEN} {session_id}")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/resume_tests.rs"]
mod tests;
