package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.Anchor
import com.ccmonitor.mobile.cache.CachedRecords
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.cache.RawPage
import com.ccmonitor.mobile.cache.appendRows
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 聊天屏先上本地缓存、再跟那台对齐：离线打开看得到上次那几条；续读先比那台文件的大小、变短从头；缓存里是那台的成品原样；
 * 草稿变待发同一步。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatCacheTest {
    /** 记在内存里的缓存（末页怎么长照生产那一份 [appendRows]）。 */
    private class MemCache : ChatCache {
        val pages = mutableMapOf<String, List<RawPage>>()
        val anchors = mutableMapOf<String, Anchor?>()
        val pend = mutableListOf<Pending>()
        val drafts = mutableMapOf<String, String>()
        val promoted = mutableListOf<Pair<String, Pending>>()
        val viewed = mutableListOf<String>()

        override suspend fun load(sid: String) = pages[sid]?.takeIf { it.isNotEmpty() }?.let { CachedRecords(it, anchors[sid]) }

        override suspend fun viewed(sid: String) {
            viewed += sid
        }

        override suspend fun replace(
            sid: String,
            pages: List<RawPage>,
            anchor: Anchor?,
        ) {
            this.pages[sid] = pages
            anchors[sid] = anchor
        }

        override suspend fun prepend(
            sid: String,
            pages: List<RawPage>,
        ) {
            this.pages[sid] = (pages + this.pages[sid].orEmpty()).sortedBy { it.start }
        }

        override suspend fun append(
            sid: String,
            rows: List<Any?>,
            anchor: Anchor?,
        ) {
            val have = pages[sid].orEmpty()
            val put = appendRows(have.lastOrNull(), rows, anchor?.offset)
            pages[sid] = (have.filter { p -> put.none { it.start == p.start } } + put).sortedBy { it.start }
            anchors[sid] = anchor
        }

        override suspend fun pending(sid: String) = pend.filter { it.sid == sid }

        override suspend fun promote(
            draftSid: String,
            p: Pending,
        ) {
            drafts.remove(draftSid)
            pend += p
            promoted += draftSid to p
        }

        override suspend fun settle(
            id: String,
            outcome: String,
            attempts: Int,
        ) {
            pend.replaceAll { if (it.id == id) it.copy(outcome = outcome, attempts = attempts) else it }
        }

        override suspend fun bind(
            ids: List<String>,
            sid: String,
        ) {
            pend.replaceAll { if (it.id in ids) it.copy(sid = sid) else it }
        }

        override suspend fun forget(ids: List<String>) {
            pend.removeAll { it.id in ids }
        }

        override suspend fun draft(sid: String) = drafts[sid]

        override suspend fun saveDraft(
            sid: String,
            text: String,
        ) {
            if (text.isEmpty()) drafts.remove(sid) else drafts[sid] = text
        }
    }

    private fun TestScope.open(
        link: FakeCoreLink,
        cache: ChatCache,
    ) = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).also { it.open() }

    /** 一格 `rows[]`（`{end, record}`），同 [FakeCoreLink.File] 答的那一形：第 i 行占 `[10i, 10i+10)`。 */
    private fun cell(
        i: Int,
        record: Map<String, Any?>,
    ) = mapOf("end" to (i + 1) * 10L, "hash" to i.toLong(), "record" to record)

    private fun ChatSession.keys() = state.value.units.map { it.key }

    private fun MemCache.seed(vararg records: Map<String, Any?>) {
        pages["s1"] = listOf(RawPage(0, records.size * 10L, records.mapIndexed { i, r -> cell(i, r) }))
        anchors["s1"] = Anchor("/p/s1.jsonl", records.size * 10L, records.size.toLong(), "f1")
    }

    @Test
    fun `离线打开：那台一句都答不上，屏上照样是上次那几条（缓存里的页交给同一个解码器）`() =
        runTest {
            val link = FakeCoreLink() // 什么都答 LinkDown
            val cache = MemCache().apply { seed(human("u1", "你好"), reply("a1", "在")) }
            val s = open(link, cache)
            runCurrent()
            assertEquals(listOf("u1", "a1#0"), s.keys())
            val cachedRows =
                cache.pages
                    .getValue("s1")
                    .single()
                    .rows
            val sameDecoder = RecordUnits.of(HistoryRead.decodeRows(cachedRows)!!)
            assertEquals("缓存上屏的就是同一个解码器解出来的", sameDecoder, s.state.value.units)
            assertEquals(listOf("s1"), cache.viewed)
        }

    @Test
    fun `连上了、那台的文件没变短 ⇒ 从续点往下读到 end、接在缓存后面（不从尾段起点重读）`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("u1", "你好"), reply("a1", "在"), human("u2", "再来")))
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            val cache = MemCache().apply { seed(human("u1", "你好"), reply("a1", "在")) }
            val s = open(link, cache)
            runCurrent()
            assertEquals("交回手上那一份、那台从续点那一行起回", 2L, link.calls("history-tail").single()!!.let { (it["held"] as Map<*, *>)["next"] })
            assertTrue("第一页随 history-tail 来", link.calls("history-read").isEmpty())
            assertEquals(listOf("u1", "a1#0", "u2"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 30, 3, "f1"), cache.anchors["s1"])
            assertNull("续得上 ⇒ 那台不说那一句", s.state.value.note)
        }

    @Test
    fun `照续点里的路径接上、目录还不知道 ⇒ 目录取读 chip 那一趟事实里的 projectDir，不整份问 history-list`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("u1", "你好"), reply("a1", "在")))
            link.answer = { cmd, args -> if (cmd == "history-facts") Reply.Ok(Wires.facts("projectDir" to "/h/proj")) else file.answer(cmd, args) ?: Reply.LinkDown }
            val s = open(link, MemCache().apply { seed(human("u1", "你好"), reply("a1", "在")) })
            runCurrent()
            assertTrue(link.calls("history-list").isEmpty())
            assertEquals("按续点里的路径问", listOf(mapOf("path" to "/p/s1.jsonl")), link.calls("history-facts"))
            assertEquals("/h/proj", s.state.value.cwd)
        }

    @Test
    fun `那台的文件变短了（end 比续点小）⇒ 从头读最后一屏，缓存里那几条整份换掉`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("n1", "新的"), reply("n2", "也新")))
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            val cache = MemCache().apply { seed(human("u1", "旧"), reply("a1", "旧"), human("u2", "旧"), reply("a2", "旧"), human("u3", "旧")) }
            val s = open(link, cache)
            runCurrent()
            assertEquals(listOf("n1", "n2#0"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 20, 2, "f1"), cache.anchors["s1"])
            assertEquals("变短那一句是那台写好的", FakeCoreLink.File.SAID["truncated"], s.state.value.note)
        }

    @Test
    fun `同一路径换了一份（记号对不上、长度够）⇒ 从头读最后一屏，不从旧续点往后读`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("n1", "新"), reply("n2", "新"), human("n3", "新")), id = "f2")
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            val cache = MemCache().apply { seed(human("u1", "旧"), reply("a1", "旧")) }
            val s = open(link, cache)
            runCurrent()
            assertEquals(listOf("n1", "n2#0", "n3"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 30, 3, "f2"), cache.anchors["s1"])
            assertEquals("被改写那一句是那台写好的", FakeCoreLink.File.SAID["rewritten"], s.state.value.note)
        }

    @Test
    fun `按行号补完一段：续点跟到那一段之后（不作废），每格带它那一行之后的字节落末页`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("u1", "你好")))
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            val cache = MemCache()
            open(link, cache)
            runCurrent()
            file.records += listOf(reply("a1", "在"), human("u2", "再来"), reply("a2", "好"))
            link.line("s1", 3, file.records[3], byteOffset = 40) // 中间 [1, 3) 缺着 ⇒ 按行号补
            runCurrent()
            assertEquals(listOf(1L), link.calls("history-lines").map { it!!["from"] })
            assertEquals(Anchor("/p/s1.jsonl", 40, 4, "f1"), cache.anchors["s1"])
            val rows = cache.pages.getValue("s1").flatMap { it.rows }
            assertEquals(listOf(10L, 20L, 30L, 40L), rows.map { (it as Map<*, *>)["end"] })
        }

    @Test
    fun `缓存里存的是那台交来的 rows 原样；流上来的一行照那一形接在末页、续点跟着走`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("u1", "你好")))
            var replied: Any? = null
            link.answer = { cmd, args -> (file.answer(cmd, args) ?: Reply.LinkDown).also { if (cmd == "history-tail") replied = (it as Reply.Ok).data } }
            link.report("s1", "/w")
            val cache = MemCache()
            open(link, cache)
            runCurrent()
            val stored =
                cache.pages
                    .getValue("s1")
                    .single()
                    .rows
            assertSame("同一份 rows[]，不是挑过格的", HistoryRead.rawRows(replied)!![0], stored[0])

            val r = reply("a1", "在")
            link.line("s1", 1, r, byteOffset = 20)
            runCurrent()
            val grown =
                cache.pages
                    .getValue("s1")
                    .single()
                    .rows
            assertEquals(mapOf("end" to 20L, "record" to r), grown[1])
            assertEquals(Anchor("/p/s1.jsonl", 20, 2, "f1"), cache.anchors["s1"])
        }

    @Test
    fun `草稿：打开时填回上次没发的；发出去那一下草稿变待发（一步）；回执到了就不再是待发`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()
            link.answer = { cmd, args -> if (cmd == "session-queue-put") Reply.Ok(mapOf("outcome" to "queued", "seq" to 1L, "said" to "q-1")) else file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            val cache = MemCache().apply { drafts["s1"] = "没发的" }
            val s = open(link, cache)
            runCurrent()
            assertEquals("没发的", s.state.value.draft)

            s.editDraft("没发的，接着打")
            advanceTimeBy(1_000)
            runCurrent()
            assertEquals("没发的，接着打", cache.drafts["s1"])

            s.editDraft("你好")
            s.send("你好")
            advanceTimeBy(1_000)
            runCurrent()
            assertEquals("发了 ⇒ 撤掉还没存的那一下，草稿不会被写回来", null, cache.drafts["s1"])
            val (key, p) = cache.promoted.single()
            assertEquals("s1", key)
            assertEquals("你好", p.text)
            assertTrue("回执到了 ⇒ 待发里删掉", cache.pend.isEmpty())
        }

    /**
     * ★ 回执到之前 App 被杀：重开那一句在输入框上方「发送中」、不在消息流里；接上那台照同一个编号再交（断了又接上再交一遍），
     * 那台的队按编号去重只有一条；回执到了离开待发（待发里删掉）。后台重交时回执到过的（落成 `delivered`）重开不挂。
     */
    @Test
    fun `回执到之前 App 被杀：重开那一句在发送中；接上照同一个编号重交，那台队里只有一条；回执到了离开待发`() =
        runTest {
            val link = FakeCoreLink()
            link.state.value = linkDown("断开")
            val file = FakeCoreLink.File()
            // 那台的队：编号 ⇒ 那一句（同编号只进一条）。
            val queue = linkedMapOf<String, String>()
            link.answer = { cmd, args ->
                if (cmd == "session-queue-put") {
                    val id = args!!["id"] as String
                    queue.putIfAbsent(id, args["text"] as String)
                    val place = queue.keys.indexOf(id) + 1L
                    Reply.Ok(mapOf("outcome" to "queued", "seq" to place, "said" to "q-$place"))
                } else {
                    file.answer(cmd, args) ?: Reply.LinkDown
                }
            }
            link.report("s1", "/w")
            val cache =
                MemCache().apply {
                    pend += Pending("local-x", "s1", "半路的", PendingOutcome.PENDING, 1)
                    pend += Pending("local-y", "s1", "后台交到了的", PendingOutcome.DELIVERED, 1)
                }
            val s = open(link, cache)
            runCurrent()
            val box = s.box().single()
            assertEquals("local-x" to "半路的", box.id to box.text)
            assertEquals("上次没等到回执 ⇒ 交了没回执那一句（那台的 heldSaid）", FakeCoreLink.HELD, box.said)
            assertTrue("回执到之前不进流", s.said().isEmpty())
            assertEquals("后台交到了的不挂、删掉", listOf("local-x"), cache.pend.map { it.id })
            assertTrue("断着不交", link.calls("session-queue-put").isEmpty())

            link.state.value = linkUp()
            runCurrent()
            link.state.value = linkDown("又断了")
            runCurrent()
            link.state.value = linkUp()
            runCurrent()
            assertEquals("那台队里只有一条", listOf("local-x"), queue.keys.toList())
            assertTrue(link.calls("session-queue-put").all { it!!["id"] == "local-x" })
            assertTrue(s.box().isEmpty())
            assertTrue("回执到了不另画进流", s.said().isEmpty())
            assertTrue("回执到了 ⇒ 待发里删掉", cache.pend.isEmpty())
            assertNull(s.state.value.draft)
        }

    /** 待发里（还没回执）的那几句。 */
    private fun ChatSession.box() = state.value.outbox

    /** 流里本地那几句（不该有：回执到了的离开待发、不另画）。 */
    private fun ChatSession.said() = state.value.units.filterIsInstance<RenderUnit.UserText>()

    /** 待发里还没回执的那一句撤掉：本地删掉、不再交；照编号请那台撤（万一已经交到了）。 */
    @Test
    fun `撤待发里的一句：不再交，照编号请那台撤`() =
        runTest {
            val link = FakeCoreLink()
            link.state.value = linkDown("断开")
            val file = FakeCoreLink.File()
            link.answer = { cmd, args -> if (cmd == "session-queue-cancel") Reply.Ok(mapOf("cancelled" to false, "queue" to mapOf("items" to emptyList<Any>(), "sent" to emptyList<Any>()))) else file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            val cache = MemCache().apply { pend += Pending("local-x", "s1", "半路的", PendingOutcome.PENDING, 1) }
            val s = open(link, cache)
            runCurrent()
            s.cancel("local-x")
            runCurrent()
            link.state.value = linkUp()
            runCurrent()
            assertTrue(s.box().isEmpty())
            assertTrue(cache.pend.isEmpty())
            assertTrue("撤了就不再交", link.calls("session-queue-put").isEmpty())
            assertEquals(listOf("local-x"), link.calls("session-queue-cancel").map { it!!["id"] })
        }
}
