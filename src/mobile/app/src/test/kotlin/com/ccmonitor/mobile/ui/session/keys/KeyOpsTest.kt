package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.defaultKeyGroups
import com.ccmonitor.mobile.core.data.db.defaultKeys
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 编辑页的读写：新建组排最后并切过去 · 删组连按键、撤销原样放回 · 删一颗撤销放回原位 · 存一颗 · 拖动重编先后 · 恢复默认。 */
class KeyOpsTest {
    private val esc = KeyBinding.Input(listOf(KeyPress.Special(SpecialKey.ESC)))
    private var n = 0
    private val dao = FakeKeyDao(defaultKeyGroups(), defaultKeys())
    private val settingsDao = FakeSettingsDao()
    private val settings = SettingsRepository(settingsDao)
    private val ops = KeyOps(dao, settings) { "id${n++}" }

    @Test
    fun aNewGroupGoesLastIsKeptEmptyAndBecomesCurrent() =
        runTest {
            val id = ops.createGroup("git")
            assertEquals(KeyGroup(id, "git", 3), dao.groups().last())
            assertEquals(emptyList<TermKey>(), dao.keys(id))
            assertEquals(id, settings.observeCurrentKeyGroup().first())
        }

    @Test
    fun deletingAGroupTakesItsKeysAndUndoPutsBothBackInPlace() =
        runTest {
            val before = dao.allKeys() to dao.groups()
            val vim = dao.groups().last()
            val gone = ops.deleteGroup(vim)
            assertEquals(11, gone.size)
            assertEquals(2, dao.groups().size)
            ops.undoDeleteGroup(vim, gone)
            assertEquals(before, dao.allKeys() to dao.groups())
        }

    @Test
    fun undoingAKeyDeletePutsItBackWhereItWas() =
        runTest {
            val before = dao.allKeys()
            val k = before[3]
            ops.deleteKey(k)
            ops.undoDeleteKey(k)
            assertEquals(before, dao.allKeys())
        }

    @Test
    fun aSavedNewKeyGoesLastInItsGroup() =
        runTest {
            val gid = defaultKeyGroups().first().id
            val k = ops.save(KeyDraft(id = null, groupId = gid, tap = Slot.of(esc)))
            assertEquals(dao.keys(gid).last(), k)
            assertEquals(9, k.sortOrder)
            val m = ops.addModifier(gid, KeyMod.ALT)
            assertEquals(TermKey("tk-id1", gid, 10, modifier = KeyMod.ALT), m)
        }

    @Test
    fun draggingRenumbersTheGroupsAndTheKeys() =
        runTest {
            ops.moveGroup(defaultKeyGroups().last().id, 0)
            assertEquals(listOf("kg-vim", "kg-common", "kg-shell"), dao.groups().map { it.id })
            assertEquals(listOf(0, 1, 2), dao.groups().map { it.sortOrder })
            val gid = "kg-common"
            val first = dao.keys(gid).first().id
            ops.moveKey(gid, 0, 2)
            assertEquals(first, dao.keys(gid)[2].id)
            assertEquals((0 until 9).toList(), dao.keys(gid).map { it.sortOrder })
        }

    @Test
    fun restoreDefaultsReallyGoesBack() =
        runTest {
            ops.createGroup("git")
            ops.deleteKey(dao.allKeys().first())
            ops.restoreDefaults()
            assertEquals(defaultKeyGroups(), dao.groups())
            assertEquals(defaultKeys().sortedWith(compareBy({ it.groupId }, { it.sortOrder })), dao.allKeys())
        }

    @Test
    fun totalDurationsGoToSettingsAndNullFollowsTheSystem() =
        runTest {
            ops.setDoubleTapMs(250)
            ops.setLongPressMs(600)
            assertEquals(250, settings.observeKeyDoubleTapMs().first())
            assertEquals(600, settings.observeKeyLongPressMs().first())
            ops.setDoubleTapMs(null)
            assertNull(settings.observeKeyDoubleTapMs().first())
        }
}
