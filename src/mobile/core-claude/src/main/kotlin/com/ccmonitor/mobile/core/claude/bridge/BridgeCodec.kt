package com.ccmonitor.mobile.core.claude.bridge

import com.squareup.moshi.Moshi

/**
 * NDJSON 一行 → [BridgeFrame]。
 *
 * - 未知帧型不丢，落 [BridgeFrame.Unknown] 带原始 map：远端比手机新是常态。
 * - 字段缺失不崩，用默认值。
 * - 数字一律经 `Number` 转：Moshi 的 `Any` 适配器把所有数字读成 `Double`，直接 `as Int` 会抛。
 */
class BridgeCodec(
    moshi: Moshi = Moshi.Builder().build(),
) {
    private val adapter = moshi.adapter(Any::class.java)

    /**
     * 解析一行裸帧。空行、半行、非 JSON 返回 `null`：不抛（一行坏了不该断整条流），
     * 也不变成空帧（上层要分得清「坏行」与「没内容」）。
     */
    fun decodeOrNull(line: String): BridgeFrame? {
        if (line.isBlank()) return null
        val map = runCatching { adapter.fromJson(line) }.getOrNull() as? Map<*, *> ?: return null
        return decode(map)
    }

    /**
     * 解析录制文件的一行外层包封（`{"t_ns":…,"frame":{…}}` 或 `{"__meta__":…}`）。
     * 不能拿 [decodeOrNull] 读录制文件：整行会被当成一个未知帧。
     */
    fun rawRow(line: String): Map<*, *>? =
        runCatching { adapter.fromJson(line) }.getOrNull() as? Map<*, *>

    /**
     * 按来源分两组分派：块级帧来自 `content_block_*` 流事件（带 `m`/`i` 坐标），
     * 会话级帧来自 SDK 的消息对象。
     */
    fun decode(map: Map<*, *>): BridgeFrame =
        decodeBlockFrame(map) ?: decodeSessionFrame(map)

    /** 块级帧：都带 `m`（message_id）+ `i`（块序号）这对坐标。认不出返回 null。 */
    private fun decodeBlockFrame(map: Map<*, *>): BridgeFrame? =
        when (map["t"] as? String) {
            "bs" ->
                BridgeFrame.BlockStart(
                    m = map.str("m"),
                    i = map.int("i"),
                    bt = map.str("bt"),
                    id = map.str("id"),
                    name = map.str("name"),
                    p = map.str("p"),
                )

            "d" -> BridgeFrame.TextDelta(map.str("m"), map.int("i"), map.text(), map.str("p"))
            "td" -> BridgeFrame.ThinkingDelta(map.str("m"), map.int("i"), map.text(), map.str("p"))
            "ij" -> BridgeFrame.InputJsonDelta(map.str("m"), map.int("i"), map.text(), map.str("p"))
            "be" -> BridgeFrame.BlockEnd(map.str("m"), map.int("i"), map.str("p"))
            "at" -> BridgeFrame.AssistantText(map.str("m"), map.int("i"), map.str("p"), map.text())
            "tt" -> BridgeFrame.ThinkingText(map.str("m"), map.int("i"), map.str("p"), map.text())

            "tu" ->
                BridgeFrame.ToolUse(
                    m = map.str("m"),
                    i = map.int("i"),
                    p = map.str("p"),
                    id = map.str("id"),
                    name = map.str("name"),
                    inp = map["inp"].toRawMapOrNull(),
                )

            "tr" ->
                BridgeFrame.ToolResult(
                    id = map.str("id"),
                    p = map.str("p"),
                    // 缺 `ok` 时默认 true：对端不写 None，而 ok=false 一定会写出来。
                    ok = map["ok"] as? Boolean ?: true,
                    x = map.text(),
                    truncatedFullLength = (map["trunc"] as? Map<*, *>)?.int("n").takeIf { it != 0 },
                )

            else -> null
        }

    /** 会话级帧：init / 结果 / 限流 / 错误 / 透传 / 未知。 */
    private fun decodeSessionFrame(map: Map<*, *>): BridgeFrame {
        val raw = map.toRawMap()
        return when (val t = map["t"] as? String) {
            "init" ->
                BridgeFrame.Init(
                    sid = map.str("sid"),
                    model = map.str("model"),
                    cwd = map.str("cwd"),
                    cmds = map.strList("cmds"),
                    skills = map.strList("skills"),
                    tools = map.strList("tools"),
                    agents = map.strList("agents"),
                    mcp = map.mcpList(),
                    plugins = map.strList("plugins"),
                    raw = map["raw"].toRawMapOrNull(),
                )

            "ut" -> BridgeFrame.UserText(map.str("p"), map.text())

            "res" ->
                BridgeFrame.Result(
                    sid = map.str("sid"),
                    ok = map["ok"] as? Boolean ?: false,
                    cost = map.dbl("cost"),
                    turns = map["turns"]?.let { map.int("turns") },
                    why = map.str("why"),
                    api = map.str("api"),
                    dur = map["dur"]?.let { map.lng("dur") },
                    usage = map["usage"].toRawMapOrNull(),
                    den = map["den"] as? List<Any?>,
                    mu = map["mu"].toRawMapOrNull(),
                    raw = map["raw"].toRawMapOrNull(),
                )

            "rl" -> BridgeFrame.RateLimit(map["raw"].toRawMapOrNull())
            "err" -> BridgeFrame.Err(map.str("m"), map.str("code"))
            "ev" ->
                BridgeFrame.Event(
                    k = map.str("k"),
                    i = map["i"]?.let { map.int("i") },
                    raw = map["raw"].toRawMapOrNull(),
                )

            // `t` 为 null（根本不是帧）也落这里，标成 "?"。
            else -> BridgeFrame.Unknown(t ?: "?", raw)
        }
    }
}

// ---- 取值助手：全部「缺了不崩」 ------------------------------------------------

private fun Map<*, *>.str(k: String): String? = this[k] as? String

/** `x` 缺失 → 空串。正文帧少了 `x` 时渲染空块，比抛异常整条流断掉好。 */
private fun Map<*, *>.text(): String = this["x"] as? String ?: ""

/** Moshi 的 `Any` 适配器把所有数字读成 `Double`，直接 `as Int` 会抛。 */
private fun Map<*, *>.int(k: String): Int = (this[k] as? Number)?.toInt() ?: 0

private fun Map<*, *>.lng(k: String): Long = (this[k] as? Number)?.toLong() ?: 0L

private fun Map<*, *>.dbl(k: String): Double? = (this[k] as? Number)?.toDouble()

private fun Map<*, *>.strList(k: String): List<String> =
    (this[k] as? List<*>)?.mapNotNull { it as? String }.orEmpty()

/** init 的五个清单里只有 `mcp` 带结构（`{name,status}`），其余四个是裸字符串。 */
private fun Map<*, *>.mcpList(): List<BridgeFrame.McpServer> =
    (this["mcp"] as? List<*>).orEmpty().mapNotNull { e ->
        when (e) {
            is Map<*, *> -> (e["name"] as? String)?.let { BridgeFrame.McpServer(it, e["status"] as? String) }
            // 对端若给裸字符串也接住。
            is String -> BridgeFrame.McpServer(e, null)
            else -> null
        }
    }

private fun Map<*, *>.toRawMap(): Map<String, Any?> =
    entries.mapNotNull { (k, v) -> (k as? String)?.let { it to v } }.toMap()

private fun Any?.toRawMapOrNull(): Map<String, Any?>? = (this as? Map<*, *>)?.toRawMap()
