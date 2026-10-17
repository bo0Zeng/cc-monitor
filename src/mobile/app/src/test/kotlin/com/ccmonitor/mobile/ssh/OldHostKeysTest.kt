package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.KnownHost
import com.ccmonitor.mobile.core.data.db.KnownHostDao
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** 旧表迁进核心那份库：全部行平铺交核心；核心说对上才清空旧表，对不上一行不动、留那一句；那一句的出路核心清成了才清旧表。 */
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
            val old =
                OldHostKeys(t, inCore = {
                    handed = it.toList()
                    null
                })
            old.migrate()
            assertEquals(listOf("box", "22", "ssh-ed25519", "AAAA", "other", "2200", "ecdsa-sha2-nistp256", "BBBB"), handed)
            assertEquals(1, t.cleared)
            assertEquals(null, old.unmigrated.value)
        }

    @Test
    fun `核心说对不上：旧表一行不动，留着那一句`() =
        runBlocking {
            val t = Table(two)
            val old = OldHostKeys(t, inCore = { "said" })
            old.migrate()
            assertEquals(0, t.cleared)
            assertEquals(two, t.rows)
            assertEquals("said", old.unmigrated.value)
        }

    @Test
    fun `旧表空着（迁过了 · 新装）：不问核心`() =
        runBlocking {
            val t = Table(emptyList())
            var asked = false
            OldHostKeys(t, inCore = {
                asked = true
                null
            }).migrate()
            assertEquals(false, asked)
            assertEquals(0, t.cleared)
        }

    @Test
    fun `那一句的出路：核心清成了才清旧表、那一句收起`() =
        runBlocking {
            val t = Table(two)
            val old = OldHostKeys(t, inCore = { "said" }, forgetInCore = { true })
            old.migrate()
            assertEquals(true, old.forgetAll())
            assertEquals(1, t.cleared)
            assertEquals(null, old.unmigrated.value)
        }

    @Test
    fun `核心没清成：旧表不动，那一句还在`() =
        runBlocking {
            val t = Table(two)
            val old = OldHostKeys(t, inCore = { "said" }, forgetInCore = { false })
            old.migrate()
            assertEquals(false, old.forgetAll())
            assertEquals(0, t.cleared)
            assertEquals("said", old.unmigrated.value)
        }
}
