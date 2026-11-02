package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingSearchTest {
    @Test fun searchableTextPerVariant() {
        assertEquals("hi user", RenderUnit.UserText("k", "hi user", null).searchableText())
        assertEquals("pondering", RenderUnit.Thinking("k", "pondering", null).searchableText())
    }

    @Test fun searchableTextOfProseIsTheTreesText() {
        // 树里的字原样（尖括号是字），标题不带 `#`、加粗不带 `**`。
        val doc =
            Doc(
                listOf(
                    MdBlock.H(2, listOf(Inl.Text("md "), Inl.InlNode(InlNode.B(listOf(Inl.Text("body")))))),
                    MdBlock.P(listOf(Inl.Text("把 <Composer> 换掉"))),
                ),
                emptyList(),
            )
        assertEquals("md body\n把 <Composer> 换掉", RenderUnit.AssistantMarkdown("k", doc, null).searchableText())
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
