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
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 新装（onCreate）种子路径：只写迁移的话新装一颗按键都没有。
 * `MigrationTestHelper` 不触发 onCreate，所以这里直接建库，用与 [dataModule] 同款的 onCreate 回调（都调 [seedDefaultKeys]），
 * 经 DAO 读回来就是默认三组原样（各格经类型转换写进去、读得回来）。需设备或模拟器。
 */
@RunWith(AndroidJUnit4::class)
class SeedOnCreateTest {
    private lateinit var db: AppDatabase

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
    }

    @After
    fun tearDown() = db.close()

    @Test
    fun onCreateSeedsTheThreeDefaultGroupsExactly() =
        runBlocking {
            val dao = db.keyDao()
            assertEquals(defaultKeyGroups(), dao.observeGroups().first())
            assertEquals(defaultKeys().sortedWith(compareBy({ it.groupId }, { it.sortOrder }, { it.id })), dao.allKeys())
            assertEquals(listOf("⇌", "Esc", "Ctrl-C", "↑", "↓", "⏎", "Tab", "Shift-Tab", "粘贴"), dao.keys(DEFAULT_GROUP_COMMON).map { it.cap() })
        }
}
