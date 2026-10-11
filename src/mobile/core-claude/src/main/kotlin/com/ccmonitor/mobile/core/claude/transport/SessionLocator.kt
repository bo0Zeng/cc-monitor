package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.model.AgentKind

/**
 * 每个 [AgentKind] 的 agent-dir 解析（主机覆盖 > 应用默认 > 那一家的默认表达式）。
 * 找会话 · 读记录都是那台核心的命令，手机不扫目录。
 */
interface SessionLocator {
    /** 解析实际 agent-dir（主机覆盖 > 应用默认 > 默认表达式）。 */
    fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String
}

/** Claude：[ClaudePaths]。 */
object ClaudeSessionLocator : SessionLocator {
    override fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String = ClaudePaths.resolveClaudeDir(hostDir, appDefault)
}

/** Codex：[CodexPaths]。 */
object CodexSessionLocator : SessionLocator {
    override fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String = CodexPaths.resolveCodexDir(hostDir, appDefault)
}
