package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 一次性查询的正向成功标记。
 *
 * 后端契约是「错误 → stderr + exit 2，stdout 空」，而 `RemoteCommandChannel.exec` 只搬 stdout，
 * 于是「空」既可能是「真的没有」也可能是「命令不存在」。调用方拿不到退出码，只能靠标记区分。
 */
class DaemonQueryGuardTest {
    @Test
    fun theCommandOnlyPrintsTheMarkerAfterASuccessfulExit() {
        val guarded = DaemonCommands.guarded("some --query")
        assertTrue("必须是 && 短路 —— 失败就不打标记", guarded.contains("&& printf"))
        assertTrue(guarded.startsWith("some --query "))
        assertTrue("标记前要补换行（原始文件字节末尾未必有）", guarded.contains("'\\n${DaemonCommands.QUERY_OK_MARKER}\\n'"))
    }

    /** 空 stdout 不算成功。 */
    @Test
    fun anEmptyStdoutIsAFailureNotAnEmptyResult() {
        assertNull("旧 daemon 不认子命令时就是这样（stdout 空 / stderr 有话 / exit 2）", DaemonCommands.bodyOrNullIfFailed(""))
        assertNull(DaemonCommands.bodyOrNullIfFailed("\n"))
        // 真的「没有内容但成功了」长这样 —— 与上面必须能区分
        assertEquals("", DaemonCommands.bodyOrNullIfFailed("\n${DaemonCommands.QUERY_OK_MARKER}\n"))
    }

    @Test
    fun theBodyComesBackWithoutTheMarker() {
        val body = "{\"a\":1}\n{\"b\":2}"
        assertEquals(body, DaemonCommands.bodyOrNullIfFailed("$body\n${DaemonCommands.QUERY_OK_MARKER}\n"))
        assertEquals("没有尾换行也要认", body, DaemonCommands.bodyOrNullIfFailed("$body\n${DaemonCommands.QUERY_OK_MARKER}"))
    }

    /** 标记出现在中间（正文里恰好有这个词）不算成功，判据是「结尾」。 */
    @Test
    fun aMarkerLookingStringInTheBodyDoesNotFakeSuccess() {
        assertNull(DaemonCommands.bodyOrNullIfFailed("${DaemonCommands.QUERY_OK_MARKER}\n{\"a\":1}\n"))
    }

    /** 每一条一次性查询都恒带正向标记，不是 opt-in；带参数的那条顺带覆盖「参数值要 quote」。 */
    @Test
    fun everyOneShotQueryCarriesTheMarker() {
        for (cmd in listOf(
            DaemonCommands.listProjects("/opt/d"),
            DaemonCommands.query("/opt/d", "--list-sessions", "proj"),
        )) {
            assertTrue("$cmd 必须带正向标记", cmd.contains("&& printf") && cmd.contains(DaemonCommands.QUERY_OK_MARKER))
        }
        // 路径要 quote、参数值要 quote；分派门 `--` 与子命令名不 quote
        assertTrue(
            DaemonCommands.query("/opt/my daemon", "--list-sessions", "a dir").startsWith("'/opt/my daemon' -- --list-sessions 'a dir' "),
        )
    }

    /** 「查询失败」与「查到了但没有内容」用返回类型区分：自选的检查器等于没有检查器。 */
    @Test
    fun parsingDistinguishesAFailedQueryFromAnEmptyOne() {
        val emptyOk = "\n${DaemonCommands.QUERY_OK_MARKER}\n"
        assertEquals("空结果但成功了", emptyList<String>(), DaemonCommands.parseQueryLines(emptyOk))

        // daemon 不认子命令：stdout 空、没有标记
        assertNull("失败必须是 null，而不是一个看起来正常的空列表", DaemonCommands.parseQueryLines(""))

        val body = "{\"a\":1}\n${DaemonCommands.QUERY_OK_MARKER}\n"
        assertEquals(listOf("{\"a\":1}"), DaemonCommands.parseQueryLines(body))
    }
}
