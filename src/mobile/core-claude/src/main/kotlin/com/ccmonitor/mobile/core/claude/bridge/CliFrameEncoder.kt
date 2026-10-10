package com.ccmonitor.mobile.core.claude.bridge

/**
 * CLI 原生 stream-json → 紧凑帧。
 *
 * 字段名以 `bridge/tools/reference_encoder.py`（吃 SDK 消息的那份编码器）为准，本类是它的 Kotlin 孪生。
 * 两边读的形状不同：CLI 按 `type`+`subtype` 分派，`init` 字段在顶层，`assistant` 嵌在 `message` 下且用 `id`，
 * 内容块用 `type: text/thinking/tool_use`，`result` 里是驼峰的 `modelUsage`。
 * `stream_event` 两侧逐字节相同，增量帧（`bs`/`d`/`td`/`ij`/`be`）的映射照搬。
 *
 * 有状态：`message_start` 是唯一能拿到 message_id 的地方；块序号按 message_id 自己数（见 [nextBlockIndex]）。
 */
class CliFrameEncoder {
    /**
     * 当前 message_id，按 `parent_tool_use_id` 分桶（键 `null` = 主流）。
     *
     * 共用一格的话，子 agent 与主流交错到达时，子 agent 的 `message_start` 会覆盖主流的 id，
     * 主流增量被标上子 agent 的 `m`，上层以为换了消息，主块已露出的正文清零重来。
     */
    private val messageIdByParent = HashMap<String?, String?>()
    private val delivered = HashMap<String, Int>()

    /** 喂一行 CLI stream-json（已解析成 Map）。返回它映射出的帧（可能 0..n 条）。 */
    fun feed(line: Map<*, *>): List<BridgeFrame> =
        when (line["type"] as? String) {
            "system" -> system(line)
            "stream_event" -> stream(line)
            "assistant" -> assistant(line)
            "user" -> user(line)
            "result" -> listOf(result(line))
            "rate_limit_event" -> listOf(BridgeFrame.RateLimit(rest(line, "type")))
            // 未知即透传：不崩、不丢，留原文。
            else -> listOf(ev("cli:${line["type"]}", rest(line, "type")))
        }

    // ---- 信封类：各自拆包 ----------------------------------------------------

    /** CLI 的 `init` 字段在顶层（SDK 嵌在 `data` 下），字段名两侧一致。 */
    private fun system(line: Map<*, *>): List<BridgeFrame> {
        val sub = line["subtype"] as? String
        if (sub != "init") return listOf(ev("system:$sub", rest(line, "type", "subtype")))
        val taken =
            listOf(
                "type",
                "subtype",
                "session_id",
                "model",
                "cwd",
                "slash_commands",
                "skills",
                "tools",
                "mcp_servers",
                "agents",
                "plugins",
            )
        return listOf(
            BridgeFrame.Init(
                sid = line["session_id"] as? String,
                model = line["model"] as? String,
                cwd = line["cwd"] as? String,
                cmds = strings(line["slash_commands"]),
                skills = strings(line["skills"]),
                tools = strings(line["tools"]),
                agents = strings(line["agents"]),
                mcp = mcpServers(line["mcp_servers"]),
                plugins = strings(line["plugins"]),
                // init 丢了就再也拿不到，剩余字段全留着。
                raw = restExcept(line, taken),
            ),
        )
    }

    /** `stream_event` 两侧逐字节相同，这段与参考编码器一致。 */
    private fun stream(line: Map<*, *>): List<BridgeFrame> {
        val e = line["event"] as? Map<*, *> ?: return listOf(ev("stream:?", null))
        // 子 agent（Task）的归属在外层行上，与 `event` 同级。
        // 不读它，子 agent 的增量会按主流块路由，主块已露出的正文被清零重来。
        val p = line["parent_tool_use_id"] as? String
        return when (val et = e["type"] as? String) {
            "message_start" -> {
                // 唯一能拿到 message_id 的地方。
                messageIdByParent[p] = ((e["message"] as? Map<*, *>)?.get("id")) as? String
                listOf(ev("stream:message_start", e))
            }
            "content_block_start" -> listOf(blockStart(e, p))
            "content_block_delta" -> listOf(blockDelta(e, p))
            // `be` 不保证到达，只是渲染时机提示；正文以 `at`/`tt` 为准，turn 状态以 `res` 为准。
            "content_block_stop" -> listOf(BridgeFrame.BlockEnd(messageIdByParent[p], int(e["index"]) ?: 0, p))
            else -> listOf(ev("stream:$et", e))
        }
    }

    private fun blockStart(
        e: Map<*, *>,
        p: String?,
    ): BridgeFrame {
        val blk = e["content_block"] as? Map<*, *> ?: emptyMap<Any, Any>()
        return BridgeFrame.BlockStart(
            m = messageIdByParent[p],
            i = int(e["index"]) ?: 0,
            bt = blk["type"] as? String,
            id = blk["id"] as? String,
            name = blk["name"] as? String,
            p = p,
        )
    }

    /** 按子类型取值；`signature_delta`/`thinking_delta` 没有 `text`。 */
    private fun blockDelta(
        e: Map<*, *>,
        p: String?,
    ): BridgeFrame {
        val d = e["delta"] as? Map<*, *> ?: emptyMap<Any, Any>()
        val i = int(e["index"]) ?: 0
        return when (val dt = d["type"] as? String) {
            "text_delta" -> BridgeFrame.TextDelta(messageIdByParent[p], i, d["text"] as? String ?: "", p)
            "thinking_delta" -> BridgeFrame.ThinkingDelta(messageIdByParent[p], i, d["thinking"] as? String ?: "", p)
            "input_json_delta" -> BridgeFrame.InputJsonDelta(messageIdByParent[p], i, d["partial_json"] as? String ?: "", p)
            else -> ev("delta:$dt", d)
        }
    }

    /** CLI 的 assistant 嵌在 `message` 下，message_id 的键是 `id`。 */
    private fun assistant(line: Map<*, *>): List<BridgeFrame> {
        val msg = line["message"] as? Map<*, *> ?: emptyMap<Any, Any>()
        // 先读归属：下面校正 message_id 要按它分桶，否则子 agent 的全文帧会覆盖主流的 id。
        val parent = line["parent_tool_use_id"] as? String
        val m = (msg["id"] as? String) ?: messageIdByParent[parent]
        if (msg["id"] is String) messageIdByParent[parent] = msg["id"] as String
        val out = mutableListOf<BridgeFrame>()
        for (raw in (msg["content"] as? List<*>).orEmpty()) {
            val blk = raw as? Map<*, *>
            val idx = nextBlockIndex(m)
            out += contentBlock(blk, raw, m, idx, parent)
        }
        // 认证失败在这里暴露，result 的 subtype 仍是 'success'。
        (line["error"] ?: msg["error"])?.let { out += BridgeFrame.Err(m, it.toString()) }
        // 不返回空列表：`content` 为空且无 error 时落一个 `ev`，不让整行静默消失。
        return out.ifEmpty { listOf(ev("assistant:empty", restExcept(line, listOf("type")))) }
    }

    /** `m` + `i` 是全文帧覆盖哪个块的唯一依据。 */
    private fun contentBlock(
        blk: Map<*, *>?,
        raw: Any?,
        m: String?,
        idx: Int,
        parent: String?,
    ): BridgeFrame =
        when (blk?.get("type") as? String) {
            "text" -> BridgeFrame.AssistantText(m, idx, parent, blk["text"] as? String ?: "")
            "thinking" -> BridgeFrame.ThinkingText(m, idx, parent, blk["thinking"] as? String ?: "")
            "tool_use" ->
                BridgeFrame.ToolUse(
                    m,
                    idx,
                    parent,
                    blk["id"] as? String,
                    blk["name"] as? String,
                    (blk["input"] as? Map<*, *>)?.let { rest(it) },
                )
            // 元素不是 dict 也不崩。
            else -> ev("block:${blk?.get("type")}", blk ?: mapOf("v" to raw.toString()), i = idx)
        }

    private fun user(line: Map<*, *>): List<BridgeFrame> {
        val msg = line["message"] as? Map<*, *> ?: emptyMap<Any, Any>()
        val parent = line["parent_tool_use_id"] as? String
        return when (val content = msg["content"]) {
            is String -> listOf(BridgeFrame.UserText(parent, content))
            is List<*> -> content.map { toolResultOrEv(it, parent) }.ifEmpty { listOf(ev("user:empty", null)) }
            else -> listOf(ev("user:unknown-content", mapOf("v" to content.toString())))
        }
    }

    private fun toolResultOrEv(
        raw: Any?,
        parent: String?,
    ): BridgeFrame {
        val blk = raw as? Map<*, *> ?: return ev("user-block:non-dict", mapOf("v" to raw.toString()))
        if (blk["type"] != "tool_result") return ev("user-block:${blk["type"]}", blk)
        val content = blk["content"]
        // `content` 常是数组（MCP 工具、ToolSearch、读图），序列化成 JSON 再上屏；
        // `toString()` 会得到 Kotlin map 字面量。
        val text = content as? String ?: JSON.toJson(content)
        val truncated = text.length > TR_MAX_CHARS
        return BridgeFrame.ToolResult(
            id = blk["tool_use_id"] as? String,
            p = parent,
            // 与 `res.ok` 同名同极性。
            ok = blk["is_error"] != true,
            x = if (truncated) text.take(TR_MAX_CHARS) else text,
            // 全文长度：截断后 bridge 侧另落 `att/tr-<id>.json`，「展开全文」按需拉。
            truncatedFullLength = if (truncated) text.length else null,
        )
    }

    /** CLI 的 `modelUsage` 是驼峰，SDK 是 `model_usage`，两个都认。 */
    private fun result(line: Map<*, *>): BridgeFrame {
        val taken =
            listOf(
                "type",
                "subtype",
                "session_id",
                "is_error",
                "total_cost_usd",
                "num_turns",
                "terminal_reason",
                "api_error_status",
                "duration_ms",
                "usage",
                "permission_denials",
                "model_usage",
                "modelUsage",
            )
        return BridgeFrame.Result(
            sid = line["session_id"] as? String,
            // 取 `!is_error`，不看 subtype：未登录时 subtype 仍是 success。
            ok = line["is_error"] != true,
            cost = (line["total_cost_usd"] as? Number)?.toDouble(),
            turns = int(line["num_turns"]),
            why = line["terminal_reason"] as? String,
            api = line["api_error_status"]?.toString(),
            dur = (line["duration_ms"] as? Number)?.toLong(),
            usage = (line["usage"] as? Map<*, *>)?.let { rest(it) },
            // 工具被拒的唯一数据落点。
            den = (line["permission_denials"] as? List<*>)?.takeIf { it.isNotEmpty() },
            mu = ((line["model_usage"] ?: line["modelUsage"]) as? Map<*, *>)?.let { rest(it) },
            raw = restExcept(line, taken),
        )
    }

    // ---- 小工具 --------------------------------------------------------------

    /**
     * 同一个 message_id 下，assistant 消息按块顺序逐条到达，与流里 `bs` 的 index 0,1,2… 一一对应。
     * 不能用 `content` 的下标：每个块单独发一条，下标恒为 0，thinking 块与 text 块会撞键。
     */
    private fun nextBlockIndex(m: String?): Int {
        val key = m.orEmpty()
        val idx = delivered[key] ?: 0
        delivered[key] = idx + 1
        return idx
    }

    private fun ev(
        k: String,
        raw: Map<*, *>?,
        i: Int? = null,
    ) = BridgeFrame.Event(k = k, i = i, raw = raw?.let { m -> rest(m) })

    /** 未被特化字段吸收的剩余部分；全被吸收 → null。 */
    private fun rest(
        d: Map<*, *>,
        vararg taken: String,
    ): Map<String, Any?>? = restExcept(d, taken.toList())

    private fun restExcept(
        d: Map<*, *>,
        taken: List<String>,
    ): Map<String, Any?>? =
        d.entries
            .mapNotNull { (k, v) -> (k as? String)?.takeIf { it !in taken }?.let { it to v } }
            .toMap()
            .takeIf { it.isNotEmpty() }

    private fun strings(v: Any?): List<String> = (v as? List<*>)?.mapNotNull { it as? String } ?: emptyList()

    private fun mcpServers(v: Any?): List<BridgeFrame.McpServer> =
        (v as? List<*>).orEmpty().mapNotNull { item ->
            val m = item as? Map<*, *> ?: return@mapNotNull null
            // 三格原样透传，不折叠、不填默认值。
            (m["name"] as? String)?.let {
                BridgeFrame.McpServer(it, m["status"] as? String, m["source"] as? String)
            }
        }

    private fun int(v: Any?): Int? = (v as? Number)?.toInt()

    companion object {
        private val JSON =
            com.squareup.moshi.Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)

        /**
         * `tr.x` 截断阈值：一轮的体积大头是 tool_result 而不是 delta。
         * 与 `reference_encoder.py` 的 `TR_MAX_CHARS` 必须一致。
         */
        const val TR_MAX_CHARS = 4096
    }
}
