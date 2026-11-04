package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 文件屏的搜索：按名字（`files-find`）· 按内容（`files-grep`），搜都由那台做、起点是当前目录；结果与「没有」照回包摆，没问成写那台那一句。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class FileSearchTest {
    private val dir = RemotePath.of("/w")

    private fun nameHit(
        path: String,
        location: String,
        kind: String = "file",
    ) = mapOf(
        "path" to path,
        "kind" to kind,
        "class" to if (kind == "dir") "folder" else "text",
        "location" to location,
        "size" to 3L,
        "mtime_secs" to null,
        "mtime_text" to null,
        "marks" to emptyList<Any>(),
    )

    private fun found(vararg hits: Map<String, Any?>) =
        Reply.Ok(
            mapOf(
                "hits" to hits.toList(),
                "start" to "/w",
                "sort" to "relevance",
                "desc" to false,
                "total_hits" to hits.size.toLong(),
                "truncated" to false,
                "scanned" to 10L,
                "index_age_secs" to 0L,
                "index_missing" to false,
                "stale" to false,
                "index_root" to "/w",
                "out_of_index" to false,
                "cover_root" to null,
                "seq" to null,
                "offset" to 0L,
            ),
        )

    private fun grepHit(
        rel: String,
        line: Long,
        text: String,
    ) = mapOf("path" to "/w/$rel", "rel" to rel, "line" to line, "text" to text, "matches" to 1L, "lines" to emptyList<Any>())

    private fun grepped(vararg hits: Map<String, Any?>) =
        Reply.Ok(
            mapOf(
                "path" to "/w",
                "hits" to hits.toList(),
                "files" to 4L,
                "bytes" to 100L,
                "links" to 0L,
                "skipped_binary" to 0L,
                "skipped_large" to 0L,
                "skipped_mounts" to 0L,
                "unreadable" to 0L,
                "limit" to 500L,
                "truncated" to false,
                "stopped" to null,
            ),
        )

    @Test
    fun `搜名字：交 files-find 带那一句与当前目录；回包两条 ⇒ 两行，照那台给的路径与所在目录`() =
        runTest {
            val link = FakeCoreLink().apply { answer = { cmd, _ -> if (cmd == "files-find") found(nameHit("/w/a/main.rs", "a"), nameHit("/w/src", ".", "dir")) else Reply.LinkDown } }
            val s = FileSearch(link, { "box" }, backgroundScope)
            s.open()
            s.search(dir, "main")
            runCurrent()
            assertEquals(listOf(mapOf("query" to "main", "under" to "/w", "scope" to "under")), link.calls("files-find"))
            val rows = s.state.value!!.rows!!
            assertEquals(2, rows.size)
            val first = rows[0] as FileSearch.Row.Name
            assertEquals(RemotePath.of("/w/a/main.rs"), first.path)
            assertEquals("a", first.location)
            assertEquals(false, first.dir)
            assertTrue((rows[1] as FileSearch.Row.Name).dir)
            assertNull(s.state.value!!.said)
        }

    @Test
    fun `搜内容：交 files-grep 带当前目录与那一串；每条一行：相对路径 · 行号 · 那一行`() =
        runTest {
            val link = FakeCoreLink().apply { answer = { cmd, _ -> if (cmd == "files-grep") grepped(grepHit("a/x.kt", 12L, "val main = 1")) else Reply.LinkDown } }
            val s = FileSearch(link, { "box" }, backgroundScope)
            s.open()
            s.mode(content = true)
            s.search(dir, "main")
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/w", "needle" to "main")), link.calls("files-grep"))
            val rows = s.state.value!!.rows!!
            val row = rows.single() as FileSearch.Row.Line
            assertEquals(RemotePath.of("/w/a/x.kt"), row.path)
            assertEquals(listOf("a/x.kt", "12", "val main = 1"), listOf(row.rel, row.line.toString(), row.text))
        }

    @Test
    fun `零条 ⇒ 无匹配那一句；没问成 ⇒ 那台那一句`() =
        runTest {
            val link = FakeCoreLink().apply { answer = { cmd, _ -> if (cmd == "files-find") found() else FakeCoreLink.failed("那台不认") } }
            val s = FileSearch(link, { "box" }, backgroundScope)
            s.open()
            s.search(dir, "zzz")
            runCurrent()
            assertEquals(emptyList<FileSearch.Row>(), s.state.value!!.rows)
            assertEquals(copyText("history.list.noMatch"), s.state.value!!.said)
            s.mode(content = true)
            s.search(dir, "zzz")
            runCurrent()
            assertNull(s.state.value!!.rows)
            assertEquals("那台不认", s.state.value!!.said)
        }

    @Test
    fun `空着的词不问；关了就没有那一面`() =
        runTest {
            val link = FakeCoreLink()
            val s = FileSearch(link, { "box" }, backgroundScope)
            s.open()
            s.search(dir, "  ")
            runCurrent()
            assertTrue(link.calls.isEmpty())
            s.close()
            assertNull(s.state.value)
        }
}
