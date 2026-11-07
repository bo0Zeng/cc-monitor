package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.MachineStripLine
import com.ccmonitor.mobile.core.wire.gen.Tone
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
        tone: Tone,
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
        val bar = barOf(linkDown("s-down", "d", listOf(retry)).striped(Tone.OFFLINE))
        assertEquals(BarLook("s-down", Tone.OFFLINE, retry, detail = "d"), bar)
    }

    @Test
    fun `正在连 ⇒ 核心那一句、不带按钮；正在装 ⇒ 那一句带进度；通着无事 ⇒ 不画`() {
        assertEquals(BarLook("s-conn", Tone.BUSY), barOf(LinkLine.Connecting(machineOf(MachineStateKind.CONNECTING, "s-conn")).striped(Tone.BUSY)))
        assertEquals(BarLook("s-inst", Tone.BUSY, progress = true), barOf(linkDown("s-inst", state = MachineStateKind.INSTALLING).striped(Tone.BUSY, progress = true)))
        assertNull(barOf(linkUp()))
        assertNull("核心还没交那一行 ⇒ 不猜", barOf(null))
    }

    @Test
    fun `进不进条只照核心那一格：比这一版新 · 停用的核心不给 ⇒ 不画；态再怎样，没给就不画`() {
        assertNull(barOf(linkUp("s-newer", state = MachineStateKind.NEWER)))
        assertNull(barOf(linkDown("s-off", state = MachineStateKind.DISABLED)))
        assertNull(barOf(linkDown("s-down", fixes = listOf(retry))))
        assertEquals(Tone.FAIL, barOf(linkDown("s-key", state = MachineStateKind.HOST_KEY_CHANGED).striped(Tone.FAIL))?.tone)
    }

    @Test
    fun `那台旧了 ⇒ 琥珀条、核心那一句与［更新］`() {
        val old = linkUp("s-old", fixes = listOf(update), state = MachineStateKind.NEEDS_UPDATE).striped(Tone.WARN)
        assertEquals(BarLook("s-old", Tone.WARN, update), barOf(old))
    }

    @Test
    fun `聊天屏（含空新会话）与用量屏摆的都是这一个组件，用量屏不再自己画离线那一条`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val code = { rel: String -> KotlinSourceScanner.codeOnlyKeepingLiterals(File(root, "app/src/main/kotlin/com/ccmonitor/mobile/$rel").readText()) }
        assertTrue(code("ui/chat/ChatRoute.kt").contains("MachineBar("))
        val usage = code("ui/usage/UsageScreen.kt")
        assertTrue(usage.contains("MachineBar("))
        assertTrue("用量屏离线那一条交给机器条", !usage.contains("history.list.reconnect"))
    }

    @Test
    fun `机器条只一种：读一台的那几屏都摆它，旧的那几种条一个不剩`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val base = File(root, "app/src/main/kotlin/com/ccmonitor/mobile")
        val code = { rel: String -> KotlinSourceScanner.codeOnlyKeepingLiterals(File(base, rel).readText()) }
        val bar = Regex("""\bMachineBar(View)?\(""")
        val screens =
            listOf(
                "ui/chats/ChatsScreen.kt",
                "ui/session/SessionScreen.kt",
                "ui/files/FilesScreen.kt",
                "ui/rotation/RotationScreen.kt",
                "ui/chat/ChatRoute.kt",
                "ui/usage/UsageScreen.kt",
                "ui/settings/SettingsScreen.kt",
                "ui/settings/AccountsScreen.kt",
                "ui/agents/AgentsScreen.kt",
            )
        val missing = screens.filterNot { bar.containsMatchIn(code(it)) }
        assertEquals("这几屏读一台，却没摆机器条", emptyList<String>(), missing)
        // 原来各屏自画的那几种（会话列表顶条 · 终端掉线条幅 · 文件离线框 · 轮换离线框）。
        val old = Regex("""\b(HostBarView|DroppedBanner|OfflineBand|DownBox)\b""")
        val all = base.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue("前提：扫到了机器条自己", all.any { it.name == "MachineBar.kt" })
        val left = all.filter { old.containsMatchIn(KotlinSourceScanner.codeOnlyKeepingLiterals(it.readText())) }.map { it.name }
        assertEquals("还有屏自画机器条", emptyList<String>(), left)
    }

    @Test
    fun `修法各做什么只一张表：换成密钥走换钥匙那一下，改设置那几种没给编辑页就不画按钮`() {
        val hit = mutableListOf<String>()
        val acts = FixActs({ hit += "reconnect" }, { hit += "deploy" }, null, { hit += "useKey" })
        fixAction(MachineFix.USE_KEY, acts)?.invoke()
        fixAction(MachineFix.RETRY, acts)?.invoke()
        fixAction(MachineFix.INSTALL, acts)?.invoke()
        assertEquals(listOf("useKey", "reconnect", "deploy"), hit)
        assertNull(fixAction(MachineFix.PUSH_KEY, acts))
        assertNull(fixAction(MachineFix.CONN_SETTINGS, acts))
    }
}
