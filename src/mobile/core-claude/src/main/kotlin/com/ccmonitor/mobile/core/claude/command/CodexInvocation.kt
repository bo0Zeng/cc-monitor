package com.ccmonitor.mobile.core.claude.command

/**
 * Codex CLI 命令载荷（对偶 [ClaudeInvocation]）。POSIX shell，不含 tmux 语法。
 *
 * 与 Claude 的差异：默认命令 `codex`。
 */
object CodexInvocation : AgentInvocation {
    /** launcher 注入元字符 denylist（同 [ClaudeInvocation]：挡串联/展开/重定向真注入向量，放行引号/括号/星号）。 */
    private val LAUNCH_UNSAFE = Regex("[;|&\$`><\\n\\r]")

    /** 净化主机自定义 Codex 启动命令；null/空白/含注入元字符 → 回退 `codex`（fail-closed）。 */
    override fun sanitizeLaunchCommand(command: String?): String {
        val c = command?.trim().orEmpty()
        return if (c.isNotEmpty() && !LAUNCH_UNSAFE.containsMatchIn(c)) c else "codex"
    }

    /** 候选链取首个原样通过 [sanitizeLaunchCommand] 的；全被拒/无候选 → 兜底 `codex`。 */
    override fun resolveLaunchCommand(candidates: List<String?>): String {
        val cs = candidates.mapNotNull { it?.trim()?.takeIf(String::isNotEmpty) }
        return cs.firstOrNull { sanitizeLaunchCommand(it) == it } ?: "codex"
    }
}
