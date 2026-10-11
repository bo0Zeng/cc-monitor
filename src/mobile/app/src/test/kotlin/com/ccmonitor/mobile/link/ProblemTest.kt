package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.ui.copy.copyText
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
        val p = Reply.Unreadable("""{"kind":"reply","ok":true}""").problem("box", "history-list")!!
        assertEquals(copyText("peerVersion.said.unreadable", "machine" to "box"), p.text)
        assertTrue(p.detail, p.detail.contains("history-list"))
        assertTrue(p.detail, p.detail.contains(""""ok":true"""))
    }

    @Test
    fun `成品解不出（应答在、形状不对）与应答缺格同一个说法`() {
        val p = unreadableReply("box", "session-new", "{outcome=started}")
        assertEquals(copyText("peerVersion.said.unreadable", "machine" to "box"), p.text)
        assertTrue(p.detail, p.detail.contains("session-new"))
    }
}
