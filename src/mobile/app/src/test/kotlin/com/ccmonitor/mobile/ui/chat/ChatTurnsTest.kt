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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 会话屏的过程折叠接线：问 `history-turns`（`cover` ＝ 已拉到的最上面那一行；之后从还没收尾那一轮的 `at` 续取），流照分段折；
 * 开合记在内存、不替人开合；跳到一条落在折着的段里 ⇒ 先展开那一段。问不到分段 ⇒ 照摊开画，不报错。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatTurnsTest {
    private fun TestScope.existing(link: FakeCoreLink) = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }

    private fun row(
        end: Long,
        record: Map<String, Any?>,
    ) = mapOf("end" to end, "hash" to 0, "record" to record)

    private fun seg(
        after: String,
        until: String?,
        fail: String? = null,
    ) = mapOf(
        "after" to after,
        "afterLine" to 0,
        "parts" to listOf(mapOf("text" to "过程", "tone" to "plain")),
        "span" to mapOf("text" to "{dur}", "from" to 0, "to" to 1000),
        "until" to until,
        "untilLine" to null,
        "phase" to "idle",
        "firstFail" to fail?.let { mapOf("uuid" to it, "tool" to null, "line" to 0) },
    )

    private fun turn(
        at: Long,
        uuid: String,
        done: Boolean,
        vararg folds: Map<String, Any?>,
    ): Map<String, Any?> {
        val text = listOf("start", "end", "startText", "endText", "reply").associateWith { "" }
        val counts = listOf("line", "tools", "thinking", "agents", "background", "retries", "peers", "fails").associateWith { 0 }
        val rest =
            mapOf(
                "at" to at,
                "uuid" to uuid,
                "said" to "q",
                "shown" to emptyList<String>(),
                "done" to done,
                "phase" to if (done) "idle" else "running",
                "folds" to folds.toList(),
            )
        return text + counts + rest
    }

    private fun turns(
        from: Long,
        vararg rows: Map<String, Any?>,
    ) = Reply.Ok(mapOf("from" to from, "end" to 400, "next" to null, "turns" to rows.toList()))

    private fun ChatSession.shape() =
        state.value.stream.map {
            when (it) {
                is StreamItem.Card -> it.unit.key
                is StreamItem.Process -> "proc:${it.fold.after}:${if (it.open) "open" else "shut"}"
            }
        }

    /** 尾段 u1 · a1 · a2 · a3（字节 100 起）；一轮从 u1 起，u1 之后到 a3 之前是一段过程。 */
    private fun link(turnsReply: (Map<String, Any?>?) -> Reply): FakeCoreLink {
        val link = FakeCoreLink()
        link.report("s1", "/w", Faces.of("live-working-plain"))
        link.answer = { cmd, args ->
            when (cmd) {
                "history-tail" ->
                    Reply.Ok(
                        FakeCoreLink.tail(
                            400,
                            100,
                            4,
                            listOf(row(150, human("u1", "问")), row(200, reply("a1", "一")), row(300, reply("a2", "二")), row(400, reply("a3", "答"))),
                        ),
                    )
                "history-turns" -> turnsReply(args)
                else -> Reply.LinkDown
            }
        }
        return link
    }

    @Test
    fun `冷读：分段按已拉到的最上面那一行问（cover），两段之间的卡不画、只画一行过程`() =
        runTest {
            val link = link { turns(100, turn(100, "u1", true, seg("u1", "a3"))) }
            val s = existing(link)
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl", "cover" to 100L)), link.calls("history-turns"))
            assertEquals(listOf("u1", "proc:u1:shut", "a3#0"), s.shape())
        }

    @Test
    fun `开合按 after 记在内存：点开接上段里的卡，再点收起；来了新的一行也不替人收`() =
        runTest {
            val link = link { turns(100, turn(100, "u1", false, seg("u1", null))) }
            val s = existing(link)
            runCurrent()
            assertEquals(listOf("u1", "proc:u1:shut"), s.shape())
            s.toggle("u1")
            runCurrent()
            assertEquals(listOf("u1", "proc:u1:open", "a1#0", "a2#0", "a3#0"), s.shape())
            link.line("s1", 4, reply("a4", "四"), byteOffset = 400)
            runCurrent()
            assertEquals(listOf("u1", "proc:u1:open", "a1#0", "a2#0", "a3#0", "a4#0"), s.shape())
            assertEquals("还没收尾的那一轮从它的 at 续取", mapOf("path" to "/p/s1.jsonl", "from" to 100L), link.calls("history-turns").last())
            s.toggle("u1")
            runCurrent()
            assertEquals(listOf("u1", "proc:u1:shut"), s.shape())
        }

    @Test
    fun `跳到第一处失败：先展开所在那一段，落点是那张卡`() =
        runTest {
            val link = link { turns(100, turn(100, "u1", true, seg("u1", "a3", fail = "a2"))) }
            val s = existing(link)
            runCurrent()
            assertNull(s.state.value.landing)
            s.jumpFail("u1")
            runCurrent()
            assertEquals(listOf("u1", "proc:u1:open", "a1#0", "a2#0", "a3#0"), s.shape())
            assertEquals("a2#0", s.state.value.landing)
        }

    @Test
    fun `问不到分段：照摊开画，不报读不出`() =
        runTest {
            val link = link { Reply.LinkDown }
            val s = existing(link)
            runCurrent()
            assertEquals(listOf("u1", "a1#0", "a2#0", "a3#0"), s.shape())
            assertNull(s.state.value.historyError)
            assertNull(s.state.value.failedWhy)
            assertTrue(
                s.state.value.units
                    .isNotEmpty(),
            )
        }
}
