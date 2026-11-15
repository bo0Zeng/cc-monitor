package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 装后端那一颗：字照成品给的那一颗（没给 ⇒［核对版本］）；按下去连上了才叫核心装；主机设置那一行忙不忙 · 句子照成品条那一格。
 *
 * 判别力边界：那一按连着 SSH 那层与核心，app 没引 Robolectric、门禁里起不来；装完 / 没装成那一句由 `ResidentFeedTest` 照行为钉
 * （`deploying`）。这里照行为钉 [deployLabel] 与那一按交不交（[DeployGate]：连上才交 · 一按最多交一次），
 * 照源码钉那一按只经那道门、主机设置那一行接的是哪几格。夹具里的按钮字是结构占位。
 */
class BackendDeployTest {
    @Test
    fun `成品给了［安装］或［更新］⇒ 按钮上是那一颗的字；没给 ⇒［核对版本］`() {
        assertEquals("b-install", deployLabel(MachineFixButton(MachineFix.INSTALL, "b-install")))
        assertEquals("b-update", deployLabel(MachineFixButton(MachineFix.UPDATE, "b-update")))
        assertEquals(copyText("mobHostEdit.backend.check"), deployLabel(null))
    }

    @Test
    fun `连上了才交核心装：还在连 · 接不上不交，连上那一下交`() {
        val gate = DeployGate()
        assertFalse(gate.take(1, HostLink.Connecting))
        assertFalse(gate.take(1, HostLink.Failed("x")))
        assertTrue(gate.take(1, HostLink.Ready))
    }

    @Test
    fun `一按最多交一次：按过一次、连接掉了又回到连上 ⇒ 不再交；再按一次才再交`() {
        val gate = DeployGate()
        val sent =
            listOf(HostLink.Connecting, HostLink.Ready, HostLink.Failed("x"), HostLink.Connecting, HostLink.Ready)
                .count { gate.take(1, it) }
        assertEquals("同一按交了 $sent 次", 1, sent)
        assertTrue("第二按连着也该交", gate.take(2, HostLink.Ready))
        assertFalse(gate.take(2, HostLink.Ready))
    }

    @Test
    fun `按下去才连那台；交不交只经那一道门、叫核心装只这一处`() {
        val press = bodyOf(code("src/main/kotlin/com/ccmonitor/mobile/ui/common/BackendDeploy.kt"), "fun rememberDeployPress(")
        val gate = press.indexOf("if (presses > 0)")
        assertTrue("没按过也连那台：$press", gate >= 0 && press.indexOf("rememberHostLink(") > gate)
        assertTrue(
            "交不交没经 DeployGate：$press",
            Regex("""if \(gate\.take\(presses, conn\)\) backends\.deploy\(hostId\)""").containsMatchIn(press),
        )
        assertEquals("叫核心装只这一处", 1, Regex("""backends\.deploy\(""").findAll(press).count())
        // 按数与那道门同生同灭：只把按数改成跨重建留着，重建后门是新的、一连上就补发那一按。
        assertTrue("按数不是 remember(hostId)：$press", press.contains("var presses by remember(hostId) {"))
        assertTrue("那道门不是 remember(hostId)：$press", press.contains("val gate = remember(hostId) { DeployGate() }"))
        assertFalse("按数或那道门跨重建留着：$press", press.contains("rememberSaveable"))
    }

    @Test
    fun `主机设置那一行：忙不忙 · 带不带进度照成品条那一格，忙着按钮置灰、摆成品那一句`() {
        val row = bodyOf(code("src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt"), "private fun BackendRow(")
        assertTrue(
            "忙不忙没照成品条那一格：$row",
            row.contains("barOf(link)?.takeIf { it.tone == Tone.BUSY }?.let { Busy(it.text, it.progress) }"),
        )
        assertTrue("忙着按钮没置灰：$row", row.contains("enabled = busy == null"))
        assertTrue("忙着没摆成品那一句：$row", row.contains("busy?.let { return BusyLine(it) }"))
        assertTrue("按钮上的字没走 deployLabel：$row", row.contains("Text(deployLabel(fix))"))
    }

    /** 从 [head] 起到同一层下一个声明之前那一段（注释已剥）。 */
    private fun bodyOf(
        code: String,
        head: String,
    ): String {
        val at = code.indexOf(head)
        assertTrue("前提：得找得到 $head", at >= 0)
        val next = Regex("\n(fun |val |private |internal |class |object |data class |@Composable)").find(code, at + head.length)?.range?.first ?: code.length
        return code.substring(at, next)
    }

    private fun code(relative: String): String =
        (File(relative).takeIf { it.isFile } ?: File("app/$relative")).let {
            assertTrue("前提：得找得到 $relative（实际找的是 ${it.absolutePath}）", it.isFile)
            KotlinSourceScanner.codeOnlyKeepingLiterals(it.readText())
        }
}
