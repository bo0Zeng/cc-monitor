package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [DaemonTurnEndSource]：探测 → 能力门控 → 起流 → 折成「完成一轮」事件。
 * 证明的是各档降级判准各自成立，以及帧按后端契约到达时折出来的事件对不对。
 *
 * 全部用 `runTest` 的虚拟时钟（`testScheduler.currentTime` 喂进 [DaemonTurnEndSource.events] 的 `nowMs`），
 * 判据不等真时间；`withTimeoutOrNull` 那一层也走虚拟时钟、自动推进。
 */
@OptIn(ExperimentalCoroutinesApi::class) // `testScheduler.currentTime`：本文件的时钟就是它（判据不等真时间）
class DaemonTurnEndSourceTest {
    private val path = "/home/u/.cc-monitor/bin/cc-monitor-remote"

    /** fixture 里那条真录下来的 hello（`p1t`：capabilities=['bg','tail-only']、emits 含 `turn_end`）。 */
    private fun realHelloLine(ver: String): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        return f
            .readLines()
            .asSequence()
            .filter { it.contains("\"ver\":\"$ver\"") && it.contains("流模式首帧") }
            .mapNotNull {
                Regex("\"raw\":\"(.*)\"\\}$")
                    .find(it)
                    ?.groupValues
                    ?.get(1)
                    ?.replace("\\\"", "\"")
                    ?.replace("\\\\", "\\")
            }.firstOrNull() ?: error("fixture 里找不到 $ver 的流模式首帧")
    }

    /**
     * 假通道：按命令串分探测流与主流（不按调用次序）。
     *
     * 这个分法同时钉住探测必须是裸命令（见 `DaemonProbe` 头注）：探测那条命令里要是出现了
     * `--tail-only`，它就会被当成主流，多条判据当场红。
     */
    private class ByCommandChannel(
        private val hello: String,
        private val streamLines: List<String>,
        /** 主流发完 [streamLines] 之后挂着不结束（= 真实的长流）。false ⇒ 立刻结束（= 对端退出）。 */
        private val keepStreamOpen: Boolean,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        val streamCommands: List<String> get() = commands.filter { it.contains("--tail-only") }

        override fun exec(command: String): Flow<ByteArray> {
            commands += command
            val isStream = command.contains("--tail-only")
            val lines = if (isStream) listOf(hello) + streamLines else listOf(hello)
            return flow {
                emit((lines.joinToString("\n") + "\n").toByteArray())
                if (isStream && keepStreamOpen) awaitCancellation()
            }
        }
    }

    private fun turnEnd(
        sid: String,
        uuid: String,
    ) = """{"kind":"turn_end","session_id":"$sid","uuid":"$uuid"}"""

    private fun sessionAdded(
        sid: String,
        path: String,
    ) = """{"kind":"session_added","sid":"$sid","path":"$path"}"""

    // ===== daemon 流通了就走它 =====

    /**
     * 能力门全过 ⇒ [DaemonTurnEndSource.Event.Engaged]，然后 `turn_end` 帧折成按 path 的「完成一轮」。
     *
     * 起流必须带 `--tail-only`：不带的话后端会把历史里每一条 turn-end 都发成帧。
     */
    @Test
    fun whenEverythingIsDeclaredTheStreamEngagesAndRoundsComeOutPerPath() =
        runTest {
            val chan =
                ByCommandChannel(
                    hello = realHelloLine("p1t"),
                    streamLines =
                        listOf(
                            sessionAdded("s1", "/p/enc/s1.jsonl"),
                            // 后端逐记录发、不去重：同一轮多帧、uuid 各不同
                            turnEnd("s1", "u-a"),
                            turnEnd("s1", "u-b"),
                        ),
                    keepStreamOpen = true,
                )
            val events =
                DaemonTurnEndSource(chan, path)
                    .events(nowMs = { testScheduler.currentTime })
                    .take(2)
                    .toList()

            val engaged = events[0] as DaemonTurnEndSource.Event.Engaged
            assertEquals("p1t-removal-cause", engaged.hello.buildId)
            assertTrue("起流必须带 --tail-only，否则会被灌一串历史轮次", engaged.command.contains("--tail-only"))
            assertTrue("要的 bg 也要带上（否则后台会话连 session_added 都不宣告）", engaged.command.contains("--with-bg"))

            val round = (events[1] as DaemonTurnEndSource.Event.Round).done
            assertEquals("同一轮多帧只出一条事件、uuid 取最后一帧", "u-b", round.uuid)
            assertEquals("投到与逐行跟尾同一个槽（path）", "/p/enc/s1.jsonl", round.slotKey)

            assertEquals("探测一条、主流一条（DaemonSessionSource 头注记的那笔「两次 exec」代价）", 2, chan.commands.size)
            assertEquals(1, chan.streamCommands.size)
        }

    /**
     * 一条流管所有会话 ⇒ 两条会话不许互相压掉。
     * 去抖那一层由 `TurnEndDebouncerTest` 钉；这里钉的是整条流上真出两条事件。
     */
    @Test
    fun twoSessionsOnTheOneStreamBothGetTheirOwnRound() =
        runTest {
            val chan =
                ByCommandChannel(
                    hello = realHelloLine("p1t"),
                    streamLines =
                        listOf(
                            sessionAdded("s1", "/p/a/s1.jsonl"),
                            sessionAdded("s2", "/p/b/s2.jsonl"),
                            turnEnd("s1", "u1"),
                            turnEnd("s2", "u2"),
                        ),
                    keepStreamOpen = true,
                )
            val rounds =
                DaemonTurnEndSource(chan, path)
                    .events(nowMs = { testScheduler.currentTime })
                    .take(3)
                    .toList()
                    .filterIsInstance<DaemonTurnEndSource.Event.Round>()
            assertEquals(
                "两条会话各出一条，一条都不许被另一条压掉",
                listOf("/p/a/s1.jsonl" to "u1", "/p/b/s2.jsonl" to "u2"),
                rounds.map { it.done.slotKey to it.done.uuid },
            )
        }

    // ===== 降级，每档一条 =====

    /** 探不到 `hello`（没装 / 路径不对 / 不说话）⇒ `NotSpeaking`，并且带得出诊断。 */
    @Test
    fun aDaemonThatNeverSpeaksFallsBackWithADiagnosableReason() =
        runTest {
            val silent =
                object : RemoteCommandChannel {
                    override fun exec(command: String): Flow<ByteArray> = flow { }
                }
            val events = DaemonTurnEndSource(silent, path).events(nowMs = { testScheduler.currentTime }).toList()
            assertEquals(1, events.size)
            val u = events[0] as DaemonTurnEndSource.Event.Unavailable
            assertEquals(AlphaUnavailable.NotSpeaking, u.why)
            assertTrue("loud never silent：不许只说「失败了」，要带原因", u.detail.contains("hello"))
        }

    /**
     * daemon 不声明 `tail-only` ⇒ 不许起流。
     *
     * 素材是 fixture 里那条真录下来的 hello（`p1h-bg-badge`，缺 `capabilities`/`emits`）。
     * 不拦的话：`DaemonProbe.streamCommand` 拿 `want ∩ capabilities` 拼 flag ⇒ flag 被静默丢掉、
     * 命令看起来一切正常 ⇒ 后端全量推流 ⇒ 每条活会话在连上那一刻被灌一串历史轮次
     * （去抖折成每条会话一条假的「完成了」）。
     */
    @Test
    fun aDaemonThatDoesNotDeclareTailOnlyIsRefusedBecauseItWouldReplayHistory() =
        runTest {
            val chan = ByCommandChannel(hello = realHelloLine("p1h"), streamLines = emptyList(), keepStreamOpen = false)
            val events = DaemonTurnEndSource(chan, path).events(nowMs = { testScheduler.currentTime }).toList()
            assertEquals("不许 Engaged", 1, events.size)
            val u = events[0] as DaemonTurnEndSource.Event.Unavailable
            assertEquals(AlphaUnavailable.NoTailOnly, u.why)
            assertTrue("要说出是哪一版", u.detail.contains("p1h-bg-badge"))
            assertEquals("一条主流都不许起", emptyList<String>(), chan.streamCommands)
        }

    /**
     * 声明了 `tail-only` 但 `emits` 里没有 `turn_end` ⇒ `NoTurnEndFrames`：缺则不依赖该帧、回退逐行跟尾。
     *
     * 顺序也钉住：`tail-only` 那一档排在前面，它是「会做错事」，比「用不了」更要紧。
     */
    @Test
    fun aDaemonThatDeclaresTailOnlyButNoTurnEndFramesFallsBack() =
        runTest {
            val hello = """{"kind":"hello","v":1,"build_id":"p1x-fake","capabilities":["bg","tail-only"],"emits":["line"]}"""
            val chan = ByCommandChannel(hello = hello, streamLines = emptyList(), keepStreamOpen = false)
            val events = DaemonTurnEndSource(chan, path).events(nowMs = { testScheduler.currentTime }).toList()
            assertEquals(1, events.size)
            assertEquals(
                AlphaUnavailable.NoTurnEndFrames,
                (events[0] as DaemonTurnEndSource.Event.Unavailable).why,
            )
            assertEquals("一条主流都不许起", emptyList<String>(), chan.streamCommands)
        }

    /**
     * 流起来过、然后结束了 ⇒ 必须先把没结算的倒出来，再说 `StreamDied`。
     *
     * 不倒就漏通知：流断 ⇒ 回落逐行跟尾 ⇒ 它第一轮吞历史 ⇒ 静默窗口里那一轮两条路都不发。
     * 删掉 `pump` 里那行 `route.flushAll(...)` ⇒ 这条红。
     */
    @Test
    fun whenTheStreamEndsThePendingRoundIsFlushedBeforeReportingDeath() =
        runTest {
            val chan =
                ByCommandChannel(
                    hello = realHelloLine("p1t"),
                    streamLines = listOf(sessionAdded("s1", "/p/a/s1.jsonl"), turnEnd("s1", "u1")),
                    // daemon 退出 ⇒ 流正常结束，而那一轮还在静默窗口里
                    keepStreamOpen = false,
                )
            val events = DaemonTurnEndSource(chan, path).events(nowMs = { testScheduler.currentTime }).toList()
            assertEquals("Engaged · Round · Unavailable 三条，顺序不许换", 3, events.size)
            assertTrue(events[0] is DaemonTurnEndSource.Event.Engaged)
            assertEquals(
                "没结算的那一轮必须在报死之前倒出来",
                "u1",
                (events[1] as DaemonTurnEndSource.Event.Round).done.uuid,
            )
            val u = events[2] as DaemonTurnEndSource.Event.Unavailable
            assertEquals(AlphaUnavailable.StreamDied, u.why)
            assertTrue("「正常结束」也要说出来 —— 静默完成会让服务以为 daemon 流还在供货", u.detail.contains("对端退出"))
        }

    /**
     * 绝不静默结束：流无论从哪条路走到尽头，最后一条事件必须是 `Unavailable`。
     *
     * 一条「起了流、然后无声无息地停了」的流会让 `TurnEndPathArbiter` 以为它还在供货 ⇒
     * 逐行跟尾永远不起来 ⇒ 再也收不到完成通知，而且没有任何线索。
     */
    @Test
    fun everyWayTheStreamCanEndFinishesWithAnUnavailableEvent() =
        runTest {
            val cases =
                listOf(
                    "探不到 hello" to
                        object : RemoteCommandChannel {
                            override fun exec(command: String): Flow<ByteArray> = flow { }
                        },
                    "太旧（不声明 tail-only）" to
                        ByCommandChannel(realHelloLine("p1h"), emptyList(), keepStreamOpen = false),
                    "起来过然后对端退出" to
                        ByCommandChannel(realHelloLine("p1t"), listOf(turnEnd("s1", "u1")), keepStreamOpen = false),
                )
            for ((name, chan) in cases) {
                val events = DaemonTurnEndSource(chan, path).events(nowMs = { testScheduler.currentTime }).toList()
                assertTrue(
                    "「$name」这条路的最后一条事件必须是 Unavailable（静默结束 ⇒ 逐行跟尾永远不起来）",
                    events.last() is DaemonTurnEndSource.Event.Unavailable,
                )
            }
        }

    /** 探测用的那条命令必须是裸的（见 `DaemonProbe` 头注；这里在本流上再确认一次）。 */
    @Test
    fun theProbeCommandCarriesNoFlagsAtAll() =
        runTest {
            val chan = ByCommandChannel(realHelloLine("p1t"), emptyList(), keepStreamOpen = false)
            DaemonTurnEndSource(chan, path).events(nowMs = { testScheduler.currentTime }).toList()
            val probeCmd = chan.commands.first()
            assertTrue("探测必须带 --stream", probeCmd.contains("--stream"))
            assertNull(
                "探测一个能力 flag 都不许带（发 daemon 不认识的 flag ⇒ 重连死循环）",
                listOf("--with-bg", "--tail-only", "--with-raw", "--with-pid").firstOrNull { probeCmd.contains(it) },
            )
        }
}
