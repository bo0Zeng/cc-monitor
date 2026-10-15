package com.ccmonitor.mobile.core.data.db

import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 真 SQLite 上的「恢复默认」「还原一颗」「删组 · 撤销」：恢复默认真的回到默认（自建组与改过的都没了，不是只补缺的）；
 * 还原一颗只动那一颗；删一组连组里的按键一起删、撤销照原样放回。需设备或模拟器。
 */
@RunWith(AndroidJUnit4::class)
class RestoreDefaultKeysTest {
    private lateinit var db: AppDatabase
    private lateinit var dao: KeyDao

    @Before
    fun setUp() {
        db =
            Room
                .inMemoryDatabaseBuilder(ApplicationProvider.getApplicationContext(), AppDatabase::class.java)
                .addCallback(
                    object : RoomDatabase.Callback() {
                        override fun onCreate(db: SupportSQLiteDatabase) = seedDefaultKeys(db)
                    },
                ).build()
        dao = db.keyDao()
    }

    @After
    fun tearDown() = db.close()

    private fun sorted(keys: List<TermKey>) = keys.sortedWith(compareBy({ it.groupId }, { it.sortOrder }, { it.id }))

    @Test
    fun restoreDefaultsReallyGoesBackRevertOneTouchesOnlyThatOneAndGroupDeleteUndoes() =
        runBlocking {
            val esc = dao.key("dk-vim-esc")!!
            val edited = esc.copy(label = "E", doubleTap = KeyBinding.App(AppAction.PASTE), longPressMs = 900)
            dao.upsertKey(edited)
            dao.upsertKey(dao.key("dk-common-esc")!!.copy(label = "ESC"))
            dao.upsertGroup(KeyGroup("kg-git", "git", 3))
            dao.upsertKey(TermKey("u1", "kg-git", 0, tap = KeyBinding.Input(listOf(KeyPress.Text("git status"), KeyPress.Special(SpecialKey.ENTER)))))
            dao.deleteKey(dao.key("dk-shell-pgup")!!)

            // 还原一颗：只回那一颗。
            val back = dao.revertToDefault(dao.key("dk-vim-esc")!!)
            assertEquals(esc, back)
            assertEquals(esc, dao.key("dk-vim-esc"))
            assertFalse(dao.key("dk-vim-esc")!!.isModifiedDefault())
            assertEquals("别的改动不动", "ESC", dao.key("dk-common-esc")!!.label)

            // 删组 · 撤销。
            val git = dao.groups().first { it.id == "kg-git" }
            val gone = dao.deleteGroup(git)
            assertEquals(listOf("u1"), gone.map { it.id })
            assertNull(dao.key("u1"))
            assertTrue(dao.groups().none { it.id == "kg-git" })
            dao.putBack(git, gone)
            assertEquals(gone, dao.keys("kg-git"))

            val preview = resetPreview(dao.groups(), dao.allKeys())
            assertEquals(listOf("常用", "shell"), preview.changedDefaultGroups)
            assertEquals(listOf("git"), preview.deletedGroups.map { it.name })

            // 恢复默认：回到默认原样。
            dao.restoreDefaults()
            assertEquals(defaultKeyGroups(), dao.observeGroups().first())
            assertEquals(sorted(defaultKeys()), dao.allKeys())
            assertTrue(resetPreview(dao.groups(), dao.allKeys()).pristine)
        }

    @Test
    fun unconvertedKeysAreListedUntilTheyAreFixed() =
        runBlocking {
            val note = listOf(UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, "\\e[?1049h"))
            dao.upsertKey(TermKey("odd", DEFAULT_GROUP_COMMON, 50, label = "alt screen", unconverted = note))
            assertEquals(listOf("odd"), dao.observeUnconverted().first().map { it.id })
            assertEquals(note, dao.key("odd")!!.unconverted)
            dao.upsertKey(dao.key("odd")!!.copy(tap = KeyBinding.App(AppAction.CAPTURE), unconverted = null))
            assertEquals(emptyList<TermKey>(), dao.observeUnconverted().first())
        }
}
