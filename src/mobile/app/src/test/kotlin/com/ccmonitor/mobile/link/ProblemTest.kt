package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 回包解不出：同一个 BUILD_ID 下那头就是 cc-monitor 后端，只是这一回的内容对不上 ⇒ 说「内容无法解析」那一族，
 * 不说「远端回的不是 cc-monitor 后端」；复制详情带上命令名与原文。
 */
class ProblemTest {
    @Test
    fun `应答缺格：读取那台后端回复失败 · 内容无法解析；详情带命令名与原文`() {
        val p = Reply.Unreadable("""{"kind":"reply","ok":true}""").problem("box", "history-list", OFF)!!
        assertEquals(copyText("peerVersion.said.unreadable", "machine" to "box"), p.text)
        assertTrue(p.detail, p.detail.contains("history-list"))
        assertTrue(p.detail, p.detail.contains(""""ok":true"""))
    }

    @Test
    fun `断着：照链上交的离线那一句原样摆`() {
        assertEquals(OFF, Reply.LinkDown.problem("box", "history-list", OFF)!!.text)
    }

    @Test
    fun `期限到 · 撤了：核心写好的那一句与详情原样，手机不另取字`() {
        assertEquals(Problem("t-said", "t-detail"), Reply.TimedOut("t-said", "t-detail").problem("box", "history-list", OFF))
        assertEquals(Problem("c-said", "c-detail"), Reply.Cancelled("c-said", "c-detail").problem("box", "history-list", OFF))
    }

    @Test
    fun `成品解不出（应答在、形状不对）与应答缺格同一个说法`() {
        val p = unreadableReply("box", "session-open", "{outcome=started}")
        assertEquals(copyText("peerVersion.said.unreadable", "machine" to "box"), p.text)
        assertTrue(p.detail, p.detail.contains("session-open"))
    }

    private companion object {
        /** 链上交的离线那一句（结构夹具）。 */
        const val OFF = "夹具·box 离线那一句"
    }
}
