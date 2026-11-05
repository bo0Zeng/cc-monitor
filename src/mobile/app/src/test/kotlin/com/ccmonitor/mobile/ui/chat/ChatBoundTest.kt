package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 长开着的一条会话：来一行只排那一行、只折尾巴；跟着尾巴时手上只留最后几页（往上翻照续点读回）。
 * 夹具：一轮三行（人说 · 中间一步 · 回答），每轮一段过程从人说那一条起、到回答那一条为止；正文是占位字。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatBoundTest {
    /** 第 i 行（行号 i）：一轮三行。 */
    private fun recordAt(i: Int): Map<String, Any?> {
        val t = i / 3
        return when (i % 3) {
            0 -> human("u$t", "问 $t")
            1 -> reply("s$t", "步 $t")
            else -> reply("a$t", "答 $t")
        }
    }

    private fun seg(
        t: Int,
        done: Boolean,
    ) = mapOf(
        "after" to "u$t",
        "afterLine" to t * 3,
        "parts" to listOf(mapOf("text" to "过程", "tone" to "plain")),
        "span" to mapOf("text" to "{dur}", "from" to 0, "to" to 1000),
        "until" to if (done) "a$t" else null,
        "untilLine" to null,
        "phase" to "idle",
        "firstFail" to null,
    )

    private fun turnRow(
        t: Int,
        done: Boolean,
    ): Map<String, Any?> {
        val text = listOf("start", "end", "startText", "reply").associateWith { "" }
        val counts = listOf("line", "tools", "thinking", "agents", "background", "retries", "fails").associateWith { 0 }
        return text + counts +
            mapOf(
                "at" to t * 3 * W,
                "uuid" to "u$t",
                "said" to "q",
                "shown" to emptyList<String>(),
                "done" to done,
                "phase" to if (done) "idle" else "running",
                "folds" to listOf(seg(t, done)),
            )
    }

    /** 那台的 `history-turns`：从 `cover` · `from` 那一字节起的那几轮（照文件此刻有几行算）。 */
    private fun turnsOf(
        file: FakeCoreLink.File,
        args: Map<String, Any?>?,
    ): Reply {
        val n = file.records.size
        val at = (args?.get("from") ?: args?.get("cover")) as Long
        val rows = (0 until (n + 2) / 3).filter { it * 3 * W >= at }.map { turnRow(it, done = it * 3 + 2 < n) }
        return Reply.Ok(mapOf("from" to at, "end" to n * W, "next" to null, "turns" to rows))
    }

    private fun TestScope.open(lines: Int): Triple<FakeCoreLink, FakeCoreLink.File, ChatSession> {
        val file = FakeCoreLink.File((0 until lines).map(::recordAt).toMutableList())
        val link = FakeCoreLink()
        link.report("s1", "/w", Faces.of("live-working-plain"))
        link.answer = { cmd, args -> if (cmd == "history-turns") turnsOf(file, args) else file.answer(cmd, args) ?: Reply.LinkDown }
        val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }
        runCurrent()
        return Triple(link, file, s)
    }

    /** 那台写下一行、流上来。 */
    private suspend fun TestScope.more(
        link: FakeCoreLink,
        file: FakeCoreLink.File,
    ) {
        val i = file.records.size
        file.records += recordAt(i)
        link.line("s1", i.toLong(), file.records[i], byteOffset = (i + 1) * W)
        runCurrent()
    }

    @Test
    fun `量尺：2000 行之后再追 1000 行，每行的耗时（打印，改前改后同一把尺子）`() =
        runTest {
            val (link, file, s) = open(60)
            repeat(2_000 - 60) { more(link, file) }
            val t0 = System.nanoTime()
            repeat(1_000) { more(link, file) }
            val us = (System.nanoTime() - t0) / 1_000 / 1_000
            val st = s.state.value
            println("CHATBOUND-RULER us/line=$us units=${st.units.size} stream=${st.stream.size} procs=${st.stream.count { it is StreamItem.Process }}")
        }

    /** 手上至多这么多行（留几页 ＋ 末页那一页还没满的）。 */
    private val bound = (ChatSession.KEEP_PAGES + 1) * ChatSession.PAGE_LINES

    private fun ChatSession.keys() = state.value.units.map { it.sourceUuid }

    @Test
    fun `跟着尾巴灌 2000 行 ⇒ 单元表与流截在上界内、还能往上翻；再追 1 行 ⇒ 旧单元与最后一段过程之前的项原样留着`() =
        runTest {
            val (link, file, s) = open(60)
            repeat(2_000 - 60) { more(link, file) }
            val before = s.state.value
            assertTrue("单元表 ${before.units.size} 截在 $bound 内", before.units.size <= bound)
            assertTrue("流 ${before.stream.size} 截在 $bound 内", before.stream.size <= bound)
            assertTrue(before.canLoadOlder)
            more(link, file)
            val after = s.state.value
            assertEquals("这一下没丢页（夹具排好的）", before.units.first().key, after.units.first().key)
            assertEquals("只多了这一行的单元", before.units.size + 1, after.units.size)
            before.units.forEachIndexed { i, u -> assertTrue("第 $i 个单元原样留着", u === after.units[i]) }
            val k = before.stream.indexOfLast { it is StreamItem.Process }
            assertTrue(k > 0)
            (0 until k).forEach { i -> assertTrue("流里第 $i 项原样留着（只折尾巴）", before.stream[i] === after.stream[i]) }
        }

    @Test
    fun `人往上翻着（不贴底）⇒ 不丢；滚回底 ⇒ 丢到上界内；往上翻照续点读回丢掉的那一页，不重不漏`() =
        runTest {
            val (link, file, s) = open(60)
            s.follow(false)
            runCurrent()
            repeat(400) { more(link, file) }
            assertEquals("往上翻着一行不丢", 460, s.state.value.units.size)
            s.follow(true)
            runCurrent()
            val kept = s.keys()
            assertTrue(kept.size <= bound)
            val first = file.records.indexOfFirst { it["id"] == kept.first() }
            s.loadOlder()
            runCurrent()
            val back = s.keys()
            assertEquals("读回前面那一页、接得上", (first - ChatSession.PAGE_LINES until file.records.size).map { file.records[it]["id"] }, back)
            more(link, file)
            assertEquals("往上翻读回之后（人在顶上）不丢", back.size + 1, s.keys().size)
        }

    @Test
    fun `从命中打开：落点那一页一直在手上（不贴底，不丢）`() =
        runTest {
            val file = FakeCoreLink.File((0 until 300).map(::recordAt).toMutableList())
            val link = FakeCoreLink()
            link.report("s1", "/w", Faces.of("live-working-plain"))
            link.answer = { cmd, args -> if (cmd == "history-turns") turnsOf(file, args) else file.answer(cmd, args) ?: Reply.LinkDown }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
            s.jump(30 * W, null)
            s.open()
            runCurrent()
            assertEquals("u10", s.state.value.landing)
            repeat(300) { more(link, file) }
            assertEquals("u10", s.state.value.landing)
            assertEquals("u10", s.keys().first())
        }

    private companion object {
        const val W = 10L
    }
}
