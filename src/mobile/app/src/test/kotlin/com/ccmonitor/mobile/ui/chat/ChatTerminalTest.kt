package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.wire.gen.CanView
import com.ccmonitor.mobile.core.wire.gen.TermOpen
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.session.SessionTabManager.TabTarget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 顶栏「终端」那一颗照脸上「看它的终端」那一格 `can.view` 画（手机拉不了窗，只看）：开哪个、灰不灰、长按那一句都是那台写好的，
 * 手机不另问那台它在哪个终端里（`sessions-where`），也不按那台的答自己判「附着还是恢复」。脸从整份脸金样取。
 */
class ChatTerminalTest {
    private fun view(face: String) =
        Faces
            .of(face)
            .can.view

    @Test
    fun `接进它住的那个终端：开的是那台写在脸上的句柄；句柄判不出 ⇒ 灰着`() {
        val here = CanView.Yes(said = "S", open = TermOpen.Attach(terminal = "tmux-3-7"))
        assertEquals(TabTarget.Terminal(tmuxSession = "tmux-3-7"), terminalTarget(here, "s1"))
        assertNull(terminalTarget(CanView.Yes(said = "S", open = TermOpen.Attach(terminal = null)), "s1"))
        assertEquals("S", terminalSaid(here))
    }

    @Test
    fun `已结束能恢复 ⇒ 先恢复再附着；做不了 ⇒ 灰着、长按出那台写的那一句`() {
        assertEquals(TabTarget.Terminal(resume = "s1"), terminalTarget(view("ended"), "s1"))
        val gone = view("gone")
        assertNull(terminalTarget(gone, "s1"))
        assertEquals((gone as CanView.No).why, terminalSaid(gone))
        val bare = view("live-working-plain")
        assertNull("不在任何宿主里：别处没有终端可接", terminalTarget(bare, "s1"))
    }

    @Test
    fun `还没收到脸 ⇒ 照画、灰着、不说为什么`() {
        assertNull(terminalTarget(null, "s1"))
        assertNull(terminalSaid(null))
    }

    @Test
    fun `顶栏那一颗常驻：会话起好了就画、带 TERMINAL 标、在 ⋯ 前面，不问那台在哪`() {
        val route = KotlinSourceScanner.codeOnlyKeepingLiterals(source("ChatRoute.kt"))
        val bar = route.substringAfter("private fun ChatTopBar(").substringAfter("actions = {")
        val gate = bar.indexOf("if (sid != null) {")
        val term = bar.indexOf("TerminalButton(")
        val more = bar.indexOf("ChatTestTags.MORE")
        assertTrue("「终端」那一颗只看会话起没起好", gate in 0 until term)
        assertTrue("「终端」在 ⋯ 前面", term in 0 until more)
        val button = route.substringAfter("private fun TerminalButton(").substringBefore("internal fun terminalTarget(")
        assertTrue("那一颗带 TERMINAL 标", button.contains("testTag(ChatTestTags.TERMINAL)"))
        assertFalse("不问那台它在哪个终端里", route.contains("sessions-where"))
    }

    private fun source(name: String): String {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().first { it.isFile && it.name == name }.readText()
    }
}
