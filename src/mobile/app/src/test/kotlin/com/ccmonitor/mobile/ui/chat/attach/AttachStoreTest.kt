package com.ccmonitor.mobile.ui.chat.attach

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.io.InputStream

/** 草稿上的附件住本机私有目录：被杀回来照样在（副本 · 先后 · 已落的编号），撤 · 发出去都删干净；待发那一句的编号另记。 */
class AttachStoreTest {
    @get:Rule val dir = TemporaryFolder()

    private fun bytes(n: Int) = ByteArray(n) { (it % 7).toByte() }.inputStream()

    @Test
    fun a_kept_copy_survives_a_new_store_on_the_same_folder() {
        val a = AttachStore(dir.root)
        val one = a.keep("s1", "shot.png", image = true) { bytes(5) }
        val two = a.keep("s1", "login.css", image = false) { bytes(9) }
        a.landed("s1", one.key, "1027-1200-ab12")

        val again = AttachStore(dir.root).list("s1")
        assertEquals(listOf("shot.png" to "1027-1200-ab12", "login.css" to null), again.map { it.name to it.id })
        assertEquals(listOf(5L, 9L), again.map { it.bytes })
        assertEquals(listOf(true, false), again.map { it.image })
        assertTrue(again.all { it.file.isFile })
        assertEquals(emptyList<AttachStore.Kept>(), again.filter { it.file.length() != it.bytes })
        assertEquals(emptyList<AttachStore.Kept>(), AttachStore(dir.root).list("s2"))
    }

    @Test
    fun drop_and_clear_leave_nothing_behind() {
        val a = AttachStore(dir.root)
        val one = a.keep("s1", "a.png", image = true) { bytes(3) }
        val two = a.keep("s1", "b.png", image = true) { bytes(3) }
        a.drop("s1", one.key)
        assertFalse(one.file.exists())
        assertEquals(listOf(two.key), a.list("s1").map { it.key })
        a.clear("s1")
        assertEquals(emptyList<AttachStore.Kept>(), a.list("s1"))
        assertFalse(two.file.exists())
    }

    @Test
    fun a_copy_that_breaks_halfway_leaves_no_entry() {
        val a = AttachStore(dir.root)
        val broken =
            object : InputStream() {
                var n = 0

                override fun read(): Int = if (n++ < 4) 1 else throw IOException("ENOSPC")
            }
        val failed = runCatching { a.keep("s1", "big.bin", image = false) { broken } }
        assertTrue(failed.exceptionOrNull() is IOException)
        assertEquals(emptyList<AttachStore.Kept>(), a.list("s1"))
        assertEquals(emptyList<String>(), dir.root.walk().filter { it.isFile && it.name == "big.bin" }.map { it.path }.toList())
    }

    @Test
    fun the_ids_handed_to_a_pending_line_are_kept_until_it_settles() {
        val a = AttachStore(dir.root)
        a.sent("local-1", listOf("x", "y"))
        assertEquals(listOf("x", "y"), AttachStore(dir.root).sentOf("local-1"))
        a.settled("local-1")
        assertEquals(emptyList<String>(), a.sentOf("local-1"))
    }

    @Test
    fun a_name_from_elsewhere_keeps_only_its_last_segment() {
        val k = AttachStore(dir.root).keep("s1", "../../etc/x.txt", image = false) { bytes(1) }
        assertEquals("x.txt", k.name)
        assertTrue(k.file.canonicalPath.startsWith(dir.root.canonicalPath))
    }
}
