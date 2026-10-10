package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.Json
import com.ccmonitor.mobile.core.claude.link.RecordLine
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Codex 那一家：造的中性 rollout（`tests/__fixtures__/codex-rollout.sample.jsonl`）走核心读页那一遍写成的通用记录金样
 * （`record-codex.golden.jsonl`，后端 `codex_sample_rollout_reads_to_the_record_golden` 写、界面 `codex-record.vitest.ts` 也解它）
 * ⇒ 照 `history-read` 的回话形状交进来 ⇒ [HistoryRead] ⇒ [RecordUnits]。手机不认 Codex 的盘上格式，渲染单元逐格照记录。
 */
class CodexRecordGoldenTest {
    private val golden: List<Map<String, Any?>> =
        File(repoRoot(), "tests/__fixtures__/record-codex.golden.jsonl")
            .readLines()
            .filter { it.isNotBlank() }
            .mapNotNull(Json::obj)

    private fun record(case: String): Map<String, Any?> = golden.first { it["case"] == case }["record"] as Map<String, Any?>

    /** 金样照 `history-read` 的回话交进来（`rows[].record` · `end`）。 */
    private fun read(): List<Map<String, Any?>> {
        val rows = golden.mapIndexed { i, g -> mapOf("end" to (i + 1).toLong(), "hash" to 0L, "record" to g["record"]) }
        val page = HistoryRead.of(mapOf("rows" to rows, "next" to rows.size.toLong(), "eof" to true))!!
        return page.records
    }

    private val units: List<RenderUnit> by lazy { RecordUnits.of(read()) }

    @Test
    fun `金样每一条都解得出、都是 codex 那一家的`() {
        val records = read()
        assertEquals(golden.size, records.size)
        assertEquals(setOf("codex"), records.map { it["agent"] }.toSet())
    }

    @Test
    fun `逐格照记录：人话 · 推理 · 两种工具（入参与配上的结果）· 回复；系统注入 · 空推理 · 工具结果那一条 · 认不出的那一行不单出`() {
        val human = record("line-05")
        val think = record("line-07")
        val shell = record("line-09")
        val patch = record("line-11")
        val answer = record("line-13")
        val expected =
            listOf(
                RenderUnit.UserText(human["id"] as String, "list the files", human["id"] as String),
                RenderUnit.Thinking("${think["id"]}#0", "look around first", think["id"] as String),
                RenderUnit.ToolCall(
                    key = "${shell["id"]}#0",
                    toolUseId = "call-1",
                    name = "shell",
                    input = mapOf("command" to listOf("ls"), "workdir" to "/work/demo"),
                    resultText = "a.txt\nb.txt",
                    isError = false,
                    pending = false,
                    sourceUuid = shell["id"] as String,
                ),
                RenderUnit.ToolCall(
                    key = "${patch["id"]}#0",
                    toolUseId = "call-2",
                    name = "apply_patch",
                    input = mapOf("input" to "*** Begin Patch\n*** End Patch"),
                    resultText = "patch applied",
                    isError = false,
                    pending = false,
                    sourceUuid = patch["id"] as String,
                ),
                RenderUnit.AssistantMarkdown("msg-9#0", "two files", "msg-9"),
            )
        assertEquals(expected, units)
    }

    @Test
    fun `渲染单元里的字都是记录里那几格的原样`() {
        // 反过来对一遍：每个单元的字在金样对应那一格里逐字出现，手机不改写、不拼接。
        val cells = golden.joinToString("\n") { Json.line(it["record"]) }
        for (u in units) {
            val text =
                when (u) {
                    is RenderUnit.UserText -> u.text
                    is RenderUnit.Thinking -> u.text
                    is RenderUnit.AssistantMarkdown -> u.markdown
                    is RenderUnit.ToolCall -> u.resultText.orEmpty()
                    else -> ""
                }
            assertTrue("单元 ${u.key} 的字不在记录里：$text", cells.contains(Json.line(text)))
        }
    }

    @Test
    fun `流上那一路：Codex 会话的 line 帧带的记录解成同一种渲染单元`() {
        // 金样 `codex-session-stream.golden.jsonl`：后端 watcher 跟一份造的 Codex rollout 发出来的帧（cxstream 那路钉着）。
        val frames =
            File(repoRoot(), "tests/__fixtures__/codex-session-stream.golden.jsonl")
                .readLines()
                .filter { it.isNotBlank() }
                .mapNotNull(Json::obj)
                .filter { it["kind"] == "line" }
        val lines = frames.map { requireNotNull(RecordLine.of(it)) { "line 帧解不出：$it" } }
        val records = lines.mapNotNull { it.record }
        assertTrue("金样里得有带记录的 line 帧", records.isNotEmpty())
        assertEquals(setOf("codex"), records.map { it["agent"] }.toSet())
        val said = records.single()
        assertEquals(listOf(RenderUnit.UserText(said["id"] as String, "hello", said["id"] as String)), RecordUnits.of(records))
    }

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
}
