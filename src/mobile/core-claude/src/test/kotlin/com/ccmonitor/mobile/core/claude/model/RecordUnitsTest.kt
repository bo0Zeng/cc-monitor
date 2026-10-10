package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 核心的通用记录 ⇒ 渲染单元：只按 `t` · `speaker.kind` · 块的 `type` 排，不认任何一家的盘上格式。
 * 样本是后端真序列化器写的 `tests/__fixtures__/record.golden.jsonl`。
 */
class RecordUnitsTest {
    private val cases: Map<String, Map<String, Any?>> =
        File(repoRoot(), "tests/__fixtures__/record.golden.jsonl")
            .readLines()
            .mapNotNull(Json::obj)
            .associate { (it["case"] as String) to (it["record"] as Map<String, Any?>) }

    private fun units(vararg names: String) = RecordUnits.of(names.map { cases.getValue(it) })

    @Test
    fun `人说的画成气泡；正文照 who_text`() {
        assertEquals(listOf(RenderUnit.UserText("u-min", "hello", "u-min")), units("said-min"))
    }

    @Test
    fun `回复：思考 · 正文 · 工具调用各一单元，按块的先后；工具的结果从后来那条人侧记录里配上`() {
        val u = units("reply-full")
        assertEquals(listOf("a-full#0", "a-full#1", "a-full#2", "a-full#3"), u.map { it.key })
        assertEquals(RenderUnit.Thinking("a-full#0", "plan it", "a-full"), u[0])
        assertEquals(RenderUnit.AssistantMarkdown("a-full#1", "done", "a-full"), u[1])
        val edit = u[3] as RenderUnit.ToolCall
        assertEquals("Edit", edit.name)
        assertTrue("没有结果 ⇒ 还在跑", edit.pending)
    }

    @Test
    fun `工具结果：正文 · 出错 · 核心给的 patch 原样带上`() {
        val reply = mapOf("agent" to "claude", "id" to "a", "t" to "reply", "autoReply" to false, "endsTurn" to false, "blocks" to listOf(mapOf("type" to "tool_use", "id" to "call-1", "name" to "Edit", "input" to mapOf("file_path" to "/w/a.txt"))))
        val call = RecordUnits.of(listOf(reply, cases.getValue("said-full"))).first() as RenderUnit.ToolCall
        assertEquals("line a\nline b", call.resultText)
        assertEquals(false, call.pending)
        assertEquals(false, call.isError)
        assertEquals(listOf(PatchHunk(1, 1, 1, 2, listOf("-a", "+b", "+c"))), call.structuredPatch)
        assertEquals("/w/a.txt", call.patchFilePath)
    }

    @Test
    fun `自动应答不建卡；斜杠命令 · bash 入出 · 摘要 · 中断各画各的`() {
        assertEquals(emptyList<RenderUnit>(), units("reply-auto"))
        assertEquals(RenderUnit.SlashCommand("u-said-slash", "/review", "x", "u-said-slash"), units("said-slash").single())
        assertTrue(units("said-bash-input").single() is RenderUnit.BashInput)
        assertEquals("o", (units("said-bash-output").single() as RenderUnit.BashOutput).stdout)
        assertTrue(units("said-compact-summary").single() is RenderUnit.CompactSummary)
        assertTrue(units("said-interrupt").single() is RenderUnit.Interrupt)
    }

    @Test
    fun `没出稿的那几类（事件条 · 系统 · 重试 · 标题 · 认不出）不上屏`() {
        val silent =
            listOf("said-system", "said-task-notification", "said-agent-message", "said-peer-session", "said-coordinator", "said-command-output", "said-agent-task", "retry-full", "title-agent", "unread-unknown", "said-read")
        assertEquals(emptyList<RenderUnit>(), units(*silent.toTypedArray()))
    }

    @Test
    fun `一轮跑着时插进去的那句也是人说的`() {
        assertTrue(units("queued").single() is RenderUnit.UserText)
    }

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
}
