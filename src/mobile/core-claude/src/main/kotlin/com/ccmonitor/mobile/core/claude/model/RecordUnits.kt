package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Block
import com.ccmonitor.mobile.core.wire.gen.Draw
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.Raw
import com.ccmonitor.mobile.core.wire.gen.Speaker
import com.ccmonitor.mobile.core.wire.gen.ToolCard
import com.squareup.moshi.JsonReader
import okio.Buffer
import com.ccmonitor.mobile.core.wire.gen.PatchHunk as WirePatchHunk

/**
 * 核心的通用记录（`IPC-PROTOCOL.md`「通用记录」，线上类 [LineRecord]）⇒ 渲染单元。只按记录 · 说话人 · 块是哪一形排，
 * 不认任何一家的盘上格式；字照记录里写好的抄。哪一类画成什么有一张表，
 * 那张表里画成事件条的几类（交回 · 后台任务 · 来话 …）还没定画法，先不上屏。
 *
 * 一条会话一份，随记录增量排：新来的记录只排它自己；工具结果到了只换它配上的那一张调用卡，别的单元原样留着。
 */
class RecordUnits {
    /** 一条记录与它排出的单元。 */
    private class Row(
        val record: LineRecord,
        var units: List<RenderUnit>,
    )

    private val rows = ArrayList<Row>()
    private val results = HashMap<String, Result>()

    /** 工具调用 id ⇒ 那张调用卡在哪一条的第几块（结果后到时只换它）。 */
    private val calls = HashMap<String, MutableList<Pair<Row, Int>>>()

    fun isEmpty(): Boolean = rows.isEmpty()

    /** 此刻的整张单元表（只拼，不排）。 */
    fun units(): List<RenderUnit> = rows.flatMapTo(ArrayList(rows.size * 2)) { it.units }

    /** 接在后面。 */
    fun append(rs: List<LineRecord>) = place(rs, rows.size)

    /** 往上翻读到的更早那一段：接在前面。 */
    fun prepend(rs: List<LineRecord>) = place(rs, 0)

    /** 整份换成 [rs]。 */
    fun reset(rs: List<LineRecord>) {
        rows.clear()
        results.clear()
        calls.clear()
        append(rs)
    }

    /** 先换已有的、配上了新结果的调用卡，再排新来的（照整张结果表排）。 */
    private fun place(
        rs: List<LineRecord>,
        at: Int,
    ) {
        for ((id, res) in rs.flatMap(::resultsOf)) {
            results[id] = res
            calls[id]?.forEach { (row, i) ->
                val r = row.record as LineRecord.Reply
                val card = toolCall(r, i, r.blocks[i] as Block.ToolUse, results)
                row.units = row.units.map { u -> if (u.key == card.key) card else u }
            }
        }
        val added = rs.map { Row(it, unitsOf(it, results)) }
        rows.addAll(at, added)
        for (row in added) {
            val r = row.record as? LineRecord.Reply
            if (r == null || r.draw == Draw.NONE) continue
            for ((i, b) in r.blocks.withIndex()) {
                if (b is Block.ToolUse) calls.getOrPut(b.id) { mutableListOf() } += row to i
            }
        }
    }

    companion object {
        /** 一份记录整份排一次。 */
        fun of(records: List<LineRecord>): List<RenderUnit> = RecordUnits().also { it.append(records) }.units()
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
        if (r !is LineRecord.Said) return emptyList()
        val said = r.results.orEmpty()
        return r.blocks.filterIsInstance<Block.ToolResult>().map { b ->
            val id = b.`for`
            val text = b.content.filterIsInstance<Block.Text>().joinToString("\n") { it.text }
            val one = said[id]
            id to Result(text, b.isError, one?.patch?.map(::hunk), one?.file, one?.text?.takeIf { it.isNotEmpty() })
        }
    }

    /** 线上那一块 diff ⇒ 渲染用的那一块。 */
    private fun hunk(h: WirePatchHunk): PatchHunk =
        PatchHunk(h.oldStart.toInt(), h.oldLines.toInt(), h.newStart.toInt(), h.newLines.toInt(), h.lines)

    private fun unitsOf(
        r: LineRecord,
        results: Map<String, Result>,
    ): List<RenderUnit> =
        // 画不画是后端那一格（`draw`）：不画的与系统注入细条手机不摆。
        if (r.draw == Draw.NONE || r.draw == Draw.STRIP) {
            emptyList()
        } else {
            unitsOfDrawn(r, results)
        }

    private fun unitsOfDrawn(
        r: LineRecord,
        results: Map<String, Result>,
    ): List<RenderUnit> =
        when (r) {
            is LineRecord.Said -> listOfNotNull(said(r))
            is LineRecord.Reply -> reply(r, results)
            is LineRecord.Unread -> listOf(RenderUnit.Unread(r.id, r.text, r.timeText, r.excerpt, r.id))
            else -> emptyList()
        }

    private fun said(r: LineRecord.Said): RenderUnit? {
        val id = r.id
        val text = r.who.text
        return when (val s = r.who.speaker) {
            Speaker.Human -> RenderUnit.UserText(id, text, id, images = imagesOf(r.blocks), attachments = r.attachments.orEmpty())
            is Speaker.SlashCommand -> RenderUnit.SlashCommand(id, s.name, s.args, id)
            is Speaker.BashInput -> RenderUnit.BashInput(id, s.command, id)
            is Speaker.BashOutput -> RenderUnit.BashOutput(id, s.stdout, s.stderr, id)
            is Speaker.CompactSummary -> RenderUnit.CompactSummary(id, text, id)
            Speaker.Interrupt -> RenderUnit.Interrupt(id, r.timeText, id)
            else -> null
        }
    }

    private fun reply(
        r: LineRecord.Reply,
        results: Map<String, Result>,
    ): List<RenderUnit> {
        val id = r.id
        return r.blocks.mapIndexedNotNull { i, b ->
            val key = "$id#$i"
            when (b) {
                is Block.Text -> b.text.takeIf { it.isNotBlank() }?.let { RenderUnit.AssistantMarkdown(key, it, id) }
                is Block.Thinking -> RenderUnit.Thinking(key, b.text, id)
                is Block.ToolUse -> toolCall(r, i, b, results)
                else -> null
            }
        }
    }

    private fun toolCall(
        r: LineRecord.Reply,
        i: Int,
        b: Block.ToolUse,
        results: Map<String, Result>,
    ): RenderUnit.ToolCall {
        val callId = b.id
        val res = results[callId]
        val step = r.steps?.get(callId)
        return RenderUnit.ToolCall(
            key = "${r.id}#$i",
            toolUseId = callId,
            name = b.name,
            input = inputOf(b.input),
            resultText = res?.text,
            isError = res?.isError == true,
            pending = res == null,
            sourceUuid = r.id,
            structuredPatch = res?.patch,
            patchFilePath = res?.file,
            summary = (step?.arg ?: step?.text).orEmpty(),
            resultLine = res?.line,
            interactive = r.cards?.get(callId) == ToolCard.INTERACTIVE,
        )
    }

    /** 人说的那一句里的图片块：来源认得出的那一形（base64 ＋ 图片类型）才收，别的画不出、不收（同桌面）。 */
    private fun imagesOf(blocks: List<Block>): List<SaidImage> =
        blocks.filterIsInstance<Block.Image>().mapNotNull { b ->
            val m = inputOf(b.source).takeIf { it["type"] == "base64" }
            val type = (m?.get("media_type") as? String)?.takeIf { it.startsWith("image/") }
            val data = m?.get("data") as? String
            if (type != null && data != null) SaidImage(type, data) else null
        }

    /** 工具入参原样（形状照那一家，不解）：是一团键值 ⇒ 交卡片；别的 ⇒ 空。 */
    private fun inputOf(raw: Raw): Map<String, Any?> {
        val m = JsonReader.of(Buffer().writeUtf8(raw.json)).readJsonValue() as? Map<*, *> ?: return emptyMap()
        return m.entries.mapNotNull { (k, x) -> (k as? String)?.let { it to x } }.toMap()
    }
}
