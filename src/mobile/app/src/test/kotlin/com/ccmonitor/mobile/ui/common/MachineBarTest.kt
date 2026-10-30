package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.MachineStripLine
import com.ccmonitor.mobile.core.wire.gen.StripTone
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import com.ccmonitor.mobile.ui.chat.machineOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 机器条：一台的连接态只照核心交的那一行画（那台的状态成品：进不进条与语气 `strip` · 那一句 · 点名的修法），手机不按态挑样子、不按 SSH 层自推。
 * 读一台机器的屏都摆同一个组件。夹具里的句子 · 按钮字是结构占位。
 */
class MachineBarTest {
    private val retry = MachineFixButton(MachineFix.RETRY, "b-retry")
    private val update = MachineFixButton(MachineFix.UPDATE, "b-update")

    /** 核心给那一行带上条上那一格。 */
    private fun LinkLine.striped(
        tone: StripTone,
        progress: Boolean = false,
    ): LinkLine {
        val m = machine.copy(strip = MachineStripLine(tone, progress))
        return when (this) {
            is LinkLine.Up -> copy(machine = m)
            is LinkLine.Down -> copy(machine = m)
            is LinkLine.Connecting -> copy(machine = m)
        }
    }

    @Test
    fun `那台离线 ⇒ 条上是核心那一句，按钮是核心点名的那一颗`() {
        val bar = barOf(linkDown("s-down", "d", listOf(retry)).striped(StripTone.OFFLINE))
        assertEquals(BarLook("s-down", StripTone.OFFLINE, retry, detail = "d"), bar)
    }

    @Test
    fun `正在连 ⇒ 核心那一句、不带按钮；正在装 ⇒ 那一句带进度；通着无事 ⇒ 不画`() {
        assertEquals(BarLook("s-conn", StripTone.BUSY), barOf(LinkLine.Connecting(machineOf(MachineStateKind.CONNECTING, "s-conn")).striped(StripTone.BUSY)))
        assertEquals(BarLook("s-inst", StripTone.BUSY, progress = true), barOf(linkDown("s-inst", state = MachineStateKind.INSTALLING).striped(StripTone.BUSY, progress = true)))
        assertNull(barOf(linkUp()))
        assertNull("核心还没交那一行 ⇒ 不猜", barOf(null))
    }

    @Test
    fun `进不进条只照核心那一格：比这一版新 · 停用的核心不给 ⇒ 不画；态再怎样，没给就不画`() {
        assertNull(barOf(linkUp("s-newer", state = MachineStateKind.NEWER)))
        assertNull(barOf(linkDown("s-off", state = MachineStateKind.DISABLED)))
        assertNull(barOf(linkDown("s-down", fixes = listOf(retry))))
        assertEquals(StripTone.FAULT, barOf(linkDown("s-key", state = MachineStateKind.HOST_KEY_CHANGED).striped(StripTone.FAULT))?.tone)
    }

    @Test
    fun `那台旧了 ⇒ 琥珀条、核心那一句与［更新］；装没装上 ⇒ 失败那一句与详情、按钮照留`() {
        val old = linkUp("s-old", fixes = listOf(update), state = MachineStateKind.NEEDS_UPDATE).striped(StripTone.WARN)
        assertEquals(BarLook("s-old", StripTone.WARN, update), barOf(old))
        assertEquals(BarLook("s-fail", StripTone.FAULT, update, detail = "d-fail"), barOf(old, Deploying.Failed(Said("s-fail", "d-fail"))))
    }

    @Test
    fun `聊天屏（含空新会话）与用量屏摆的都是这一个组件，用量屏不再自己画离线那一条`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val code = { rel: String -> KotlinSourceScanner.codeOnlyKeepingLiterals(File(root, "app/src/main/kotlin/com/ccmonitor/mobile/$rel").readText()) }
        assertTrue(code("ui/chat/ChatRoute.kt").contains("MachineBar("))
        val usage = code("ui/usage/UsageScreen.kt")
        assertTrue(usage.contains("MachineBar("))
        assertTrue("用量屏离线那一条交给机器条", !usage.contains("newSession.machine.reconnect"))
    }
}
