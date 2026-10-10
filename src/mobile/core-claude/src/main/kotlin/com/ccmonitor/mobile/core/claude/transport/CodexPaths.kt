package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.ShellWord

/**
 * Codex CLI 会话文件路径推导（对偶 [ClaudePaths]）。
 *
 * 与 Claude 不同：Codex 按日期分区 `~/.codex/sessions/YYYY/MM/DD/rollout-<ISO-ts>-<uuid>.jsonl`，
 * 冷会话压缩成 `.jsonl.zst`。所以 cwd 过滤不在目录上（要读 `session_meta.payload.cwd`，catalog 层做）；
 * sessionId 是文件名末尾的 UUID；发现靠扫日期目录（`state_5.sqlite` 会过期，不用它）。
 */
object CodexPaths {
    /** 默认 codexDir——远端 shell 探测 `$CODEX_HOME`、回退 `$HOME/.codex`。 */
    const val DEFAULT_CODEX_DIR = "\${CODEX_HOME:-\$HOME/.codex}"

    /** rollout 文件名末尾的标准 UUID（8-4-4-4-12；= ThreadId = session_meta.id，跨 resume 不变）。 */
    private val UUID_RE = Regex("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")

    /** 实际 codexDir 前缀：主机覆盖 > 应用默认 > [DEFAULT_CODEX_DIR]。 */
    fun resolveCodexDir(
        hostCodexDir: String?,
        appDefault: String?,
    ): String =
        hostCodexDir?.trim()?.ifEmpty { null }
            ?: appDefault?.trim()?.ifEmpty { null }
            ?: DEFAULT_CODEX_DIR

    /**
     * codexDir 前缀 shell quote，策略同 [ClaudePaths]，判定在 [ShellWord.configDir]。
     */
    internal fun quotedCodexDirPrefix(codexDir: String): String = ShellWord.configDir(codexDir, DEFAULT_CODEX_DIR).text

    /**
     * 会话发现 glob：`<codexDir>/sessions/` 下年/月/日三级 + `rollout-` 前缀，`.jsonl*` 兼吃 `.jsonl` 与 `.jsonl.zst`。
     * cwd 不进 glob。只 quote 前缀，`*` 留在引号外。
     */
    fun sessionsGlob(codexDir: String): String = "${quotedCodexDirPrefix(codexDir)}/sessions/*/*/*/rollout-*.jsonl*"

    /** `<codexDir>/sessions` 目录表达式（quote 后），供 resume 的 `find` 按 UUID 定位 rollout。 */
    fun sessionsDirExpr(codexDir: String): String = ShellWord.configDir(codexDir, DEFAULT_CODEX_DIR, "/sessions").text

    /**
     * rollout 文件路径 → sessionId（末尾 UUID）。ISO ts 里也有 `-`，所以取最后一个 UUID 匹配；无匹配 → 去后缀兜底。
     */
    fun sessionIdOf(path: String): String {
        val name = path.substringAfterLast('/')
        return UUID_RE.findAll(name).lastOrNull()?.value
            ?: name.removeSuffix(".zst").removeSuffix(".jsonl")
    }
}
