package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineDot
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.machineOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File

/**
 * 机器列表一行的点与第二层尾字照那台的状态成品画（核心交的那一行，同桌面一份）：点是成品的 `dot`（后端那一处定），
 * 不照态、也不照 SSH 那层的通没通自己推。尾字是成品的 `word`（夹具里的字是结构占位）。
 */
class HostRowLookTest {
    private fun word(
        state: MachineStateKind,
        w: String,
        dot: MachineDot,
    ) = machineOf(state, dot = dot).copy(word = w)

    @Test
    fun `一台正在连 ⇒ 那一行出成品给的待定那一档与「连接中」`() {
        val look = hostRowLook(LinkLine.Connecting(word(MachineStateKind.CONNECTING, "w1", MachineDot.PENDING)))
        assertEquals(MachineDot.PENDING, look.dot)
        assertEquals("w1", look.word)
    }

    @Test
    fun `点只照成品的 dot：通着 · 那台旧了 · 断着 · 停用各是成品那一档；核心还没交 ⇒ 不画`() {
        assertEquals(HostRowLook(MachineDot.UP, null), hostRowLook(LinkLine.Up(1L, word(MachineStateKind.UP, "", MachineDot.UP))))
        assertEquals(HostRowLook(MachineDot.WARN, "w2"), hostRowLook(LinkLine.Up(1L, word(MachineStateKind.NEEDS_UPDATE, "w2", MachineDot.WARN))))
        assertEquals(HostRowLook(MachineDot.FAILED, "w3"), hostRowLook(LinkLine.Down(word(MachineStateKind.DOWN, "w3", MachineDot.FAILED))))
        assertEquals(HostRowLook(MachineDot.PENDING, "w4"), hostRowLook(LinkLine.Down(word(MachineStateKind.INSTALLING, "w4", MachineDot.PENDING))))
        // 停用：手机与桌面同一档（关着），不再画成离线的红点。
        assertEquals(HostRowLook(MachineDot.OFF, "w5"), hostRowLook(LinkLine.Down(word(MachineStateKind.DISABLED, "w5", MachineDot.OFF))))
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
