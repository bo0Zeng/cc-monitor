package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.claude.transport.SignalSource
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * 接线判据：总览通路上的等待态真的投进了跨通路信号汇。
 *
 * `DaemonSignalBridge` 与它两条到达路径各有判据；可若没有生产代码调它（`SessionOverviewViewModel`
 * 收到快照时调一次 `onSnapshot`），就是「已 build 零引用」：聊天屏那半永远收不到等待态，
 * 而 `SessionSignalsTest` 与 `ChatSignalsTest` 各自造自己的汇、各自都绿。
 *
 * 它量的是总线的输出，不是源码：不扫 `SessionOverviewViewModel.kt` 里有没有 `onSnapshot(` 这几个字符
 * （那种针连注释和函数声明都匹配）。这里喂一份真 daemon 帧给真 ViewModel，然后从 [SessionSignals] 上读。
 *
 * 射程之外：
 * - 不钉「聊天屏读到了」：那一跳走 `ChatController` 注入的同一个 `single`，由 `ChatSignalsTest` 守；
 *   这里只钉「总览这一端真的往里投了」。
 * - 不钉时刻的真实性：`nowMs` 是构造参数，本条喂的是一个固定值。
 *   「它是什么时候开始等的」流上根本没有（见 `SessionSignals` 头注里时刻的含义）。
 */
@Suppress("DEPRECATION")
class SessionOverviewSignalBridgeTest {
    private val dispatcher = UnconfinedTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** 与 `SessionOverviewResyncTest` 同一个取法：不手编 hello，那会绕开能力协商。 */
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

    private class Channel(
        private val lines: List<String>,
    ) : RemoteCommandChannel {
        override fun exec(command: String) =
            if (command.startsWith("for f in ")) {
                flowOf(ByteArray(0))
            } else {
                flowOf((lines.joinToString("\n") + "\n").toByteArray())
            }
    }

    private fun vmOver(
        lines: List<String>,
        bus: SessionSignals,
    ) = SessionOverviewViewModel(
        source = DaemonSessionSource(Channel(lines), "/opt/d"),
        signals = bus,
        probeTimeoutMs = 1_000,
        nowMs = { OBSERVED_AT_MS },
    )

    /**
     * 一条 `activity=needs_you` 的会话进来 ⇒ 总线上真的多了一条等待态。
     *
     * 变异：把 VM 里那句 `DaemonSignalBridge.onSnapshot(...)` 删掉 ⇒ 本条红。
     * 夹具用的是后端帧的形状（帧上没有 `status`）；「前提：到了屏上的『需手动』区」靠的是灯，灯读活动档。
     */
    @Test
    fun aWaitingSnapshotReachesTheCrossChannelBus() =
        runTest(dispatcher) {
            val bus = SessionSignals()
            val vm =
                vmOver(
                    listOf(
                        realHello(),
                        """{"kind":"session_added","sid":"w1","path":"/p/w1.jsonl",""" +
                            """"activity":"needs_you","waiting_for":"permission prompt"}""",
                    ),
                    bus,
                )
            val job = launch { vm.state.collect { } }
            yield()

            // 前提：这条会话确实到了屏上的「需手动」那一区，否则下面读到 null 分不清是
            //   「桥没接」还是「这份夹具根本没喂出一条等待态的会话」。
            assertEquals(
                "前提：夹具得真的产出一条在等人的会话",
                listOf("w1"),
                vm.state.value.needsYou
                    .map { it.sessionId },
            )

            val waiting = bus.signalFor("w1")?.waiting
            assertTrue("总线上一条等待态都没有 —— `DaemonSignalBridge` 又成了零调用方", waiting != null)
            assertEquals("等什么要原样带进总线（翻译是屏的事，不是总线的事）", "permission prompt", waiting!!.waitingFor)
            assertEquals("来源要如实记成「合并后的快照」", SignalSource.DAEMON_SNAPSHOT, waiting.source)
            assertEquals("时刻走注入的时钟，不是 `System.currentTimeMillis()`", OBSERVED_AT_MS, waiting.observedAtMs)

            // 接上之后，拦不拦上行这条结论也该在总线上算得出来（这是接线的意义所在）
            val gate = bus.waitingGate("w1", OBSERVED_AT_MS)
            assertTrue("新鲜的等待态该拦住上行", gate.blocks)
            job.cancel()
        }

    /**
     * 阴性对照：不在等人的会话（`activity=working`）不许在总线上留下等待态。
     *
     * 没有这条的话，一个「见到任何快照就无脑投一条 waiting」的桥也能让上面那条绿，
     * 而那样的桥会把每一条在跑的对话都变成「在等人」，屏上从此永远拦着上行。
     */
    @Test
    fun aBusySnapshotLeavesNoWaitingSignalBehind() =
        runTest(dispatcher) {
            val bus = SessionSignals()
            val vm =
                vmOver(
                    listOf(
                        realHello(),
                        """{"kind":"session_added","sid":"b1","path":"/p/b1.jsonl","activity":"working"}""",
                    ),
                    bus,
                )
            val job = launch { vm.state.collect { } }
            yield()

            assertEquals(
                "前提：这条会话确实到了屏上（否则「总线上没有」是因为它压根没进来）",
                listOf("b1"),
                vm.state.value.running
                    .map { it.sessionId },
            )
            assertNull("在跑的对话不许被投成「在等人」", bus.signalFor("b1")?.waiting)
            assertTrue("也就不该拦上行", !bus.waitingGate("b1", OBSERVED_AT_MS).blocks)
            job.cancel()
        }

    private companion object {
        /** 注入的固定「本地观察时刻」。不是「它开始等的时刻」：流上没有后者。 */
        const val OBSERVED_AT_MS = 1_700_000_000_000L
    }
}
