package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_HAS_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_NO_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_NO_PGREP
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * resume 失败 watchdog 的三态判定（fake channel，虚拟时间）。
 * 空探针不提前终止（走到 T+10s 复核）；空终态=ABSENT（清 tmuxSession 不 kill）。
 * 前台=shell 时先探 pane 子进程——明确无子进程才 kill（FAILED）；有子进程/无 pgrep → 保守 SUCCESS（不误杀 Claude）。
 */
class ResumeWatchdogTest {
    /**
     * fake channel：按 probe 次序吐 [foregroundOutputs]（`pane_current_command` 探测），
     * pane 子进程探测（`pane_pid`+pgrep 合成命令）吐 [childProbeOutput]，kill 单独记账。
     */
    private class FakeChannel(
        private val foregroundOutputs: List<String>,
        private val childProbeOutput: String = PANE_PROBE_NO_CHILD,
    ) {
        val commands = mutableListOf<String>()
        private var fgIdx = 0

        val channel =
            RemoteCommandChannel { cmd ->
                commands.add(cmd)
                when {
                    // 子进程探测：以 `p=$(tmux display-message ... '#{pane_pid}'` 开头（区别于前台探测）。
                    cmd.startsWith("p=\$(tmux display-message") -> flowOf(childProbeOutput.toByteArray())
                    cmd.startsWith("tmux display-message") -> {
                        val out = foregroundOutputs.getOrElse(fgIdx) { "" }
                        fgIdx++
                        flowOf(out.toByteArray())
                    }
                    else -> flow { emit(ByteArray(0)) }
                }
            }

        val killCommands get() = commands.filter { it.startsWith("tmux kill-session") }
        val foregroundProbeCount get() = commands.count { it.startsWith("tmux display-message") }
        val childProbeCount get() = commands.count { it.startsWith("p=\$(tmux display-message") }
    }

    @Test
    fun shellTwiceWithNoChildKillsAndFails() =
        runTest {
            val fake = FakeChannel(listOf("bash\n", "bash\n"), childProbeOutput = PANE_PROBE_NO_CHILD)
            val outcome = ResumeWatchdog(fake.channel).watch("cc-abcd1234")
            assertEquals("双探仍闲置 shell + 明确无子进程 → 失败", ResumeOutcome.FAILED, outcome)
            // kill 命令加了 `2>&1 && echo <marker>`（让失败可辨）⇒ 断言改为「命令以 kill-session 开头且带会话名」
            assertEquals("kill 空会话（防泄漏）—— 应恰好一条", 1, fake.killCommands.size)
            assertTrue("应 kill 目标会话：${fake.killCommands}", fake.killCommands.single().contains("-t 'cc-abcd1234'"))
            assertEquals("两次前台探测", 2, fake.foregroundProbeCount)
            assertEquals("kill 前探一次子进程", 1, fake.childProbeCount)
        }

    @Test
    fun nodeForegroundMeansClaudeStartedSuccessNoKill() =
        runTest {
            val fake = FakeChannel(listOf("node\n"))
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals("前台=node（Claude 起跑）→ 成功", ResumeOutcome.SUCCESS, outcome)
            assertTrue("不得 kill", fake.killCommands.isEmpty())
            assertEquals("首探即成功 → 不复核", 1, fake.foregroundProbeCount)
            assertEquals("不探子进程", 0, fake.childProbeCount)
        }

    // 慢机上 T+4s 时 cc-<sid8> 还没建（空）→ 不再提前终止；T+10s 复核仍空 → ABSENT（清 tmuxSession，不 kill）。
    @Test
    fun emptyFirstThenEmptyIsAbsentNotEarlyStop() =
        runTest {
            val fake = FakeChannel(listOf("", ""))
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals("空探针不是终态——复核仍空 → ABSENT", ResumeOutcome.ABSENT, outcome)
            assertTrue("无物可杀 → 不 kill", fake.killCommands.isEmpty())
            assertEquals("空不提前终止，走到复核 → 两次前台探测", 2, fake.foregroundProbeCount)
        }

    // T+4s 空（会话还没建），但 T+10s 前台=shell（命令已跑、cc 不存在报错回落 shell）→ 走 kill 判定（无子进程→FAILED）。
    @Test
    fun emptyFirstThenShellNoChildStillCatchesLeak() =
        runTest {
            val fake = FakeChannel(listOf("", "bash\n"), childProbeOutput = PANE_PROBE_NO_CHILD)
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals("空→shell 无子进程 → 仍判失败并清理（否则泄漏）", ResumeOutcome.FAILED, outcome)
            assertEquals(1, fake.killCommands.size)
            assertTrue("应 kill 目标会话：${fake.killCommands}", fake.killCommands.single().contains("-t 'cc-x'"))
        }

    // 前台=shell 但 pane 有子进程（包装脚本下的 Claude 在跑）→ 不 kill、判 SUCCESS（宁留良性空会话不误杀）。
    @Test
    fun shellWithChildProcessNotKilledSuccess() =
        runTest {
            val fake = FakeChannel(listOf("bash\n", "bash\n"), childProbeOutput = PANE_PROBE_HAS_CHILD)
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals("有子进程 → 疑似 Claude 在跑 → 不误杀", ResumeOutcome.SUCCESS, outcome)
            assertTrue("不得 kill", fake.killCommands.isEmpty())
            assertEquals("探了子进程", 1, fake.childProbeCount)
        }

    // 无 pgrep → 无从判定 → 保守不 kill（判 SUCCESS）。
    @Test
    fun shellWithNoPgrepConservativelyNotKilled() =
        runTest {
            val fake = FakeChannel(listOf("bash\n", "bash\n"), childProbeOutput = PANE_PROBE_NO_PGREP)
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals("无 pgrep → 保守不 kill", ResumeOutcome.SUCCESS, outcome)
            assertTrue(fake.killCommands.isEmpty())
        }

    // 慢启动：T+4s shell，T+10s Claude 已接管（node）→ 成功、不 kill、不探子进程。
    @Test
    fun slowStartRecoversOnRecheckSuccess() =
        runTest {
            val fake = FakeChannel(listOf("bash\n", "node\n"))
            val outcome = ResumeWatchdog(fake.channel).watch("cc-x")
            assertEquals(ResumeOutcome.SUCCESS, outcome)
            assertTrue(fake.killCommands.isEmpty())
            assertEquals("复核已非 shell → 不探子进程", 0, fake.childProbeCount)
        }
}
