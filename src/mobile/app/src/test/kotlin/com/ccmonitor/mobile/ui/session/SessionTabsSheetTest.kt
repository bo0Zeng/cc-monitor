package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.ui.session.SessionTabManager.OpenSession
import com.ccmonitor.mobile.ui.session.SessionTabManager.TabTarget
import org.junit.Assert.assertEquals
import org.junit.Test

/** 面板可读标签的纯函数：[tabBaseLabel] 的兜底链与 [numberDuplicates] 的同名副本编号。 */
class SessionTabsSheetTest {
    private fun sess(
        target: TabTarget,
        label: String? = null,
    ) = OpenSession(key = "sess-0", hostId = "h1", target = target, label = label)

    // 兜底链第 1 级：label（resume 会话可读标题）优先——即便 tmuxSession/resumeSessionId 同时在场。
    @Test
    fun labelWinsOverTmuxAndSid() {
        val s = sess(TabTarget.Terminal(tmuxSession = "work", resumeSessionId = "deadbeef-1234"), label = "修复登录 bug")
        assertEquals("devbox · 修复登录 bug", tabBaseLabel(s, "devbox"))
    }

    // 兜底链第 2 级：无 label → tmux 会话名（attach tab）。
    @Test
    fun tmuxSessionFallsBackWhenNoLabel() {
        val s = sess(TabTarget.Terminal(tmuxSession = "work"))
        assertEquals("devbox · work", tabBaseLabel(s, "devbox"))
    }

    // 兜底链第 3 级：无 label/tmux 名 → 以 resumeSessionId 推导实际 tmux 名 cc-<sid8>。
    @Test
    fun resumeSidDerivesCcNameWhenNoLabelNorTmux() {
        val s = sess(TabTarget.Terminal(cd = "/p", resumeSessionId = "deadbeef-1234-5678"))
        assertEquals("devbox · cc-deadbeef", tabBaseLabel(s, "devbox"))
    }

    // 兜底链第 4 级：全无 → 纯主机名（默认终端 tab）。
    @Test
    fun plainTerminalShowsHostLabelOnly() {
        assertEquals("devbox", tabBaseLabel(sess(TabTarget.Terminal()), "devbox"))
    }

    // SFTP tab 无 Terminal payload → 纯主机名（类型符 📁 由渲染处另加）。
    @Test
    fun sftpShowsHostLabelOnly() {
        assertEquals("devbox", tabBaseLabel(sess(TabTarget.Sftp), "devbox"))
    }

    // 编号规则：首个不编号，第 2/3 次出现补 ·2/·3；不同名互不影响；顺序保持。
    @Test
    fun numberDuplicatesAppendsOrdinalFromSecondOccurrence() {
        assertEquals(
            listOf("devbox", "nas", "devbox ·2", "devbox ·3", "nas ·2"),
            numberDuplicates(listOf("devbox", "nas", "devbox", "devbox", "nas")),
        )
    }

    @Test
    fun numberDuplicatesLeavesUniqueLabelsUntouched() {
        assertEquals(listOf("a", "b", "c"), numberDuplicates(listOf("a", "b", "c")))
        assertEquals(emptyList<String>(), numberDuplicates(emptyList()))
    }
}
