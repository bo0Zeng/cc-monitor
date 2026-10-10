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
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 恢复默认按键在真 SQLite 上的行为：删掉的补回、改过的同 id 行不被覆盖、自建按钮原封。
 * 需设备或模拟器（CI 的 API-26 job 跑）。
 */
@RunWith(AndroidJUnit4::class)
class RestoreDefaultButtonsTest {
    private lateinit var db: AppDatabase
    private lateinit var dao: CustomButtonDao

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
        dao = db.customButtonDao()
    }

    @After
    fun tearDown() = db.close()

    @Test
    fun restoreBringsBackDeletedKeepsEditedUntouchedLeavesUserButtons() =
        runBlocking {
            // 触发 onCreate 建库并写入种子。
            dao.restoreDefaultButtons() // 幂等：全命中已存在 id → 无变化（先验一次幂等）
            assertEquals("首轮幂等：仍 40 条种子", 40, dao.observeGlobal().first().count { it.id.startsWith("ak-") })

            // 用户操作：删一条种子、改一条同 id 种子的 label、建一条自建按钮。
            val esc = dao.observeGlobal().first().first { it.id == "ak-fn-left" }
            dao.delete(esc)
            dao.upsert(
                dao
                    .observeGlobal()
                    .first()
                    .first { it.id == "ak-vim-i" }
                    .copy(label = "我的-i"),
            )
            dao.upsert(
                CustomButton(id = "user-1", hostId = null, label = "部署", command = "make deploy", sortOrder = 99, type = "command"),
            )

            dao.restoreDefaultButtons()

            val rows = dao.observeGlobal().first()
            // ① 被删的 ak-fn-left 补回原样（label=←、group=fn、command=\e[D）。
            val left = rows.first { it.id == "ak-fn-left" }
            assertEquals("←", left.label)
            assertEquals("fn", left.btnGroup)
            assertEquals("\\e[D", left.command)
            // ② 用户改过的 ak-vim-i 不被覆盖（label 仍是用户的「我的-i」，非默认 i）。
            assertEquals("恢复不覆盖用户改动", "我的-i", rows.first { it.id == "ak-vim-i" }.label)
            // ③ 自建按钮原封。
            val user = rows.first { it.id == "user-1" }
            assertEquals("部署", user.label)
            assertEquals("make deploy", user.command)
            // ④ 无重复种子行。
            assertEquals("种子 id 唯一（无重复插入）", 40, rows.count { it.id.startsWith("ak-") })
            assertTrue("自建按钮仍在", rows.any { it.id == "user-1" })
        }
}
