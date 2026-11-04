package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.wire.gen.PreviewGate
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

    @Test
    fun `门是 Image ⇒ 照门的上限经核心分块读，交回图的字节`() =
        runTest {
            val l =
                FakeCoreLink().apply {
                    transferred = { Reply.Ok(mapOf("bytes" to 3.0, "content" to mapOf("b16" to "89504e"), "undecodable" to "解不开")) }
                }
            val png = RemotePath.of("/w/p.png")
            val got = FilePreview(l) { "box" }.show(png, PreviewGate.Image(4_194_304)).getOrThrow()
            assertEquals(Transfer.Pull(png, 4_194_304), l.transfers.single().first)
            assertTrue(got is FilePreview.Image && got.bytes.contentEquals(byteArrayOf(0x89.toByte(), 0x50, 0x4e)))
            assertEquals("解不开", (got as FilePreview.Image).undecodable)
        }

    @Test
    fun `门是 Text ⇒ 照文本读，不走分块`() =
        runTest {
            val l = link()
            val got = FilePreview(l) { "box" }.show(path, PreviewGate.Text(TextPreview(cap = 100))).getOrThrow()
            assertEquals(FilePreview.Text(code, emptyList()), got)
            assertTrue(l.transfers.isEmpty())
        }

    @Test
    fun `门不许 ⇒ 那台那一句，不读`() =
        runTest {
            val l = link()
            val got = FilePreview(l) { "box" }.show(path, PreviewGate.No("太大")).exceptionOrNull()
            assertEquals("太大", got?.message)
            assertTrue(l.transfers.isEmpty() && l.calls("files-read-text").isEmpty())
        }
}
