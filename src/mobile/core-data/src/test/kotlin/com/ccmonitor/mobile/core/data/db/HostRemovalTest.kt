package com.ccmonitor.mobile.core.data.db

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 删一台的那个事务（[HostRemovalDao.remove] · [HostRemovalDao.restore]）跑在假的那几张表上：每个抽象查询对一张表，
 * 漏删一张、漏放回一样都红。SQL 写得对不对看设备上的 `HostRemovalDaoTest`。缓存住另一个库，删一台时另清（`CacheDao.forgetHost`）。
 */
class HostRemovalTest {
    /** 内存里状态库那四张表，一张一个可变表；行都带 hostId（机器那一张是 id）。 */
    private class FakeTables : HostRemovalDao() {
        val hosts = mutableListOf<Host>()
        val pendingRows = mutableListOf<PendingSend>()
        val draftRows = mutableListOf<Draft>()
        val batchRows = mutableListOf<PendingBatch>()

        override suspend fun host(id: String) = hosts.firstOrNull { it.id == id }

        override suspend fun drafts(id: String) = draftRows.filter { it.hostId == id }

        override suspend fun pending(id: String) = pendingRows.filter { it.hostId == id }

        override suspend fun batches(id: String) = batchRows.filter { it.hostId == id }

        override suspend fun dropHost(id: String) {
            hosts.removeAll { it.id == id }
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

        /** 这一台在四张表里各有几行。 */
        fun count(id: String): Map<String, Int> =
            mapOf(
                "hosts" to hosts.count { it.id == id },
                "pending" to pendingRows.count { it.hostId == id },
                "drafts" to draftRows.count { it.hostId == id },
                "pending_batches" to batchRows.count { it.hostId == id },
            )

        /** 给 [id] 那一台每张表写一行。 */
        fun fill(id: String) {
            hosts += Host(id = id, label = id, host = "192.0.2.1", username = "u")
            pendingRows += PendingSend("p-$id", id, "s", "hi", null, 1, PendingOutcome.UNSURE, 1)
            draftRows += Draft(id, "s", "draft", null, null, null, null, 1)
            batchRows += PendingBatch(id, "t", "{}")
        }
    }

    private fun ones() = listOf("hosts", "pending", "drafts", "pending_batches").associateWith { 1 }

    /** 删 h1 ⇒ 它在四张表里一行不剩；h2 一行不少。 */
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

    /** 回来的那一份带着这一端自己的那几样；放回之后它们与机器那一行都回来。 */
    @Test
    fun undoPutsBackTheHostAndItsOwnRows() =
        runBlocking {
            val t = FakeTables().apply { fill("h1") }
            val kept = t.remove("h1")!!
            assertEquals(1, kept.drafts.size)
            assertEquals(1, kept.pending.size)
            assertEquals(1, kept.batches.size)
            t.restore(kept)
            assertEquals(ones(), t.count("h1"))
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
