package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.remote.shellQuote

/**
 * 会话定位 SPI：每个 [AgentKind] 给出远端会话发现命令、`path → sessionId`、agent-dir 解析。
 *
 * Claude 按 cwd 编码目录 `<dir>/projects/<encoded-cwd>/` 下的 `.jsonl`（发现即按 cwd 过滤），sessionId = 文件名 stem；
 * Codex 按日期分区 `<dir>/sessions/` 下年/月/日三级的 `rollout-*.jsonl[.zst]`（cwd 过滤靠读 session_meta，
 * catalog 层做），sessionId = 末尾 UUID。发现以扫目录为准。
 */
interface SessionLocator {
    /** 该 kind 默认 agent-dir shell 表达式。 */
    val defaultAgentDir: String

    /** 解析实际 agent-dir（主机覆盖 > 应用默认 > 默认表达式）。 */
    fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String

    /** 远端 SSH 会话发现命令（`ls -t`，最近在前；stdout 每行一 jsonl 绝对路径）。 */
    fun listCommand(
        agentDir: String,
        cwd: String?,
    ): String

    /** 文件路径 → sessionId。 */
    fun sessionIdOf(path: String): String

    /**
     * 按 sessionId 找回那一份会话文件的远端命令（stdout = 第一个命中的路径；找不到 ⇒ 空）。
     * resume 的 tmux 脚手架拿它定位记录、读权威 cwd；两家布局的差别只在这里分。
     *
     * 调用方须先过 `AgentInvocation.isValidSessionId`；[sessionId] 进 `-name` 时仍单引号转义（纵深防御）。
     */
    fun findBySessionIdCommand(
        agentDir: String,
        sessionId: String,
    ): String
}

/** Claude：复用 [ClaudePaths]（projects/<cwd> glob + 文件名 stem）。 */
object ClaudeSessionLocator : SessionLocator {
    override val defaultAgentDir: String = ClaudePaths.DEFAULT_CLAUDE_DIR

    override fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String = ClaudePaths.resolveClaudeDir(hostDir, appDefault)

    override fun listCommand(
        agentDir: String,
        cwd: String?,
    ): String = "ls -t ${ClaudePaths.projectsGlob(agentDir, cwd)} 2>/dev/null"

    override fun sessionIdOf(path: String): String = path.substringAfterLast('/').removeSuffix(".jsonl")

    /** `<claudeDir>/projects/<encoded-cwd>/<sid>.jsonl`：maxdepth 2、精确文件名。 */
    override fun findBySessionIdCommand(
        agentDir: String,
        sessionId: String,
    ): String = "find ${ClaudePaths.projectsDirExpr(agentDir)} -maxdepth 2 -name ${shellQuote("$sessionId.jsonl")} -print -quit 2>/dev/null"
}

/** Codex：[CodexPaths]（sessions/日期 glob 含 .zst + 末尾 UUID）。cwd 忽略（日期分区，按 cwd 过滤在 catalog 层）。 */
object CodexSessionLocator : SessionLocator {
    override val defaultAgentDir: String = CodexPaths.DEFAULT_CODEX_DIR

    override fun resolveAgentDir(
        hostDir: String?,
        appDefault: String?,
    ): String = CodexPaths.resolveCodexDir(hostDir, appDefault)

    override fun listCommand(
        agentDir: String,
        cwd: String?,
    ): String = "ls -t ${CodexPaths.sessionsGlob(agentDir)} 2>/dev/null"

    override fun sessionIdOf(path: String): String = CodexPaths.sessionIdOf(path)

    /**
     * `rollout-<ISO-ts>-<uuid>.jsonl[.zst]`：uuid 在尾 ⇒ `*<uuid>.jsonl*`；年/月/日三级 ⇒ maxdepth 4。
     */
    override fun findBySessionIdCommand(
        agentDir: String,
        sessionId: String,
    ): String = "find ${CodexPaths.sessionsDirExpr(agentDir)} -maxdepth 4 -name ${shellQuote("*$sessionId.jsonl*")} -print -quit 2>/dev/null"
}
