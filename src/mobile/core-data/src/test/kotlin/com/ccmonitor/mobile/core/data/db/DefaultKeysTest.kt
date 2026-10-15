package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 默认三组的样子（照键帽钉住）、还原一颗、恢复默认确认框那几样数。真库上的整批替换在设备上的 RestoreDefaultKeysTest。
 */
class DefaultKeysTest {
    private fun capsOf(group: String) =
        defaultKeys().filter { it.groupId == group }.sortedBy { it.sortOrder }.map { it.cap() }

    @Test
    fun theThreeDefaultGroupsAreCommonShellVimInThatOrder() {
        assertEquals(
            listOf(KeyGroup(DEFAULT_GROUP_COMMON, "常用", 0), KeyGroup(DEFAULT_GROUP_SHELL, "shell", 1), KeyGroup(DEFAULT_GROUP_VIM, "vim", 2)),
            defaultKeyGroups(),
        )
        assertEquals(listOf("⇌", "Esc", "Ctrl-C", "↑", "↓", "⏎", "Tab", "Shift-Tab", "粘贴"), capsOf(DEFAULT_GROUP_COMMON))
        assertEquals(
            listOf("⇌", "Ctrl", "Alt", "Tab", "←", "→", "Home", "End", "PgUp", "PgDn", "⌫", "Del"),
            capsOf(DEFAULT_GROUP_SHELL),
        )
        assertEquals(listOf("⇌", "Esc", "i", ":", "/", "dd", "u", "p", ":w⏎", ":wq⏎", ":q!⏎"), capsOf(DEFAULT_GROUP_VIM))
    }

    @Test
    fun everyDefaultKeyIsItsOwnDefaultWithUniqueIdsAndOnlySwitchKeysHaveALongPress() {
        val keys = defaultKeys()
        assertEquals(keys.size, keys.map { it.id }.toSet().size)
        assertTrue(keys.all { it.defaultId == it.id && it.label == null && it.unconverted == null && it.doubleTap == null })
        val switches = keys.filter { it.tap == KeyBinding.App(AppAction.NEXT_GROUP) }
        assertEquals("每组最左一颗 ⇌", defaultKeyGroups().map { it.id to 0 }, switches.map { it.groupId to it.sortOrder })
        assertTrue(switches.all { it.longPress == KeyBinding.App(AppAction.EDIT_KEYS) })
        assertTrue("别的按键长按都没设（自然行为）", keys.filter { it !in switches }.all { it.longPress == null })
        assertEquals(
            "修饰键不绑触发",
            listOf(KeyMod.CTRL to null, KeyMod.ALT to null),
            keys.filter { it.modifier != null }.map { it.modifier to it.tap },
        )
    }

    @Test
    fun revertingOneKeyRestoresItsContentButKeepsWhereItIs() {
        val esc = defaultKeys().first { it.id == "dk-vim-esc" }
        assertFalse(esc.isModifiedDefault())
        val edited =
            esc.copy(
                groupId = "kg-mine",
                sortOrder = 7,
                label = "E",
                doubleTap = KeyBinding.App(AppAction.PASTE),
                longPressMs = 900,
                unconverted = listOf(UnconvertedNote(UnconvertedNote.Issue.LONG_NO_REPEAT, "x")),
            )
        assertTrue(edited.isModifiedDefault())
        val back = edited.revertedToDefault()
        assertEquals(esc.copy(groupId = "kg-mine", sortOrder = 7), back)
        assertFalse(back.isModifiedDefault())
        val mine = TermKey("u1", DEFAULT_GROUP_VIM, 0, tap = KeyBinding.App(AppAction.PASTE))
        assertFalse("自建的不是默认按键，没有还原", mine.isModifiedDefault())
        assertEquals(mine, mine.revertedToDefault())
    }

    @Test
    fun resetPreviewListsWhatWouldChangeAndIsPristineOnTheDefaults() {
        assertTrue(resetPreview(defaultKeyGroups(), defaultKeys()).pristine)

        val groups = defaultKeyGroups().map { if (it.id == DEFAULT_GROUP_SHELL) it.copy(name = "sh") else it } + KeyGroup("kg-git", "git", 3)
        val keys =
            defaultKeys()
                .filterNot { it.id == "dk-vim-u" } // 删了一颗默认
                .map { if (it.id == "dk-common-esc") it.copy(label = "ESC") else it } + // 改了一颗默认
                TermKey("u1", DEFAULT_GROUP_COMMON, 99, tap = KeyBinding.App(AppAction.CAPTURE)) + // 往默认组里加了一颗
                TermKey("u2", "kg-git", 0, tap = KeyBinding.App(AppAction.PASTE))
        val p = resetPreview(groups, keys)
        assertEquals(listOf("常用", "shell", "vim"), p.changedDefaultGroups)
        assertEquals(listOf("git"), p.deletedGroups.map { it.name })
        assertEquals("改过的 1 颗 ＋ 加进默认组的 1 颗", 2, p.changedKeys)
        assertFalse(p.pristine)

        val reordered = defaultKeyGroups().reversed().mapIndexed { i, g -> g.copy(sortOrder = i) }
        assertEquals("组的先后变了也算改动", listOf("常用", "vim"), resetPreview(reordered, defaultKeys()).changedDefaultGroups)
    }

    @Test
    fun repeatableIsOneRepeatableSpecialKeyOrOneCharacter() {
        fun sp(
            k: SpecialKey,
            vararg m: KeyMod,
        ) = KeyBinding.Input(listOf(KeyPress.Special(k, m.toSet())))
        assertTrue(sp(SpecialKey.LEFT).isRepeatable())
        assertTrue(sp(SpecialKey.BACKSPACE).isRepeatable())
        assertTrue(KeyBinding.Input(listOf(KeyPress.Text("j"))).isRepeatable())
        assertFalse(sp(SpecialKey.ENTER).isRepeatable())
        assertFalse(sp(SpecialKey.LEFT, KeyMod.CTRL).isRepeatable())
        assertFalse(KeyBinding.Input(listOf(KeyPress.Text("dd"))).isRepeatable())
        assertFalse(KeyBinding.App(AppAction.PASTE).isRepeatable())
        assertFalse((null as KeyBinding?).isRepeatable())
    }
}
