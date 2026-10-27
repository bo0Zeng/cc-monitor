package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.BatchAnswer
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.Decide
import com.ccmonitor.mobile.core.wire.gen.HiddenNow
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.LiveCards
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MenuEntry
import com.ccmonitor.mobile.core.wire.gen.PhoneMenu
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
import org.junit.Assert.assertNull
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

        val transfers = mutableListOf<List<Any>>()
        val batches = mutableListOf<String>()
        val answers = mutableListOf<Pair<Long, String?>>()

        /** 读不动的那一份（`{kept}`）回 0。 */
        var unreadable: String? = null

        override fun batchStart(
            handle: Long,
            want: String,
        ): Long {
            batches += want
            return if (unreadable?.let { it in want } == true) 0L else 40L + batches.size
        }

        override fun batchAnswer(
            id: Long,
            answer: String?,
        ) {
            answers += id to answer
        }

        val picks = mutableListOf<Pair<Long, String>>()

        override fun clashRows(data: String): String = """[{"key":"0","name":"a","meta":"m","answers":["skip"],"there":null}]"""

        override fun searchMerge(data: String): String = ""

        override fun downloadEvent(want: String): String = """{"id":-1,"phase":"cut","title":"t","kept":false,"line":"l","resume":true}"""

        override fun answerOf(
            keys: String,
            answer: String?,
        ): String {
            answered += keys to answer
            return """{"0":"skip","1":"overwrite"}"""
        }

        val answered = mutableListOf<Pair<String, String?>>()

        override fun transferStop(id: Long) {
            transfers += listOf("stop", id)
        }

        override fun pickMenu(
            handle: Long,
            rows: String,
        ): String {
            picks += handle to rows
            return """{"bar":[{"id":"download","text":"下载","enabled":false,"why":"docs"}],"more":[],"tap":"open"}"""
        }

        override fun batchStop(id: Long) {
            answers += id to "stop"
        }

        override fun stage(
            handle: Long,
            fd: Int,
            name: String,
            withinMs: Long,
        ): Long {
            transfers += listOf("stage", fd, name, withinMs)
            return 40L + transfers.size
        }

        override fun pull(
            handle: Long,
            fd: Int,
            args: String,
            shown: String,
            withinMs: Long,
        ): Long {
            transfers += listOf("pull", fd, args, shown, withinMs)
            return 40L + transfers.size
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

    /** 落盘那张表的替身：票 ⇒ 那一串。 */
    private class Disk : KeptBatches {
        val rows = linkedMapOf<String, String>()

        override suspend fun all(): List<Pair<String, String>> = rows.toList()

        override suspend fun put(
            ticket: String,
            plan: String,
        ) {
            rows[ticket] = plan
        }

        override suspend fun drop(ticket: String) {
            rows -= ticket
        }
    }

    private fun TestScope.feed(
        core: FakeCore,
        ssh: Flow<String?> = MutableStateFlow("""{"host":"h"}"""),
        onBreak: (String) -> Unit = {},
        disk: KeptBatches = Disk(),
    ): ResidentFeed =
        ResidentFeed(
            scope = backgroundScope,
            core = core,
            machine = { "box" },
            ssh = ssh,
            mine = "p10g-mine",
            tz = "Asia/Shanghai",
            kept = disk,
            onBreak = onBreak,
        ).also { it.ensure() }

    /** 金样里那一形的 `state` 那一行（核心现产的原样）。 */
    private fun line(name: String): String =
        Json.args(
            Fixtures
                .json(GOLDEN)
                .objs("cases")
                .need()
                .first { it.str("name") == name }
                .obj("line")
                .need(),
        )

    private fun fixOf(wire: String): MachineFix = MachineFix.entries.first { it.name == wire.uppercase() }

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
            core.emit("state", line("没有后端"))
            assertEquals(true, f.sshReached.value)
            core.emit("ssh", "false")
            assertEquals(false, f.sshReached.value)
        }

    /**
     * `state` 那一行读的是核心现产的那一份（跨语言金样 `mobile-state-line.golden.json`，Rust 那一侧逐字比）：[CoreLink.state] 就是那一行
     * （契约里的 [LinkLine]：通没通 ＋ 那台的 [MachineState]），手机不另抄一份；那一句 · 详情 · 修法 · 离线不离线都从成品取，
     * 部署那一趟的几步（成品说正在问 / 装 / 更新）由 [busy] 照成品那一态取。
     */
    @Test
    fun `核心交的每一行状态原样落：通没通照标签、成品逐格不动，epoch 跟着变`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val cases = Fixtures.json(GOLDEN).objs("cases").need()
            val busy =
                mapOf(
                    "部署在问那台" to Busy("连接 box · 核对版本", bar = false),
                    "正在装" to Busy("安装 cc-monitor → box", bar = true),
                    "正在更新" to Busy("更新 box · cc-monitor", bar = true),
                )
            assertNull("核心还没交过 ⇒ 没有那一行（当正在连）", f.state.value)
            assertNull("成品还没到 ⇒ 没有离线那一句（未知不当离线）", f.offline.value)
            for (c in cases) {
                val name = c.str("name").need()
                val line = c.obj("line").need()
                val m = line.obj("machine").need()
                core.emit("state", Json.args(line))
                assertEquals("$name：离线那一句照成品", m.str("offlineSaid").need(), f.offline.value)
                val got = f.state.value!!
                assertEquals(name, line.str("state"), got.state)
                assertEquals(name, m.str("said").need(), got.machine.said)
                assertEquals(name, m.str("detail"), got.machine.detail)
                assertEquals(name, m["offline"] as Boolean, got.machine.offline)
                val fixes = m.objs("fixes").need().map { MachineFixButton(fixOf(it.str("fix").need()), it.str("label").need()) }
                assertEquals(name, fixes, got.machine.fixes)
                assertEquals(name, busy[name], got.busy())
                line.num("epoch")?.let { assertEquals(name, it, f.epoch.value) }
            }
            assertEquals(12, cases.size)
            assertEquals(busy.keys, cases.map { it.str("name").need() }.filter { it in busy }.toSet())
        }

    /** 装一次会好的那一颗只照成品挑：没有后端 ⇒［安装］· 后端旧了 · 那台旧 ⇒［更新］；离线 · 密钥被拒 · 那台新 ⇒ 没有。 */
    @Test
    fun `装一次会好的那一颗照成品挑`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val cases =
                listOf(
                    "没有后端" to MachineFix.INSTALL,
                    "后端旧了" to MachineFix.UPDATE,
                    "那台旧" to MachineFix.UPDATE,
                    "离线" to null,
                    "密钥被拒" to null,
                    "那台新" to null,
                )
            for ((name, fix) in cases) {
                core.emit("state", line(name))
                val got = f.state.value.deployFix()
                assertEquals(name, fix, got?.fix)
            }
        }

    /** 成品装不进（缺格 · 修法认不出）⇒ 报一次契约对不上，状态不动。 */
    @Test
    fun `成品装不进 ⇒ 状态不动、报契约对不上`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val breaks = mutableListOf<String>()
            val f = feed(core, onBreak = { breaks += it })
            core.emit("state", line("连着"))
            core.emit("state", """{"state":"down","machine":{"said":"x","detail":null,"fixes":[{"fix":"fly","label":"y"}]}}""")
            core.emit("state", """{"state":"down"}""")
            assertEquals(Wire.read<LinkLine>(line("连着")), f.state.value)
            assertEquals(2, breaks.size)
        }

    @Test
    fun `装后端：包里那几格交给核心；核心交回的结局 · 失败原样落（话是核心写好的；走到哪一步在成品里）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            f.deploy { """[{"os":"Linux","arch":"x86_64","id":"p10g-mine","path":"/d/x"}]""" }
            assertEquals(listOf("""[{"os":"Linux","arch":"x86_64","id":"p10g-mine","path":"/d/x"}]"""), core.deploys)
            core.emit("deploy", """{"end":"done","act":"deploy","text":"已安装后端 p10g-mine"}""")
            assertEquals(Deploying.Done("已安装后端 p10g-mine"), f.deploying.value)
            core.emit("state", line("连着"))
            assertEquals(Deploying.Done("已安装后端 p10g-mine"), f.deploying.value)
            core.emit("state", line("那台旧"))
            assertEquals("又说要更新 ⇒ 装好那一句收起", null, f.deploying.value)
            core.emit("deploy", """{"end":"failed","code":"io_failed","text":"上传后校验失败","detail":"d"}""")
            assertEquals(Deploying.Failed(Said("上传后校验失败", "d")), f.deploying.value)
            // 缺一格（详情）· 认不出的结局 ⇒ 不补默认、不动，报契约对不上。
            core.emit("deploy", """{"end":"failed","code":"io_failed","text":"别的"}""")
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
            // 回包里多一格这一版不认的：原样交到这里（核心不先解再转）。
            core.emit("reply", """{"call":1,"end":"ok","data":{"rows":[],"later":{"x":1}}}""")
            assertEquals(Reply.Ok(mapOf("rows" to emptyList<Any>(), "later" to mapOf("x" to 1.0))), ok.await())
            val failed = async { f.call("session-open") }
            core.emit("reply", """{"call":2,"end":"failed","code":"differs","message":"box 后端版本不同","detail":"d","data":null}""")
            assertEquals(Reply.Failed(CoreFailure("differs", "box 后端版本不同", "d", null)), failed.await())
            for ((line, want) in listOf("""{"call":3,"end":"timeout"}""" to Reply.TimedOut, """{"call":4,"end":"down"}""" to Reply.LinkDown)) {
                val r = async { f.call("ping") }
                core.emit("reply", line)
                assertEquals(want, r.await())
            }
        }

    @Test
    fun `分块传：描述符交给核心、走到哪按编号交给那一趟、结局同一问一答那一行`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val seen = mutableListOf<Pair<Long, Long>>()
            val up = async { f.transfer(Transfer.Stage("up.bin"), { 17 }, 20_000) { d, t -> seen += d to t } }
            assertEquals(listOf<Any>("stage", 17, "up.bin", 20_000L), core.transfers.single())
            core.emit("moved", """{"call":41,"done":5,"total":10}""")
            core.emit("moved", """{"call":99,"done":1,"total":1}""")
            core.emit("moved", """{"call":41,"done":10,"total":null}""")
            core.emit("reply", """{"call":41,"end":"ok","data":{"key":"k"}}""")
            assertEquals(Reply.Ok(mapOf("key" to "k")), up.await())
            assertEquals("别的那一趟的不算；总数不知道 ⇒ -1", listOf(5L to 10L, 10L to -1L), seen)
            val down = async { f.transfer(Transfer.Pull(RemotePath.of("/w/f"), "/sd/f"), { 18 }, 9_000) { _, _ -> } }
            assertEquals(listOf<Any>("pull", 18, """{"path":"/w/f"}""", "/sd/f", 9_000L), core.transfers.last())
            core.emit("reply", """{"call":42,"end":"down"}""")
            assertEquals(Reply.LinkDown, down.await())
        }

    @Test
    fun `那一趟不等了（停）⇒ 按编号叫核心撤，不另起结局`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val down = async { f.transfer(Transfer.Pull(RemotePath.of("/w/f"), "/sd/f"), { 18 }, 9_000) { _, _ -> } }
            down.cancel()
            assertEquals(listOf<Any>("stop", 41L), core.transfers.last())
            assertEquals(2, core.transfers.size)
        }

    @Test
    fun `同名那一答交核心认：那几行的键与交核心那一形进去，每个键一格出来`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            assertEquals(mapOf("0" to Decide.SKIP, "1" to Decide.OVERWRITE), f.decided(listOf("0", "1"), BatchAnswer.Each(mapOf("1" to Decide.OVERWRITE))))
            assertEquals(listOf("""["0","1"]""" to """{"each":{"1":"overwrite"}}"""), core.answered)
            f.decided(listOf("0"), null)
            assertEquals("""["0"]""" to null, core.answered.last())
        }

    /** ［重试］（会话列表 · 文件页同一条）：断着（停在要人动手那一步的「没有后端」也算）⇒ 交核心整份重接、不另问；通着 ⇒ 只重问。 */
    @Test
    fun `［重试］断着就整份重接，通着只重问`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            var asked = 0
            core.emit("state", line("没有后端"))
            f.retry { asked++ }
            assertEquals(listOf(true), core.kicks)
            assertEquals(0, asked)
            core.emit("state", line("连着"))
            f.retry { asked++ }
            assertEquals(listOf(true), core.kicks)
            assertEquals(1, asked)
        }

    @Test
    fun `拥塞那一句不叫重接（那台就地重报）`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            feed(core)
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            core.emit("frame", """{"kind":"overflow","said":"连接拥塞 · 会话状态已重新对齐"}""")
            assertFalse("拥塞不重接：那台在原连接上整份重报", core.kicks.contains(true))
        }

    @Test
    fun `能做什么：列表那一行原样交核心（整数照整数写），核心那一行解成菜单`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val f = feed(core)
            val reply =
                """{"held":"h1","whole":true,"same":false,"order":["docs"],"up":{"docs":{"path":"/h/docs","name":"docs","kind":"dir",""" +
                    """"class":"folder","type_text":"","mtime_secs":1700000000,"size":12}},""" +
                    """"head":{"total":1,"info":{"up":null,"crumbs":[],"truncated":false,"total":1,"unreadable":0}}}"""
            val listing = FilesLs.of(Json.obj(reply))!!
            val menu = f.picks(listing.entries.map { it.row })
            assertEquals(
                """[{"path":"/h/docs","name":"docs","kind":"dir","class":"folder","type_text":"","mtime_secs":1700000000,"size":12}]""",
                core.picks.single().second,
            )
            assertEquals(PhoneMenu(listOf(MenuEntry("download", "下载", false, "docs")), emptyList(), "open"), menu)
        }

    @Test
    fun `changed 帧按主题出成那一臂、体是那一臂的类型；缺那一臂的必填格 ⇒ 不出、报契约对不上；别的帧不出`() =
        runTest(UnconfinedTestDispatcher()) {
            val core = FakeCore()
            val breaks = mutableListOf<String>()
            val f = feed(core, onBreak = { breaks += it })
            val got = mutableListOf<Changed>()
            backgroundScope.launch { f.changed.collect { got += it } }
            core.emit("frame", """{"kind":"changed","topic":"rotation","key":"s1"}""")
            core.emit("frame", """{"kind":"changed","topic":"session_hidden","key":"s2","body":{"hidden":null}}""")
            core.emit("frame", """{"kind":"changed","topic":"history_list"}""")
            core.emit("frame", """{"kind":"changed","topic":"needs"}""")
            core.emit("frame", """{"kind":"sessions_replayed"}""")
            assertEquals(listOf(Changed.Rotation("s1"), Changed.SessionHidden("s2", HiddenNow(null)), Changed.HistoryList), got)
            assertEquals(1, breaks.size)
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
    fun `一组落盘那一份原样存；进程没了再开，带去再问的那一份逐字节等于落盘那一份；接回来的卡交给屏；收场删`() =
        runTest(UnconfinedTestDispatcher()) {
            val disk = Disk()
            val before = FakeCore()
            feed(before, disk = disk)
            // 带转义 · 非 ASCII · 键序不按字母：经 JSON 一来一回也得一字不差。
            val plan = """{"cmd":"files-delete","args":{"root":"/w/文档","items":[{"rel":"a \"b\""}],"ticket":"t-1"},"done":1}"""
            before.emit("batch_keep", Json.args(mapOf("ticket" to "t-1", "plan" to plan)))
            before.emit("batch_keep", Json.args(mapOf("ticket" to "t-2", "plan" to "坏的")))
            assertEquals(mapOf("t-1" to plan, "t-2" to "坏的"), disk.rows)

            val after = FakeCore().apply { unreadable = "坏的" }
            val f = feed(after, disk = disk)
            val asked = after.batches.map { (Json.value(it) as Map<*, *>)["kept"] }
            assertEquals(listOf(plan, "坏的"), asked)
            assertEquals("核心读不动的那一份不留", listOf("t-1"), disk.rows.keys.toList())

            // 接回来的那一组：屏后开也照最后那一格画；收场的那一格只交一次（屏上没人看着时交给后开的那一屏）。
            after.emit("batch", """{"id":41,"phase":"cut","title":"删除 1 项","kept":true,"line":"box 连接已断开","resume":false}""")
            after.emit("batch", """{"id":7,"phase":"running","title":"复制 2 项","kept":false,"current":null,"frac":null}""")
            val phases = { f.resumed().map { it.id to it.phase } }
            assertEquals("只记接回来的那几组", listOf(41L to "cut"), phases())
            assertEquals("没收场的一直交", listOf(41L to "cut"), phases())
            after.emit(
                "batch",
                """{"id":41,"phase":"ended","title":"删除 1 项","kept":true,"said":"已删除 1 项","failed_title":"","failed":[],"details":null,"retry":false,"undo":false}""",
            )
            assertEquals(listOf(41L to "ended"), phases())
            assertEquals("收场那一格只交一次", emptyList<Pair<Long, String>>(), phases())
            after.emit("batch_keep", """{"ticket":"t-1","plan":null}""")
            assertEquals(emptyMap<String, String>(), disk.rows.toMap())
        }

    @Test
    fun `核心交的活卡整份发到 live、会话簿换掉手上那一本；认不出的那一行只报一次契约对不上、不发`() =
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
                    kept = Disk(),
                    onBreak = { broken += it },
                ).also { it.ensure() }
            val got = mutableListOf<LiveCards>()
            backgroundScope.launch { f.live.collect { got += it } }
            core.emit(
                "live",
                """{"sid":"s1","cards":[{"card":4,"rid":"m","phase":"streaming","clipped":false,"blocks":[{"kind":"text","text":"字"}]}]}""",
            )
            core.emit("live", """{"sid":"s1","cards":[{"card":4}]}""")
            assertEquals(
                "字",
                got
                    .single()
                    .cards
                    .single()
                    .blocks
                    .single()
                    .text,
            )
            assertEquals(1, broken.size)
            core.emit("book", """{"live":{},"left":{},"runs":{},"successors":{"a":"d"},"rereads":{},"listed":true}""")
            assertEquals(mapOf("a" to "d"), f.table.value.successors)
            core.emit("book", """{"live":{}}""")
            assertEquals("认不出的簿不换", mapOf("a" to "d"), f.table.value.successors)
            assertEquals(2, broken.size)
        }

    private companion object {
        const val GOLDEN = "mobile-state-line.golden.json"
    }
}
