package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 这条流在看哪几个会话（`stream-watch`）：列表页报 `[]`，进会话报 `[sid]`，退出再报 `[]`，换了一条（重接上）以后照当前名单重报。
 * 应答的 `from[]` 原样交给看着那几条的。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class WatchListTest {
    private class Link : CoreLink {
        override val table: StateFlow<SessionTable> = MutableStateFlow(SessionTable())
        override val lines: SharedFlow<RecordLine> = MutableSharedFlow()
        override val epoch = MutableStateFlow(0L)
        val reported = mutableListOf<List<String>>()
        var from: List<Map<String, Any?>> = emptyList()

        override suspend fun call(
            cmd: String,
            args: Map<String, Any?>?,
            withinMs: Long,
        ): Reply {
            if (cmd != StreamWatch.COMMAND) return Reply.LinkDown
            @Suppress("UNCHECKED_CAST")
            reported += args!!["sids"] as List<String>
            return Reply.Ok(mapOf("from" to from))
        }
    }

    @Test
    fun `接上就报空名单；进会话报它、退出再报空；同一条两处看只算一份`() =
        runTest {
            val link = Link()
            val w = WatchList(link, backgroundScope)
            runCurrent()
            assertEquals("还没接上不报", emptyList<List<String>>(), link.reported)
            link.epoch.value = 1
            runCurrent()
            assertEquals("列表页：报空（从没报过 ＝ 全看，所以一接上就得报）", listOf(emptyList<String>()), link.reported)

            val a = w.hold("s1")
            val b = w.hold("s1")
            runCurrent()
            assertEquals(listOf("s1"), link.reported.last())
            a.close()
            runCurrent()
            assertEquals("还有一处在看", listOf("s1"), link.reported.last())
            b.close()
            b.close()
            runCurrent()
            assertEquals(listOf(emptyList(), listOf("s1"), emptyList<String>()), link.reported)
        }

    @Test
    fun `断了再接上：照当前名单重报；from 交给看着的`() =
        runTest {
            val link = Link()
            link.epoch.value = 1
            val w = WatchList(link, backgroundScope)
            val got = mutableListOf<WatchFrom>()
            backgroundScope.launch { w.from.collect { got += it } }
            runCurrent()
            link.from = listOf(mapOf("sid" to "s1", "path" to "/p/s1.jsonl", "seq" to 5))
            w.hold("s1")
            runCurrent()
            assertEquals(listOf(WatchFrom("s1", "/p/s1.jsonl", 5)), got)

            link.from = listOf(mapOf("sid" to "s1", "path" to "/p/s1.jsonl", "seq" to 9))
            link.epoch.value = 2
            runCurrent()
            assertEquals("新的一条 ＝ 没报过：重报", listOf("s1"), link.reported.last())
            assertEquals(WatchFrom("s1", "/p/s1.jsonl", 9), got.last())
        }
}
