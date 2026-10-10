package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 历史列表列的是远端真实存在的全部对话。
 *
 * 喂的是真跑 daemon 录下的字节（`bridge/vectors/daemon-query-{projects,sessions}.ndjson`，
 * 含正向成功标记）：手编 fixture 的话字段名拼错了也照样绿，而字段名正是这里唯一会错的地方。
 *
 * 交叉判据：`--list-projects` 自报该项目 `sessionCount = 4`，`--list-sessions <该项目>` 吐回 4 条。
 * 两个数字来自两次独立调用，解析器把任何一边读错，等式就不成立。
 */
class DaemonConversationCatalogTest {
    private fun vector(name: String): String {
        val f =
            File("../bridge/vectors/$name").takeIf { it.isFile } ?: File("bridge/vectors/$name")
        return f.readText()
    }

    private val projectsOut get() = vector("daemon-query-projects.ndjson")
    private val sessionsOut get() = vector("daemon-query-sessions.ndjson")

    /** 录制时用的那个项目。 */
    private val recordedDir = "-home-u-work-android-terminal"

    @Test
    fun realProjectsOutputParsesIntoEveryProject() {
        val projects = DaemonConversationCatalog.parseProjects(projectsOut)
        assertNotNull("真实输出必须解析得出来", projects)
        // 录制时远端是 49 个项目 / 297 条对话
        assertEquals("项目条数", 49, projects!!.size)
        assertEquals("对话总数（sessionCount 求和）", 297, projects.sumOf { it.sessionCount })
        // 每条都要有目录名：它是 `--list-sessions` 的参数，空了就一条都查不到
        assertTrue("每个项目都要有 dirName", projects.all { it.dirName.isNotBlank() })
        assertTrue("每个项目都要有活动时间（翻页要按它倒序）", projects.all { it.lastActivityMs > 0 })
    }

    /**
     * 两次独立调用互相对账：解析器把 `sessionCount` 读错、或把某条对话漏掉，等式立刻不成立。
     */
    @Test
    fun sessionsOfAProjectMatchTheCountThatListProjectsReported() {
        val declared =
            DaemonConversationCatalog
                .parseProjects(projectsOut)!!
                .first { it.dirName == recordedDir }
                .sessionCount
        val actual = DaemonConversationCatalog.parseSessions(sessionsOut)
        assertNotNull("真实输出必须解析得出来", actual)
        assertEquals("--list-projects 自报的条数与 --list-sessions 实际吐回的条数必须相等", declared, actual!!.size)
    }

    /** 总览要的那组元数据一格都不能少，少了就认不出「是不是这条」。 */
    @Test
    fun everyConversationCarriesTheMetadataTheOverviewNeeds() {
        val rows = DaemonConversationCatalog.parseSessions(sessionsOut)!!
        assertTrue("每条都要有标识", rows.all { it.sessionId.isNotBlank() })
        assertTrue("每条都要有正文路径（聊天面读正文靠它）", rows.all { !it.jsonlPath.isNullOrBlank() })
        assertTrue("每条都要有消息数", rows.all { it.messageCount > 0 })
        assertTrue("每条都要有时间", rows.all { it.startedAtMs > 0 })
        // `aiTitle` 会是 null（同一个项目里就有一条）⇒ 标题回退是必经路径，不是边角。
        assertTrue("录到的数据里有没有 AI 标题的对话", rows.any { it.title == null })
    }

    /**
     * 「问不出来」与「真的没有」必须分得开。
     *
     * 客户端结构上拿不到退出码，所以判据是那个正向标记。没有标记 ⇒ `null`，
     * 不许退化成空列表：那会让总览面显示「这台服务器上还没有对话」，而真相是命令没跑起来。
     */
    @Test
    fun outputWithoutTheSuccessMarkerIsAFailureNotAnEmptyResult() {
        // daemon 遇到不认识的子命令：stdout 空、错误走 stderr、exit 2 ⇒ 这里就是空 stdout
        assertNull("空 stdout = 查询失败，不是「没有项目」", DaemonConversationCatalog.parseProjects(""))
        assertNull("空 stdout = 查询失败，不是「没有对话」", DaemonConversationCatalog.parseSessions(""))
        // 有内容但没标记（命令被中途杀掉）⇒ 同样是失败
        val truncated = sessionsOut.substringBefore(DaemonCommands.QUERY_OK_MARKER)
        assertNull("没有正向标记就不许认结果", DaemonConversationCatalog.parseSessions(truncated))
    }

    /** 查到了但确实没有 ⇒ 空列表，与上面那条 `null` 是两件事。 */
    @Test
    fun aSuccessfulButEmptyQueryIsAnEmptyListNotNull() {
        val empty = "\n${DaemonCommands.QUERY_OK_MARKER}\n"
        assertEquals(emptyList<Any>(), DaemonConversationCatalog.parseProjects(empty))
        assertEquals(emptyList<Any>(), DaemonConversationCatalog.parseSessions(empty))
    }

    /** 目录名要被 quote：含空格的项目目录不 quote 会被 word-split 成两个参数。 */
    @Test
    fun theProjectDirectoryIsQuotedIntoTheCommand() {
        val cmd = DaemonConversationCatalog.sessionsCommand("/opt/d", "a dir")
        assertTrue("目录名必须被 quote：$cmd", cmd.contains("'a dir'"))
        assertTrue("要带正向标记：$cmd", cmd.contains(DaemonCommands.QUERY_OK_MARKER))
    }
}
