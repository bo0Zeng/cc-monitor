package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.Anchor
import com.ccmonitor.mobile.cache.CachedRecords
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.cache.RawPage
import com.ccmonitor.mobile.cache.appendRows
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.data.db.PendingOutcome
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
            cwd: String?,
            account: String?,
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
    ) = mapOf("end" to (i + 1) * 10L, "record" to record)

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
            assertEquals(listOf(20L), link.calls("history-read").map { it!!["offset"] })
            assertEquals(listOf("u1", "a1#0", "u2"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 30, 3, "f1"), cache.anchors["s1"])
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
            assertEquals("从尾段起点（0）读，不从续点 50 往后读", listOf(0L), link.calls("history-read").map { it!!["offset"] })
            assertEquals(listOf("n1", "n2#0"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 20, 2, "f1"), cache.anchors["s1"])
        }

    @Test
    fun `同一路径换了一份（身份对不上、长度够）⇒ 从头读最后一屏，不从旧续点往后读`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("n1", "新"), reply("n2", "新"), human("n3", "新")), id = "f2")
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            val cache = MemCache().apply { seed(human("u1", "旧"), reply("a1", "旧")) }
            val s = open(link, cache)
            runCurrent()
            assertEquals("从尾段起点（0）读，不从续点 20 往后读", listOf(0L), link.calls("history-read").map { it!!["offset"] })
            assertEquals(listOf("n1", "n2#0", "n3"), s.keys())
            assertEquals(Anchor("/p/s1.jsonl", 30, 3, "f2"), cache.anchors["s1"])
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
            link.answer = { cmd, args -> (file.answer(cmd, args) ?: Reply.LinkDown).also { if (cmd == "history-read") replied = (it as Reply.Ok).data } }
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
    fun `草稿：打开时填回上次没发的；发出去那一下草稿变待发（一步）；结局照回包落；记录里出现了就不再是待发`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()
            link.answer = { cmd, args -> if (cmd == "session-say") Reply.Ok(mapOf("result" to "unsure")) else file.answer(cmd, args) ?: Reply.LinkDown }
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
            assertEquals(PendingOutcome.UNSURE, cache.pend.single().outcome)

            file.records += human("u1", "你好")
            link.line("s1", 0, human("u1", "你好"))
            runCurrent()
            assertTrue("记录里出现了 ⇒ 待发里删掉", cache.pend.isEmpty())
        }

    @Test
    fun `上次还在等回包、App 就没了的那一句：重开挂回去，标送达未知（别重发）`() =
        runTest {
            val link = FakeCoreLink()
            val cache = MemCache().apply { pend += Pending("local-x", "s1", "半路的", PendingOutcome.PENDING, 1) }
            val s = open(link, cache)
            runCurrent()
            val u =
                s.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("半路的", u.text)
            assertEquals(DeliveryState.UNSURE, u.delivery)
            assertNull(s.state.value.draft)
        }
}
