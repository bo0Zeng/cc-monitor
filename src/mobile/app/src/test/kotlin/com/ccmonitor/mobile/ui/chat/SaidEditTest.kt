package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.RewindChoiceRow
import com.ccmonitor.mobile.core.wire.gen.RewindHow
import com.ccmonitor.mobile.core.wire.gen.RewindOptions
import com.ccmonitor.mobile.core.wire.gen.RewindWhen
import com.ccmonitor.mobile.core.wire.gen.RewindWhenRow
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 编辑已发的一句（手机）：编辑草稿落一格字、读回来一样；发送那一下的缺省照那台给的；确认那一下投进队的那一句带 `rewind`，
 * 没回执被杀回来照同一个编号带 `rewind` 再交，回执到了编辑草稿删掉。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SaidEditTest {
    /** 只记草稿的缓存（别的照不缓存）。 */
    private class DraftCache : ChatCache by ChatCache.None {
        val drafts = mutableMapOf<String, String>()

        override suspend fun draft(sid: String) = drafts[sid]

        override suspend fun saveDraft(
            sid: String,
            text: String,
        ) {
            if (text.isEmpty()) drafts.remove(sid) else drafts[sid] = text
        }
    }

    private val queued = Reply.Ok(mapOf("outcome" to "queued", "seq" to 1L, "said" to "q-1"))

    @Test
    fun `编辑草稿写成一格字再读回来一样（带换行 · 带待发编号）；读不懂的那几条不要`() {
        val all =
            listOf(
                EditDraft("u1", "第一行\n第二行"),
                EditDraft("u2", "改过的", EditDraft.Sent("local-1", RewindHow.CODE_AND_CONVERSATION, RewindWhen.NOW)),
            )
        assertEquals(all, EditDrafts.decode(EditDrafts.encode(all)))
        assertEquals(emptyList<EditDraft>(), EditDrafts.decode("乱写的"))
        assertEquals(emptyList<EditDraft>(), EditDrafts.decode(null))
    }

    @Test
    fun `发送那一下的缺省：照那台标的；没给什么时候退 ⇒ 等这一轮跑完；怎么退没有项 ⇒ 不支持`() {
        val how = listOf(RewindChoiceRow(RewindHow.CONVERSATION, "a", false, true), RewindChoiceRow(RewindHow.CODE_AND_CONVERSATION, "b", true, true))
        assertEquals(RewindHow.CODE_AND_CONVERSATION to RewindWhen.AFTER_TURN, defaultsOf(RewindOptions(how, emptyList())))
        val whn = listOf(RewindWhenRow(RewindWhen.AFTER_TURN, "c", false), RewindWhenRow(RewindWhen.NOW, "d", true))
        assertEquals(RewindHow.CODE_AND_CONVERSATION to RewindWhen.NOW, defaultsOf(RewindOptions(how, whn)))
        assertNull(defaultsOf(RewindOptions(emptyList(), emptyList(), "不支持")))
    }

    @Test
    fun `缺省只落在能选的那一项上；一项都不能选 ⇒ 退不了`() {
        val off = RewindChoiceRow(RewindHow.CODE_AND_CONVERSATION, "b", true, false, "此句之后未改动代码")
        val on = RewindChoiceRow(RewindHow.CONVERSATION, "a", false, true)
        assertEquals(RewindHow.CONVERSATION to RewindWhen.AFTER_TURN, defaultsOf(RewindOptions(listOf(on, off), emptyList())))
        assertNull(defaultsOf(RewindOptions(listOf(off), emptyList())))
    }

    @Test
    fun `问选项带被编辑那一句的 uuid`() =
        runTest {
            val link = FakeCoreLink()
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = DraftCache()).also { it.open() }
            runCurrent()
            s.edits.options("u2")
            assertEquals("u2", link.calls("session-rewind-options").single()!!["to"])
        }

    @Test
    fun `确认那一下：投进队的那一句带 rewind；没回执被杀回来照同一个编号带 rewind 再交；回执到了编辑草稿删掉`() =
        runTest {
            val link = FakeCoreLink()
            val cache = DraftCache()
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).also { it.open() }
            runCurrent()
            s.edits.text("u2", "改了一半")
            runCurrent()
            assertEquals(mapOf("u2" to "改了一半"), s.state.value.editDrafts)
            s.edits.send("u2", "改过的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            val first = link.calls("session-queue-put").single()!!
            assertEquals(mapOf("to" to "u2", "how" to "conversation", "when" to "after_turn"), first["rewind"])
            assertEquals("改过的", first["text"])

            link.calls.clear()
            link.answer = { cmd, _ -> if (cmd == "session-queue-put") queued else Reply.LinkDown }
            ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).open()
            runCurrent()
            val again = link.calls("session-queue-put").single()!!
            assertEquals("同一个编号再交", first["id"], again["id"])
            assertEquals(first["rewind"], again["rewind"])
            assertNull("回执到了 ⇒ 编辑草稿删掉", EditDrafts.decode(cache.drafts[EditDrafts.key("s1")]).firstOrNull { it.uuid == "u2" })
        }

    @Test
    fun `退法抽屉的按钮：在问 ⇒ 骨架 ＋［取消］＋ 灰着的主按钮；有项 ⇒［取消］＋ 主按钮；都不能选 ⇒［关闭］；问不到 ⇒［重试］＋［取消］`() {
        val on = RewindChoiceRow(RewindHow.CONVERSATION, "a", true, true)
        val off = RewindChoiceRow(RewindHow.CODE_AND_CONVERSATION, "b", false, false, "主 agent 正在等批准")
        val asking = sheetFace(RewindPick.Asking)
        assertEquals(true, asking.skeleton)
        assertEquals(listOf(SheetAct.CANCEL, SheetAct.CONFIRM), asking.acts)
        assertEquals(false, asking.confirm)
        val got = sheetFace(RewindPick.Got(RewindOptions(listOf(on, off), emptyList())))
        assertEquals(listOf(SheetAct.CANCEL, SheetAct.CONFIRM), got.acts)
        assertEquals(true, got.confirm)
        assertEquals(false, got.skeleton)
        assertEquals(listOf(SheetAct.CLOSE), sheetFace(RewindPick.Got(RewindOptions(listOf(off), emptyList(), "先答完批准再改这一句"))).acts)
        assertEquals(listOf(SheetAct.RETRY, SheetAct.CANCEL), sheetFace(RewindPick.Failed("devbox 无应答")).acts)
    }
}
