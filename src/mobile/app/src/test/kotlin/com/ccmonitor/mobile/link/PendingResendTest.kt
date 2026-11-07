package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkUp
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 待发重交挂在「那台接上」那一下，不挂在「打开那条会话」上：被杀重开后没打开的会话里的待发，那台一接上就照原编号重交
 * （屏上写着「发送中」就得真在送；那台按编号去重，多交不会多送）。夹具只造结构（编号 · 占位句子）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PendingResendTest {
    /** 一台落盘的待发替身：该再交的那几句 · 记回去的结局。 */
    private class Box(
        var rows: List<Pending>,
    ) : HostOutbox {
        val settled = mutableMapOf<String, String>()

        override suspend fun due() = rows.filter { it.id !in settled && it.sid != null && (it.outcome == PendingOutcome.PENDING || it.outcome == PendingOutcome.UNSURE) }

        override suspend fun settle(
            id: String,
            outcome: String,
            attempts: Int,
        ) {
            settled[id] = outcome
        }

        override fun attachments(id: String) = if (id == "p2") listOf("att-1") else emptyList()
    }

    private val receipt = Reply.Ok(mapOf("outcome" to "queued", "seq" to 1L, "said" to "q-1"))

    @Test
    fun `两条会话各有一条待发、哪条都没打开、那台接上 ⇒ 两条都照原编号重交、回执记回去`() =
        runTest {
            val link = FakeCoreLink().apply { state.value = linkConnecting() }
            link.answer = { cmd, _ -> if (cmd == "session-queue-put") receipt else Reply.LinkDown }
            val box =
                Box(
                    listOf(
                        Pending("p1", "s1", "t-1", PendingOutcome.PENDING, 1),
                        Pending("p2", "s2", "t-2", PendingOutcome.UNSURE, 1),
                        Pending("p3", "s3", "t-3", PendingOutcome.REFUSED, 1),
                        Pending("p4", null, "t-4", PendingOutcome.PENDING, 1),
                    ),
                )
            PendingResend(MutableStateFlow(mapOf<String, CoreLink>("h" to link)), { box }, backgroundScope)
            runCurrent()
            assertEquals("还没接上 ⇒ 不交", 0, link.calls("session-queue-put").size)
            link.state.value = linkUp()
            runCurrent()
            val puts = link.calls("session-queue-put")
            assertEquals(listOf("p1" to "s1", "p2" to "s2"), puts.map { it?.get("id") to it?.get("sid") })
            assertEquals("随带的附件照带", listOf("att-1"), puts[1]?.get("attachments"))
            assertEquals(mapOf("p1" to PendingOutcome.DELIVERED, "p2" to PendingOutcome.DELIVERED), box.settled)
        }

    @Test
    fun `那台拒了 ⇒ 记成没送到、不再交；没回执（断着 · 期限到）⇒ 留着等下一回接上`() =
        runTest {
            val link = FakeCoreLink().apply { state.value = linkConnecting() }
            val replies = ArrayDeque(listOf<Reply>(Reply.Failed(Failed("too_long", "s-refused", "")), Reply.TimedOut("t-said", "")))
            link.answer = { cmd, _ -> if (cmd == "session-queue-put") replies.removeFirst() else Reply.LinkDown }
            val box = Box(listOf(Pending("p1", "s1", "t-1", PendingOutcome.PENDING, 1), Pending("p2", "s2", "t-2", PendingOutcome.PENDING, 1)))
            PendingResend(MutableStateFlow(mapOf<String, CoreLink>("h" to link)), { box }, backgroundScope)
            link.state.value = linkUp()
            runCurrent()
            assertEquals(mapOf("p1" to PendingOutcome.REFUSED), box.settled)
        }

    @Test
    fun `那条会话开着 ⇒ 它的待发归它自己交，这里不交；没开的照交`() =
        runTest {
            val link = FakeCoreLink().apply { state.value = linkConnecting() }
            link.answer = { cmd, _ -> if (cmd == "session-queue-put") receipt else Reply.LinkDown }
            val box = Box(listOf(Pending("p1", "s1", "t-1", PendingOutcome.PENDING, 1), Pending("p2", "s2", "t-2", PendingOutcome.PENDING, 1)))
            PendingResend(MutableStateFlow(mapOf<String, CoreLink>("h" to link)), { box }, backgroundScope) { setOf("s1") }
            link.state.value = linkUp()
            runCurrent()
            assertEquals(listOf("p2"), link.calls("session-queue-put").map { it?.get("id") })
            assertEquals(mapOf("p2" to PendingOutcome.DELIVERED), box.settled)
        }
}
