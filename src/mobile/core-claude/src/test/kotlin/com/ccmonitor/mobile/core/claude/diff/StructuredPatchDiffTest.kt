package com.ccmonitor.mobile.core.claude.diff

import com.ccmonitor.mobile.core.claude.model.PatchHunk
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** [structuredPatchToDiff] 纯函数——hunk → DiffResult，真行号 + ADD/DEL/CTX。 */
class StructuredPatchDiffTest {
    private fun hunk(
        oldStart: Int,
        oldLines: Int,
        newStart: Int,
        newLines: Int,
        lines: List<String>,
    ) = PatchHunk(oldStart, oldLines, newStart, newLines, lines)

    @Test
    fun lineNumbersAndKinds() {
        val d = structuredPatchToDiff(listOf(hunk(10, 3, 10, 3, listOf(" ctx1", "-del1", "+add1", " ctx2"))))
        // row0 = hunk 头（CTX、无行号）
        assertEquals(DiffType.CTX, d.rows[0].type)
        assertEquals("@@ -10,3 +10,3 @@", d.rows[0].text)
        assertEquals(null, d.rows[0].oldNo)
        // ctx1：oldNo=10 newNo=10、前缀已剥
        assertEquals(DiffType.CTX, d.rows[1].type)
        assertEquals("ctx1", d.rows[1].text)
        assertEquals(10, d.rows[1].oldNo)
        assertEquals(10, d.rows[1].newNo)
        // -del1：DEL oldNo=11 newNo=null
        assertEquals(DiffType.DEL, d.rows[2].type)
        assertEquals("del1", d.rows[2].text)
        assertEquals(11, d.rows[2].oldNo)
        assertEquals(null, d.rows[2].newNo)
        // +add1：ADD oldNo=null newNo=11
        assertEquals(DiffType.ADD, d.rows[3].type)
        assertEquals("add1", d.rows[3].text)
        assertEquals(null, d.rows[3].oldNo)
        assertEquals(11, d.rows[3].newNo)
        // ctx2：del 后 oldNo 已 12、add 后 newNo 已 12
        assertEquals(12, d.rows[4].oldNo)
        assertEquals(12, d.rows[4].newNo)
        assertEquals(1, d.addCount)
        assertEquals(1, d.delCount)
        assertFalse(d.truncated)
    }

    @Test
    fun multipleHunksEachHeaderedAndNumbered() {
        val d =
            structuredPatchToDiff(
                listOf(
                    hunk(1, 1, 1, 2, listOf(" a", "+b")),
                    hunk(50, 1, 51, 1, listOf("-x", "+y")),
                ),
            )
        assertEquals("@@ -1,1 +1,2 @@", d.rows[0].text)
        assertEquals(1, d.rows[1].oldNo) // 第一 hunk 从 1
        assertEquals("@@ -50,1 +51,1 @@", d.rows[3].text) // 第二 hunk 头
        assertEquals(50, d.rows[4].oldNo) // 第二 hunk 从 50/51
        assertEquals(51, d.rows[5].newNo)
        assertEquals(2, d.addCount) // +b +y
        assertEquals(1, d.delCount) // -x
    }

    @Test
    fun allAddedNewFile() {
        // 新文件：全 + 行 → addCount=全量、delCount=0。
        val d = structuredPatchToDiff(listOf(hunk(0, 0, 1, 3, listOf("+l1", "+l2", "+l3"))))
        assertEquals(3, d.addCount)
        assertEquals(0, d.delCount)
        assertEquals(1, d.rows[1].newNo)
        assertEquals(3, d.rows[3].newNo)
    }

    @Test
    fun emptyHunks() {
        val d = structuredPatchToDiff(emptyList())
        assertTrue(d.rows.isEmpty())
        assertEquals(0, d.addCount)
        assertEquals(0, d.delCount)
        assertFalse(d.truncated)
    }

    @Test
    fun noPrefixLineTreatedAsContext() {
        val d = structuredPatchToDiff(listOf(hunk(1, 1, 1, 1, listOf("nopfx", ""))))
        assertEquals(DiffType.CTX, d.rows[1].type)
        assertEquals("nopfx", d.rows[1].text) // 无前缀原样
        assertEquals(DiffType.CTX, d.rows[2].type)
        assertEquals("", d.rows[2].text) // 空行 → CTX
    }

    @Test
    fun truncatesToMaxLines() {
        val lines = (1..100).map { "+l$it" }
        val d = structuredPatchToDiff(listOf(hunk(1, 0, 1, 100, lines)), maxLines = 10)
        assertTrue(d.truncated)
        assertEquals(10, d.rows.size)
        assertEquals("全量 addCount 不受截断影响", 100, d.addCount)
    }

    @Test
    fun noNewlineMarkerSkippedNotCountedNoLineAdvance() {
        // jsdiff 在无尾换行文件里插裸 `\ No newline at end of file`——零行、不占行号、不成正文。
        val d =
            structuredPatchToDiff(
                listOf(
                    hunk(
                        1,
                        3,
                        1,
                        3,
                        listOf(" a", " b", "-c", "\\ No newline at end of file", "+c2", "\\ No newline at end of file"),
                    ),
                ),
            )
        // 标记不产 row：头 + a + b + -c + +c2 = 5 行
        assertEquals(5, d.rows.size)
        assertEquals(DiffType.DEL, d.rows[3].type)
        assertEquals(3, d.rows[3].oldNo)
        // +c2 的 newNo=3（未被标记的双推进污染成 4）
        assertEquals(DiffType.ADD, d.rows[4].type)
        assertEquals("c2", d.rows[4].text)
        assertEquals(3, d.rows[4].newNo)
        assertEquals(1, d.addCount) // 标记不计数
        assertEquals(1, d.delCount)
        assertFalse(d.rows.any { it.text.contains("No newline") }) // 标记不成正文行
    }

    @Test
    fun stripsTrailingCr() {
        // CRLF 源按 \n 切行后每行残留 \r → 剥掉，不进渲染。
        val d = structuredPatchToDiff(listOf(hunk(1, 1, 1, 2, listOf(" ctx\r", "+add\r"))))
        assertEquals("ctx", d.rows[1].text)
        assertEquals("add", d.rows[2].text)
    }

    @Test
    fun capsOverlongLine() {
        // minified 单行几十万字符 → 截到 2000 + …（对齐 LineDiff）。
        val d = structuredPatchToDiff(listOf(hunk(1, 0, 1, 1, listOf("+" + "x".repeat(3000)))))
        assertEquals(2001, d.rows[1].text.length) // 2000 + '…'
        assertTrue(d.rows[1].text.endsWith("…"))
    }
}
