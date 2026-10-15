package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.defaultKeys
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 键条一个触发派发出去：送输入的字节（xterm）· App 动作路由 · 要 tmux 的在没 tmux 的服务器上无害。 */
class KeyDispatchTest {
    private fun sp(
        k: SpecialKey,
        vararg m: KeyMod,
    ) = KeyPress.Special(k, m.toSet())

    private fun bytes(vararg p: KeyPress) = String(inputBytes(p.toList()), Charsets.ISO_8859_1)

    @Test
    fun specialKeysAndCombosSendXtermBytes() {
        assertEquals("\u001b", bytes(sp(SpecialKey.ESC)))
        assertEquals("\u001b\u001b", bytes(sp(SpecialKey.ESC, KeyMod.ALT)))
        assertEquals("\t", bytes(sp(SpecialKey.TAB)))
        assertEquals("\u001b[Z", bytes(sp(SpecialKey.TAB, KeyMod.SHIFT)))
        assertEquals("\r", bytes(sp(SpecialKey.ENTER)))
        assertEquals("\u007f", bytes(sp(SpecialKey.BACKSPACE)))
        assertEquals("\u001b[3~", bytes(sp(SpecialKey.DELETE)))
        assertEquals("\u001b[A", bytes(sp(SpecialKey.UP)))
        assertEquals("\u001b[1;5D", bytes(sp(SpecialKey.LEFT, KeyMod.CTRL)))
        assertEquals("\u001b[1;5F", bytes(sp(SpecialKey.END, KeyMod.CTRL)))
        assertEquals("\u001b[H", bytes(sp(SpecialKey.HOME)))
        assertEquals("\u001b[5~", bytes(sp(SpecialKey.PAGE_UP)))
        assertEquals("\u001b[6;2~", bytes(sp(SpecialKey.PAGE_DOWN, KeyMod.SHIFT)))
        assertEquals("\u001bOP", bytes(sp(SpecialKey.F1)))
        assertEquals("\u001b[1;3S", bytes(sp(SpecialKey.F4, KeyMod.ALT)))
        assertEquals("\u001b[24~", bytes(sp(SpecialKey.F12)))
        assertEquals("\u0003", bytes(KeyPress.Chord("c", setOf(KeyMod.CTRL))))
        assertEquals("\u001bx", bytes(KeyPress.Chord("x", setOf(KeyMod.ALT))))
        assertEquals("\u0002c", bytes(KeyPress.Chord("b", setOf(KeyMod.CTRL)), KeyPress.Text("c")))
        assertEquals("git status\r", bytes(KeyPress.Text("git status"), sp(SpecialKey.ENTER)))
        assertArrayEquals("中".toByteArray(Charsets.UTF_8), inputBytes(listOf(KeyPress.Text("中"))))
    }

    @Test
    fun everyDefaultKeyThatSendsInputSendsSomething() {
        defaultKeys().mapNotNull { it.tap as? KeyBinding.Input }.forEach {
            assert(inputBytes(it.presses).isNotEmpty()) { "$it 送出空字节" }
        }
    }

    @Test
    fun appActionsRouteAndTmuxOnesAreHarmlessWithoutTmux() {
        val hit = mutableListOf<String>()

        fun actions(tmux: Boolean) =
            KeyActions(
                supportsTmux = tmux,
                onShowTabs = { hit += "tabs" },
                onCommandPalette = { hit += "palette" },
                onTmuxSessions = { hit += "tmux" },
                onPaste = { hit += "paste" },
                onCapture = { hit += "capture" },
                onPushKey = { hit += "pushKey" },
                onShowKeyboard = { hit += "keyboard" },
                onEditKeys = { hit += "edit" },
                onNextGroup = { hit += "next" },
                onGotoGroup = { hit += "goto:$it" },
            )
        var sent: ByteArray? = null
        AppAction.entries.forEach { a -> dispatchBinding(KeyBinding.App(a, "g1"), { sent = it }, actions(true)) }
        assertEquals(listOf("next", "goto:g1", "edit", "paste", "tabs", "palette", "tmux", "keyboard", "capture", "pushKey"), hit)
        assertNull("App 动作不出线字节", sent)
        hit.clear()
        AppAction.entries.forEach { a -> dispatchBinding(KeyBinding.App(a, "g1"), { sent = it }, actions(false)) }
        assertEquals(listOf("next", "goto:g1", "edit", "paste", "tabs", "palette", "keyboard"), hit)
        hit.clear()
        dispatchBinding(KeyBinding.App(AppAction.GOTO_GROUP), { }, actions(true))
        assertEquals("切到某一组没带组 ⇒ 不动", emptyList<String>(), hit)
        dispatchBinding(KeyBinding.Input(listOf(KeyPress.Text("ls"))), { sent = it }, actions(true))
        assertEquals("ls", String(sent!!))
    }
}
