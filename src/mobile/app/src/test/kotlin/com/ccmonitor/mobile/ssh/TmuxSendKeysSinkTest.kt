package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.ssh.TmuxCommands.SENT_MARKER
import com.ccmonitor.mobile.ssh.TmuxCommands.buildSendCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.exactTarget
import com.ccmonitor.mobile.ssh.TmuxCommands.isCcmTmuxName
import com.ccmonitor.mobile.ssh.TmuxSendKeysSink.Companion.classify
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TmuxSendKeysSinkTest {
    // ---- 命令串：两条路径都逐字节钉住 -----------------------------------
    // 内联拼接容易让几个 `-t` 位点里有的没被测到。两条路径都要钉：只钉「自有名」那条的话，
    // 未拥有名那条把 `cut -f2` 改成 `cut -f1`（身份门变成看 session_windows，对任何存在的会话恒通过 ⇒
    // 门等于拆了）测试照样全绿。

    private val ownedCommand =
        "if command -v tmux >/dev/null 2>&1; then " +
            "scr=\"\$(tmux capture-pane -p -t '=cc-abc12345:' 2>/dev/null)\"; " +
            "if printf '%s' \"\$scr\" | tail -n 12 | grep -q '❯[ ]*[0-9][0-9]*\\.'; then " +
            "printf 'ATERM_MODAL_WAIT\\n%s\\n' \"\$scr\"; " +
            "else tmux set-buffer -b aterm-uplink -- '你好' 2>&1 && " +
            "tmux paste-buffer -d -b aterm-uplink -p -t '=cc-abc12345:' 2>&1 && " +
            "tmux send-keys -t '=cc-abc12345:' Enter 2>&1 && printf 'ATERM_SENT\\n'; fi; " +
            "else printf 'NO_TMUX\\n'; fi"

    /**
     * 兄弟/前缀会话反例。
     *
     * 裸 `-t cc-abc1234` 会命中 `cc-abc12345`（tmux 解析：精确名 → 名字开头 → glob），
     * 于是用户的话被打进另一个还活着的 claude。`='名:'` 是唯一解，尾冒号不能省。
     */
    @Test
    fun everyTargetSlotUsesTheExactMatchForm() {
        val cmd = buildSendCommand("cc-abc12345", "你好")!!
        assertFalse("前提：不能有任何裸 -t 目标残留", cmd.contains("-t cc-abc12345"))
        assertEquals("每个 -t 位点都要是 '=名:' 形式", 3, Regex("-t '=cc-abc12345:'").findAll(cmd).count())
        assertEquals(ownedCommand, cmd)
    }

    /** 名字不是我们造的 ⇒ 身份门要真出现；且不带 `windows=`（那只给 kill 这种破坏性动作）。 */
    @Test
    fun anUnownedNameGetsTheIdentityGateButNotTheWindowsCheck() {
        val cmd = buildSendCommand("someone-elses", "hi")!!
        assertFalse("send-keys 不受 windows==1 约束，判断和拒绝消息里都不许出现它", cmd.contains("windows="))
        assertFalse(cmd.contains("\"\$w\" = \"1\""))
        assertEquals(
            "if command -v tmux >/dev/null 2>&1; then " +
                "info=\"\$(tmux display-message -p -t '=someone-elses:' " +
                "'#{session_windows}\t#{@ccm_sid}' 2>/dev/null)\"; " +
                "if [ -z \"\$info\" ]; then printf 'CCM_NO_SESSION\\n'; else " +
                "sid=\"\$(printf '%s' \"\$info\" | cut -f2)\"; " +
                "if [ -n \"\$sid\" ]; then " +
                "scr=\"\$(tmux capture-pane -p -t '=someone-elses:' 2>/dev/null)\"; " +
                "if printf '%s' \"\$scr\" | tail -n 12 | grep -q '❯[ ]*[0-9][0-9]*\\.'; then " +
                "printf 'ATERM_MODAL_WAIT\\n%s\\n' \"\$scr\"; " +
                "else tmux set-buffer -b aterm-uplink -- 'hi' 2>&1 && " +
                "tmux paste-buffer -d -b aterm-uplink -p -t '=someone-elses:' 2>&1 && " +
                "tmux send-keys -t '=someone-elses:' Enter 2>&1 && printf 'ATERM_SENT\\n'; fi; " +
                "else printf 'CCM_GUARD_REJECTED sid=%s\\n' \"\$sid\"; fi; fi; " +
                "else printf 'NO_TMUX\\n'; fi",
            cmd,
        )
    }

    /**
     * 正文走 `set-buffer` + `paste-buffer -p`（bracketed paste），不是 `send-keys <正文>`。
     *
     * `send-keys` 会把参数当键名查一遍（tmux 3.6）：正文 `C-c` 真的往 pane 打了 Ctrl-C（打死了进程）、
     * `Escape` 变成裸 `^[`、`-N 5 hello` 被当选项解析成 `repeat count invalid`（一个字都没发出去）。
     * 手机端聊天的正文是任意用户文本，必然撞上这些。
     *
     * 换成 paste-buffer 还顺带解决换行：bracketed paste 下多行正文是一次粘贴、
     * 不会逐行提前回车；逐行提交会让第 2..n 行绕过模态探测。
     */
    @Test
    fun theMessageBodyIsPastedNotTypedAsKeyNames() {
        val cmd = buildSendCommand("cc-abc12345", "C-c")!!
        assertFalse("正文绝不能出现在 send-keys 的参数位", cmd.contains("send-keys -t '=cc-abc12345:' 'C-c'"))
        assertTrue("正文走 buffer", cmd.contains("set-buffer -b aterm-uplink -- 'C-c'"))
        assertTrue("bracketed paste", cmd.contains("paste-buffer -d -b aterm-uplink -p -t"))
        assertTrue("回车单独发", cmd.contains("send-keys -t '=cc-abc12345:' Enter"))
        // 多行正文原样进 buffer，不切成多次提交
        assertTrue(buildSendCommand("cc-abc12345", "第一行\n第二行")!!.contains("-- '第一行\n第二行'"))
    }

    /** 空 target ⇒ `=:` 会被 tmux 当成「当前会话」，是这里唯一真正危险的默认值。必须结构性拒掉。 */
    @Test
    fun anEmptyTargetIsRefusedInsteadOfMeaningCurrentSession() {
        assertNull(exactTarget(""))
        assertNull("构造函数本身就拦住 ⇒ 没有第二条路可走", buildSendCommand("", "hi"))
    }

    /** 单引号必须闭合再续，否则一句带 `'` 的话就能把命令串撑破（shell 层注入面）。 */
    @Test
    fun aQuoteInTheMessageCannotBreakOutOfTheCommand() {
        val cmd = buildSendCommand("cc-abc12345", "it's a 'test'; touch PWNED")!!
        assertTrue(cmd.contains("""set-buffer -b aterm-uplink -- 'it'\''s a '\''test'\''; touch PWNED'"""))
    }

    @Test
    fun ownedNamesSkipTheIdentityGateBothOldAndNewShapes() {
        assertTrue(isCcmTmuxName("cc-abc12345")) // 老前缀要认：老会话没有 @ccm_sid
        assertTrue(isCcmTmuxName("myproj-cc"))
        assertTrue(isCcmTmuxName("myproj-cc-2"))
        assertFalse("裸 -cc 这种退化名不算", isCcmTmuxName("-cc"))
        assertFalse(isCcmTmuxName("cc-"))
        assertFalse("有奇怪字符就不认", isCcmTmuxName("cc-a;rm -rf"))
        // 名字命中 ⇒ 退化成不带门的一行 ⇒ 真实流量下这两个哨兵都不可达
        val real = buildSendCommand("cc-abc12345", "x")!!
        assertFalse(real.contains("CCM_GUARD_REJECTED"))
        assertFalse("CCM_NO_SESSION 同样不可达：所以 catch-all 必须是拒绝", real.contains("CCM_NO_SESSION"))
    }

    // ---- stdout → SendOutcome -----------------------------------------------

    /**
     * 没有肯定的成功证据就不算送到。
     *
     * 写成 `else -> Accepted` 的话，下面这些串全被判成「送达」，叠加 `echoesBack=true`
     * （成功就把本地那条撤下等回声），用户打的字会无声消失。
     */
    @Test
    fun realTmuxErrorsAreRejectedNotSilentlyAccepted() {
        // 都是真 tmux 3.6 上的原文
        for (raw in listOf(
            "can't find session: cc-abc12345",
            "can't find pane: =cc-abc12345:",
            "no server running on /tmp/tmux-1000/default",
            "repeat count invalid",
            "", // 什么都没回也不算成功
        )) {
            val outcome = classify(raw)
            assertTrue("「$raw」必须是拒绝，不能判成送达", outcome is SendOutcome.Rejected)
            assertTrue("未知失败要给重试键（宁可吵，不可静默）", (outcome as SendOutcome.Rejected).retryable)
        }
    }

    @Test
    fun onlyThePositiveMarkerCountsAsDelivered() {
        assertEquals(SendOutcome.Accepted, classify("$SENT_MARKER\n"))
        // paste-buffer 途中可能有噪声输出，但标记必须在最后（`&&` 链的末端）
        assertEquals(SendOutcome.Accepted, classify("some noise\n$SENT_MARKER\n"))
        assertTrue("标记不在末端 ⇒ 链没跑完", classify("$SENT_MARKER\ncan't find pane") is SendOutcome.Rejected)
    }

    @Test
    fun theThreeMonitorSentinelsAreNotRetryable() {
        // 共同点：远端明确答复了，重试同一目标不会改变答复 ⇒ 不该给重试键
        for (raw in listOf("NO_TMUX", "CCM_NO_SESSION", "CCM_GUARD_REJECTED sid=")) {
            val outcome = classify("$raw\n") as SendOutcome.Rejected
            assertFalse("$raw 不该可重试", outcome.retryable)
        }
    }

    /** 认出模态 ⇒ 拒发，并把屏幕原文原样带回（不翻译、不美化）。 */
    @Test
    fun aModalOnScreenBlocksTheSendAndCarriesTheScreenBackVerbatim() {
        val screen = "Do you trust the files in this folder?\n ❯ 1. Yes, I trust\n   2. No"
        val outcome = classify("${TmuxCommands.MODAL_SENTINEL}\n$screen\n") as SendOutcome.Rejected
        assertFalse("要用户去处理，不是重试", outcome.retryable)
        assertTrue("屏幕原文必须原样带回：${outcome.reason}", outcome.reason.contains(screen))
    }

    /**
     * 模态探测只看屏幕末尾几行。
     *
     * 用户发一句正文含 `❯ 1.` 的消息，TUI 回显到 pane 之后，若探测看整屏，之后每次发送都恒被拦，
     * 手机端没有任何清屏手段 ⇒ 永久锁死。限定末尾若干行让它随对话滚动自然失效。
     */
    @Test
    fun theModalProbeOnlyLooksAtTheTailSoEchoedTextDoesNotLockTheUserOut() {
        assertTrue(buildSendCommand("cc-abc12345", "x")!!.contains("| tail -n 12 | grep -q"))
    }

    // ---- send() 端到端 ------------------------------------------------------

    @Test
    fun sendReadsTheOutcomeOffStdoutBecauseThereIsNoExitCode() =
        runTest {
            // RemoteCommandChannel 只搬 stdout 字节，exit code 永远拿不到
            var seen: String? = null
            val sink =
                TmuxSendKeysSink({ cmd ->
                    seen = cmd
                    flowOf("ATERM".toByteArray(), "_SENT\n".toByteArray()) // 跨块到达也要认出来
                }, "cc-abc12345")
            val outcome = sink.send(SendRequest("local#1", "你好"))
            assertTrue("前提：命令真的发出去了", seen!!.contains("paste-buffer"))
            assertEquals(SendOutcome.Accepted, outcome)
        }

    /** 会话没了 ⇒ 真机上走的是这条（`CCM_NO_SESSION` 在自有名下不生成），必须报错而不是静默送达。 */
    @Test
    fun aDeadSessionSurfacesAsAFailureOnTheRealOwnedNamePath() =
        runTest {
            val sink = TmuxSendKeysSink({ flowOf("can't find session: cc-abc12345\n".toByteArray()) }, "cc-abc12345")
            val outcome = sink.send(SendRequest("local#1", "你好")) as SendOutcome.Rejected
            assertTrue("原文要带给用户看：${outcome.reason}", outcome.reason.contains("can't find session"))
        }

    /**
     * 上行单行 > 56KB 一律走 SFTP。整条命令是 sshd 的单个 argv 元素
     * （内核 `MAX_ARG_STRLEN`=131070B）。手机上粘一段堆栈就能超，纯文本也会。
     */
    @Test
    fun anOversizedMessageIsRefusedBeforeItEverReachesTheWire() =
        runTest {
            var reached = false
            val sink =
                TmuxSendKeysSink({
                    reached = true
                    flowOf()
                }, "cc-abc12345")
            val big = "堆".repeat(TmuxSendKeysSink.MAX_UPLINK_BYTES) // UTF-8 下每字 3 字节，必超
            val outcome = sink.send(SendRequest("local#1", big)) as SendOutcome.Rejected
            assertFalse("一个字节都不该发出去", reached)
            assertFalse("重试同一条只会再失败一次", outcome.retryable)
            assertTrue("要说清是长度问题：${outcome.reason}", outcome.reason.contains("太长"))

            // 闸门量的是拼好之后的命令串，不是转义前的正文：
            //   要管的是 sshd 的单个 argv 元素 = 最后真发出去那一串，而转义会让它明显变长。
            //   所以「正文正好 56KB」应该被拒；放行它就是偏松的闸门，一条全是引号的 55KB 正文照样能顶穿内核那 128KB 上限。
            val exactlyLimitText = "a".repeat(TmuxSendKeysSink.MAX_UPLINK_BYTES)
            //   判据必须落在拒绝的理由上：桩若给空 flow，sink 会因为「没看到成功标记」
            //   也返回 Rejected，只断言 `is Rejected` 就会因为错误的原因通过。
            val nowRejected =
                TmuxSendKeysSink({ flowOf("$SENT_MARKER\n".toByteArray()) }, "cc-abc12345")
                    .send(SendRequest("local#2", exactlyLimitText)) as SendOutcome.Rejected
            assertTrue("要因为长度被拒，不是别的原因：${nowRejected.reason}", nowRejected.reason.contains("太长"))
            assertFalse("长度问题重试没意义", nowRejected.retryable)

            // 边界另一侧：留足外壳余量的正文照常放行（闸门是收紧，不是关死）
            val ok = TmuxSendKeysSink({ flowOf("$SENT_MARKER\n".toByteArray()) }, "cc-abc12345")
            val fits = "a".repeat(TmuxSendKeysSink.MAX_UPLINK_BYTES / 2)
            assertEquals(SendOutcome.Accepted, ok.send(SendRequest("local#3", fits)))
        }

    @Test
    fun aTransportFailureIsRetryableLikeAnyUnknownFailure() =
        runTest {
            val sink = TmuxSendKeysSink({ throw java.io.IOException("连接断了") }, "cc-abc12345")
            val outcome = sink.send(SendRequest("local#1", "你好")) as SendOutcome.Rejected
            assertTrue(outcome.retryable)
            assertEquals("连接断了", outcome.reason)
        }

    @Test
    fun theTuiEchoesSoTheViewModelMustNotEchoOptimistically() {
        assertTrue(TmuxSendKeysSink({ flowOf() }, "cc-abc12345").echoesBack)
    }

    // ---- 这条路停不了，而且要明说 ------------------------------------

    /**
     * `interrupt()` 在这条路上是明确拒绝，不是悄悄什么都不做。
     *
     * 不投 `C-c` / `Escape`：这条路的对面是 tmux 里那个官方 TUI，不是我们起的常驻管道。
     * 管道那条走的是 stdin 上的 `control_request`（`PipeSession.interruptCommand`），TUI 那条没有那个通道。
     * 剩下的候选是投键，而投键在这条路上不安全：模态探测的整个理由就是「盲发最坏会替用户点『是，我信任』」，
     * 中断键同样是键，一样会落进正在等回答的弹窗里。而且 TUI 的中断语义随版本变（ESC 一次 / 两次 / `C-c` 退整个进程）。
     *
     * 判据钉「拒绝 + 说人话 + 不可重试」三件事：一个 `Rejected("")` 会让 UI 停在一句空话上；
     * `retryable = true` 会让界面给出一个重试同一条路的按钮，按多少次都还是停不了。
     */
    @Test
    fun interruptIsRefusedLoudlyBecauseThisPathCannotStopTheRemote() =
        runTest {
            var reached = false
            val sink =
                TmuxSendKeysSink({
                    reached = true
                    flowOf()
                }, "cc-abc12345")

            val outcome = sink.interrupt() as SendOutcome.Rejected

            assertFalse("一个键都不许投出去（盲发最坏会替用户点『是，我信任』）", reached)
            assertFalse("重试同一条路不会变成能停", outcome.retryable)
            assertTrue("要说人话，不许是空话：${outcome.reason}", outcome.reason.length > 5)
            assertTrue(
                "而且要说得出该干什么（响亮还要能行动）：${outcome.reason}",
                outcome.reason.contains("电脑"),
            )
        }

    /**
     * 取消不是失败：不许被谎报成「远端拒绝、可重试」。
     *
     * 协程取消（离开这条对话 / 上层换了协程）若变成 `Rejected(retryable = true)`，屏上就出现一个重试按钮，
     * 点一下就是第二遍。判据形状照抄 `PipeSessionTest.aCancelledSendIsNotReportedAsARemoteRejection`。
     *
     * 判别力边界：`TmuxSendKeysSink` 没有生产调用方，这条是给将来准备的。
     */
    @Test
    fun aCancelledSendIsNotReportedAsARemoteRejection() =
        runTest {
            val cancelling =
                object : com.ccmonitor.mobile.core.remote.RemoteCommandChannel {
                    override fun exec(command: String) =
                        kotlinx.coroutines.flow.flow<ByteArray> {
                            throw kotlinx.coroutines.CancellationException("走了")
                        }
                }

            // 前提：同样的调用在普通异常下确实走「拒绝」那条，否则下面可能因别的原因通过
            val boom =
                object : com.ccmonitor.mobile.core.remote.RemoteCommandChannel {
                    override fun exec(command: String) =
                        kotlinx.coroutines.flow.flow<ByteArray> { throw java.io.IOException("断了") }
                }
            val rejected = TmuxSendKeysSink(boom, "=cc-abc12345:").send(SendRequest("l0", "hi"))
            assertTrue("前提：普通异常该判拒绝", rejected is SendOutcome.Rejected)

            var outcome: SendOutcome? = null
            val threw =
                runCatching {
                    outcome = TmuxSendKeysSink(cancelling, "=cc-abc12345:").send(SendRequest("l1", "hi"))
                }.exceptionOrNull()
            assertTrue(
                "取消必须原样抛出去，不许变成一条带重试按钮的失败（那就是第二遍的入口），实得 outcome=$outcome",
                threw is kotlinx.coroutines.CancellationException,
            )
        }

    /** 那句话过禁用词表：同 `ChatAccountGateTest` 的口径，大小写不敏感。 */
    @Test
    fun theRefusalObeysTheProductWordList() {
        val banned = listOf("连接", "SSH", "tmux", "会话", "exec", "bridge", "daemon", "offset")
        val lower = TmuxSendKeysSink.INTERRUPT_UNSUPPORTED.lowercase()
        banned.forEach {
            assertFalse(
                "禁用词「$it」不许出现在用户看得见的地方：${TmuxSendKeysSink.INTERRUPT_UNSUPPORTED}",
                lower.contains(it.lowercase()),
            )
        }
    }
}
