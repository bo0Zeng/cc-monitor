package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Tone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 状态点（V10，九种，与桌面 `kit/status-dot` 同一套）只照核心给的格画：颜色 ＝ 会话帧的 `activity_tone`，
 * 形状 ＝ 进程还在不在（活会话表 · `session_state.state` · `history-list` 行的 `status`）。字不参与：同一句不同语气 ⇒ 不同点。
 */
class SessionDotTest {
    private val frames = Fixtures.text("session-stream.golden.jsonl").lines().mapNotNull(Json::obj)

    private fun golden(kind: String) = frames.filter { it.str("kind") == kind }

    private fun live(tone: Tone?) = LiveSession(sid = "s", activity = Toned("同一句", tone))

    @Test
    fun `活着：点的颜色照语气，不看字`() {
        assertEquals(SessionDot.RUNNING, SessionDot.of(live(Tone.NOW), null, null))
        assertEquals(SessionDot.NEEDS_YOU, SessionDot.of(live(Tone.NEED), null, null))
        assertEquals(SessionDot.BACKGROUND, SessionDot.of(live(Tone.BUSY), null, null))
        assertEquals(SessionDot.IDLE, SessionDot.of(live(Tone.PLAIN), null, null))
        assertEquals("还没收到语气 ⇒ 同桌面，照在跑画", SessionDot.RUNNING, SessionDot.of(live(null), null, null))
    }

    @Test
    fun `离开活：session_state 的去向 ⇒ 已退出（tmux 还开着）· 已结束；活会话表在就照活的画`() {
        assertEquals(SessionDot.EXITED, SessionDot.of(null, SessionFate(SessionFate.State.RECONNECTABLE, "Claude 已退出"), "ended"))
        assertEquals(SessionDot.ENDED, SessionDot.of(null, SessionFate(SessionFate.State.ENDED, "已结束"), "live"))
        assertEquals(SessionDot.IDLE, SessionDot.of(live(Tone.PLAIN), SessionFate(SessionFate.State.ENDED, "已结束"), "ended"))
    }

    @Test
    fun `流上没有它：照 history-list 那一行的 status（live · ended · unknown）；认不出的不画`() {
        assertEquals(SessionDot.ENDED, SessionDot.of(null, null, "ended"))
        assertEquals(SessionDot.UNKNOWN, SessionDot.of(null, null, "unknown"))
        assertEquals("清单说活着、流上还没报到 ⇒ 同「还没收到语气」", SessionDot.RUNNING, SessionDot.of(null, null, "live"))
        assertNull(SessionDot.of(null, null, "别的词"))
        assertNull(SessionDot.of(null, null, null))
    }

    @Test
    fun `会话表折 session_state：记下去向与那一句；同一条再宣告 ⇒ 去向抹掉`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_state")[0], breaks::add)
        assertEquals(SessionFate(SessionFate.State.RECONNECTABLE, "Claude 已退出"), t.fates["s1"])
        assertFalse("去向不进活会话表", "s1" in t.sessions)
        t = t.step(golden("session_added")[0], breaks::add)
        assertNull("又活了 ⇒ 去向作废", t.fates["s1"])
        t = t.step(mapOf("kind" to "session_state", "sid" to "s9", "state" to "别的词", "state_text" to "x"), breaks::add)
        assertNull("认不出的去向不记（不猜）", t.fates["s9"])
        assertTrue(breaks.toString(), breaks.size == 1)
    }

    @Test
    fun `history-list 每行的 status 照抄`() {
        val a = HistoryList.of(Fixtures.json("history-list.golden.json"))!!
        assertEquals(listOf("ended", "live", "ended", "unknown", "ended"), a.rows.map { it.status })
    }
}
