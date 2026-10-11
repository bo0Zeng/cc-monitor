package com.ccmonitor.mobile.core.claude.transport

/** Codex CLI 的配置目录（对偶 [ClaudePaths]）。 */
object CodexPaths {
    /** 默认 codexDir——远端 shell 探测 `$CODEX_HOME`、回退 `$HOME/.codex`。 */
    const val DEFAULT_CODEX_DIR = "\${CODEX_HOME:-\$HOME/.codex}"

    /** 实际 codexDir 前缀：主机覆盖 > 应用默认 > [DEFAULT_CODEX_DIR]。 */
    fun resolveCodexDir(
        hostCodexDir: String?,
        appDefault: String?,
    ): String =
        hostCodexDir?.trim()?.ifEmpty { null }
            ?: appDefault?.trim()?.ifEmpty { null }
            ?: DEFAULT_CODEX_DIR
}
