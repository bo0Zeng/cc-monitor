package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.machineOf
import com.ccmonitor.mobile.ui.common.MachineDot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File

/**
 * 机器列表一行的点与第二层尾字照那台的状态成品画（核心交的那一行，同桌面一份）：连接中 · 在线 · 要更新 · 离线；
 * 不照 SSH 那层的通没通自己推。尾字是成品的 `word`（夹具里的字是结构占位）。
 */
class HostRowLookTest {
    private fun word(
        state: MachineStateKind,
        w: String,
    ) = machineOf(state).copy(word = w)

    @Test
    fun `一台正在连 ⇒ 那一行出空心点与成品的「连接中」`() {
        val look = hostRowLook(LinkLine.Connecting(word(MachineStateKind.CONNECTING, "w1")))
        assertEquals(MachineDot.CONNECTING, look.dot)
        assertEquals("w1", look.word)
    }

    @Test
    fun `通着 ⇒ 绿点不带字；那台旧了 ⇒ 琥珀点与「要更新」；断着 ⇒ 红点与成品那个词；核心还没交 ⇒ 不画`() {
        assertEquals(HostRowLook(MachineDot.UP, null), hostRowLook(LinkLine.Up(1L, word(MachineStateKind.UP, ""))))
        assertEquals(HostRowLook(MachineDot.WARN, "w2"), hostRowLook(LinkLine.Up(1L, word(MachineStateKind.NEEDS_UPDATE, "w2"))))
        assertEquals(HostRowLook(MachineDot.FAILED, "w3"), hostRowLook(LinkLine.Down(word(MachineStateKind.DOWN, "w3"))))
        assertEquals(HostRowLook(MachineDot.CONNECTING, "w4"), hostRowLook(LinkLine.Down(word(MachineStateKind.INSTALLING, "w4"))))
        assertEquals(HostRowLook(null, null), hostRowLook(null))
    }

    @Test
    fun `机器列表顶栏没有「标签页」那一颗（终端标签页从抽屉与终端工具条进）`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val code = KotlinSourceScanner.codeOnlyKeepingLiterals(File(root, "app/src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt").readText())
        assertFalse(code.contains("mobHosts.list.tabs"))
        assertFalse("不再按 SSH 那层的状态推点", code.contains("linkDots"))
    }
}
