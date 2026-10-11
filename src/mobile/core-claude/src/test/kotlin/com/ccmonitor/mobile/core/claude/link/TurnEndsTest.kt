package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 常驻流上的 `turn_end` 帧 ⇒ 「一轮完成」：后端逐记录发、不去重，折成一轮一条在这里。 */
class TurnEndsTest {
    private val frames = Fixtures.text("session-stream.golden.jsonl").lines().mapNotNull(Json::obj)

    @Test
    fun `金样里的 turn_end 解得出会话 · uuid · 后端写好的通知两行，缺格解不出`() {
        val golden = frames.first { it.str("kind") == "turn_end" }
        assertEquals(TurnEnd("s1", "u1", golden.str("title")!!, golden.str("body")!!), TurnEnd.of(golden))
        assertTrue("标题是后端写好的、不空", TurnEnd.of(golden)!!.title.isNotBlank())
        assertNull(TurnEnd.of(mapOf("kind" to "turn_end", "session_id" to "s1")))
        assertNull("缺标题不自己拼", TurnEnd.of(mapOf("kind" to "turn_end", "session_id" to "s1", "uuid" to "u1", "body" to "b")))
        assertNull("缺正文不自己拼", TurnEnd.of(mapOf("kind" to "turn_end", "session_id" to "s1", "uuid" to "u1", "title" to "t")))
        assertNull("别的帧不是 turn_end", TurnEnd.of(frames.first { it.str("kind") == "session_added" }))
    }

    private fun te(
        sid: String,
        uuid: String,
    ) = TurnEnd(sid, uuid, "T-$uuid", "B")

    /** 喂 [script]（时刻 ms ⇒ 一帧），收 [TurnEnds.rounds] 吐出来的（时刻 ⇒ 一轮）。 */
    private fun rounds(vararg script: Pair<Long, TurnEnd>): List<Pair<Long, TurnEnd>> {
        val got = mutableListOf<Pair<Long, TurnEnd>>()
        runTest {
            val input = Channel<TurnEnd>(Channel.UNLIMITED)
            val job = launch { TurnEnds.rounds(input.receiveAsFlow()).collect { got += testScheduler.currentTime to it } }
            runCurrent()
            var now = 0L
            for ((at, te) in script) {
                advanceTimeBy(at - now)
                runCurrent()
                now = at
                input.send(te)
                runCurrent()
            }
            delay(TurnEnds.SETTLE_MS * 4)
            job.cancel()
        }
        return got
    }

    @Test
    fun `同一轮拆成几帧只报一次，取最后那一帧（uuid 与标题），静默满了才报`() {
        val got = rounds(0L to te("s1", "a"), 100L to te("s1", "b"), 200L to te("s1", "c"))
        assertEquals(listOf(200 + TurnEnds.SETTLE_MS to te("s1", "c")), got)
    }

    @Test
    fun `两条会话各折各的，一条的帧压不掉另一条`() {
        val got = rounds(0L to te("s1", "a"), 600L to te("s2", "x"), 1_000L to te("s1", "b"))
        assertEquals(listOf(te("s2", "x"), te("s1", "b")), got.map { it.second })
    }

    @Test
    fun `同一 uuid 再来不再报，换了 uuid 照报`() {
        val later = TurnEnds.SETTLE_MS * 3
        val got = rounds(0L to te("s1", "a"), later to te("s1", "a"), later * 2 to te("s1", "b"))
        assertEquals(listOf(te("s1", "a"), te("s1", "b")), got.map { it.second })
    }
}
