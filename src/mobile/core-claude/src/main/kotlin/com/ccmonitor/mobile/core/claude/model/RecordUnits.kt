package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.wire.BlockType
import com.ccmonitor.mobile.core.claude.link.wire.Hunk
import com.ccmonitor.mobile.core.claude.link.wire.LineRecord
import com.ccmonitor.mobile.core.claude.link.wire.RecordBlock
import com.ccmonitor.mobile.core.claude.link.wire.RecordKind
import com.ccmonitor.mobile.core.claude.link.wire.SpeakerKind

/**
 * 核心的通用记录（`IPC-PROTOCOL.md`「通用记录」，线上类 [LineRecord]）⇒ 渲染单元。只按 `t` · `speaker.kind` · 块的 `type` 排，
 * 不认任何一家的盘上格式；字照记录里写好的抄。哪一类画成什么有一张表，
 * 那张表里画成事件条的几类（交回 · 后台任务 · 来话 …）还没定画法，先不上屏。
 */
object RecordUnits {
    fun of(records: List<LineRecord>): List<RenderUnit> {
        val results = records.flatMap(::resultsOf).toMap()
        return records.flatMap { unitsOf(it, results) }
    }

    /** 一个工具调用配上的结果：正文 · 出错 · 核心给的 patch · 结果那一句（`StepResult.text`）。 */
    private class Result(
        val text: String,
        val isError: Boolean,
        val patch: List<PatchHunk>?,
        val file: String?,
        val line: String?,
    )

    private fun resultsOf(r: LineRecord): List<Pair<String, Result>> {
        if (r.t != RecordKind.SAID) return emptyList()
        val said = r.results.orEmpty()
        return r.blocks.orEmpty().filter { it.type == BlockType.TOOL_RESULT }.mapNotNull { b ->
            val id = b.of ?: return@mapNotNull null
            val text =
                b.content
                    .orEmpty()
                    .filter { it.type == BlockType.TEXT }
                    .mapNotNull(RecordBlock::text)
                    .joinToString("\n")
            val one = said[id]
            id to Result(text, b.isError == true, one?.patch?.map(::hunk), one?.file, one?.text?.takeIf { it.isNotEmpty() })
        }
    }

    private fun hunk(h: Hunk): PatchHunk = PatchHunk(h.oldStart, h.oldLines, h.newStart, h.newLines, h.lines)

    private fun unitsOf(
        r: LineRecord,
        results: Map<String, Result>,
    ): List<RenderUnit> =
        when (r.t) {
            RecordKind.SAID, RecordKind.QUEUED -> listOfNotNull(said(r))
            RecordKind.REPLY -> if (r.autoReply == true) emptyList() else reply(r, results)
            RecordKind.UNREAD -> listOfNotNull(r.text?.let { RenderUnit.Unread(r.id, it, r.timeText, r.excerpt.orEmpty(), r.id) })
            else -> emptyList()
        }

    private fun said(r: LineRecord): RenderUnit? {
        val id = r.id
        val who = r.who ?: return null
        val s = who.speaker
        val text = who.text
        return when (s.kind) {
            SpeakerKind.HUMAN -> RenderUnit.UserText(id, text, id)
            SpeakerKind.SLASH_COMMAND -> RenderUnit.SlashCommand(id, s.name.orEmpty(), s.args.orEmpty(), id)
            SpeakerKind.BASH_INPUT -> RenderUnit.BashInput(id, s.command.orEmpty(), id)
            SpeakerKind.BASH_OUTPUT -> RenderUnit.BashOutput(id, s.stdout.orEmpty(), s.stderr.orEmpty(), id)
            SpeakerKind.COMPACT_SUMMARY -> RenderUnit.CompactSummary(id, text, id)
            SpeakerKind.INTERRUPT -> RenderUnit.Interrupt(id, r.timeText, id)
            else -> null
        }
    }

    private fun reply(
        r: LineRecord,
        results: Map<String, Result>,
    ): List<RenderUnit> {
        val id = r.id
        return r.blocks.orEmpty().mapIndexedNotNull { i, b ->
            val key = "$id#$i"
            when (b.type) {
                BlockType.TEXT -> b.text?.takeIf { it.isNotBlank() }?.let { RenderUnit.AssistantMarkdown(key, it, id) }
                BlockType.THINKING -> b.text?.let { RenderUnit.Thinking(key, it, id) }
                BlockType.TOOL_USE -> {
                    val callId = b.id.orEmpty()
                    val res = results[callId]
                    val step = r.steps?.get(callId)
                    RenderUnit.ToolCall(
                        key = key,
                        toolUseId = callId,
                        name = b.name.orEmpty(),
                        input = inputOf(b.input),
                        resultText = res?.text,
                        isError = res?.isError == true,
                        pending = res == null,
                        sourceUuid = id,
                        structuredPatch = res?.patch,
                        patchFilePath = res?.file,
                        summary = (step?.arg ?: step?.text).orEmpty(),
                        resultLine = res?.line,
                    )
                }
                else -> null
            }
        }
    }

    /** 工具入参原样（形状照那一家，不解）：是一团键值 ⇒ 交卡片；别的 ⇒ 空。 */
    private fun inputOf(v: Any?): Map<String, Any?> {
        val m = v as? Map<*, *> ?: return emptyMap()
        return m.entries.mapNotNull { (k, x) -> (k as? String)?.let { it to x } }.toMap()
    }
}
