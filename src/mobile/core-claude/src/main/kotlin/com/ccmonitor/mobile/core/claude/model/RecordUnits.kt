package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.num
import com.ccmonitor.mobile.core.claude.link.obj
import com.ccmonitor.mobile.core.claude.link.objs
import com.ccmonitor.mobile.core.claude.link.str

/**
 * 核心的通用记录（`IPC-PROTOCOL.md`「通用记录」）⇒ 渲染单元。只按 `t` · `speaker.kind` · 块的 `type` 排，
 * 不认任何一家的盘上格式；字照记录里写好的抄。哪一类画成什么照 `界面.md`「一条东西画成什么」，
 * 那张表里画成事件条的几类（交回 · 后台任务 · 来话 …）还没出稿，先不上屏。
 */
object RecordUnits {
    fun of(records: List<Map<String, Any?>>): List<RenderUnit> {
        val results = records.flatMap(::resultsOf).toMap()
        return records.flatMap { unitsOf(it, results) }
    }

    /** 一个工具调用配上的结果：正文 · 出错 · 核心给的 patch。 */
    private class Result(
        val text: String,
        val isError: Boolean,
        val patch: List<PatchHunk>?,
        val file: String?,
    )

    private fun resultsOf(r: Map<String, Any?>): List<Pair<String, Result>> {
        if (r.str("t") != "said") return emptyList()
        val said = r.obj("results").orEmpty()
        return blocks(r).filter { it.str("type") == "tool_result" }.mapNotNull { b ->
            val id = b.str("for") ?: return@mapNotNull null
            val text =
                b
                    .objs("content")
                    .orEmpty()
                    .mapNotNull { it.takeIf { c -> c.str("type") == "text" }?.str("text") }
                    .joinToString("\n")
            val one = said.obj(id)
            id to Result(text, b["isError"] == true, one?.objs("patch")?.map(::hunk), one?.str("file"))
        }
    }

    private fun hunk(h: Map<String, Any?>): PatchHunk =
        PatchHunk(
            oldStart = h.num("oldStart")?.toInt() ?: 0,
            oldLines = h.num("oldLines")?.toInt() ?: 0,
            newStart = h.num("newStart")?.toInt() ?: 0,
            newLines = h.num("newLines")?.toInt() ?: 0,
            lines = (h["lines"] as? List<*>)?.filterIsInstance<String>().orEmpty(),
        )

    private fun unitsOf(
        r: Map<String, Any?>,
        results: Map<String, Result>,
    ): List<RenderUnit> {
        val id = r.str("id") ?: return emptyList()
        return when (r.str("t")) {
            "said", "queued" -> listOfNotNull(said(id, r))
            "reply" -> if (r["autoReply"] == true) emptyList() else reply(id, r, results)
            else -> emptyList()
        }
    }

    private fun said(
        id: String,
        r: Map<String, Any?>,
    ): RenderUnit? {
        val who = r.obj("who") ?: return null
        val speaker = who.obj("speaker").orEmpty()
        val text = who.str("text").orEmpty()
        return when (speaker.str("kind")) {
            "human" -> RenderUnit.UserText(id, text, id)
            "slashCommand" -> RenderUnit.SlashCommand(id, speaker.str("name").orEmpty(), speaker.str("args").orEmpty(), id)
            "bashInput" -> RenderUnit.BashInput(id, speaker.str("command").orEmpty(), id)
            "bashOutput" -> RenderUnit.BashOutput(id, speaker.str("stdout").orEmpty(), speaker.str("stderr").orEmpty(), id)
            "compactSummary" -> RenderUnit.CompactSummary(id, text, id)
            "interrupt" -> RenderUnit.Interrupt(id, r.str("timeText"), id)
            else -> null
        }
    }

    private fun reply(
        id: String,
        r: Map<String, Any?>,
        results: Map<String, Result>,
    ): List<RenderUnit> =
        blocks(r).mapIndexedNotNull { i, b ->
            val key = "$id#$i"
            when (b.str("type")) {
                "text" -> b.str("text")?.takeIf { it.isNotBlank() }?.let { RenderUnit.AssistantMarkdown(key, it, id) }
                "thinking" -> b.str("text")?.let { RenderUnit.Thinking(key, it, id) }
                "tool_use" -> {
                    val callId = b.str("id").orEmpty()
                    val res = results[callId]
                    RenderUnit.ToolCall(
                        key = key,
                        toolUseId = callId,
                        name = b.str("name").orEmpty(),
                        input = b.obj("input").orEmpty(),
                        resultText = res?.text,
                        isError = res?.isError == true,
                        pending = res == null,
                        sourceUuid = id,
                        structuredPatch = res?.patch,
                        patchFilePath = res?.file,
                    )
                }
                else -> null
            }
        }

    private fun blocks(r: Map<String, Any?>): List<Map<String, Any?>> = r.objs("blocks").orEmpty()
}
