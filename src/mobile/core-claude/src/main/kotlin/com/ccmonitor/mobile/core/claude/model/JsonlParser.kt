package com.ccmonitor.mobile.core.claude.model

import com.squareup.moshi.Moshi

/**
 * 容错解析 Claude Code 会话 JSONL。
 *
 * parse 永不抛异常：坏行/缺字段/类型不符 → [JsonlRecord.Unknown]。tail 到半截行或 Claude 改 schema 都不拖垮渲染管线。
 */
object JsonlParser : RecordParser {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    override fun parse(line: String): JsonlRecord {
        val trimmed = line.trim()
        if (trimmed.isEmpty()) return JsonlRecord.Unknown("(blank)")
        val obj =
            runCatching { anyAdapter.fromJson(trimmed) as? Map<*, *> }.getOrNull()
                ?: return JsonlRecord.Unknown("(unparseable)", rawJson = trimmed)
        // 逃生舱：fromMap 抛了也从已解析的 obj 抽 uuid/parentUuid/timestamp（参与链，不让其后的对话变孤儿）并存原始行。
        return runCatching { fromMap(obj, trimmed) }.getOrElse {
            JsonlRecord.Unknown("(error)", obj.str("uuid"), obj.str("parentUuid"), obj.str("timestamp"), trimmed)
        }
    }

    private fun fromMap(
        o: Map<*, *>,
        raw: String,
    ): JsonlRecord =
        when (val type = o.str("type")) {
            "user" ->
                JsonlRecord.User(
                    uuid = o.str("uuid"),
                    parentUuid = o.str("parentUuid"),
                    blocks = parseContent(o["message"]),
                    isMeta = o["isMeta"] == true,
                    timestamp = o.str("timestamp"),
                    sessionId = o.str("sessionId"),
                    cwd = o.str("cwd"),
                    isSidechain = o["isSidechain"] == true,
                    forkedFrom = parseForkedFrom(o["forkedFrom"]),
                    structuredPatch = parseStructuredPatch(o["toolUseResult"]),
                    patchFilePath = (o["toolUseResult"] as? Map<*, *>)?.str("filePath"), // diff 卡片显示文件名
                )
            "assistant" -> {
                val msg = o["message"] as? Map<*, *>
                JsonlRecord.Assistant(
                    uuid = o.str("uuid"),
                    parentUuid = o.str("parentUuid"),
                    blocks = parseContent(o["message"]),
                    model = msg?.str("model"),
                    timestamp = o.str("timestamp"),
                    sessionId = o.str("sessionId"),
                    isApiError = o["isApiErrorMessage"] == true,
                    stopReason = msg?.str("stop_reason"), // turn-end 检测
                    isSidechain = o["isSidechain"] == true,
                    usage = parseUsage(msg?.get("usage")),
                    requestId = o.str("requestId"), // 用量按 requestId 去重
                )
            }
            "system" ->
                JsonlRecord.System(
                    uuid = o.str("uuid"),
                    parentUuid = o.str("parentUuid"),
                    subtype = o.str("subtype"),
                    level = o.str("level"),
                    isMeta = o["isMeta"] == true,
                    timestamp = o.str("timestamp"),
                )
            "ai-title" -> JsonlRecord.Title(o.str("aiTitle") ?: "", o.str("sessionId"))
            "custom-title" -> JsonlRecord.Title(o.str("customTitle") ?: "", o.str("sessionId"), custom = true)
            "attachment" -> JsonlRecord.Attachment(o.str("uuid"), o.str("parentUuid"), o.str("timestamp"))
            // 逃生舱：未知/无 type 也保 uuid/parentUuid/timestamp（有则参与链）+ 原始行。
            null -> JsonlRecord.Unknown("(no-type)", o.str("uuid"), o.str("parentUuid"), o.str("timestamp"), raw)
            else -> JsonlRecord.Unknown(type, o.str("uuid"), o.str("parentUuid"), o.str("timestamp"), raw)
        }

    /** `forkedFrom` = `{sessionId, messageUuid}`（缺任一 → null）。 */
    private fun parseForkedFrom(raw: Any?): ForkedFrom? {
        val m = raw as? Map<*, *> ?: return null
        val sid = m.str("sessionId") ?: return null
        val mu = m.str("messageUuid") ?: return null
        return ForkedFrom(sid, mu)
    }

    /** `message.usage` → [Usage]（映射 `*_tokens` + `cache_creation.ephemeral_*` 拆分，缺字段计 0）。 */
    private fun parseUsage(raw: Any?): Usage? {
        val m = raw as? Map<*, *> ?: return null

        fun tok(
            map: Map<*, *>,
            k: String,
        ): Long = (map[k] as? Number)?.toLong() ?: 0L
        val cc = m["cache_creation"] as? Map<*, *>
        return Usage(
            input = tok(m, "input_tokens"),
            output = tok(m, "output_tokens"),
            cacheCreation = tok(m, "cache_creation_input_tokens"),
            cacheRead = tok(m, "cache_read_input_tokens"),
            cacheCreation5m = cc?.let { tok(it, "ephemeral_5m_input_tokens") } ?: 0L,
            cacheCreation1h = cc?.let { tok(it, "ephemeral_1h_input_tokens") } ?: 0L,
        )
    }

    /** `toolUseResult.structuredPatch[]` → [PatchHunk] 列表（无 toolUseResult / 无该字段 → null；空数组 → 空 list）。 */
    private fun parseStructuredPatch(toolUseResult: Any?): List<PatchHunk>? {
        val tur = toolUseResult as? Map<*, *> ?: return null
        val arr = tur["structuredPatch"] as? List<*> ?: return null
        return arr.mapNotNull { h ->
            val m = h as? Map<*, *> ?: return@mapNotNull null

            fun num(k: String): Int = (m[k] as? Number)?.toInt() ?: 0
            val lines = (m["lines"] as? List<*>)?.mapNotNull { it as? String } ?: emptyList()
            PatchHunk(num("oldStart"), num("oldLines"), num("newStart"), num("newLines"), lines)
        }
    }

    /** message.content 可能是裸字符串(简单用户输入)或块数组。 */
    private fun parseContent(message: Any?): List<ContentBlock> {
        val msg = message as? Map<*, *> ?: return emptyList()
        when (val content = msg["content"]) {
            is String -> return if (content.isEmpty()) emptyList() else listOf(ContentBlock.Text(content))
            is List<*> -> return content.mapNotNull { parseBlock(it) }
            else -> return emptyList()
        }
    }

    private fun parseBlock(raw: Any?): ContentBlock? {
        val b = raw as? Map<*, *> ?: return null
        return when (val t = b.str("type")) {
            "text" -> ContentBlock.Text(b.str("text") ?: "")
            "thinking" -> ContentBlock.Thinking(b.str("thinking") ?: "")
            "tool_use" ->
                ContentBlock.ToolUse(
                    id = b.str("id") ?: "",
                    name = b.str("name") ?: "",
                    input =
                        (b["input"] as? Map<*, *>)?.let { m ->
                            m.entries.associate { (k, v) -> k.toString() to v }
                        } ?: emptyMap(),
                )
            "tool_result" ->
                ContentBlock.ToolResult(
                    toolUseId = b.str("tool_use_id"),
                    text = flattenToolResult(b["content"]),
                    isError = b["is_error"] == true,
                )
            null -> null
            else -> ContentBlock.Unknown(t)
        }
    }

    /** tool_result.content 同样可能是字符串或 [{type:text,text:...}] 数组。 */
    private fun flattenToolResult(content: Any?): String =
        when (content) {
            is String -> content
            is List<*> -> content.joinToString("\n") { (it as? Map<*, *>)?.str("text") ?: "" }
            else -> ""
        }

    private fun Map<*, *>.str(key: String): String? = this[key] as? String
}
