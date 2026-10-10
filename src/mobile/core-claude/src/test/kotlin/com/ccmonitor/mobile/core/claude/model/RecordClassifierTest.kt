package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RecordClassifierTest {
    private fun user(
        vararg blocks: ContentBlock,
        meta: Boolean = false,
        uuid: String? = "u",
    ) =
        JsonlRecord.User(uuid, null, blocks.toList(), meta, null, null, null)

    private fun assistant(
        vararg blocks: ContentBlock,
        uuid: String? = "a",
    ) =
        JsonlRecord.Assistant(uuid, null, blocks.toList(), "m", null, null, false)

    @Test fun userPromptThenAssistantText() {
        val units =
            RecordClassifier.classify(
                listOf(
                    user(ContentBlock.Text("do it")),
                    assistant(ContentBlock.Text("ok")),
                ),
            )
        assertEquals(2, units.size)
        assertEquals("do it", (units[0] as RenderUnit.UserText).text)
        assertEquals("ok", (units[1] as RenderUnit.AssistantMarkdown).markdown)
    }

    @Test fun toolUsePairedWithLaterResult() {
        val units =
            RecordClassifier.classify(
                listOf(
                    assistant(ContentBlock.ToolUse("t1", "Bash", mapOf("command" to "ls"))),
                    user(ContentBlock.ToolResult("t1", "file1\nfile2", false)),
                ),
            )
        assertEquals(1, units.size) // 合并成一个 ToolCall，不是两个
        val call = units[0] as RenderUnit.ToolCall
        assertEquals("Bash", call.name)
        assertEquals("file1\nfile2", call.resultText)
        assertFalse(call.pending)
        assertFalse(call.isError)
    }

    @Test fun pendingToolUseHasNoResult() {
        val units =
            RecordClassifier.classify(
                listOf(assistant(ContentBlock.ToolUse("t9", "Read", emptyMap()))),
            )
        val call = units[0] as RenderUnit.ToolCall
        assertTrue(call.pending)
        assertNull(call.resultText)
    }

    @Test fun errorResultMarksIsError() {
        val units =
            RecordClassifier.classify(
                listOf(
                    assistant(ContentBlock.ToolUse("e1", "Bash", emptyMap())),
                    user(ContentBlock.ToolResult("e1", "boom", true)),
                ),
            )
        assertTrue((units[0] as RenderUnit.ToolCall).isError)
    }

    @Test fun metaUserSkippedButToolResultStillReconciles() {
        val units =
            RecordClassifier.classify(
                listOf(
                    user(ContentBlock.Text("injected reminder"), meta = true), // 跳过
                    assistant(ContentBlock.ToolUse("t1", "Grep", emptyMap())),
                    user(ContentBlock.ToolResult("t1", "match", false)), // 非 meta，正常回填
                ),
            )
        assertEquals(1, units.size)
        assertEquals("match", (units[0] as RenderUnit.ToolCall).resultText)
    }

    @Test fun orphanToolResultDropped() {
        val units =
            RecordClassifier.classify(
                listOf(user(ContentBlock.ToolResult("nomatch", "x", false))),
            )
        assertTrue(units.isEmpty())
    }

    @Test fun thinkingAndOrderingPreserved() {
        val units =
            RecordClassifier.classify(
                listOf(
                    assistant(
                        ContentBlock.Thinking("reason"),
                        ContentBlock.Text("answer"),
                        ContentBlock.ToolUse("t1", "Bash", emptyMap()),
                    ),
                    user(ContentBlock.ToolResult("t1", "done", false)),
                ),
            )
        assertEquals(3, units.size)
        assertTrue(units[0] is RenderUnit.Thinking)
        assertTrue(units[1] is RenderUnit.AssistantMarkdown)
        assertTrue(units[2] is RenderUnit.ToolCall)
    }

    @Test fun blankTextBlocksSkipped() {
        val units = RecordClassifier.classify(listOf(assistant(ContentBlock.Text("   "))))
        assertTrue(units.isEmpty())
    }

    @Test fun keysAreUniqueAndIdentityBased() {
        // 一条 assistant 记录里多块 → 各得不同 key（uuid#blockIndex）
        val units =
            RecordClassifier.classify(
                listOf(
                    assistant(
                        ContentBlock.Thinking("t"),
                        ContentBlock.Text("a"),
                        ContentBlock.ToolUse("x", "Bash", emptyMap()),
                        uuid = "A",
                    ),
                    user(ContentBlock.Text("hi"), uuid = "B"),
                ),
            )
        val keys = units.map { it.key }
        assertEquals(keys.size, keys.toSet().size) // 唯一
        assertEquals(listOf("A#0", "A#1", "A#2", "B#0"), keys) // 基于 uuid+块序号
    }

    @Test fun reconcileKeepsToolCallKey() {
        val units =
            RecordClassifier.classify(
                listOf(
                    assistant(ContentBlock.ToolUse("t1", "Bash", emptyMap()), uuid = "A"),
                    user(ContentBlock.ToolResult("t1", "out", false), uuid = "B"),
                ),
            )
        // 回填后 ToolCall 仍是 tool_use 那条记录的 key，不随 result 漂移
        assertEquals("A#0", (units[0] as RenderUnit.ToolCall).key)
    }

    // --- 非真实用户输入分类 ---

    @Test fun slashCommandBecomesSlashUnit() {
        val units =
            RecordClassifier.classify(
                listOf(user(ContentBlock.Text("<command-name>/model</command-name><command-args>opus</command-args>"))),
            )
        assertEquals(1, units.size)
        val s = units[0] as RenderUnit.SlashCommand
        assertEquals("/model", s.name)
        assertEquals("opus", s.args)
    }

    @Test fun bashInputAndOutputBecomeBashUnits() {
        val ui = RecordClassifier.classify(listOf(user(ContentBlock.Text("<bash-input>ls -la</bash-input>"))))
        assertEquals("ls -la", (ui[0] as RenderUnit.BashInput).command)
        val uo = RecordClassifier.classify(listOf(user(ContentBlock.Text("<bash-stdout>ok</bash-stdout>"))))
        assertEquals("ok", (uo[0] as RenderUnit.BashOutput).stdout)
    }

    @Test fun compactSummaryBecomesCompactUnit() {
        val units =
            RecordClassifier.classify(
                listOf(user(ContentBlock.Text("This session is being continued from a previous conversation. Summary: …"))),
            )
        assertTrue(units[0] is RenderUnit.CompactSummary)
    }

    @Test fun interruptMarkerIsSkipped() {
        val units =
            RecordClassifier.classify(
                listOf(
                    user(ContentBlock.Text("[Request interrupted by user]")),
                    assistant(ContentBlock.Text("resumed")),
                ),
            )
        assertEquals(1, units.size) // 中断标记 skip，只剩 assistant
        assertEquals("resumed", (units[0] as RenderUnit.AssistantMarkdown).markdown)
    }

    @Test fun pureNoiseUserIsSkipped() {
        val units =
            RecordClassifier.classify(
                listOf(user(ContentBlock.Text("<local-command-stdout>Set model to Fable 5</local-command-stdout>"))),
            )
        assertTrue("纯 stdout 噪音 → skip", units.isEmpty())
    }

    @Test fun sidechainUserAndAssistantSkipped() {
        val side = JsonlRecord.User("s", null, listOf(ContentBlock.Text("hi")), false, null, null, null, isSidechain = true)
        val sideA = JsonlRecord.Assistant("sa", null, listOf(ContentBlock.Text("yo")), "m", null, null, false, isSidechain = true)
        assertTrue(RecordClassifier.classify(listOf(side, sideA)).isEmpty())
    }

    @Test fun realPromptStillUserText() {
        // 剥噪音后仍是普通文本 → UserText（真实输入不误伤）。
        val units = RecordClassifier.classify(listOf(user(ContentBlock.Text("全面理解审计这个项目"))))
        assertEquals("全面理解审计这个项目", (units[0] as RenderUnit.UserText).text)
    }
}
