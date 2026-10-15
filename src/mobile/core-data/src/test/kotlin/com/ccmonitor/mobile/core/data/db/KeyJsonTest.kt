package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 按键那几格小 JSON：每种形状写出去读得回来、写出来的字钉住；读不懂的整格当没设。 */
class KeyJsonTest {
    private val conv = KeyConverters()

    private fun roundTrip(b: KeyBinding) = assertEquals(b, conv.textToBinding(conv.bindingToText(b)))

    @Test
    fun everyShapeRoundTrips() {
        roundTrip(KeyBinding.Input(listOf(KeyPress.Text("ls -la"), KeyPress.Special(SpecialKey.ENTER))))
        roundTrip(KeyBinding.Input(listOf(KeyPress.Chord("b", setOf(KeyMod.CTRL)), KeyPress.Text("c"))))
        roundTrip(KeyBinding.Input(listOf(KeyPress.Special(SpecialKey.TAB, setOf(KeyMod.SHIFT)))))
        roundTrip(KeyBinding.Input(listOf(KeyPress.Chord("a", setOf(KeyMod.CTRL, KeyMod.SHIFT, KeyMod.ALT)))))
        SpecialKey.entries.forEach { roundTrip(KeyBinding.Input(listOf(KeyPress.Special(it)))) }
        AppAction.entries.forEach { roundTrip(KeyBinding.App(it)) }
        roundTrip(KeyBinding.App(AppAction.GOTO_GROUP, "kg-vim"))
        roundTrip(KeyBinding.Input(listOf(KeyPress.Text("引号\" 反斜杠\\ 换行\n"))))
    }

    @Test
    fun theStoredTextIsPinned() {
        assertEquals(
            """{"input":[{"text":"git status"},{"key":"enter"},{"char":"b","mods":["ctrl","alt"]},{"key":"tab","mods":["shift"]}]}""",
            conv.bindingToText(
                KeyBinding.Input(
                    listOf(
                        KeyPress.Text("git status"),
                        KeyPress.Special(SpecialKey.ENTER),
                        KeyPress.Chord("b", setOf(KeyMod.ALT, KeyMod.CTRL)),
                        KeyPress.Special(SpecialKey.TAB, setOf(KeyMod.SHIFT)),
                    ),
                ),
            ),
        )
        assertEquals("""{"app":"goto_group","group":"g1"}""", conv.bindingToText(KeyBinding.App(AppAction.GOTO_GROUP, "g1")))
        assertEquals("""{"app":"next_group"}""", conv.bindingToText(KeyBinding.App(AppAction.NEXT_GROUP)))
        assertEquals("ctrl", conv.modToText(KeyMod.CTRL))
        assertNull("没设 ＝ null，不写空 JSON", conv.bindingToText(null))
    }

    @Test
    fun unreadableCellsAreNotSet() {
        listOf(
            "",
            "{",
            "[]",
            """{"app":"make_coffee"}""",
            """{"input":[{"key":"hyper"}]}""",
            """{"input":[{"char":"ab","mods":["ctrl"]}]}""",
            """{"input":[{"char":"a"}]}""",
            """{"input":[{"text":"x","mods":["ctrl"]}]}""",
            """{"input":[{"key":"tab","mods":["meta"]}]}""",
            """{"input":[],"app":"paste"}""",
            """{"app":"paste"} trailing""",
        ).forEach { assertNull("「$it」应当当没设", conv.textToBinding(it)) }
        assertNull(conv.textToMod("meta"))
    }

    @Test
    fun notesRoundTripAndEmptyIsNull() {
        val notes =
            listOf(
                UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, "\\e[?1049h"),
                UnconvertedNote(UnconvertedNote.Issue.LONG_NO_REPEAT, "make"),
            )
        assertEquals(notes, conv.textToNotes(conv.notesToText(notes)))
        assertEquals("""[{"issue":"tap_unparsed","original":"x"}]""", conv.notesToText(notes.take(1).map { it.copy(original = "x") }))
        assertNull("空列表不落库", conv.notesToText(emptyList()))
        assertNull(conv.textToNotes("[]"))
    }
}
