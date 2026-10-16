package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.KnownHost
import com.ccmonitor.mobile.core.data.db.KnownHostDao
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** 旧表迁进核心那份库：全部行平铺交核心；核心说对上才清空旧表，对不上一行不动。 */
class OldHostKeysTest {
    private class Table(
        rows: List<KnownHost>,
    ) : KnownHostDao {
        var rows = rows
        var cleared = 0

        override suspend fun all() = rows

        override suspend fun clear() {
            cleared++
            rows = emptyList()
        }
    }

    private val two =
        listOf(
            KnownHost("box", 22, "ssh-ed25519", "AAAA", 1),
            KnownHost("other", 2200, "ecdsa-sha2-nistp256", "BBBB", 2),
        )

    @Test
    fun `每一行四格平铺交核心，核心说对上才清空`() =
        runBlocking {
            val t = Table(two)
            var handed: List<String>? = null
            migrateOldHostKeys(t) {
                handed = it.toList()
                null
            }
            assertEquals(listOf("box", "22", "ssh-ed25519", "AAAA", "other", "2200", "ecdsa-sha2-nistp256", "BBBB"), handed)
            assertEquals(1, t.cleared)
        }

    @Test
    fun `核心说对不上：旧表一行不动`() =
        runBlocking {
            val t = Table(two)
            migrateOldHostKeys(t) { "said" }
            assertEquals(0, t.cleared)
            assertEquals(two, t.rows)
        }

    @Test
    fun `旧表空着（迁过了 · 新装）：不问核心`() =
        runBlocking {
            val t = Table(emptyList())
            var asked = false
            migrateOldHostKeys(t) {
                asked = true
                null
            }
            assertEquals(false, asked)
            assertEquals(0, t.cleared)
        }
}
