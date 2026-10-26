package com.ccmonitor.mobile.core.data.db

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 删一台的那个事务（[HostRemovalDao.remove] · [HostRemovalDao.restore]）跑在假的那几张表上：每个抽象查询对一张表，
 * 漏删一张、漏放回一样都红。SQL 写得对不对看设备上的 `HostRemovalDaoTest`。
 */
class HostRemovalTest {
    /** 内存里的九张表，一张一个可变表；行都带 hostId（机器那一张是 id）。 */
    private class FakeTables : HostRemovalDao() {
        val hosts = mutableListOf<Host>()
        val sessions = mutableListOf<CachedSession>()
        val holds = mutableListOf<ListHold>()
        val pages = mutableListOf<RecordPage>()
        val anchors = mutableListOf<SyncAnchor>()
        val pendingRows = mutableListOf<PendingSend>()
        val draftRows = mutableListOf<Draft>()
        val batchRows = mutableListOf<PendingBatch>()
        val bookmarkRows = mutableListOf<Bookmark>()

        override suspend fun host(id: String) = hosts.firstOrNull { it.id == id }

        override suspend fun drafts(id: String) = draftRows.filter { it.hostId == id }

        override suspend fun pending(id: String) = pendingRows.filter { it.hostId == id }

        override suspend fun batches(id: String) = batchRows.filter { it.hostId == id }

        override suspend fun bookmarks(id: String) = bookmarkRows.filter { it.hostId == id }

        override suspend fun dropHost(id: String) {
            hosts.removeAll { it.id == id }
        }

        override suspend fun dropSessions(id: String) {
            sessions.removeAll { it.hostId == id }
        }

        override suspend fun dropListHold(id: String) {
            holds.removeAll { it.hostId == id }
        }

        override suspend fun dropPages(id: String) {
            pages.removeAll { it.hostId == id }
        }

        override suspend fun dropAnchors(id: String) {
            anchors.removeAll { it.hostId == id }
        }

        override suspend fun dropPending(id: String) {
            pendingRows.removeAll { it.hostId == id }
        }

        override suspend fun dropDrafts(id: String) {
            draftRows.removeAll { it.hostId == id }
        }

        override suspend fun dropBatches(id: String) {
            batchRows.removeAll { it.hostId == id }
        }

        override suspend fun dropBookmarks(id: String) {
            bookmarkRows.removeAll { it.hostId == id }
        }

        override suspend fun putHost(host: Host) {
            hosts.removeAll { it.id == host.id }
            hosts += host
        }

        override suspend fun putDrafts(rows: List<Draft>) {
            draftRows += rows
        }

        override suspend fun putPending(rows: List<PendingSend>) {
            pendingRows += rows
        }

        override suspend fun putBatches(rows: List<PendingBatch>) {
            batchRows += rows
        }

        override suspend fun putBookmarks(rows: List<Bookmark>) {
            bookmarkRows += rows
        }

        /** 这一台在九张表里各有几行。 */
        fun count(id: String): Map<String, Int> =
            mapOf(
                "hosts" to hosts.count { it.id == id },
                "sessions" to sessions.count { it.hostId == id },
                "list_holds" to holds.count { it.hostId == id },
                "record_pages" to pages.count { it.hostId == id },
                "sync_anchors" to anchors.count { it.hostId == id },
                "pending" to pendingRows.count { it.hostId == id },
                "drafts" to draftRows.count { it.hostId == id },
                "pending_batches" to batchRows.count { it.hostId == id },
                "bookmarks" to bookmarkRows.count { it.hostId == id },
            )

        /** 给 [id] 那一台每张表写一行。 */
        fun fill(id: String) {
            hosts += Host(id = id, label = id, host = "192.0.2.1", username = "u")
            sessions += CachedSession(id, "s", "{}", 0, 1)
            holds += ListHold(id, "e.1.1", null)
            pages += RecordPage(id, "s", 0, 10, "[]", 1)
            anchors += SyncAnchor(id, "s", "/p", 10, 1, "m")
            pendingRows += PendingSend("p-$id", id, "s", "hi", null, 1, PendingOutcome.UNSURE, 1)
            draftRows += Draft(id, "s", "draft", null, null, null, null, 1)
            batchRows += PendingBatch(id, "t", "{}")
            bookmarkRows += Bookmark("b-$id", id, "/srv", "srv")
        }
    }

    private fun ones() = listOf("hosts", "sessions", "list_holds", "record_pages", "sync_anchors", "pending", "drafts", "pending_batches", "bookmarks").associateWith { 1 }

    /** 删 h1 ⇒ 它在九张表里一行不剩；h2 一行不少。 */
    @Test
    fun removingAHostLeavesNoRowOfItInAnyTableAndTouchesNoOtherHost() =
        runBlocking {
            val t =
                FakeTables().apply {
                    fill("h1")
                    fill("h2")
                }
            assertNotNull(t.remove("h1"))
            assertEquals(ones().mapValues { 0 }, t.count("h1"))
            assertEquals(ones(), t.count("h2"))
        }

    /** 回来的那一份带着这一端自己的那几样；放回之后它们与机器那一行回来，那台的缓存不回来。 */
    @Test
    fun undoPutsBackTheHostAndItsOwnRowsButNotTheCache() =
        runBlocking {
            val t = FakeTables().apply { fill("h1") }
            val kept = t.remove("h1")!!
            assertEquals(1, kept.drafts.size)
            assertEquals(1, kept.pending.size)
            assertEquals(1, kept.batches.size)
            assertEquals(1, kept.bookmarks.size)
            t.restore(kept)
            assertEquals(
                ones() + mapOf("sessions" to 0, "list_holds" to 0, "record_pages" to 0, "sync_anchors" to 0),
                t.count("h1"),
            )
        }

    /** 删一台不在的 ⇒ `null`，别的不动。 */
    @Test
    fun removingAnUnknownHostChangesNothing() =
        runBlocking {
            val t = FakeTables().apply { fill("h2") }
            assertNull(t.remove("h1"))
            assertEquals(ones(), t.count("h2"))
        }
}
