package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Launch
import com.ccmonitor.mobile.core.wire.gen.StandingWord
import com.ccmonitor.mobile.core.wire.gen.TerminalAt
import com.ccmonitor.mobile.core.wire.gen.Whereabout
import com.ccmonitor.mobile.ui.session.SessionTabManager.TabTarget
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * ⋯「在终端里打开」照那台 `sessions-where` 的答摆：有终端带着它 ⇒ 附着第一个（第二行是它的名字；几个由那台数）；
 * 没有 ⇒ 终端屏先让那台恢复这条会话再附着。手机不拿名单自己找。
 */
class TermEntryTest {
    private fun at(
        standing: StandingWord,
        vararg names: String,
    ) = Whereabout(sid = "s1", standing = standing, names = names.toList(), terminals = names.map { TerminalAt("tmux", "h-$it") }, run = Launch.TMUX)

    @Test
    fun `终端还在 ⇒ 附着它；不在 ⇒ 带着 sid 去终端屏先恢复`() {
        val there = TermEntry.Attach("proj-cc")
        assertEquals("proj-cc", there.second)
        assertEquals(TabTarget.Terminal(tmuxSession = "proj-cc"), there.target("s1"))
        assertEquals(copyText("terminal.empty.endedHint"), TermEntry.Resume.second)
        assertEquals(TabTarget.Terminal(resume = "s1"), TermEntry.Resume.target("s1"))
    }

    @Test
    fun `在跑 · 空着 ⇒ 附着那一个`() {
        assertEquals(TermEntry.Attach("w"), TermEntry.of(at(StandingWord.RUNNING, "w")))
        assertEquals(TermEntry.Attach("idle-1"), TermEntry.of(at(StandingWord.IDLE, "idle-1")))
    }

    @Test
    fun `不止一个在跑 ⇒ 附着第一个，第二行写几个`() {
        val e = TermEntry.of(at(StandingWord.AMBIGUOUS, "a", "b"))
        assertEquals(TermEntry.Attach("a", many = 2), e)
        assertEquals(copyText("tabMenu.attach.dupesWhy", "n" to 2, "name" to "a"), e.second)
    }

    @Test
    fun `没有终端带着它 ⇒ 恢复`() {
        assertEquals(TermEntry.Resume, TermEntry.of(at(StandingWord.NONE)))
        assertEquals(TermEntry.Resume, TermEntry.of(at(StandingWord.NO_TMUX)))
    }
}
