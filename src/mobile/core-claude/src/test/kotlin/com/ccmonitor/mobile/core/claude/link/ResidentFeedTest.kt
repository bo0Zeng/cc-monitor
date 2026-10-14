package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 常驻流在 Kotlin 这一侧只剩落地：核心交的事（`state` · `frame` · `watch_from` · `lost` · `reply`）落成几条流，
 * 一问一答 · 在看名单 · ［重试］原样交给核心。重连 · 握手 · 版本门 · 退避都在核心里（`resident-client` 的判据管）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ResidentFeedTest {
    /** 核心的替身：记下每一下，交事经 [emit]。 */
    private class FakeCore : ResidentCore {
        var events: ((String, String) -> Unit)? = null
        val ssh = mutableListOf<String?>()
        val calls = mutableListOf<Triple<String, String, Long>>()
        val watches = mutableListOf<Pair<String, Boolean>>()
        val kicks = mutableListOf<Boolean>()
        var opened: Triple<String, String, String>? = null

        override fun open(
            machine: String,
            mine: String,
            tz: String,
            events: (String, String) -> Unit,
        ): Long {
            opened = Triple(machine, mine, tz)
            this.events = events
            return 9
        }

        override fun ssh(
            handle: Long,
            request: String?,
        ) {
            ssh += request
        }

        override fun call(
            handle: Long,
            cmd: String,
            args: String,
            withinMs: Long,
        ): Long {
            calls += Triple(cmd, args, withinMs)
            return calls.size.toLong()
        }

        override fun watch(
            handle: Long,
            sid: String,
            holding: Boolean,
        ) {
            watches += sid to holding
        }

        override fun kick(
            handle: Long,
            retry: Boolean,
        ) {
            kicks += retry
        }

        fun emit(
            kind: String,
            body: String,
        ) = events!!.invoke(kind, body)
    }

    private fun TestScope.feed(
        core: FakeCore,
        ssh: MutableStateFlow<SshLink?> = MutableStateFlow(SshLink("""{"host":"h"}""")),
        lost: () -> Unit = {},
    ): ResidentFeed =
        ResidentFeed(
            scope = backgroundScope,
            core = core,
            machine = { "box" },
            ssh = ssh,
            mine = "p10g-mine",
            tz = "Asia/Shanghai",
            onLost = lost,
        ).also { it.ensure() }

    @Test
    fun `看一台：名字 · 这一版 · 时区交给核心；SSH 那层每换一条交一次（请求一字不差也交，没通交 null）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val ssh = MutableStateFlow<SshLink?>(SshLink("""{"host":"a"}"""))
            feed(core, ssh)
            assertEquals(Triple("box", "p10g-mine", "Asia/Shanghai"), core.opened)
            ssh.value = null
            ssh.value = SshLink("""{"host":"a"}""")
            ssh.value = SshLink("""{"host":"a"}""")
            assertEquals(listOf("""{"host":"a"}""", null, """{"host":"a"}""", """{"host":"a"}"""), core.ssh)
        }

    @Test
    fun `核心说的状态原样落：通着带一句要更新 · 断着要人动手 · epoch 跟着变`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            core.emit("state", """{"state":"up","epoch":1,"notice":{"text":"box 后端要更新","detail":"theirs: p9"}}""")
            assertEquals(LinkState.Up(Said("box 后端要更新", "theirs: p9")), f.state.value)
            assertEquals(1L, f.epoch.value)
            core.emit("state", """{"state":"down","problem":{"text":"远端 cc-monitor 未装","detail":""},"needs_hand":true}""")
            assertEquals(LinkState.Down(Said("远端 cc-monitor 未装", ""), needsHand = true), f.state.value)
            core.emit("state", """{"state":"up","epoch":2,"notice":null}""")
            assertEquals(LinkState.Up(null), f.state.value)
            assertEquals(2L, f.epoch.value)
        }

    @Test
    fun `一问一答：交给核心、按编号等它交回的那一行`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val ok = async { f.call("history-list", mapOf("limit" to 3), 7_000) }
            assertEquals(Triple("history-list", """{"limit":3}""", 7_000L), core.calls.single())
            core.emit("reply", """{"call":1,"ok":true,"data":{"rows":[]}}""")
            assertEquals(Reply.Ok(mapOf("rows" to emptyList<Any>())), ok.await())
            val failed = async { f.call("session-new") }
            core.emit("reply", """{"call":2,"ok":false,"code":"differs","message":"box 后端版本不同","detail":"d"}""")
            assertEquals(Reply.Failed(CoreFailure("differs", "box 后端版本不同", "d", null)), failed.await())
            for ((line, want) in listOf("""{"call":3,"timeout":true}""" to Reply.TimedOut, """{"call":4,"down":true}""" to Reply.LinkDown)) {
                val r = async { f.call("ping") }
                core.emit("reply", line)
                assertEquals(want, r.await())
            }
        }

    @Test
    fun `流上的帧折成会话表与几条流；丢了会话帧就叫核心整份重来`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val kinds = mutableListOf<String>()
            backgroundScope.launch { f.changes.collect { kinds += it } }
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            assertEquals(listOf("sessions_replayed"), kinds)
            core.emit("frame", """{"kind":"overflow","dropped":3,"lost":[],"lost_truncated":true}""")
            assertTrue("丢了会话帧 ⇒ ［重试］那一下（整份重来）", core.kicks.contains(true))
        }

    @Test
    fun `changed 帧出成主题 ＋ 键（出口据此重问那一份）；别的帧不出`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val got = mutableListOf<Changed>()
            backgroundScope.launch { f.pushes.collect { got += it } }
            core.emit("frame", """{"kind":"changed","topic":"needs","key":"s1"}""")
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            assertEquals(listOf(Changed("needs", "s1")), got)
        }

    @Test
    fun `在看名单：每一处 hold 交一次、关掉交一次；核心交回的 from 原样发出去；断了请 SSH 那层探活`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            var lost = 0
            val f = feed(core, lost = { lost++ })
            val froms = mutableListOf<WatchFrom>()
            backgroundScope.launch { f.from.collect { froms += it } }
            val h = f.hold("s1")
            h.close()
            h.close()
            assertEquals(listOf("s1" to true, "s1" to false), core.watches)
            core.emit("watch_from", """[{"sid":"s1","path":"/p/s1.jsonl","seq":7}]""")
            assertEquals(listOf(WatchFrom("s1", "/p/s1.jsonl", 7)), froms)
            core.emit("lost", "")
            assertEquals(1, lost)
        }
}
