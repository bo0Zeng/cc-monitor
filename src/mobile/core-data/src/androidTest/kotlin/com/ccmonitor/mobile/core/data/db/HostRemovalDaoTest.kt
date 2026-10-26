package com.ccmonitor.mobile.core.data.db

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** 删一台在真 Room 上：一个事务把它在九张表里的行删光，别台不动；撤销放回这一端自己的那几样。 */
@RunWith(AndroidJUnit4::class)
class HostRemovalDaoTest {
    private lateinit var db: AppDatabase

    @Before
    fun setup() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        db = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java).allowMainThreadQueries().build()
    }

    @After
    fun teardown() = db.close()

    private suspend fun fill(id: String) {
        db.hostDao().upsert(Host(id = id, label = id, host = "192.0.2.1", username = "u"))
        val cache = db.cacheDao()
        cache.keepList(id, listOf("s" to "{}"), ListHold(id, "e.1.1", null))
        cache.viewed(id, "s", at = 1)
        cache.putPages(id, "s", listOf(RecordPage(id, "s", 0, 10, "[]", 1)), SyncAnchor(id, "s", "/p", 10, 1, "m"))
        cache.insertPending(PendingSend("p-$id", id, "s", "hi", null, 1, PendingOutcome.UNSURE, 1))
        cache.putDraft(Draft(id, "s", "draft", null, null, null, null, 1))
        cache.putBatch(PendingBatch(id, "t", "{}"))
        db.bookmarkDao().upsert(Bookmark("b-$id", id, "/srv", "srv"))
    }

    /** 每张表里这一台几行（直接问库，不经 DAO）。 */
    private fun count(id: String): Map<String, Int> =
        (mapOf("hosts" to "id") + TABLES.associateWith { "hostId" }).mapValues { (table, col) ->
            db.query("SELECT COUNT(*) FROM $table WHERE $col = ?", arrayOf(id)).use { c ->
                c.moveToFirst()
                c.getInt(0)
            }
        }

    @Test
    fun removingAHostDeletesEveryRowOfItAndNoneOfTheOthers() =
        runTest {
            fill("h1")
            fill("h2")
            val ones = count("h2")
            assertEquals("夹具每张表一行", (listOf("hosts") + TABLES).associateWith { 1 }, ones)
            db.hostRemovalDao().remove("h1")
            assertEquals((listOf("hosts") + TABLES).associateWith { 0 }, count("h1"))
            assertEquals(ones, count("h2"))
            assertNull("删一台不在的 ⇒ null", db.hostRemovalDao().remove("h1"))
        }

    @Test
    fun undoPutsBackTheHostAndItsOwnRowsButNotTheCache() =
        runTest {
            fill("h1")
            val kept = db.hostRemovalDao().remove("h1")!!
            db.hostRemovalDao().restore(kept)
            val cache = listOf("sessions", "list_holds", "record_pages", "sync_anchors")
            assertEquals((listOf("hosts") + TABLES).associateWith { if (it in cache) 0 else 1 }, count("h1"))
            assertEquals("draft", db.cacheDao().draft("h1", "s")?.text)
        }

    private companion object {
        val TABLES = listOf("sessions", "list_holds", "record_pages", "sync_anchors", "pending", "drafts", "pending_batches", "bookmarks")
    }
}
