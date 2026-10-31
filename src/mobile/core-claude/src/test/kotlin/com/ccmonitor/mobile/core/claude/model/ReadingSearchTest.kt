package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingSearchTest {
    @Test fun searchableTextPerVariant() {
        assertEquals("hi user", RenderUnit.UserText("k", "hi user", null).searchableText())
        assertEquals("# md body", RenderUnit.AssistantMarkdown("k", "# md body", null).searchableText())
        assertEquals("pondering", RenderUnit.Thinking("k", "pondering", null).searchableText())
    }

    @Test fun searchableTextToolCallIncludesNameSummaryResult() {
        val tc =
            RenderUnit.ToolCall(
                key = "k",
                toolUseId = "t1",
                name = "Bash",
                summary = "ls -la",
                resultText = "total 0",
                isError = false,
                pending = false,
                sourceUuid = null,
            )
        assertEquals("Bash\nls -la\ntotal 0", tc.searchableText())
    }

    @Test fun extractCodeBlocksSingle() {
        val md = "before\n```kotlin\nval x = 1\n```\nafter"
        assertEquals(listOf("val x = 1"), extractCodeBlocks(md))
    }

    @Test fun extractCodeBlocksMultipleAndNoLang() {
        val md = "```\nplain\n```\ntext\n```sh\necho hi\necho bye\n```"
        assertEquals(listOf("plain", "echo hi\necho bye"), extractCodeBlocks(md))
    }

    @Test fun extractCodeBlocksNoneOrUnclosed() {
        assertEquals(emptyList<String>(), extractCodeBlocks("no code here `inline` only"))
        assertEquals(emptyList<String>(), extractCodeBlocks("```\nunclosed fence\n")) // 未闭合不提取
    }

    @Test fun extractCodeBlocksEmptyBlock() {
        assertEquals(listOf(""), extractCodeBlocks("```\n```")) // 空代码块 → 一个空串
    }

    @Test fun searchableTextToolCallPendingNullResult() {
        val tc =
            RenderUnit.ToolCall(
                key = "k",
                toolUseId = "t1",
                name = "Read",
                summary = "/a/b.kt",
                resultText = null,
                isError = false,
                pending = true,
                sourceUuid = null,
            )
        assertEquals("Read\n/a/b.kt", tc.searchableText())
    }

    @Test fun searchableTextToolCallWithoutSummary() {
        val tc = RenderUnit.ToolCall("k", "t1", "Read", "r", false, false, null)
        assertEquals("Read\nr", tc.searchableText())
    }
}
