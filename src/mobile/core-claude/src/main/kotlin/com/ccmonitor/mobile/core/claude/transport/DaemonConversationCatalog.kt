package com.ccmonitor.mobile.core.claude.transport

import com.squareup.moshi.Moshi

/**
 * 「这台机器上有哪些对话」的读侧：走后端的一次性查询（`--list-projects` / `--list-sessions <dirName>`），
 * 不走流。流上只有 tmux 里活着的那几条，而且大部分流量是列表用不上的 `line` 正文帧；
 * 查询面拿到的是远端全量。流只用来给活着的那几条盖状态章。
 *
 * 返回类型就是判据：`null` = 问不出来，空列表 = 问出来了但没有。远端执行通道拿不到退出码，
 * 空 stdout 有二义，所以判据走 [DaemonCommands.QUERY_OK_MARKER]。两者合成一个，
 * 总览面会把「命令没跑起来」显示成「还没有对话」。
 *
 * 边界：
 * - 后端没有分页参数（`--limit` 被静默忽略），分页只能由客户端按项目切（见 [ConversationPager]），
 *   一页的代价随项目大小抖动。
 * - `<sid>/subagents/` 下的 `.jsonl` 是 subagent 转录，不是一条对话，查询面不报它们。
 */
object DaemonConversationCatalog {
    private val adapter = Moshi.Builder().build().adapter(Any::class.java)

    /**
     * 一个项目（`~/.claude/projects/` 下的一个目录）。字段名照后端输出。
     *
     * @param dirName 目录名，`--list-sessions` 要的就是它，不是 [projectPath]。
     * @param sessionCount 后端自报的对话条数，是权威：用它判断「列全了没有」，不去数磁盘上的文件。
     */
    data class Project(
        val dirName: String,
        val projectPath: String?,
        val sessionCount: Int,
        val lastActivityMs: Long,
    )

    /**
     * 一条对话。
     *
     * @param title 后端生成的 AI 摘要标题，可以为 null（`aiTitle: null`），UI 侧要有回退。
     * @param jsonlPath 正文文件的绝对路径；从对话定位到文件、聊天面读正文都靠它。
     */
    data class Conversation(
        val sessionId: String,
        val title: String?,
        val cwd: String?,
        val jsonlPath: String?,
        val messageCount: Int,
        val startedAtMs: Long,
        val updatedAtMs: Long,
        val background: Boolean,
    )

    /** 一次性查询：这台机器上有哪些项目。 */
    fun projectsCommand(daemonPath: String): String = DaemonCommands.listProjects(daemonPath)

    /**
     * 一次性查询：某个项目下有哪些对话。
     *
     * [dirName] 必须是 [Project.dirName]（目录名），不是 [Project.projectPath]（真实路径）。
     * 它经 `DaemonCommands.query` 被 shell-quote，所以含空格的目录名是安全的。
     */
    fun sessionsCommand(
        daemonPath: String,
        dirName: String,
    ): String = DaemonCommands.query(daemonPath, "--list-sessions", dirName)

    /**
     * 解析 `--list-projects` 的 stdout。
     *
     * @return `null` = 查询失败（没见到正向标记）。空列表 = 查到了，但这台机器上没有项目。
     */
    fun parseProjects(stdout: String): List<Project>? =
        DaemonCommands.parseQueryLines(stdout)?.mapNotNull { line ->
            val row = row(line) ?: return@mapNotNull null
            // 没有 `dirName` 的行不是项目，跳过而不是崩（后端加别的行型时还能用）。
            val dir = row["dirName"] as? String ?: return@mapNotNull null
            Project(
                dirName = dir,
                projectPath = row["projectPath"] as? String,
                sessionCount = (row["sessionCount"] as? Number)?.toInt() ?: 0,
                lastActivityMs = (row["lastActivityMs"] as? Number)?.toLong() ?: 0L,
            )
        }

    /**
     * 解析 `--list-sessions <dirName>` 的 stdout。
     *
     * @return `null` = 查询失败。空列表 = 这个项目下确实没有对话。
     */
    fun parseSessions(stdout: String): List<Conversation>? =
        DaemonCommands.parseQueryLines(stdout)?.mapNotNull { line ->
            val row = row(line) ?: return@mapNotNull null
            // 没有 `sessionId` 的行不是对话。别回退成造一条 id 为空的对话：字段名拼错时列表会全是空行。
            val sid = row["sessionId"] as? String ?: return@mapNotNull null
            Conversation(
                sessionId = sid,
                title = row["aiTitle"] as? String,
                cwd = row["cwd"] as? String,
                jsonlPath = row["jsonlPath"] as? String,
                messageCount = (row["messageCountApprox"] as? Number)?.toInt() ?: 0,
                startedAtMs = (row["startedAtMs"] as? Number)?.toLong() ?: 0L,
                updatedAtMs = (row["updatedAtMs"] as? Number)?.toLong() ?: 0L,
                background = row["isBg"] == true,
            )
        }

    private fun row(line: String): Map<*, *>? = runCatching { adapter.fromJson(line) as? Map<*, *> }.getOrNull()
}
