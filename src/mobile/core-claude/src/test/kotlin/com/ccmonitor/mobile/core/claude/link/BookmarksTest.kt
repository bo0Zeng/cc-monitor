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

    /** 那台的替身：回 [dirs]；问的 `at` 里在 [dirs] 里的那几个各答一个下标（同后端：不在的不列）。 */
    private fun answering(dirs: List<String>): suspend (String, Map<String, Any?>) -> Reply =
        { cmd, args ->
            asked += cmd to args
            val at = (args["at"] as? List<*>).orEmpty().map { it as String }
            Reply.Ok(mapOf("dirs" to dirs, "here" to at.filter { it in dirs }.associateWith { dirs.indexOf(it).toDouble() }))
        }

    /** 🔴 每一问都带此刻那个目录（`at`），在不在书签里照回包的 `here` 认，手机不自己比。 */
    @Test
    fun `读 · 切换 · 删：入参与桌面同形、带此刻那个目录，在不在照那台答的`() =
        runTest {
            val shelf = BookmarkShelf(answering(listOf("/srv")))
            shelf.refresh("/srv")
            assertEquals(listOf("/srv"), shelf.dirs.value)
            assertEquals(mapOf("/srv" to 0L), shelf.here.value)
            shelf.toggle("/srv/")
            shelf.remove("/srv")
            assertEquals(
                listOf(
                    Bookmarks.READ to mapOf("at" to listOf("/srv")),
                    Bookmarks.EDIT to mapOf("op" to "toggle", "dirs" to listOf("/srv/"), "at" to listOf("/srv")),
                    Bookmarks.EDIT to mapOf("op" to "remove", "dirs" to listOf("/srv"), "at" to listOf("/srv")),
                ),
                asked,
            )
            shelf.refresh("/etc")
            assertTrue("那台没把它列进 here ⇒ 不在", shelf.here.value.isEmpty())
        }

    @Test
    fun `先发后到的回包不盖掉后发的；问不成说出来、手上那份不动`() =
        runTest {
            val slow = CompletableDeferred<Reply>()
            val shelf =
                BookmarkShelf { cmd, _ ->
                    if (cmd == Bookmarks.READ) slow.await() else Reply.Ok(mapOf("dirs" to listOf("/new")))
                }
            val first = async(start = CoroutineStart.UNDISPATCHED) { shelf.refresh("/new") }
            shelf.toggle("/new")
            slow.complete(Reply.Ok(mapOf("dirs" to listOf("/old"))))
            first.await()
            assertEquals(listOf("/new"), shelf.dirs.value)
            val failing =
                BookmarkShelf { _, _ -> Reply.Failed(CoreFailure("io_failed", "书签读不出来", "")) }
            failing.refresh("/a")
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
            assertNull(Bookmarks.of(mapOf("dirs" to "x")))
        }
}
