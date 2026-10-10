package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.ShellWord

/**
 * Claude Code 会话文件路径推导。
 *
 * Claude Code 把 cwd 编码进 `~/.claude/projects/<dir>/` 的目录名：每个非字母数字字符 → `-`
 * （`/home/pi/.claude` → `-home-pi--claude`，`/home/pi/Resilio Sync` → `-home-pi-Resilio-Sync`）。
 */
object ClaudePaths {
    private val NON_ALNUM = Regex("[^A-Za-z0-9]")

    fun projectDirName(cwd: String): String = NON_ALNUM.replace(cwd, "-")

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

    /**
     * claudeDir 前缀的 shell quote，免得含空格的覆写路径被 word-split、glob 失配、静默丢掉全部会话。
     * 默认值是要远端展开的 shell 表达式 ⇒ 双引号；用户覆写是字面路径 ⇒ POSIX 单引号
     * （代价：覆写值里的 `~` / `$HOME` 不展开，覆写须填绝对路径）。判定在 [ShellWord.configDir]。
     */
    internal fun quotedClaudeDirPrefix(claudeDir: String): String = claudeDirWord(claudeDir).text

    /** 本文件三处出口共用的那一次调用。 */
    private fun claudeDirWord(
        claudeDir: String,
        innerTail: String = "",
    ): ShellWord = ShellWord.configDir(claudeDir, DEFAULT_CLAUDE_DIR, innerTail)

    /**
     * 一条对话的会话记录在远端的哪个文件（`<配置目录>/projects/<编码后的 cwd>/<编号>.jsonl`）。
     *
     * 起管道时探「有没有记录」（决定 `--resume` 还是 `--session-id`）和聊天面按字节区间翻历史都用它；
     * 两处各拼一遍会漂，漂了的表现（每次都当新对话 / 历史永远空）都是静默的。
     *
     * 返回 [ShellWord]（能原样拼进命令行的词），不是路径字符串：默认账号目录时它要靠远端 shell 展开，
     * 再 `shellQuote` 一层就开不到文件。接字面路径的函数收不下它，编译期就拦住。
     *
     * @return null = cwd 未知，答不了（记录落在编码后的 cwd 目录下）。不拿猜的路径糊弄。
     */
    fun sessionRecordPath(
        claudeDir: String,
        cwd: String?,
        sessionId: String,
    ): ShellWord? {
        val dir = cwd?.trim()?.ifEmpty { null } ?: return null
        // 尾巴是字面的：`projectDirName` 恒为 `[A-Za-z0-9-]`、合法编号是 UUID ⇒ 原样接；
        // 编号若不是这个形状，[ShellWord.plus] 会把它引起来，而不是让它把命令拆开。
        return claudeDirWord(claudeDir) + "/projects/${projectDirName(dir)}/$sessionId.jsonl"
    }

    /**
     * 会话发现 / tail 用的远端 jsonl glob（cwd 非空 → 该项目目录，空 → 全项目）。
     * 只 quote claudeDir 前缀，`*` 与 [projectDirName]（恒为 `[A-Za-z0-9-]`）留在引号外。
     */
    fun projectsGlob(
        claudeDir: String,
        cwd: String?,
    ): String {
        val prefix = quotedClaudeDirPrefix(claudeDir)
        return if (!cwd.isNullOrBlank()) {
            "$prefix/projects/${projectDirName(cwd)}/*.jsonl"
        } else {
            "$prefix/projects/*/*.jsonl"
        }
    }

    /** resume 用的远端 projects 目录表达式（无 glob ⇒ 整体 quote）。 */
    fun projectsDirExpr(claudeDir: String): String = claudeDirWord(claudeDir, "/projects").text
}
