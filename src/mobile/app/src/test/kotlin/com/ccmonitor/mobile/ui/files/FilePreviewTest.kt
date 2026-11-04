package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.TextPreview
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 文件屏的文本预览照那道门（`files-ls` 那一行的 `gates.preview.text`，那台判的）：照门的上限读；门带语言 ⇒ 整份交那台上色
 * （`prose-hl`，同聊天里代码块那一条），拉不到 ⇒ 照纯字。手机不认扩展名、不自己上色。
 */
class FilePreviewTest {
    private val path = RemotePath.of("/w/a.rs")
    private val code = "fn main() {}"

    private fun link(hl: Reply = Reply.Ok(mapOf("hl" to listOf(listOf(0.0, 2.0, 1.0))))) =
        FakeCoreLink().apply {
            answer = { cmd, _ ->
                when (cmd) {
                    "files-read-text" -> Reply.Ok(mapOf("path" to "/w/a.rs", "text" to code, "bytes" to 12.0, "sha256" to "ab"))
                    "prose-hl" -> hl
                    else -> Reply.LinkDown
                }
            }
        }

    @Test
    fun `门带语言 ⇒ 照门的上限读，再把整份交 prose-hl，色照回包第一块`() =
        runTest {
            val l = link()
            val got = FilePreview(l) { "box" }.text(path, TextPreview(cap = 65_536, lang = "rs")).getOrThrow()
            assertEquals(65_536L, l.calls("files-read-text").single()!!["max_bytes"])
            assertEquals(listOf(mapOf("lang" to "rs", "x" to code)), l.calls("prose-hl").single()!!["codes"])
            assertEquals(FilePreview.Text(code, listOf(0L, 2L, 1L)), got)
        }

    @Test
    fun `门不带语言 ⇒ 不问 prose-hl，照纯字`() =
        runTest {
            val l = link()
            val got = FilePreview(l) { "box" }.text(path, TextPreview(cap = 100)).getOrThrow()
            assertTrue(l.calls("prose-hl").isEmpty())
            assertEquals(FilePreview.Text(code, emptyList()), got)
        }

    @Test
    fun `上色没问成 ⇒ 照纯字，预览照样出`() =
        runTest {
            val got = FilePreview(link(hl = Reply.LinkDown)) { "box" }.text(path, TextPreview(cap = 100, lang = "rs")).getOrThrow()
            assertEquals(FilePreview.Text(code, emptyList()), got)
        }
}
