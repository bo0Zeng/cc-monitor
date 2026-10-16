package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * 常驻流在 Kotlin 这一侧只剩落地：核心交的事（`state` · `frame` · `watch_from` · `reply`）落成几条流，
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
        val deploys = mutableListOf<String>()
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

        override fun deploy(
            handle: Long,
            slots: String,
        ): Boolean {
            deploys += slots
            return true
        }

        fun emit(
            kind: String,
            body: String,
        ) = events!!.invoke(kind, body)
    }

    private fun TestScope.feed(
        core: FakeCore,
        ssh: Flow<String?> = MutableStateFlow("""{"host":"h"}"""),
    ): ResidentFeed =
        ResidentFeed(
            scope = backgroundScope,
            core = core,
            machine = { "box" },
            ssh = ssh,
            mine = "p10g-mine",
            tz = "Asia/Shanghai",
        ).also { it.ensure() }

    @Test
    fun `看一台：名字 · 这一版 · 时区交给核心；拨号请求变了才交（同一份身份上换一条不交，断开交 null）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val ssh = MutableSharedFlow<String?>(replay = 1).apply { tryEmit("""{"host":"a"}""") }
            feed(core, ssh)
            assertEquals(Triple("box", "p10g-mine", "Asia/Shanghai"), core.opened)
            ssh.emit("""{"host":"a"}""")
            ssh.emit(null)
            ssh.emit("""{"host":"b"}""")
            assertEquals(listOf("""{"host":"a"}""", null, """{"host":"b"}"""), core.ssh)
        }

    /** SSH 够没够着与流通不通分开落：后端起不来（流断着要人动手）时 SSH 照样够着，SSH 那层跟它走。 */
    @Test
    fun `SSH 够没够着单独落，与流的状态不搅在一起`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            assertEquals(false, f.sshReached.value)
            core.emit("ssh", "true")
            core.emit("state", """{"state":"down","problem":{"text":"没装","detail":""},"needs_hand":true}""")
            assertEquals(true, f.sshReached.value)
            core.emit("ssh", "false")
            assertEquals(false, f.sshReached.value)
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
    fun `核心点名装一次会好的是哪一种：断着没装 ⇒ 安装 · 老后端 ⇒ 更新 · 通着旧了 ⇒ 更新；没点名 ⇒ 没有`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            core.emit("state", """{"state":"down","problem":{"text":"远端 cc-monitor 未装","detail":""},"needs_hand":true,"fix":"install"}""")
            assertEquals(LinkState.Down(Said("远端 cc-monitor 未装", ""), needsHand = true, fix = Fix.INSTALL), f.state.value)
            core.emit("state", """{"state":"down","problem":{"text":"box 后端要更新","detail":""},"needs_hand":true,"fix":"update"}""")
            assertEquals(Fix.UPDATE, (f.state.value as LinkState.Down).fix)
            core.emit("state", """{"state":"up","epoch":1,"notice":{"text":"box 后端要更新","detail":""},"fix":"update"}""")
            assertEquals(LinkState.Up(Said("box 后端要更新", ""), fix = Fix.UPDATE), f.state.value)
            core.emit("state", """{"state":"down","problem":{"text":"box 离线","detail":""},"needs_hand":false}""")
            assertEquals(null, (f.state.value as LinkState.Down).fix)
        }

    @Test
    fun `装后端：包里那几格交给核心；核心交回的每一步 · 结局 · 失败原样落（话是核心写好的）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            f.deploy { """[{"os":"Linux","arch":"x86_64","id":"p10g-mine","path":"/d/x"}]""" }
            assertEquals(listOf("""[{"os":"Linux","arch":"x86_64","id":"p10g-mine","path":"/d/x"}]"""), core.deploys)
            core.emit("deploy", """{"step":"place","text":"安装 cc-monitor → box","bar":true}""")
            assertEquals(Deploying.Step("安装 cc-monitor → box", bar = true), f.deploying.value)
            core.emit("deploy", """{"done":"deploy","text":"已安装后端 p10g-mine"}""")
            assertEquals(Deploying.Done("已安装后端 p10g-mine"), f.deploying.value)
            core.emit("state", """{"state":"up","epoch":3,"notice":null}""")
            assertEquals(Deploying.Done("已安装后端 p10g-mine"), f.deploying.value)
            core.emit("state", """{"state":"up","epoch":4,"notice":{"text":"box 后端要更新","detail":""},"fix":"update"}""")
            assertEquals("又说要更新 ⇒ 装好那一句收起", null, f.deploying.value)
            core.emit("deploy", """{"failed":"io_failed","text":"上传后校验失败","detail":"d"}""")
            assertEquals(Deploying.Failed(Said("上传后校验失败", "d")), f.deploying.value)
            core.emit("deploy", """{"what":1}""")
            assertEquals(Deploying.Failed(Said("上传后校验失败", "d")), f.deploying.value)
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
    fun `流上的帧折成会话表与几条流；拥塞那一句不叫重接（那台就地重报）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val kinds = mutableListOf<String>()
            backgroundScope.launch { f.changes.collect { kinds += it } }
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            assertEquals(listOf("sessions_replayed"), kinds)
            core.emit("frame", """{"kind":"overflow","said":"连接拥塞 · 会话状态已重新对齐"}""")
            assertEquals(listOf("sessions_replayed", "overflow"), kinds)
            assertFalse("拥塞不重接：那台在原连接上整份重报", core.kicks.contains(true))
        }

    @Test
    fun `changed 帧出成主题 ＋ 键（出口据此重问那一份）；别的帧不出`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val got = mutableListOf<Changed>()
            backgroundScope.launch { f.changed.collect { got += it } }
            core.emit("frame", """{"kind":"changed","topic":"needs","key":"s1"}""")
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            assertEquals(listOf(Changed("needs", "s1")), got)
        }

    @Test
    fun `在看名单：每一处 hold 交一次、关掉交一次；核心交回的 from 原样发出去`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val froms = mutableListOf<WatchFrom>()
            backgroundScope.launch { f.from.collect { froms += it } }
            val h = f.hold("s1")
            h.close()
            h.close()
            assertEquals(listOf("s1" to true, "s1" to false), core.watches)
            core.emit("watch_from", """[{"sid":"s1","path":"/p/s1.jsonl","seq":7}]""")
            assertEquals(listOf(WatchFrom("s1", "/p/s1.jsonl", 7)), froms)
        }

    @Test
    fun `活卡帧解出来发到 live；认不出的那一帧只报一次契约对不上、不发`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val broken = mutableListOf<String>()
            val f =
                ResidentFeed(
                    scope = backgroundScope,
                    core = core,
                    machine = { "box" },
                    ssh = MutableStateFlow("""{"host":"h"}"""),
                    mine = "p10g-mine",
                    tz = "Asia/Shanghai",
                    onBreak = { broken += it },
                ).also { it.ensure() }
            val got = mutableListOf<LiveFrame>()
            backgroundScope.launch { f.live.collect { got += it } }
            core.emit("frame", """{"kind":"live_card","sid":"s1","card":4,"n":1,"op":"text","i":0,"s":"字"}""")
            core.emit("frame", """{"kind":"live_card","sid":"s1","card":4,"n":2,"op":"grow"}""")
            assertEquals(listOf(LiveFrame("s1", null, 4, 1, LiveFrame.Op.Text(0, "字"))), got)
            assertEquals(1, broken.size)
        }
}
