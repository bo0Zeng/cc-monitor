//! Codex 的 **resume 调用形状** —— `S2` 从 `control/resolve_query.rs` 的 `is_codex` 分支搬来。
//!
//! # 这里装的是"知识"，不是"策略"
//!
//! `resolve_query` 留着的仍然是：sid 校验 · base 的 shell-safe 校验 · `CommandPlan` 骨架 ·
//! 错误出口 —— 那些**两个 agent 一模一样**。搬过来的只有三件 Codex 独有的事实：
//! 默认命令叫什么、resume 怎么写、会话名什么前缀。
//!
//! **golden-parity aterm `CodexInvocation`**：`resumeInvocation` = `<base> resume <sid>`
//!（**子命令、无 `--resume` flag、无 unset**，真机 `codex resume <SESSION_ID>` 核过）；
//! `resumeSessionName` = `cx-<sid8>`。
//!
//! ⚠ **本文件的三条今天没有被 [`crate::agent_locality_guard`] 的格式针钉住** ——
//! 那六根针认的是"会话文件长什么样"，而这里是"命令长什么样"。
//! 钉住它的是另一半：通用层的 kind 派发点**逐个登记**（今天恰好 1 处），
//! 谁想在别处再开一个 Codex 分支，会先撞上那个计数。
//! 如实说：**这是间接的** —— 有人在那唯一的派发点里直接写 `format!("{base} resume …")`
//! 仍然不会红。真正堵死它要等 `S3` 把 Claude 那半也搬进来、两个实现凑齐后立接口（`D4`）。

/// 无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "codex";

/// resume 会话名前缀（Claude 是 `cc`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "cx";

/// resume 命令：**子命令形**，与 Claude 的 `--resume` flag 形不同。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} resume {session_id}")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/resume_tests.rs"]
mod tests;
