package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 书签归那台后端：手机按那台问 `bookmarks-read` / `-edit`（与桌面文件窗口同一形）· 照回包换 · 先发后到的不认 · 问不成出声 · 旧书签迁过去。 */
class BookmarksTest {
    private val asked = mutableListOf<Pair<String, Map<String, Any?>>>()

    private fun answering(dirs: List<String>): suspend (String, Map<String, Any?>) -> Reply =
        { cmd, args ->
            asked += cmd to args
            Reply.Ok(mapOf("dirs" to dirs))
        }

    @Test
    fun `读 · 切换 · 删：入参与桌面同形，回包整份照换`() =
        runTest {
            val shelf = BookmarkShelf(answering(listOf("/srv")))
            shelf.refresh()
            assertEquals(listOf("/srv"), shelf.dirs.value)
            shelf.toggle("/srv/")
            shelf.remove("/srv")
            assertEquals(
                listOf(
                    Bookmarks.READ to emptyMap<String, Any?>(),
                    Bookmarks.EDIT to mapOf("op" to "toggle", "dirs" to listOf("/srv/")),
                    Bookmarks.EDIT to mapOf("op" to "remove", "dirs" to listOf("/srv")),
                ),
                asked,
            )
            assertTrue(shelf.has("/srv"))
            assertFalse(shelf.has("/etc"))
        }

    @Test
    fun `先发后到的回包不盖掉后发的；问不成说出来、手上那份不动`() =
        runTest {
            val slow = CompletableDeferred<Reply>()
            val shelf =
                BookmarkShelf { cmd, _ ->
                    if (cmd == Bookmarks.READ) slow.await() else Reply.Ok(mapOf("dirs" to listOf("/new")))
                }
            val first = async(start = CoroutineStart.UNDISPATCHED) { shelf.refresh() }
            shelf.toggle("/new")
            slow.complete(Reply.Ok(mapOf("dirs" to listOf("/old"))))
            first.await()
            assertEquals(listOf("/new"), shelf.dirs.value)
            val failing =
                BookmarkShelf { _, _ -> Reply.Failed(CoreFailure("io_failed", "书签读不出来", "")) }
            failing.refresh()
            assertEquals("书签读不出来", failing.said.value)
            assertTrue(failing.dirs.value.isEmpty())
        }

    @Test
    fun `旧书签迁过去：一次 add，答 ok 才算迁好`() =
        runTest {
            val shelf = BookmarkShelf(answering(listOf("/a", "/b")))
            assertTrue(shelf.adopt(listOf("/a", "/b")))
            assertEquals(Bookmarks.EDIT to mapOf("op" to "add", "dirs" to listOf("/a", "/b")), asked.last())
            assertEquals(listOf("/a", "/b"), shelf.dirs.value)
            val down = BookmarkShelf { _, _ -> Reply.LinkDown }
            assertFalse("没答 ok ⇒ 旧的那几行不许删", down.adopt(listOf("/a")))
            assertNull(Bookmarks.dirs(mapOf("dirs" to "x")))
        }
}
