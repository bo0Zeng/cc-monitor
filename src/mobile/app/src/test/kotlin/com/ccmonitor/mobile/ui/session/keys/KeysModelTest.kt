package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.UnconvertedNote
import com.ccmonitor.mobile.core.data.db.defaultKeyGroups
import com.ccmonitor.mobile.core.data.db.defaultKeys
import com.ccmonitor.mobile.core.data.db.resetPreview
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 编辑一颗 · 编辑页每一行 · 组名 · 先后 · 恢复默认那两行的判定。 */
class KeysModelTest {
    private val esc = KeyBinding.Input(listOf(KeyPress.Special(SpecialKey.ESC)))
    private val up = KeyBinding.Input(listOf(KeyPress.Special(SpecialKey.UP)))
    private val groups = listOf(KeyGroup("a", "main", 0), KeyGroup("b", "vim", 1))

    @Test
    fun tapIsRequiredAndAnEmptyInputCountsAsUnset() {
        val d = KeyDraft(id = null, groupId = "a")
        assertFalse(d.canSave)
        assertFalse("选了送输入还没录 ＝ 没设", d.withSlot(Trigger.TAP, Slot(BindKind.INPUT)).canSave)
        assertTrue(d.withSlot(Trigger.TAP, Slot.of(esc)).canSave)
        assertTrue("修饰键不绑触发", KeyDraft(id = null, groupId = "a", modifier = KeyMod.CTRL).canSave)
    }

    @Test
    fun outOfRangeDurationsBlockSavingInsteadOfBeingClamped() {
        val d = KeyDraft(id = null, groupId = "a", tap = Slot.of(esc))
        assertFalse(d.copy(doubleTapMs = 2000).canSave)
        assertFalse(d.copy(doubleTapMs = 99).canSave)
        assertTrue(d.copy(doubleTapMs = 1000).canSave)
        assertFalse(d.copy(longPressMs = 199).canSave)
        assertTrue(d.copy(longPressMs = 2000).canSave)
    }

    @Test
    fun switchingSegmentsKeepsWhatWasRecorded() {
        val s = Slot.of(esc).copy(kind = BindKind.ACTION)
        assertNull(s.binding)
        assertEquals(esc, s.copy(kind = BindKind.INPUT).binding)
        assertNull(s.copy(kind = BindKind.NONE).binding)
    }

    @Test
    fun savingClearsTheUnconvertedMarkAndKeepsPlaceAndDefaultSource() {
        val old =
            TermKey(
                "k",
                "a",
                3,
                defaultId = "dk-x",
                icon = "i",
                unconverted = listOf(UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, "\\e[25~")),
            )
        val saved = KeyDraft.of(old).withSlot(Trigger.TAP, Slot.of(esc)).toKey(old, endOf = { 99 }, newId = { "n" })
        assertEquals(TermKey("k", "a", 3, icon = "i", tap = esc, defaultId = "dk-x"), saved)
        val moved = KeyDraft.of(saved).copy(groupId = "b").toKey(saved, endOf = { 7 }, newId = { "n" })
        assertEquals("换组排到新组最后", 7, moved.sortOrder)
        val fresh = KeyDraft(id = null, groupId = "b", label = "  ", tap = Slot.of(esc)).toKey(null, endOf = { 4 }, newId = { "n" })
        assertEquals(TermKey("n", "b", 4, tap = esc), fresh)
    }

    @Test
    fun naturalHintFollowsWhatTheTapIs() {
        assertEquals(copyText("mobKeys.natural.longRepeat"), naturalHint(Trigger.LONG, up))
        assertEquals(copyText("mobKeys.natural.longOnce"), naturalHint(Trigger.LONG, esc))
        assertEquals(copyText("mobKeys.natural.longEither"), naturalHint(Trigger.LONG, null))
        assertEquals(copyText("mobKeys.natural.double"), naturalHint(Trigger.DOUBLE, esc))
        assertEquals(copyText("mobKeys.natural.tap"), naturalHint(Trigger.TAP, null))
    }

    @Test
    fun rowLinesNameTheTapAndOnlyTheTriggersThatAreSet() {
        val name = { id: String -> groups.firstOrNull { it.id == id }?.name }
        val sw = defaultKeys().first()
        assertEquals(copyText("mobKeys.action.nextGroup"), firstLine(sw, name))
        assertEquals(copyText("mobKeys.trigger.long") + " " + copyText("mobKeys.action.editKeys"), secondLine(sw, name))
        val k = TermKey("k", "a", 0, tap = esc)
        assertEquals("Esc", firstLine(k, name))
        assertNull(secondLine(k, name))
        val both = k.copy(doubleTap = KeyBinding.App(AppAction.GOTO_GROUP, "b"), longPress = up)
        assertEquals(copyText("mobKeys.trigger.double") + " vim · " + copyText("mobKeys.trigger.long") + " ↑", secondLine(both, name))
        assertEquals(copyText("mobKeys.mod.title"), firstLine(TermKey("m", "a", 0, modifier = KeyMod.CTRL), name))
    }

    @Test
    fun aGotoWhoseGroupIsGoneIsFlagged() {
        val d = KeyDraft(id = "k", groupId = "a", tap = Slot.of(KeyBinding.App(AppAction.GOTO_GROUP, "zz")), longPress = Slot.of(KeyBinding.App(AppAction.GOTO_GROUP, "b")))
        assertEquals(setOf(Trigger.TAP), goneTargets(d, groups))
    }

    @Test
    fun groupNamesAreTrimmedAndMustBeUnique() {
        assertEquals(NameCheck.Ok("git"), checkGroupName("  git ", groups))
        assertEquals(NameCheck.Empty, checkGroupName("   ", groups))
        assertEquals(NameCheck.Dup(copyText("mobKeys.group.nameDup", "name" to "vim")), checkGroupName("vim", groups))
        assertEquals("改名成自己原来的名字不算重", NameCheck.Ok("vim"), checkGroupName("vim", groups, self = "b"))
    }

    @Test
    fun movingRenumbersTheOneOrder() {
        val three = groups + KeyGroup("c", "git", 5)
        assertEquals(listOf("c", "a", "b"), three.moved(2, 0).renumbered().map { it.id })
        assertEquals(listOf(0, 1, 2), three.moved(2, 0).renumbered().map { it.sortOrder })
        assertEquals("越界原样", three, three.moved(0, -1))
    }

    @Test
    fun resetLinesListWhatGoesBack() {
        val gs = defaultKeyGroups() + KeyGroup("g", "git", 3)
        val ks = defaultKeys().map { if (it.id == "dk-common-esc") it.copy(tap = up) else it } + TermKey("x", "g", 0, tap = esc)
        val lines = resetLines(resetPreview(gs, ks))
        assertEquals(copyText("mobKeys.reset.back", "names" to defaultKeyGroups().first().name), lines.changed)
        assertEquals(copyText("mobKeys.reset.groups", "names" to "「git」") + " · " + copyText("mobKeys.reset.keys", "n" to 1), lines.deleted)
        assertEquals(ResetLines(null, null), resetLines(resetPreview(defaultKeyGroups(), defaultKeys())))
    }

    @Test
    fun durationLineSaysWhereTheNumberComesFrom() {
        assertEquals("300 ms · " + copyText("mobKeys.dur.followSys"), durationLine(null, 300, single = false))
        assertEquals("300 ms · " + copyText("mobKeys.dur.followAll"), durationLine(null, 300, single = true))
        assertEquals("220 ms · " + copyText("mobKeys.dur.custom"), durationLine(220, 220, single = true))
    }

    @Test
    fun everyUnconvertedIssueHasItsOwnLine() {
        val lines = UnconvertedNote.Issue.entries.map { issueLine(UnconvertedNote(it, "")) }
        assertEquals(lines.size, lines.toSet().size)
    }
}
