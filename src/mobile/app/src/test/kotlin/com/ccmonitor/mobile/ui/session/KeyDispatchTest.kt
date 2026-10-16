package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.defaultKeys
import com.ccmonitor.mobile.core.terminal.KeyModifiers
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
    fun appActionsRouteAndTmuxOnesSayUnavailableWithoutTmux() {
        val hit = mutableListOf<String>()

        fun actions(
            noTmux: String? = null,
            noBackend: String? = null,
            groupExists: Boolean = true,
        ) = KeyActions(
            noTmux = noTmux,
            noBackend = noBackend,
            onShowTabs = { hit += "tabs" },
            onCommandPalette = { hit += "palette" },
            onTmuxSessions = { hit += "tmux" },
            onPaste = { hit += "paste" },
            onCapture = { hit += "capture" },
            onPushKey = { hit += "pushKey" },
            onShowKeyboard = { hit += "keyboard" },
            onEditKeys = { hit += "edit" },
            onNextGroup = { hit += "next" },
            onGotoGroup = {
                hit += "goto:$it"
                groupExists
            },
        )
        var sent: ByteArray? = null
        var takes = 0
        val take = {
            takes++
            KeyModifiers()
        }
        val ok = AppAction.entries.map { a -> dispatchBinding(KeyBinding.App(a, "g1"), { sent = it }, actions(), take) }
        assertEquals(listOf("next", "goto:g1", "edit", "paste", "tabs", "palette", "tmux", "keyboard", "capture", "pushKey"), hit)
        assertEquals(AppAction.entries.map { null }, ok)
        assertNull("App 动作不出线字节", sent)
        assertEquals("App 动作不吃修饰键", 0, takes)
        hit.clear()
        val noTmux = AppAction.entries.associateWith { a -> dispatchBinding(KeyBinding.App(a, "g1"), { sent = it }, actions(noTmux = "box 无 tmux"), take) }
        assertEquals(listOf("next", "goto:g1", "edit", "paste", "tabs", "palette", "keyboard", "pushKey"), hit)
        assertEquals(AppAction.NEEDS_TMUX, noTmux.filterValues { it == KeyActionFail.Unavailable("box 无 tmux") }.keys)
        hit.clear()
        val down = AppAction.entries.associateWith { a -> dispatchBinding(KeyBinding.App(a, "g1"), { sent = it }, actions(noBackend = "后端未连上"), take) }
        assertEquals("推公钥只要那台后端，不要 tmux", mapOf(AppAction.PUSH_KEY to KeyActionFail.Unavailable("后端未连上")), down.filterValues { it != null })
        hit.clear()
        assertEquals(KeyActionFail.GroupGone, dispatchBinding(KeyBinding.App(AppAction.GOTO_GROUP), { }, actions(), take))
        assertEquals("切到某一组没带组 ⇒ 不动", emptyList<String>(), hit)
        assertEquals(KeyActionFail.GroupGone, dispatchBinding(KeyBinding.App(AppAction.GOTO_GROUP, "gone"), { }, actions(groupExists = false), take))
        assertNull(dispatchBinding(KeyBinding.Input(listOf(KeyPress.Text("ls"))), { sent = it }, actions(), take))
        assertEquals("ls", String(sent!!))
        assertEquals("送输入取一次修饰键", 1, takes)
    }

    private fun lit(
        vararg m: KeyMod,
    ) = KeyModifiers(ctrl = KeyMod.CTRL in m, alt = KeyMod.ALT in m, shift = KeyMod.SHIFT in m)

    private fun withMods(
        mods: KeyModifiers,
        vararg p: KeyPress,
    ) = String(inputBytes(withModifiers(p.toList(), mods)), Charsets.ISO_8859_1)

    @Test
    fun litModifiersGoOntoTheFirstKeystrokeOfAKeyBarInput() {
        assertEquals("\u001b[1;5D", withMods(lit(KeyMod.CTRL), sp(SpecialKey.LEFT)))
        assertEquals("\u001b[Z", withMods(lit(KeyMod.SHIFT), sp(SpecialKey.TAB)))
        assertEquals("\u0003", withMods(lit(KeyMod.CTRL), KeyPress.Text("c")))
        assertEquals("C", withMods(lit(KeyMod.SHIFT), KeyPress.Text("c")))
        assertEquals("只带第一下", "\u000cs -la\r", withMods(lit(KeyMod.CTRL), KeyPress.Text("ls -la"), sp(SpecialKey.ENTER)))
        assertEquals("\u001b\u0002", withMods(lit(KeyMod.ALT), KeyPress.Chord("b", setOf(KeyMod.CTRL))))
        assertEquals("\u001b[1;7A", withMods(lit(KeyMod.ALT), sp(SpecialKey.UP, KeyMod.CTRL)))
        assertEquals("没点亮 ⇒ 原样", "ls", withMods(KeyModifiers(), KeyPress.Text("ls")))
    }

    @Test
    fun keyBarInputSendsWithWhatIsLitAtThatMoment() {
        var sent: ByteArray? = null
        val noop = {}
        val a = KeyActions(null, null, noop, noop, noop, noop, noop, noop, noop, noop, noop, { true })
        dispatchBinding(KeyBinding.Input(listOf(sp(SpecialKey.RIGHT))), { sent = it }, a) { lit(KeyMod.CTRL, KeyMod.SHIFT) }
        assertEquals("\u001b[1;6C", String(sent!!, Charsets.ISO_8859_1))
    }

    @Test
    fun combosThatCannotBeSentOrSendTheSameAsAnotherAreTold() {
        fun ch(
            c: String,
            vararg m: KeyMod,
        ) = KeyPress.Chord(c, m.toSet())
        assertEquals(PressVerdict.Sendable, pressVerdict(ch("a", KeyMod.CTRL)))
        assertEquals(PressVerdict.Sendable, pressVerdict(ch("a", KeyMod.SHIFT)))
        assertEquals(PressVerdict.Sendable, pressVerdict(KeyPress.Text("ls")))
        assertEquals(PressVerdict.Sendable, pressVerdict(sp(SpecialKey.TAB, KeyMod.SHIFT)))
        assertEquals(PressVerdict.Sendable, pressVerdict(sp(SpecialKey.UP, KeyMod.CTRL, KeyMod.ALT, KeyMod.SHIFT)))
        assertEquals(PressVerdict.Unsendable, pressVerdict(ch("1", KeyMod.CTRL)))
        assertEquals(PressVerdict.Unsendable, pressVerdict(ch("1", KeyMod.CTRL, KeyMod.SHIFT)))
        assertEquals(PressVerdict.Unsendable, pressVerdict(sp(SpecialKey.ESC, KeyMod.CTRL)))
        assertEquals(PressVerdict.Unsendable, pressVerdict(sp(SpecialKey.ENTER, KeyMod.SHIFT)))
        assertEquals(PressVerdict.Unsendable, pressVerdict(ch("中", KeyMod.ALT)))
        assertEquals(PressVerdict.SameAs(ch("a", KeyMod.CTRL)), pressVerdict(ch("a", KeyMod.CTRL, KeyMod.SHIFT)))
        assertEquals(PressVerdict.SameAs(ch("1", KeyMod.ALT)), pressVerdict(ch("1", KeyMod.CTRL, KeyMod.ALT)))
        assertEquals(PressVerdict.SameAs(sp(SpecialKey.TAB, KeyMod.SHIFT)), pressVerdict(sp(SpecialKey.TAB, KeyMod.SHIFT, KeyMod.CTRL)))
        assertEquals(PressVerdict.SameAs(sp(SpecialKey.ESC, KeyMod.ALT)), pressVerdict(sp(SpecialKey.ESC, KeyMod.ALT, KeyMod.CTRL)))
    }

    @Test
    fun nextGroupGoesBySortOrderAndWraps() {
        val gs = listOf(KeyGroup("c", "vim", 2), KeyGroup("a", "常用", 0), KeyGroup("b", "shell", 1))
        assertEquals("b", nextGroupId(gs, "a"))
        assertEquals("c", nextGroupId(gs, "b"))
        assertEquals("a", nextGroupId(gs, "c"))
        assertEquals("没记过 · 那组已删 ⇒ 当前是第一组，下一组是第二组", "b", nextGroupId(gs, "gone"))
        assertEquals("b", nextGroupId(gs, null))
        assertNull(nextGroupId(emptyList(), "a"))
        assertEquals("a", nextGroupId(listOf(KeyGroup("a", "常用", 0)), "a"))
    }

    @Test
    fun currentGroupIsTheSavedOneElseTheFirstBySortOrder() {
        val gs = listOf(KeyGroup("c", "vim", 2), KeyGroup("a", "常用", 0))
        assertEquals("c", currentGroup(gs, "c")?.id)
        assertEquals("a", currentGroup(gs, "gone")?.id)
        assertEquals("a", currentGroup(gs, null)?.id)
        assertNull(currentGroup(emptyList(), "a"))
    }
}
