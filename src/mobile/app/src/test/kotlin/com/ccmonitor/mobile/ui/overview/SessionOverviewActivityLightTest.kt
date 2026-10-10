package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 总览的灯读活动档（`activity`），不读已退役的 `status`，也不拿它兜底。
 *
 * 后端的 `session_added` / `session_status` 两帧上没有 `status`，只有活动档。点灯若是
 * `lightForStatus(status, alive = true)`，`status` 恒 `null` ⇒ 每一盏都是 `Unknown`：
 * 「需手动」「在跑」两区恒空、所有对话挤进「其余」并写着「说不好现在怎么样」。不报错、不崩。
 *
 * 从真帧出发，不从 `Session` 对象出发：灯这张表本身由 core 的 `SessionActivityLightTest` 守；这里守的是
 * 调用点读对了格，帧 → 会话表 → 行 → 灯 → 分区，整条走一遍。直接造 `Session(activity = …)` 的话，
 * 「会话表里同时躺着一个缺席保留下来的旧 `status`」这种真实形状就造不出来（见第二条）。
 *
 * 射程之外：
 * - 不钉只发 `status` 的老对端上等待态总线的行为：总线（`SessionSignals.publishWaiting`）仍认 `status == waiting`，
 *   灯不认，那种对端上两边不一致。
 * - 没在真机上跑：判据覆盖的是「帧按后端源码与金样描述的形状到达时，灯点得对不对」。
 */
class SessionOverviewActivityLightTest {
    /** 与 `SessionOverviewSignalBridgeTest` 同一个取法：不手编 hello，那会绕开能力协商。 */
    private fun realHello(): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        return f
            .readLines()
            .asSequence()
            .filter { it.contains("\"ver\":\"p1t\"") && it.contains("流模式首帧") }
            .mapNotNull { Regex("\"raw\":\"(.*)\"\\}$").find(it)?.groupValues?.get(1) }
            .first()
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    }

    /** 探测与主流用同一份行（同 `DaemonSessionSourceTest`：真实场景是两次 exec，这里简化为同一批）。 */
    private class Channel(
        private val lines: List<String>,
    ) : RemoteCommandChannel {
        override fun exec(command: String) = flowOf((lines.joinToString("\n") + "\n").toByteArray())
    }

    /** 喂一串帧，取流结束前最后一份会话表（流正常结束会补发一个 fatal，同 `DaemonSessionSourceTest`）。 */
    private suspend fun tableAfter(vararg frames: String): DaemonSessionSource.State {
        val states =
            DaemonSessionSource(Channel(listOf(realHello()) + frames), "/opt/d")
                .states(probeTimeoutMs = 1_000)
                .toList()
        return states[states.size - 2]
    }

    /**
     * 后端的帧点得亮灯：本文件的主判据。
     *
     * 阴性对照：只按已退役的 `status` 点灯（`lightForStatus(status, alive = true)`）
     * 在这串帧上三盏全是 `Unknown` ⇒「需手动」「在跑」两区恒空 ⇒ 本条红。
     * 第一帧逐字取自后端的会话流金样，不是按理解手编的。
     */
    @Test
    fun theNewPeersFramesLightTheOverview() =
        runTest {
            val table =
                tableAfter(
                    GOLDEN_MAIN_SESSION_ADDED,
                    """{"kind":"session_added","sid":"w1","cwd":"/b","activity":"working"}""",
                    """{"kind":"session_added","sid":"i1","cwd":"/c","activity":"working"}""",
                    """{"kind":"session_status","sid":"i1","activity":"idle"}""",
                )
            assertEquals("前提：三条都进了会话表", setOf("s1", "w1", "i1"), table.sessions.keys)
            assertTrue(
                "前提：这是后端帧的形状：表里一格 status 都没有（只读 status 的点灯在这里恒得中性）",
                table.sessions.values.all { it.status == null },
            )

            val ui = table.toUiState()
            assertEquals("needs_you ⇒ 排进「需手动」区", listOf("s1"), ui.needsYou.map { it.sessionId })
            assertEquals(SessionLight.WaitingInput, ui.needsYou.single().light)
            assertEquals("等什么照旧带到行上", "permission prompt", ui.needsYou.single().waitingFor)
            assertEquals("working ⇒ 在跑", listOf("w1"), ui.running.map { it.sessionId })
            assertEquals(SessionLight.Working, ui.running.single().light)
            assertEquals("状态帧带来的 idle 要盖掉宣告时的 working", listOf("i1"), ui.others.map { it.sessionId })
            assertEquals(SessionLight.Idle, ui.others.single().light)
        }

    /**
     * `status` 一格都不读，连兜底都不许：三种真实形状各布一个陷阱。
     *
     * ① `sh`：两格都发的那一代对端，把 pidfile 原词 `shell` 翻成活动档 `idle`
     *    （`"idle" | "shell" => Idle`）。「`status` 在就先看 `status`」的混读会把它点成「在跑别的命令」：
     *    前端把后端判过的再判一遍。
     * ② `st`：同一代对端在 pidfile 丢了 `status` 键时发一帧两格都缺席的 `session_status`
     *    （`activity` 由同一个键翻出来，所以一起缺席；金样第 8 行就是这个形状）。我们的会话表里 `status` 缺席保留旧值、
     *    `activity` 缺席即清空 ⇒ `activity ?: status` 的兜底会把一个过期的 `busy` 重新点亮成「在跑」，而对端此刻说的是「说不清」。
     * ③ `old`：只发 `status` 的老对端。不做向后兼容 ⇒ 中性灯，但这一行不许丢。
     */
    @Test
    fun theRetiredStatusIsNeverReadNotEvenAsAFallback() =
        runTest {
            val table =
                tableAfter(
                    """{"kind":"session_added","sid":"sh","cwd":"/a","status":"shell","activity":"idle"}""",
                    """{"kind":"session_added","sid":"st","cwd":"/b","status":"busy","activity":"working"}""",
                    """{"kind":"session_status","sid":"st"}""",
                    """{"kind":"session_added","sid":"old","cwd":"/c","status":"busy"}""",
                )
            // 前提：陷阱真的布好了，否则下面的绿说明不了任何事。
            assertEquals("前提：shell 那条的 pidfile 原词还在表里", "shell", table.sessions.getValue("sh").status)
            assertEquals("前提：st 的 status 缺席保留了旧值", "busy", table.sessions.getValue("st").status)
            assertNull("前提：st 的活动档缺席即清空", table.sessions.getValue("st").activity)

            val ui = table.toUiState()
            val lightOf = (ui.needsYou + ui.running + ui.others).associate { it.sessionId to it.light }
            assertEquals("一条都不许丢", setOf("sh", "st", "old"), lightOf.keys)
            assertEquals("① 后端判成闲着的，不许被重新判成「在跑别的命令」", SessionLight.Idle, lightOf["sh"])
            assertEquals("② 对端说不清 ⇒ 中性，不许拿过期的 status 点成「在跑」", SessionLight.Unknown, lightOf["st"])
            assertEquals("③ 只有 status 的老对端 ⇒ 中性", SessionLight.Unknown, lightOf["old"])
            assertTrue("于是「在跑」区是空的", ui.running.isEmpty())
        }

    private companion object {
        /**
         * 后端会话流金样的第 5 行，逐字。
         * 注意它没有 `status`、有 `activity`，还带着我们不读的 `container` 对象与 `pid`。
         */
        const val GOLDEN_MAIN_SESSION_ADDED =
            """{"kind":"session_added","sid":"s1","agent_kind":"claude","liveness_confidence":"pidfile","background":true,""" +
                """"attachable":true,"cwd":"/w","project_dir":"/w","name":"n","path":"/p/s1.jsonl","lines":9,""" +
                """"activity":"needs_you","waiting_for":"permission prompt","container":{"host":"tmux","terminal":"tmux-3-7"},"pid":42}"""
    }
}
