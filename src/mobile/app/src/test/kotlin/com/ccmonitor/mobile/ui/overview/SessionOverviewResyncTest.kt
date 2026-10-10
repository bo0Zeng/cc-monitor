package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * 丢过帧就自己重连一次。
 *
 * 没有这条的话，把 `autoResyncIfDropped(raw)` 整行删掉全部测试仍绿。
 */
@Suppress("DEPRECATION")
class SessionOverviewResyncTest {
    private val dispatcher = UnconfinedTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** fixture 里真有的流模式首帧（不手编 hello —— 那会绕开能力协商）。 */
    private fun realHello(): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        // 同时命中两个条件的还有那行 meta（它也写着「流模式首帧」但没有 `raw`）——
        //   用 `first {}` 会挑中它然后 NPE。必须 `mapNotNull` 之后再取首个。
        return f
            .readLines()
            .asSequence()
            .filter { it.contains("\"ver\":\"p1t\"") && it.contains("流模式首帧") }
            .mapNotNull { Regex("\"raw\":\"(.*)\"\\}$").find(it)?.groupValues?.get(1) }
            .first()
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    }

    private class CountingChannel(
        private val lines: List<String>,
    ) : RemoteCommandChannel {
        var streamExecs = 0

        override fun exec(command: String) =
            flowOf((lines.joinToString("\n") + "\n").toByteArray()).also {
                // 血统探针不是流；只数 daemon 那条
                if (!command.startsWith("for f in ")) streamExecs++
            }
    }

    /**
     * 收到 `overflow` ⇒ 自己重连一次，不等用户点。
     *
     * 没有它时 `reconnect()` 唯一调用方是重试按钮 ⇒ 用户不点补齐永不发生，
     * 而「少收了 N 行」会一直挂着 —— 说了问题却不解决。
     */
    @Test
    fun anOverflowTriggersOneAutomaticResync() =
        runTest(dispatcher) {
            val ch =
                CountingChannel(
                    listOf(
                        realHello(),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                        """{"kind":"overflow","dropped":3}""",
                    ),
                )
            val vm = SessionOverviewViewModel(DaemonSessionSource(ch, "/opt/d"), SessionSignals(), probeTimeoutMs = 1_000)
            val job = launch { vm.state.collect { } }
            yield()

            // 一代 = 探测 + 主流两次 exec；自动重连之后应该多出一代
            assertTrue("前提：至少跑过一代（探测+主流）：${ch.streamExecs}", ch.streamExecs >= 2)
            assertTrue("丢过帧就该自己重连，而不是干等用户点重试：${ch.streamExecs}", ch.streamExecs > 2)
            job.cancel()
        }

    /**
     * 用户不介入就只自动重连一次 —— 每条新流都立刻 overflow 时不许无限重连。
     *
     * 注意：守卫若按「代号」记，每次重连都产生新代号 ⇒ 新一代又拿到一次额度 ⇒ 无限重连链。
     * 那时这条测试不是断言失败，是整个测试挂住。
     */
    @Test
    fun theAutomaticResyncDoesNotLoopForever() =
        runTest(dispatcher) {
            val ch = CountingChannel(listOf(realHello(), """{"kind":"overflow","dropped":1}"""))
            val vm = SessionOverviewViewModel(DaemonSessionSource(ch, "/opt/d"), SessionSignals(), probeTimeoutMs = 1_000)
            val job = launch { vm.state.collect { } }
            yield()
            // 一代 = 探测 + 主流 2 次；自动重连恰好再来一代 ⇒ 上限 4 次，绝不该继续涨
            assertTrue("不许无限重连 —— 拥塞时重连只会更拥塞：${ch.streamExecs}", ch.streamExecs <= 4)
            job.cancel()
        }
}
