package com.ccmonitor.mobile.core.claude.model

import com.squareup.moshi.Moshi

/**
 * 容错解析 Codex CLI rollout JSONL，产出与 Claude 同一套 [JsonlRecord] 模型。
 *
 * 信封 `{"timestamp","type","payload":{...}}`，解开 `.payload`。顶层 `type` ∈ session_meta / turn_context /
 * world_state / response_item / event_msg。渲染只用 response_item；事件与元数据落 [JsonlRecord.Unknown]
 * 并保留 `rawJson`，turn-end 与用量从里面读。
 *
 * 格式没有文档、每几版就变，所以尽量防御：[parse] 永不抛，字段全部可缺。只解析已解压的行。
 */
object CodexRecordParser : RecordParser {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    override fun parse(raw: String): JsonlRecord {
        val trimmed = raw.trim()
        if (trimmed.isEmpty()) return JsonlRecord.Unknown("(blank)")
        val env =
            runCatching { anyAdapter.fromJson(trimmed) as? Map<*, *> }.getOrNull()
                ?: return JsonlRecord.Unknown("(unparseable)", rawJson = trimmed)
        return runCatching { fromEnvelope(env, trimmed) }.getOrElse {
            JsonlRecord.Unknown("(error)", rawJson = trimmed)
        }
    }

    private fun fromEnvelope(
        env: Map<*, *>,
        raw: String,
    ): JsonlRecord {
        val topType = env.str("type")
        val ts = env.str("timestamp")
        val payload = env["payload"] as? Map<*, *>
        return when (topType) {
            "response_item" -> if (payload != null) fromResponseItem(payload, ts, raw) else JsonlRecord.Unknown("response_item", rawJson = raw)
            // 事件/元数据不入渲染，保 rawJson 给 turn-end/用量/定位读。
            "session_meta", "turn_context", "world_state", "event_msg" -> JsonlRecord.Unknown(topType, timestamp = ts, rawJson = raw)
            null -> JsonlRecord.Unknown("(no-type)", timestamp = ts, rawJson = raw)
            else -> JsonlRecord.Unknown(topType, timestamp = ts, rawJson = raw)
        }
    }

    /** response_item.payload.type → JsonlRecord（message/reasoning/custom_tool_call[_output]，也容忍 function_call[_output]）。 */
    private fun fromResponseItem(
        p: Map<*, *>,
        ts: String?,
        raw: String,
    ): JsonlRecord {
        val id = p.str("id")
        return when (val t = p.str("type")) {
            "message" -> fromMessage(p, id, ts)
            "reasoning" -> {
                // reasoning.summary 常为 []（只有 encrypted_content），空文本给空 blocks，免得渲染一串空 thinking。
                val think = reasoningText(p)
                val blocks = if (think.isEmpty()) emptyList() else listOf(ContentBlock.Thinking(think))
                JsonlRecord.Assistant(id, null, blocks, null, ts, null, isApiError = false)
            }
            "custom_tool_call", "function_call" ->
                JsonlRecord.Assistant(
                    uuid = id,
                    parentUuid = null,
                    blocks = listOf(ContentBlock.ToolUse(id = p.str("call_id") ?: "", name = p.str("name") ?: "", input = toolInput(p["input"]))),
                    model = null,
                    timestamp = ts,
                    sessionId = null,
                    isApiError = false,
                )
            "custom_tool_call_output", "function_call_output" ->
                JsonlRecord.User(
                    uuid = id,
                    parentUuid = null,
                    blocks = listOf(ContentBlock.ToolResult(toolUseId = p.str("call_id"), text = outputText(p["output"]), isError = false)),
                    isMeta = false,
                    timestamp = ts,
                    sessionId = null,
                    cwd = null,
                )
            null -> JsonlRecord.Unknown("response_item", timestamp = ts, rawJson = raw)
            else -> JsonlRecord.Unknown("response_item:$t", timestamp = ts, rawJson = raw)
        }
    }

    /** message{role, content:[{type:input_text|output_text, text}]} → User(user)/Assistant(assistant)/User+isMeta(developer)。 */
    private fun fromMessage(
        p: Map<*, *>,
        id: String?,
        ts: String?,
    ): JsonlRecord {
        val text = contentText(p["content"])
        val blocks = if (text.isEmpty()) emptyList() else listOf(ContentBlock.Text(text))
        return when (p.str("role")) {
            "assistant" -> JsonlRecord.Assistant(id, null, blocks, null, ts, null, isApiError = false)
            // developer = 系统指令/元——落 User(isMeta=true)（保文本 + 渲染侧当 meta 隐藏，同 Claude isMeta）。
            "developer" -> JsonlRecord.User(id, null, blocks, isMeta = true, timestamp = ts, sessionId = null, cwd = null)
            // user：真用户输入 isMeta=false；注入上下文（environment/plugins/AGENTS.md）isMeta=true 隐藏，见 [isInjectedUserContext]。
            else -> JsonlRecord.User(id, null, blocks, isMeta = isInjectedUserContext(text), timestamp = ts, sessionId = null, cwd = null)
        }
    }

    /**
     * Codex `role=user` 但正文是注入上下文（不是真用户输入）→ true，渲染侧当 meta 隐藏。
     * 注入都以固定前缀起头：`<environment_context>`、`<recommended_plugins>`、AGENTS.md 注入头
     * （`# AGENTS.md instructions` 加机器生成的 `\n\n<INSTRUCTIONS>`）。模糊的裸标记不匹配，免得误伤用户正文。
     */
    private fun isInjectedUserContext(text: String): Boolean {
        val t = text.trimStart()
        return t.startsWith("<environment_context>") ||
            t.startsWith("<recommended_plugins>") ||
            t.startsWith("# AGENTS.md instructions")
    }

    /**
     * content=[{type:input_text|output_text, text}]（或裸串防御）→ 拼文本。
     * 先丢掉无 text/空的项再拼，否则 image 等非文本项会夹出多余空行。
     */
    private fun contentText(content: Any?): String =
        when (content) {
            is String -> content
            is List<*> ->
                content
                    .mapNotNull { (it as? Map<*, *>)?.str("text") ?: (it as? String) }
                    .filter { it.isNotEmpty() }
                    .joinToString("\n")
                    .trim()
            else -> ""
        }

    /** reasoning.summary（array of {text} 或裸串）→ 拼；空（仅 encrypted_content）→ ""。 */
    private fun reasoningText(p: Map<*, *>): String {
        val summary = p["summary"]
        return when (summary) {
            is String -> summary
            is List<*> -> summary.joinToString("\n") { (it as? Map<*, *>)?.str("text") ?: (it as? String ?: "") }.trim()
            else -> ""
        }
    }

    /** tool_call.input：Map→原样、String→包 {input}、其它→空（保 ToolUse.name 可见）。 */
    private fun toolInput(input: Any?): Map<String, Any?> =
        when (input) {
            is Map<*, *> -> input.entries.associate { (k, v) -> k.toString() to v }
            is String -> mapOf("input" to input)
            else -> emptyMap()
        }

    /**
     * tool_call_output.output → 文本。output 通常是数组 `[{type:input_text,text}]`（同 message content），
     * 由 [contentText] 兜 String/List；Map 取 content 或 toString。
     */
    private fun outputText(output: Any?): String =
        when (output) {
            is Map<*, *> -> output.str("content") ?: output.toString()
            else -> contentText(output) // String 原样 / List[{type,text}] 拼 / 其它 ""
        }

    private fun Map<*, *>.str(key: String): String? = this[key] as? String
}
