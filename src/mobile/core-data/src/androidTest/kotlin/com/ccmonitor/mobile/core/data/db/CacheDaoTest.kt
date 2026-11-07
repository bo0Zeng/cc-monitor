package com.ccmonitor.mobile.core.data.db

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** 缓存库那几张表的规矩：记录缓存只留最近看过的 50 条 · 删一台清它的缓存；状态库那边草稿变待发同一个事务。 */
@RunWith(AndroidJUnit4::class)
class CacheDaoTest {
    private lateinit var cacheDb: CacheDatabase
    private lateinit var stateDb: AppDatabase
    private lateinit var dao: CacheDao
    private lateinit var hand: HandDao

    @Before
    fun setup() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        cacheDb = Room.inMemoryDatabaseBuilder(ctx, CacheDatabase::class.java).allowMainThreadQueries().build()
        stateDb = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java).allowMainThreadQueries().build()
        dao = cacheDb.cacheDao()
        hand = stateDb.handDao()
    }

    @After
    fun teardown() {
        cacheDb.close()
        stateDb.close()
    }

    /** 删一台：它在缓存库三张表里的行清光，别台不动。 */
    @Test
    fun forgettingAHostClearsOnlyItsCache() =
        runTest {
            for (h in listOf("h1", "h2")) {
                dao.viewed(h, "s", at = 1)
                dao.putPages(h, "s", listOf(RecordPage(h, "s", 0, 10, "[]")), SyncAnchor(h, "s", "/p", 10, 1, "m"))
            }
            dao.forgetHost("h1")
            assertTrue(dao.viewedSessions("h1").isEmpty())
            assertTrue(dao.pages("h1", "s").isEmpty())
            assertNull(dao.anchor("h1", "s"))
            assertEquals(1, dao.viewedSessions("h2").size)
            assertEquals(1, dao.pages("h2", "s").size)
        }

    private fun page(
        h: String,
        sid: String,
    ) = RecordPage(h, sid, 0, 10, "[]")

    /** 看过第 51 条 ⇒ 最久没看的那一条的记录页 · 续点没了；别的 50 条一页不少。 */
    @Test
    fun recordPagesKeepTheFiftyMostRecentlyViewed() =
        runTest {
            for (i in 0..50) {
                dao.putPages("h", "s$i", listOf(page("h", "s$i")), SyncAnchor("h", "s$i", "/p", 10, 1, "f"))
                dao.viewed("h", "s$i", at = 100L + i)
            }
            assertTrue("最久没看的那条（s0）的页该淘汰", dao.pages("h", "s0").isEmpty())
            assertNull("它的续点跟着淘汰", dao.anchor("h", "s0"))
            for (i in 1..50) assertEquals("s$i 的页该留着", 1, dao.pages("h", "s$i").size)
            // 再看一次 s1（变成最近的）⇒ 下一条被挤掉的是 s2，不是 s1。
            dao.viewed("h", "s1", at = 1_000)
            dao.putPages("h", "s51", listOf(page("h", "s51")), null)
            dao.viewed("h", "s51", at = 1_001)
            assertEquals(1, dao.pages("h", "s1").size)
            assertTrue(dao.pages("h", "s2").isEmpty())
        }

    /** 草稿变待发是一个事务：待发落不下（同一个 id 已在）⇒ 草稿还在。落下了 ⇒ 草稿没了、待发在。 */
    @Test
    fun promotingADraftIsOneTransaction() =
        runTest {
            val p = PendingSend("p1", "h", "s", "你好", 1, PendingOutcome.PENDING, 1)
            hand.putDraft(Draft("h", "s", "你好", 1))
            hand.promote("h", "s", p)
            assertNull(hand.draft("h", "s"))
            assertEquals(listOf(p), hand.pending("h", "s"))

            hand.putDraft(Draft("h", "s", "再来", 2))
            val failed = runCatching { hand.promote("h", "s", p) }
            assertTrue("同一个 id 再落一次该失败", failed.isFailure)
            assertNotNull("落不下 ⇒ 删草稿那一步跟着撤回", hand.draft("h", "s"))
        }
}
