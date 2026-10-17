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

/** 缓存那几张表的规矩：记录缓存只留最近看过的 50 条 · 列表不淘汰 · 草稿变待发同一个事务。 */
@RunWith(AndroidJUnit4::class)
class CacheDaoTest {
    private lateinit var db: AppDatabase
    private lateinit var dao: CacheDao

    @Before
    fun setup() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        db = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java).allowMainThreadQueries().build()
        dao = db.cacheDao()
    }

    @After
    fun teardown() = db.close()

    private fun page(
        h: String,
        sid: String,
    ) = RecordPage(h, sid, 0, 10, "[]", 1)

    /** 看过第 51 条 ⇒ 最久没看的那一条的记录页 · 续点没了；列表那一行还在（列表不淘汰）；别的 50 条一页不少。 */
    @Test
    fun recordPagesKeepTheFiftyMostRecentlyViewedAndTheListKeepsEveryRow() =
        runTest {
            dao.replaceList("h", (0..50).map { "s$it" to "{\"sessionId\":\"s$it\"}" })
            for (i in 0..50) {
                dao.putPages("h", "s$i", listOf(page("h", "s$i")), SyncAnchor("h", "s$i", "/p", 10, 1, "f"))
                dao.viewed("h", "s$i", at = 100L + i)
            }
            assertTrue("最久没看的那条（s0）的页该淘汰", dao.pages("h", "s0").isEmpty())
            assertNull("它的续点跟着淘汰", dao.anchor("h", "s0"))
            for (i in 1..50) assertEquals("s$i 的页该留着", 1, dao.pages("h", "s$i").size)
            assertEquals("列表本身不淘汰", 51, dao.listed("h").size)
            // 再看一次 s1（变成最近的）⇒ 下一条被挤掉的是 s2，不是 s1。
            dao.viewed("h", "s1", at = 1_000)
            dao.putPages("h", "s51", listOf(page("h", "s51")), null)
            dao.viewed("h", "s51", at = 1_001)
            assertEquals(1, dao.pages("h", "s1").size)
            assertTrue(dao.pages("h", "s2").isEmpty())
        }

    /** 新清单整份换上：顺序照它；点开过、不在新清单里的那条留着行（离线不画），没点开过的删掉。 */
    @Test
    fun aNewListReplacesTheOrderAndKeepsViewedRowsOnly() =
        runTest {
            dao.replaceList("h", listOf("a" to "A", "b" to "B", "c" to "C"))
            dao.viewed("h", "b", at = 5)
            dao.replaceList("h", listOf("c" to "C2", "d" to "D"))
            assertEquals(listOf("c" to "C2", "d" to "D"), dao.listed("h").map { it.sid to it.listRow })
            val all = dao.allSessions("h").associateBy { it.sid }
            assertEquals(setOf("b", "c", "d"), all.keys)
            assertEquals(5L, all.getValue("b").viewedAt)
            assertNull(all.getValue("b").ord)
        }

    /** 记号 · 头与行同一次写下、同一次读回；再写一回整份换上（记号跟着换，点开过的那条照留）。 */
    @Test
    fun theListMarkAndItsRowsAreKeptTogether() =
        runTest {
            assertEquals(null to emptyList<CachedSession>(), dao.heldList("h"))
            dao.keepList("h", listOf("a" to "A", "b" to "B"), ListHold("h", "e.1.1", "{\"total\":2}"))
            dao.viewed("h", "a", at = 3)
            val (hold, rows) = dao.heldList("h")
            assertEquals(ListHold("h", "e.1.1", "{\"total\":2}"), hold)
            assertEquals(listOf("a" to "A", "b" to "B"), rows.map { it.sid to it.listRow })
            dao.keepList("h", listOf("b" to "B2"), ListHold("h", "e.1.2", null))
            assertEquals("e.1.2", dao.listHold("h")?.held)
            assertEquals(listOf("b" to "B2"), dao.listed("h").map { it.sid to it.listRow })
            assertEquals(3L, dao.allSessions("h").first { it.sid == "a" }.viewedAt)
        }

    /** 草稿变待发是一个事务：待发落不下（同一个 id 已在）⇒ 草稿还在。落下了 ⇒ 草稿没了、待发在。 */
    @Test
    fun promotingADraftIsOneTransaction() =
        runTest {
            val p = PendingSend("p1", "h", "s", "你好", null, 1, PendingOutcome.PENDING, 1)
            dao.putDraft(Draft("h", "s", "你好", null, null, null, null, 1))
            dao.promote("h", "s", p)
            assertNull(dao.draft("h", "s"))
            assertEquals(listOf(p), dao.pending("h", "s"))

            dao.putDraft(Draft("h", "s", "再来", null, null, null, null, 2))
            val failed = runCatching { dao.promote("h", "s", p) }
            assertTrue("同一个 id 再落一次该失败", failed.isFailure)
            assertNotNull("落不下 ⇒ 删草稿那一步跟着撤回", dao.draft("h", "s"))
        }
}
