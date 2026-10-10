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
import org.junit.Test

/** 常驻流上的 `turn_end` 帧 ⇒ 「一轮完成」：后端逐记录发、不去重，折成一轮一条在这里。 */
class TurnEndsTest {
    private val frames = Fixtures.text("session-stream.golden.jsonl").lines().mapNotNull(Json::obj)

    @Test
    fun `金样里的 turn_end 解得出会话与 uuid，缺格解不出`() {
        val golden = frames.first { it.str("kind") == "turn_end" }
        assertEquals(TurnEnd("s1", "u1"), TurnEnd.of(golden))
        assertNull(TurnEnd.of(mapOf("kind" to "turn_end", "session_id" to "s1")))
        assertNull("别的帧不是 turn_end", TurnEnd.of(frames.first { it.str("kind") == "session_added" }))
    }

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
    fun `同一轮拆成几帧只报一次，取最后那个 uuid，静默满了才报`() {
        val got = rounds(0L to TurnEnd("s1", "a"), 100L to TurnEnd("s1", "b"), 200L to TurnEnd("s1", "c"))
        assertEquals(listOf(200 + TurnEnds.SETTLE_MS to TurnEnd("s1", "c")), got)
    }

    @Test
    fun `两条会话各折各的，一条的帧压不掉另一条`() {
        val got = rounds(0L to TurnEnd("s1", "a"), 600L to TurnEnd("s2", "x"), 1_000L to TurnEnd("s1", "b"))
        assertEquals(listOf(TurnEnd("s2", "x"), TurnEnd("s1", "b")), got.map { it.second })
    }

    @Test
    fun `同一 uuid 再来不再报，换了 uuid 照报`() {
        val later = TurnEnds.SETTLE_MS * 3
        val got = rounds(0L to TurnEnd("s1", "a"), later to TurnEnd("s1", "a"), later * 2 to TurnEnd("s1", "b"))
        assertEquals(listOf(TurnEnd("s1", "a"), TurnEnd("s1", "b")), got.map { it.second })
    }
}
