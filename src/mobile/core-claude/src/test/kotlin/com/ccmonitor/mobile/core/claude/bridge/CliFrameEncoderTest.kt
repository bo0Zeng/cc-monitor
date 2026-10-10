package com.ccmonitor.mobile.core.claude.bridge

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * `CliFrameEncoder`：CLI 原生 stream-json → 家族 A 紧凑帧。
 *
 * 最强的判据：`cli-text-short.cli.ndjson`（CLI 录的）喂进编码器得到的帧序列，应与
 * `text-short.frames.ndjson`（SDK 侧同一个 prompt 的产物，由 `reference_encoder.py` 生成）一致。
 * 不比 `id`：两份 golden 是两次独立录制，session_id / message_id / uuid 必然不同；比的是帧型序列 + 正文。
 */
class CliFrameEncoderTest {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    private fun vectorsDir(): File =
        File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    /** CLI golden 的每一行（已剥掉 `__meta__` 与 `t_ns` 信封）。 */
    private fun cliLines(): List<Map<*, *>> =
        File(vectorsDir(), "cli-text-short.cli.ndjson")
            .readLines()
            .mapNotNull { anyAdapter.fromJson(it) as? Map<*, *> }
            .filterNot { it.containsKey("__meta__") }
            .map { it["line"] as Map<*, *> }

    private fun encodeGolden(): List<BridgeFrame> {
        val enc = CliFrameEncoder()
        return cliLines().flatMap { enc.feed(it) }
    }

    /** SDK 侧同 prompt 的已知正确产物（`reference_encoder.py` 生成、pytest+Kotlin 双侧钉过）。 */
    private fun sdkFrames(): List<BridgeFrame> =
        ReplayVectors
            .parse(File(vectorsDir(), "text-short.frames.ndjson").readLines().asSequence())
            .map { it.frame }

    /** 帧型序列必须一致：映射对不对的主判据。 */
    @Test
    fun theEncodedFrameSequenceMatchesTheSdkSideProduct() {
        val mine = encodeGolden()
        val theirs = sdkFrames()
        assertTrue("前提：两份 golden 都要读到", mine.isNotEmpty() && theirs.isNotEmpty())
        assertEquals(
            "帧型序列对不上\n  我的：${mine.map { it.t }}\n  SDK：${theirs.map { it.t }}",
            theirs.map { it.t },
            mine.map { it.t },
        )
    }

    /** 正文必须一致（同一个 prompt「Reply with exactly: hello」）。 */
    @Test
    fun theAuthoritativeTextMatches() {
        val mineText = encodeGolden().filterIsInstance<BridgeFrame.AssistantText>().map { it.x }
        val theirsText = sdkFrames().filterIsInstance<BridgeFrame.AssistantText>().map { it.x }
        assertEquals("前提：SDK 侧确实有权威正文", listOf("hello"), theirsText)
        assertEquals("权威正文必须一致", theirsText, mineText)

        // delta 拼起来也该是同一段（delta 是装饰，但装饰也不该错）
        val mineDelta = encodeGolden().filterIsInstance<BridgeFrame.TextDelta>().joinToString("") { it.x }
        val theirsDelta = sdkFrames().filterIsInstance<BridgeFrame.TextDelta>().joinToString("") { it.x }
        assertEquals(theirsDelta, mineDelta)
    }

    /**
     * `init` 的 catalog：CLI 侧字段在顶层（SDK 嵌在 `data` 下）。
     * 拆错包的表现是 catalog 全空，而命令选择器就建在它上面。
     */
    @Test
    fun theCatalogIsUnwrappedFromTheTopLevelNotFromData() {
        val init = encodeGolden().filterIsInstance<BridgeFrame.Init>().single()
        assertTrue("工具不许空：空就是拆错包了", init.tools.isNotEmpty())
        assertTrue("命令不许空", init.cmds.isNotEmpty())
        assertNotNull("cwd 要在", init.cwd)
        assertNotNull("model 要在", init.model)
        assertNotNull("sid 要在", init.sid)
        // 剩余字段要留着：init 是唯一一处「丢了就再也拿不到」的 catalog 源
        assertNotNull("raw 要留住 capabilities/claude_code_version 之类", init.raw)
        assertTrue("permissionMode 在 raw 里（权限模式 chip 读它）", init.raw!!.containsKey("permissionMode"))
    }

    /** `res` 的判据是 `!is_error`，绝不看 subtype（未登录时 subtype 仍是 success）。 */
    @Test
    fun theResultOkComesFromIsErrorNotSubtype() {
        val res = encodeGolden().filterIsInstance<BridgeFrame.Result>().single()
        assertTrue("这一轮是成功的", res.ok)
        assertNotNull("sid 要在", res.sid)
        // 反例：subtype=success 但 is_error=true ⇒ 必须判失败
        val enc = CliFrameEncoder()
        val bad = enc.feed(mapOf("type" to "result", "subtype" to "success", "is_error" to true)).single()
        assertTrue("subtype 说 success 也不算数", !(bad as BridgeFrame.Result).ok)
    }

    /**
     * `assistant` 嵌在 `message` 下，message_id 的键是 `id`（SDK 是顶层 `message_id`）。
     * 拆错的表现是 `at` 帧的 `m` 为 null ⇒「覆盖哪个块」失去依据。
     */
    @Test
    fun theAssistantMessageIdComesFromTheNestedIdField() {
        val at = encodeGolden().filterIsInstance<BridgeFrame.AssistantText>().single()
        assertNotNull("m 不许为 null：它是 at 覆盖哪个块的唯一依据", at.m)
        assertTrue("m 应是真的 message id", at.m!!.startsWith("msg_"))
        assertEquals("首块的 index 是 0", 0, at.i)

        // 上面那几条判别力不够：`message_start` 已经把 id 设过了，读错键（比如照 SDK 读 `message_id`）
        // 也能靠 `messageIdByParent` 里记的蒙对。真正的判据是没有前置 `message_start` 时还能不能拿到。
        val bare =
            CliFrameEncoder()
                .feed(
                    mapOf(
                        "type" to "assistant",
                        "message" to mapOf("id" to "msg_bare", "content" to listOf(mapOf("type" to "text", "text" to "x"))),
                    ),
                ).single() as BridgeFrame.AssistantText
        assertEquals("必须从嵌套的 `id` 取，而不是靠流里记的那个兜底", "msg_bare", bare.m)
    }

    /** 未知帧型透传成 `ev`，不崩不丢。 */
    @Test
    fun anUnknownLineIsPassedThroughInsteadOfCrashing() {
        val enc = CliFrameEncoder()
        val out = enc.feed(mapOf("type" to "some_future_thing", "payload" to 1)).single()
        assertTrue(out is BridgeFrame.Event)
        assertEquals("cli:some_future_thing", (out as BridgeFrame.Event).k)
        assertEquals("原文要留着", 1.0, (out.raw?.get("payload") as Number).toDouble(), 0.0)
    }

    /** delta 要按子类型取值：无条件取 `text` 会在 thinking/signature 上炸。 */
    @Test
    fun deltasAreReadByTheirOwnSubtype() {
        val enc = CliFrameEncoder()

        fun delta(d: Map<String, Any?>) =
            enc
                .feed(
                    mapOf("type" to "stream_event", "event" to mapOf("type" to "content_block_delta", "index" to 0, "delta" to d)),
                ).single()
        assertEquals("t", (delta(mapOf("type" to "text_delta", "text" to "t")) as BridgeFrame.TextDelta).x)
        assertEquals("k", (delta(mapOf("type" to "thinking_delta", "thinking" to "k")) as BridgeFrame.ThinkingDelta).x)
        assertEquals("{", (delta(mapOf("type" to "input_json_delta", "partial_json" to "{")) as BridgeFrame.InputJsonDelta).x)
        // signature_delta 没有 text/thinking：透传，不崩
        val sig = delta(mapOf("type" to "signature_delta", "signature" to "abc"))
        assertEquals("delta:signature_delta", (sig as BridgeFrame.Event).k)
    }

    /**
     * 同一个 message 下每个块单独发一条 assistant、`content` 长度恒为 1：
     * 用 `content` 的下标会让 thinking 块与 text 块撞键（都成 `m#0`，后者覆盖前者）。
     * 判据必须是「本 message 已交付几个块」。
     */
    @Test
    fun blockIndexCountsDeliveredBlocksNotTheContentIndex() {
        val enc = CliFrameEncoder()

        fun one(type: String) =
            enc
                .feed(
                    mapOf(
                        "type" to "assistant",
                        "message" to mapOf("id" to "msg_x", "content" to listOf(mapOf("type" to type, type to "…"))),
                    ),
                ).single()
        val first = one("thinking") as BridgeFrame.ThinkingText
        val second = one("text") as BridgeFrame.AssistantText
        assertEquals(0, first.i)
        assertEquals("第二个块必须是 1，否则会覆盖第一个", 1, second.i)
    }

    /** `tr` 的 `ok` 与 `res.ok` 同名同极性：反极性会让工具失败被读成成功。 */
    @Test
    fun toolResultOkHasTheSamePolarityAsResultOk() {
        val enc = CliFrameEncoder()

        fun tr(isError: Boolean?) =
            enc
                .feed(
                    mapOf(
                        "type" to "user",
                        "message" to
                            mapOf(
                                "content" to
                                    listOf(
                                        buildMap {
                                            put("type", "tool_result")
                                            put("tool_use_id", "t1")
                                            put("content", "x")
                                            if (isError != null) put("is_error", isError)
                                        },
                                    ),
                            ),
                    ),
                ).single() as BridgeFrame.ToolResult
        assertTrue("没说错就是成功", tr(null).ok)
        assertTrue("显式 false 也是成功", tr(false).ok)
        assertTrue("is_error=true ⇒ ok=false", !tr(true).ok)
        assertNull("没截断就不该有全文长度", tr(null).truncatedFullLength)
    }

    /**
     * `tool_result.content` 是数组时必须吐合法 JSON，不能是 Kotlin 的 map 字面量。
     *
     * 规范见 `bridge/tools/reference_encoder.py`：
     * `text = content if isinstance(content, str) else json.dumps(content, ensure_ascii=False)`。
     * `content.toString()` 会吐 `{type=text, text=a}`（引号没了、`:` 变 `=`），而这串会被原样渲染出来。
     * 数组形态不罕见：MCP 工具、ToolSearch、读图的结果都是数组。
     */
    @Test
    fun anArrayToolResultIsSerializedAsRealJsonNotAKotlinMapLiteral() {
        val enc = CliFrameEncoder()
        val tr =
            enc
                .feed(
                    mapOf(
                        "type" to "user",
                        "message" to
                            mapOf(
                                "content" to
                                    listOf(
                                        mapOf(
                                            "type" to "tool_result",
                                            "tool_use_id" to "t1",
                                            // 中文在这：`ensure_ascii=False` 那一半也要成立
                                            "content" to listOf(mapOf("type" to "text", "text" to "中文 a")),
                                        ),
                                    ),
                            ),
                    ),
                ).single() as BridgeFrame.ToolResult
        assertFalse("不许是 Kotlin map 字面量（`{type=text…}`）：${tr.x}", tr.x.contains("type="))
        assertTrue("要是合法 JSON 数组", tr.x.startsWith("[") && tr.x.endsWith("]"))
        assertTrue("ensure_ascii=False：中文不许被转义成 \\uXXXX", tr.x.contains("中文 a"))
        // 判据是「能被解回来」，不是钉某个具体字符串（那是钉素材）
        val back =
            com.squareup.moshi.Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(tr.x) as List<*>
        assertEquals("中文 a", (back.single() as Map<*, *>)["text"])
    }

    /** 字符串形态的 `content` 不许被再包一层引号（`json.dumps` 只作用于非字符串分支）。 */
    @Test
    fun aStringToolResultPassesThroughUnquoted() {
        val enc = CliFrameEncoder()
        val tr =
            enc
                .feed(
                    mapOf(
                        "type" to "user",
                        "message" to
                            mapOf(
                                "content" to
                                    listOf(mapOf("type" to "tool_result", "tool_use_id" to "t1", "content" to "裸文本")),
                            ),
                    ),
                ).single() as BridgeFrame.ToolResult
        assertEquals("字符串分支原样透传，不许变成 \"裸文本\"", "裸文本", tr.x)
    }

    /**
     * 一条消息进来，绝不允许零帧出去。
     *
     * 规范（`bridge/tools/reference_encoder.py`）两处都写明：空 assistant 回退 `ev` `assistant:empty`，
     * 空 user 回退 `ev` `user:empty`。空 `content` 不是理论情况：`stop_reason` 收尾、工具轮里的空壳
     * assistant 都会来。静默吞掉的话这条消息在手机上根本不存在，也没有痕迹说它来过。
     */
    @Test
    fun neitherAnEmptyAssistantNorAnEmptyUserIsSilentlySwallowed() {
        val cases =
            mapOf(
                "assistant:empty" to
                    mapOf("type" to "assistant", "message" to mapOf("id" to "m1", "content" to emptyList<Any>())),
                "user:empty" to
                    mapOf("type" to "user", "message" to mapOf("content" to emptyList<Any>())),
            )
        for ((k, line) in cases) {
            val out = CliFrameEncoder().feed(line)
            assertEquals("「$k」必须恰好吐一帧，不许零帧：$out", 1, out.size)
            assertEquals("而且要是这个 k，别的键名下游认不出", k, (out.single() as BridgeFrame.Event).k)
        }
    }

    /**
     * 编码器必须读外层的 `parent_tool_use_id`。
     *
     * 每条 `stream_event` 行的键集是 `(event, parent_tool_use_id, session_id, [ttft_ms,] type, uuid)`：
     * 它与 `event` 同级，不在 `event` 里面。不读的话子 agent 的增量被判成主流块，
     * 切流时把主块已露出的正文清零重来。
     *
     * 判据落在编码出来的帧带不带归属上：这是路由能分流的前提，`ChatTurnAssembler` 那侧的判据看不到编码器不读。
     */
    @Test
    fun theEncoderCarriesSubagentOwnershipFromTheStreamLine() {
        val enc = CliFrameEncoder()

        fun line(event: Map<String, Any?>) =
            mapOf(
                "type" to "stream_event",
                // 与 `event` 同级，照真实的键位放
                "parent_tool_use_id" to "toolu_sub",
                "session_id" to "s",
                "uuid" to "u",
                "event" to event,
            )
        enc.feed(line(mapOf("type" to "message_start", "message" to mapOf("id" to "m1"))))

        val bs = enc.feed(line(mapOf("type" to "content_block_start", "index" to 0, "content_block" to mapOf("type" to "text")))).single()
        val d = enc.feed(line(mapOf("type" to "content_block_delta", "index" to 0, "delta" to mapOf("type" to "text_delta", "text" to "x")))).single()
        val td = enc.feed(line(mapOf("type" to "content_block_delta", "index" to 0, "delta" to mapOf("type" to "thinking_delta", "thinking" to "y")))).single()
        val ij = enc.feed(line(mapOf("type" to "content_block_delta", "index" to 0, "delta" to mapOf("type" to "input_json_delta", "partial_json" to "{")))).single()
        val be = enc.feed(line(mapOf("type" to "content_block_stop", "index" to 0))).single()

        assertEquals("bs 要带归属", "toolu_sub", (bs as BridgeFrame.BlockStart).p)
        assertEquals("d 要带归属", "toolu_sub", (d as BridgeFrame.TextDelta).p)
        assertEquals("td 要带归属", "toolu_sub", (td as BridgeFrame.ThinkingDelta).p)
        assertEquals("ij 要带归属", "toolu_sub", (ij as BridgeFrame.InputJsonDelta).p)
        assertEquals("be 要带归属", "toolu_sub", (be as BridgeFrame.BlockEnd).p)
    }

    /**
     * 子 agent 的 `message_start` 不许改写主流的 `m`。
     *
     * `messageIdByParent` 若是一个共享字段，这个交错序列就会出错：
     *
     * ```
     * ① 主流 message_start(m-main)     → 记住 m-main
     * ② 子 agent message_start(m-sub)  → 把 m-main 冲掉
     * ③ 主流 text_delta（p 为 null）   → 被标成 m-sub
     * ```
     *
     * 判据落在后果上：③ 那一帧仍须属于 m-main。否则上层看成换了一条消息 ⇒ `switchStreamTo` ⇒
     * `flush()+replaceAll("")` ⇒ 主块已露出的正文当场清零重来。
     *
     * 已录的 golden 里没有 Task 子 agent，这条测的是状态不该被串，不是在断言录到过的交错形状。
     */
    @Test
    fun aSubagentMessageStartDoesNotStealTheMainFlowMessageId() {
        val enc = CliFrameEncoder()

        fun start(
            id: String,
            p: String?,
        ) = mapOf(
            "type" to "stream_event",
            "parent_tool_use_id" to p,
            "event" to mapOf("type" to "message_start", "message" to mapOf("id" to id)),
        )

        fun delta(p: String?) =
            mapOf(
                "type" to "stream_event",
                "parent_tool_use_id" to p,
                "event" to
                    mapOf(
                        "type" to "content_block_delta",
                        "index" to 0,
                        "delta" to mapOf("type" to "text_delta", "text" to "x"),
                    ),
            )

        enc.feed(start("m-main", null))
        // 前提断言：没有子 agent 掺和时，主流的 delta 本来就该是 m-main
        assertEquals("前提", "m-main", (enc.feed(delta(null)).single() as BridgeFrame.TextDelta).m)

        enc.feed(start("m-sub", "toolu_x")) // 子 agent 插进来

        val after = enc.feed(delta(null)).single() as BridgeFrame.TextDelta
        assertEquals("主流的 m 不许被子 agent 冲掉", "m-main", after.m)
        // 子 agent 自己那侧也要记对，否则等于把两条流合成一条
        assertEquals("子 agent 的 m 要是它自己的", "m-sub", (enc.feed(delta("toolu_x")).single() as BridgeFrame.TextDelta).m)
    }

    /** 主流消息里它恒为 null，所以已录 golden 的编码结果逐字节不变。 */
    @Test
    fun aMainFlowStreamLineCarriesNoOwnership() {
        val enc = CliFrameEncoder()
        val line =
            mapOf(
                "type" to "stream_event",
                "session_id" to "s",
                "event" to mapOf("type" to "content_block_delta", "index" to 0, "delta" to mapOf("type" to "text_delta", "text" to "x")),
            )
        assertNull("主流不该有归属", (enc.feed(line).single() as BridgeFrame.TextDelta).p)
    }

    /**
     * `mcp_servers` 的三格都要原样透传。
     *
     * `source` 用于按来源分组。它是可选字段、可以整个缺席（较早的 CLI 不报）⇒ 缺席时必须是 null，不许默认成任何值。
     * `status` 同理原样透传（取值有 5 个：connected · failed · needs-auth · pending · disabled）。
     */
    @Test
    fun mcpServerCarriesAllThreeFieldsAndNeverDefaultsSource() {
        val withSource = BridgeFrame.McpServer("anysearch", "connected", "user")
        assertEquals("anysearch", withSource.name)
        assertEquals("connected", withSource.status)
        assertEquals("user", withSource.source)

        val absent = BridgeFrame.McpServer("old-helper", "pending")
        assertNull("source 缺席时必须是 null，不许默认成任何值", absent.source)
    }
}
