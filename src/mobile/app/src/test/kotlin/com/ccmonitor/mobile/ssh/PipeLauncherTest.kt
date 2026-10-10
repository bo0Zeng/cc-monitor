package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.ssh.TmuxCommands.TMUX_ALREADY_MARKER
import com.ccmonitor.mobile.ssh.TmuxCommands.TMUX_STARTED_MARKER
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 起常驻管道。 */
class PipeLauncherTest {
    private val sid = "abc123-DEF_456"

    private class FakeChannel(
        private val reply: String,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) = flowOf(reply.toByteArray()).also { commands += command }
    }

    /**
     * 走 `SessionBackend`，不当第二个直接调用方：tmux 命令族只经 `SessionBackend`/`TmuxBackend` 触达。
     *
     * 判据：产出必须逐字节等于 backend 那条原语的产物。走的是幂等那条 `startOnceCommand`
     * （而不是 `newDetachedRunningCommand`）：后者在会话已存在时会把整条命令 send-keys 敲进正在跑的 pane。
     */
    @Test
    fun theStartCommandGoesThroughTheBackendPrimitiveNotRawTmux() {
        val cmd = PipeLauncher.startCommand(TmuxBackend, sid, null)!!
        val payload = ClaudeInvocation.pipeInvocation(null, sid)
        assertEquals(
            "必须就是 backend 那条原语的产物",
            TmuxBackend.startOnceCommand(PipeLauncher.tmuxNameFor(sid), payload),
            cmd,
        )
    }

    /**
     * 管道的 tmux 会话名必须与 resume 那条不同。
     *
     * `cc-<sid8>`（`ClaudeInvocation.resumeSessionName`）是 resume 跑官方 TUI 的那个会话。
     * 起会话用的若是 `new-session -d …; … ; send-keys …`（`;` 不是 `&&`），会话已存在时建失败、send-keys 照发，
     * 整条管道命令就被当成用户输入敲进正在跑的 TUI 并回车（有模态时就是盲点「是，我信任」）。
     */
    @Test
    fun thePipeSessionNameMustNotCollideWithTheResumeTuiSession() {
        assertNotEquals(
            "与 resume 的 TUI 会话同名 ⇒ 管道命令会被敲进那个 TUI",
            ClaudeInvocation.resumeSessionName(sid),
            PipeLauncher.tmuxNameFor(sid),
        )
        assertTrue("用专属前缀", PipeLauncher.tmuxNameFor(sid).startsWith(PipeLauncher.PIPE_SESSION_PREFIX))
    }

    /**
     * 载荷是管道那条，且落进对外契约文件（不是裸 `out.ndjson`）。
     *
     * 断言只挑不含引号的标记：`tmuxNewDetachedRunning` 会把整个载荷再 `shQuote` 一层，
     * 载荷里的 `'` 会变成 `'\''`，照抄载荷原文的断言在这里必然失败。
     * 「载荷逐字节正确」由上面那条 byte-equality 的测试守着，这里只验「带的是管道那件事」。
     */
    @Test
    fun itCarriesThePipePayloadAndWritesTheContractFile() {
        val cmd = PipeLauncher.startCommand(TmuxBackend, sid, null)!!
        // 管道那条是 `tail -n 0 -f`（重建时不重放旧上行，见
        //   `ClaudeInvocationPipeTest.theTailDoesNotReplayOldUplinkLinesOnRestart`）。
        assertTrue("要带管道那条命令：$cmd", cmd.contains("tail -n 0 -f "))
        assertTrue("落进契约文件 events.ndjson", cmd.contains(PipeCommands.EVENTS))
        assertTrue("首行是 __meta__ 契约头", cmd.contains("__meta__"))
        assertTrue("stderr 要有落点，不能静默死", cmd.contains(PipeCommands.LOG))
    }

    /**
     * 会话已在跑就什么都不做：不许把整条管道命令 send-keys 进正在跑的 pane。
     *
     * `tmuxNewDetachedRunning` 那三段是 `;` 分隔不是 `&&`：会话已存在 ⇒ `new-session` 失败、
     * `send-keys` 照发 ⇒ 整条 `unset …; tail -f … | claude … >> events.ndjson` 被当作
     * 用户输入敲进 pane。而 pane 前台是管道、没有进程读 pty ⇒ 字节滞留在行规程缓冲；
     * 管道一退出 shell 就执行它 ⇒ 第二条管道往同一个 `events.ndjson` 追加。
     *
     * 判据落在行为上（「已存在时不发那条命令」），不是「命令串长什么样」。
     */
    @Test
    fun startingTwiceDoesNotTypeThePipelineIntoTheRunningPane() {
        val cmd = PipeLauncher.startCommand(TmuxBackend, sid, null)!!
        // 必须先问「在不在」，且那个判断要包住 send-keys
        val probeAt = cmd.indexOf("has-session")
        val sendAt = cmd.indexOf("send-keys")
        assertTrue("要先探测会话在不在：$cmd", probeAt >= 0)
        assertTrue("探测必须在 send-keys 之前", probeAt < sendAt)
        // send-keys 必须落在 else 分支里：在 then 分支或分支外都等于「照发」
        val elseAt = cmd.indexOf("else")
        assertTrue("send-keys 必须在 else 分支里（已存在那支绝不能发）", elseAt in 0 until sendAt)
    }

    /**
     * 成功证据绑在「会话真的存在」上，不是「send-keys 返回了」。
     *
     * 用 `DaemonCommands.guarded` 包整条命令的话，那个 `&&` 只绑得住最后那条 send-keys：
     * 会话没建出来、send-keys 发了，照样报「起成功」。
     */
    @Test
    fun successEvidenceIsBoundToTheSessionExistingNotToSendKeysReturning() =
        runTest {
            // 已在跑 ⇒ 成功（幂等那一支）
            val already = FakeChannel("\n$TMUX_ALREADY_MARKER\n")
            assertNull("已在跑也算成功", PipeLauncher.start(already, TmuxBackend, sid, null))

            // 刚建起来 ⇒ 成功
            val started = FakeChannel("\n$TMUX_STARTED_MARKER\n")
            assertNull("刚建起来也算成功", PipeLauncher.start(started, TmuxBackend, sid, null))

            // 两个标记都没有 ⇒ 失败。`send-keys` 成功但会话没建出来时，stdout 里不会有任何标记，
            //   只看 send-keys 的话这一格会被误判成功。
            val neither = FakeChannel("can't find session\n")
            assertNotNull("没有任何标记 ⇒ 必须判失败", PipeLauncher.start(neither, TmuxBackend, sid, null))
        }

    /**
     * 接线错了要变成「一句可读的原因」，不是抛出去掀掉 app。
     *
     * `pipeInvocation` 对「新对话编号不是 UUID」「resume 与新编号都给了」是 `require`（当场炸）：
     * 那是给写代码的人的判据，对用户不能表现成崩溃。这里是从 `LaunchedEffect` 调下来的，
     * 逃出去就是未捕获协程异常 ⇒ 整个 app 没了。
     *
     * 判据落在「返回了原因而不是抛」上，且原因得说得出是什么事。
     */
    @Test
    fun aMiswiredLaunchBecomesAReadableReasonInsteadOfCrashing() =
        runTest {
            val ok = FakeChannel("\n$TMUX_STARTED_MARKER\n")
            // 前提：同样的调用，参数合法时是能成功的，否则下面「失败」可能是别的原因造成的
            val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
            assertNull("前提：参数合法时本来能起成功", PipeLauncher.start(ok, TmuxBackend, uuid, null, newSessionId = uuid))

            val notUuid =
                PipeLauncher.start(FakeChannel(""), TmuxBackend, sid, null, newSessionId = "aterm-$uuid")
            assertNotNull("新编号不是 UUID ⇒ 要给原因，不许抛", notUuid)
            assertTrue("原因得说得出是编号的事：$notUuid", notUuid!!.contains("UUID"))

            val both =
                PipeLauncher.start(FakeChannel(""), TmuxBackend, uuid, null, resumeSessionId = uuid, newSessionId = uuid)
            assertNotNull("两个都给 ⇒ 要给原因，不许抛", both)
        }

    /**
     * 会话名不许截断 sid：截了就跨对话串流。
     *
     * 目录 `.aterm/s/<完整 sid>` 不撞，但 tmux 名撞 ⇒ 起 B 的管道时命中 A 的会话。
     * 判据写成关系式（两个只有前 8 位相同的 sid 必须得到不同的会话名），不钉具体串。
     */
    @Test
    fun twoSessionsSharingAnEightCharPrefixGetDifferentTmuxNames() {
        val a = "abcd1234-aaaa-1111"
        val b = "abcd1234-bbbb-2222"
        assertEquals("前提：前 8 位确实一样", a.take(8), b.take(8))
        assertNotEquals(
            "截断 sid ⇒ 两条对话共用一个 tmux 会话 ⇒ 串流",
            PipeLauncher.tmuxNameFor(a),
            PipeLauncher.tmuxNameFor(b),
        )
    }

    /**
     * 我方不设 `@ccm_sid`。
     *
     * 但要知道：管道下 pane 前台命令是 `claude`，后端的谓词仍会翻真（跨仓事实，不是我们能单方面消除的）。
     */
    @Test
    fun weNeverSetTheMonitorOwnershipMarker() {
        assertFalse(PipeLauncher.startCommand(TmuxBackend, sid, null)!!.contains("@ccm_sid"))
    }

    /** 非法 sessionId 当场拒：它要进路径与会话名，不能靠「后面某处会检查」。 */
    @Test
    fun anInvalidSessionIdIsRejectedAtConstruction() {
        for (bad in listOf("", "../../etc", "a b", "a;b")) {
            assertNull("「$bad」必须被拒", PipeLauncher.startCommand(TmuxBackend, bad, null))
        }
    }

    /** 启动命令过白名单（用 `sanitizeLaunchCommand`，不另写一套）。 */
    @Test
    fun anInjectableLaunchCommandFallsBackToPlainClaude() {
        assertTrue(PipeLauncher.startCommand(TmuxBackend, sid, "cct")!!.contains("| env cct "))
        val evil = PipeLauncher.startCommand(TmuxBackend, sid, "claude; rm -rf /")!!
        assertFalse("含元字符 ⇒ 回退", evil.contains("rm -rf"))
    }

    /**
     * 要肯定的成功证据：`exec` 拿不到退出码，「stdout 空」既是成功也是「tmux 没装 / 建不出来」。
     * 而且用返回类型表达失败，不留自选检查器。
     */
    @Test
    fun startReportsFailureThroughItsReturnValue() =
        runTest {
            // 成功证据是 `tmuxStartOnceCommand` 打的那两个标记之一（见下面那条专门的判据）。
            val ok = FakeChannel("\n$TMUX_STARTED_MARKER\n")
            assertNull("起成功 ⇒ null", PipeLauncher.start(ok, TmuxBackend, sid, null))
            assertTrue("命令自己要能打出标记", ok.commands.single().contains(TMUX_STARTED_MARKER))

            val silent = FakeChannel("")
            assertNotNull("空 stdout 不算成功", PipeLauncher.start(silent, TmuxBackend, sid, null))

            assertNotNull("非法 sid 也要给人可读的原因", PipeLauncher.start(FakeChannel(""), TmuxBackend, "a b", null))
        }

    /** 传输层抛异常也要变成人可读的原因，而不是把页面弄崩。 */
    @Test
    fun aTransportFailureBecomesAReadableReason() =
        runTest {
            val dead =
                object : RemoteCommandChannel {
                    override fun exec(command: String) = flowOf<ByteArray>().also { throw java.io.IOException("断了") }
                }
            assertEquals("断了", PipeLauncher.start(dead, TmuxBackend, sid, null))
        }
}
