package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Test

/** [extractCodeBlocks]——成对围栏提取、去语言标记/围栏行/尾换行、未闭合不取（阅读面「复制代码块」）。 */
class MarkdownTest {
    @Test
    fun singleBlockStripsFenceAndLang() =
        assertEquals(listOf("val x = 1"), extractCodeBlocks("前\n```kotlin\nval x = 1\n```\n后"))

    @Test
    fun multipleBlocksInOrder() =
        assertEquals(listOf("a", "b\nc"), extractCodeBlocks("```\na\n```\n中间\n```py\nb\nc\n```"))

    @Test
    fun unclosedFenceNotExtracted() =
        assertEquals("未闭合围栏不提取（避免复制半截）", emptyList<String>(), extractCodeBlocks("```kotlin\nval x = 1"))

    @Test
    fun trailingBlankLinesTrimmed() =
        assertEquals(listOf("code"), extractCodeBlocks("```\ncode\n\n```"))

    @Test
    fun emptyAndNoFence() {
        assertEquals(emptyList<String>(), extractCodeBlocks(""))
        assertEquals(emptyList<String>(), extractCodeBlocks("纯文本无围栏"))
    }
}
