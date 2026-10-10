package com.ccmonitor.mobile.core.claude.command

/**
 * Codex CLI 命令载荷（对偶 [ClaudeInvocation]）。POSIX shell，不含 tmux 语法。
 *
 * 与 Claude 的差异：resume 是子命令 `codex resume <SESSION_ID>`（不是 `--resume` flag）；
 * 没有嵌套环境要 unset；默认命令 `codex`；resume 会话名 `cx-<uuid前8>`。
 */
object CodexInvocation : AgentInvocation {
    /** sessionId 白名单（UUID 天然匹配，限长）。sid 要插值进 shell 载荷，先验再用。 */
    private val SESSION_ID_RE = Regex("^[A-Za-z0-9_-]{1,128}$")

    /** launcher 注入元字符 denylist（同 [ClaudeInvocation]：挡串联/展开/重定向真注入向量，放行引号/括号/星号）。 */
    private val LAUNCH_UNSAFE = Regex("[;|&\$`><\\n\\r]")

    /** sessionId（Codex=UUID）是否可安全用于 resume 命令（`^[A-Za-z0-9_-]{1,128}$`）。 */
    override fun isValidSessionId(id: String): Boolean = SESSION_ID_RE.matches(id)

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

    /**
     * resume 载荷（send-keys 进交互 shell）：`<codex/自定义> resume <uuid>`。
     * [sessionId] 不合法直接 require 失败，调用方应先验并给出反馈。
     */
    override fun resumeInvocation(
        launchCommand: String?,
        sessionId: String,
    ): String {
        require(isValidSessionId(sessionId)) { "非法 Codex sessionId: $sessionId" }
        return "${sanitizeLaunchCommand(launchCommand)} resume $sessionId"
    }

    /** resume 会话名 `cx-<uuid前8>`（按 sessionId 稳定，调用方用作 tmux 会话名）。 */
    override fun resumeSessionName(sessionId: String): String = "cx-${sessionId.take(8)}"
}
