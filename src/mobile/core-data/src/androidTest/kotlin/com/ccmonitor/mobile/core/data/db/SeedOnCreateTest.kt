package com.ccmonitor.mobile.core.data.db

import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 新装（onCreate）种子路径：只写迁移的话新装一个默认键都没有。
 * `MigrationTestHelper` 不触发 onCreate，所以这里直接建库，用与 [dataModule] 同款的 onCreate 回调
 * （都调 [seedDefaultButtons]），断言种子写入、分组正确。需设备或模拟器。
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
                        override fun onCreate(db: SupportSQLiteDatabase) = seedDefaultButtons(db)
                    },
                ).build()
    }

    @After
    fun tearDown() = db.close()

    @Test
    fun onCreateSeedsAllDefaultButtons() {
        // 首次 query 触发 onCreate（建库）→ 种子写入。直接 SQL 计数（不引 coroutines-test 收 Flow）。
        db.query("SELECT COUNT(*) FROM custom_buttons WHERE id LIKE 'ak-%'", emptyArray()).use { c ->
            c.moveToFirst()
            assertEquals("新装 onCreate 应写入 40 条种子", 40, c.getInt(0))
        }
        db.query("SELECT COUNT(*) FROM custom_buttons WHERE btnGroup='fn'", emptyArray()).use { c ->
            c.moveToFirst()
            assertEquals("fn 组 9（⇌+Ctrl+Tab+Esc+↑↓←→+粘贴）", 9, c.getInt(0))
        }
        db.query("SELECT btnGroup, type FROM custom_buttons WHERE id='ak-fn-left'", emptyArray()).use { c ->
            assertTrue(c.moveToFirst())
            assertEquals("fn", c.getString(0))
            assertEquals("escape", c.getString(1))
        }
    }
}
