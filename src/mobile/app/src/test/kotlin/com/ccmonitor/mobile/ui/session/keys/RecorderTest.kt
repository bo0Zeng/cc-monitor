package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 录入抽屉：修饰点亮带给下一下 · 特殊键录一颗 · 字照原样 · 退格删最后一下 · 送不出 / 送出相同当场说一句。 */
class RecorderTest {
    @Test
    fun tmuxPrefixThenC() {
        val r = Recorder().toggle(KeyMod.CTRL).type("b").type("c")
        assertEquals(listOf(KeyPress.Chord("b", setOf(KeyMod.CTRL)), KeyPress.Text("c")), r.presses)
        assertEquals("修饰只带给下一下", emptySet<KeyMod>(), r.armed)
    }

    @Test
    fun typedTextMergesAndNewlineIsEnter() {
        val r = Recorder().type("git st").type("atus\n")
        assertEquals(listOf(KeyPress.Text("git status"), KeyPress.Special(SpecialKey.ENTER)), r.presses)
    }

    @Test
    fun specialKeyTakesArmedMods() {
        val r = Recorder().toggle(KeyMod.SHIFT).special(SpecialKey.TAB).special(SpecialKey.ESC)
        assertEquals(listOf(KeyPress.Special(SpecialKey.TAB, setOf(KeyMod.SHIFT)), KeyPress.Special(SpecialKey.ESC)), r.presses)
    }

    @Test
    fun softBackspaceRemovesTheLastPressOrLastChar() {
        val r = Recorder().type("ls").special(SpecialKey.ENTER)
        assertEquals(listOf(KeyPress.Text("ls")), r.undo().presses)
        assertEquals(listOf(KeyPress.Text("l")), r.undo().undo().presses)
        assertEquals(
            emptyList<KeyPress>(),
            r
                .undo()
                .undo()
                .undo()
                .presses,
        )
        assertEquals(emptyList<KeyPress>(), Recorder().undo().presses)
        assertEquals(Recorder(), r.toggle(KeyMod.ALT).clear())
    }

    @Test
    fun ctrlDigitCannotBeSentAndIsNotRecorded() {
        val r = Recorder().type("a").toggle(KeyMod.CTRL).type("1")
        assertEquals(listOf(KeyPress.Text("a")), r.presses)
        assertEquals(copyText("mobKeys.rec.cannot", "a" to "Ctrl-1"), r.note?.text)
        assertEquals(emptySet<KeyMod>(), r.armed)
    }

    @Test
    fun ctrlShiftASendsTheSameAsCtrlAAndIsRecordedWithANote() {
        val r = Recorder().toggle(KeyMod.CTRL).toggle(KeyMod.SHIFT).type("a")
        assertEquals(listOf(KeyPress.Chord("a", setOf(KeyMod.CTRL, KeyMod.SHIFT))), r.presses)
        assertEquals(copyText("mobKeys.rec.same", "a" to "Ctrl-Shift-A", "b" to "Ctrl-A"), r.note?.text)
    }

    @Test
    fun theNoteGoesAwayOnTheNextPress() {
        val r = Recorder().toggle(KeyMod.CTRL).type("1").type("x")
        assertNull(r.note)
    }

    @Test
    fun theSpecialRowIsTheBuiltInListAndCoversEverySpecialKeyOnce() {
        assertEquals(SpecialKey.entries.toSet(), (RECORDER_SPECIALS + RECORDER_FKEYS).toSet())
        assertEquals(SpecialKey.entries.size, RECORDER_SPECIALS.size + RECORDER_FKEYS.size)
        assertEquals((1..12).map { "F$it" }, RECORDER_FKEYS.map { it.name })
    }
}
