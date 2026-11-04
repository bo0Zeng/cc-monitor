package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Block
import com.ccmonitor.mobile.core.wire.gen.Draw
import com.ccmonitor.mobile.core.wire.gen.LineDiff
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.Speaker
import com.ccmonitor.mobile.core.wire.gen.StepBody
import com.ccmonitor.mobile.core.wire.gen.ToolCard

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

    /** 上次交出去的那一份；之后有变 ⇒ `null`（下次再拼）。 */
    private var last: List<RenderUnit>? = null

    fun isEmpty(): Boolean = rows.isEmpty()

    /** 手上几条记录。 */
    val size: Int get() = rows.size

    /** 此刻的整张单元表（只拼，不排）；没变 ⇒ 上次那一份原样（同一个实例）。拼是 O(手上的行数)：手上留多少由持有者截（见手机端按页丢）。 */
    fun units(): List<RenderUnit> = last ?: rows.flatMapTo(ArrayList(rows.size * 2)) { it.units }.also { last = it }

    /** 丢最前面 [n] 条（往上翻读进来、离视口远了的那几页）：它们的调用卡与结果一并忘掉；后面那几条原样留着。 */
    fun dropFirst(n: Int) {
        val gone = rows.subList(0, n.coerceAtMost(rows.size))
        for (row in gone) {
            resultsOf(row.record).forEach { (id, _) -> results.remove(id) }
            (row.record as? LineRecord.Reply)?.blocks?.forEach { b -> if (b is Block.ToolUse) calls.remove(b.id) }
        }
        gone.clear()
        last = null
    }

    /** 接在后面。 */
    fun append(rs: List<LineRecord>) = place(rs, rows.size)

    /** 往上翻读到的更早那一段：接在前面。 */
    fun prepend(rs: List<LineRecord>) = place(rs, 0)

    /** 整份换成 [rs]。 */
    fun reset(rs: List<LineRecord>) {
        rows.clear()
        results.clear()
        calls.clear()
        last = null
        append(rs)
    }

    /** 先换已有的、配上了新结果的调用卡，再排新来的（照整张结果表排）。 */
    private fun place(
        rs: List<LineRecord>,
        at: Int,
    ) {
        if (rs.isEmpty()) return
        last = null
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

    /** 一个工具调用配上的结果：正文 · 出错 · 核心算好的行差（`StepResult.patch`）· 结果那一句（`StepResult.text`）。 */
    private class Result(
        val text: String,
        val isError: Boolean,
        val patch: LineDiff?,
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
            id to Result(text, b.isError, one?.patch, one?.file, one?.text?.takeIf { it.isNotEmpty() })
        }
    }

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
            Speaker.Human ->
                RenderUnit.UserText(
                    id,
                    text,
                    id,
                    images = imagesOf(r.blocks, id),
                    attachments = r.attachments.orEmpty(),
                    parts = r.who.pasted?.takeIf { it.isNotEmpty() }?.let { saidParts(text, it) }.orEmpty(),
                )
            is Speaker.SlashCommand -> RenderUnit.SlashCommand(id, s.name, s.args, id)
            is Speaker.BashInput -> RenderUnit.BashInput(id, s.command, id)
            is Speaker.BashOutput -> RenderUnit.BashOutput(id, s.stdout, s.stderr, id)
            is Speaker.CompactSummary -> RenderUnit.CompactSummary(id, text, id)
            Speaker.Interrupt -> RenderUnit.Interrupt(id, r.timeText, id)
            Speaker.AgentTask -> RenderUnit.Brief(id, text, r.timeText, id)
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
                is Block.Prose -> b.doc.takeIf { it.blocks.isNotEmpty() }?.let { RenderUnit.AssistantMarkdown(key, it, b.more, id) }
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
            resultText = res?.text,
            isError = res?.isError == true,
            pending = res == null,
            sourceUuid = r.id,
            // 结果带着行差（真行号）就画结果那份，否则画调用那份（与桌面同一条）。
            diff = res?.patch ?: (step?.body as? StepBody.Changes)?.diff,
            patchFilePath = res?.file,
            summary = (step?.arg ?: step?.text).orEmpty(),
            resultLine = res?.line,
            interactive = r.cards?.get(callId) == ToolCard.INTERACTIVE,
        )
    }

    /** 人说的那一句里的图片块（画不出的来源那台已经不出块）：引用照收，记下是哪一条的。 */
    private fun imagesOf(
        blocks: List<Block>,
        record: String,
    ): List<SaidImage> = blocks.filterIsInstance<Block.Image>().map { SaidImage(it.mediaType, it.at, it.n, record) }
}
