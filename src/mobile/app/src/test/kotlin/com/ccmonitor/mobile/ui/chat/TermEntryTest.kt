package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.session.SessionTabManager.TabTarget
import org.junit.Assert.assertEquals
import org.junit.Test

/** ⋯「在终端里打开」：终端还在 ⇒ 附着那个（第二行是它的名字）；已经不在 ⇒ 终端屏先让那台恢复这条会话再附着。 */
class TermEntryTest {
    @Test
    fun `终端还在 ⇒ 附着它；不在 ⇒ 带着 sid 去终端屏先恢复`() {
        val there = TermEntry.Attach("proj-cc")
        assertEquals("proj-cc", there.second)
        assertEquals(TabTarget.Terminal(tmuxSession = "proj-cc"), there.target("s1"))
        assertEquals(copyText("terminal.empty.endedHint"), TermEntry.Resume.second)
        assertEquals(TabTarget.Terminal(resume = "s1"), TermEntry.Resume.target("s1"))
    }
}
