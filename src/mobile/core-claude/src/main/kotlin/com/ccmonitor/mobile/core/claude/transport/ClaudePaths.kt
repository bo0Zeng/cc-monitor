package com.ccmonitor.mobile.core.claude.transport

/** Claude Code 的配置目录（主机覆盖 > 应用默认 > 默认表达式）。 */
object ClaudePaths {
    /** claudeDir 未指定时的默认前缀：远端 shell 就地取 `$CLAUDE_CONFIG_DIR`，回退 `$HOME/.claude`。 */
    const val DEFAULT_CLAUDE_DIR = "\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}"

    /** 实际 claudeDir 前缀：主机覆盖 > 应用默认 > [DEFAULT_CLAUDE_DIR]。 */
    fun resolveClaudeDir(
        hostClaudeDir: String?,
        appDefault: String?,
    ): String =
        hostClaudeDir?.trim()?.ifEmpty { null }
            ?: appDefault?.trim()?.ifEmpty { null }
            ?: DEFAULT_CLAUDE_DIR
}
