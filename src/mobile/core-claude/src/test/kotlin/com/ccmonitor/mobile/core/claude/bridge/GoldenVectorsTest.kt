package com.ccmonitor.mobile.core.claude.bridge

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 跨语言 golden vectors：Kotlin 侧与 pytest 侧吃同一份真数据，两端结构上不可能各自漂移。
 *
 * 数据是仓内 `bridge/vectors/` 下的 `<case>.frames.ndjson`：由 `record_vectors.py` 录真 SDK 输出
 * （SDK 0.2.128 / CLI 2.1.220），再经 `reference_encoder.py` 映射成 aterm 帧。不是手写的样例。
 * 注意：这段注释里别写通配符路径，Kotlin 块注释可嵌套，`/` 紧跟 `*` 会报 "Unclosed comment"。
 *
 * 断言的是具体字段值，不只是「解析不崩」。
 * [codecDecodesEveryGoldenFrameConsistentlyWithTheRawJson] 把 Kotlin 解析结果与裸 JSON 逐字段对账；
 * [codecTurnsGenuinelyUnknownFrameTypesIntoUnknownNotSilentlyDropping] 测的是 codec 遇到不认识的帧型怎么做，
 * 与 [unknownKindsArePassedThroughNotDropped]（golden 里存在 ev 帧，数据的性质）不是一回事。
 */
class GoldenVectorsTest {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    private fun vectorsDir(): File {
        // JVM 测试的 CWD 是模块目录（core-claude/），仓根在其上一级。
        val fromModule = File("../bridge/vectors")
        return if (fromModule.isDirectory) fromModule else File("bridge/vectors")
    }

    private data class Row(
        val tNs: Long,
        val frame: Map<*, *>,
    )

    private fun load(case: String): List<Row> {
        val f = File(vectorsDir(), "$case.frames.ndjson")
        assertTrue(
            "golden 缺失：${f.absolutePath}。它必须随仓提交——只放在仓外的产物会丢",
            f.isFile,
        )
        return f
            .readLines()
            .filter { it.isNotBlank() }
            .mapNotNull { line ->
                @Suppress("UNCHECKED_CAST")
                val m = anyAdapter.fromJson(line) as? Map<String, Any?> ?: return@mapNotNull null
                val frame = m["frame"] as? Map<*, *> ?: return@mapNotNull null // 跳过 __meta__ 行
                // 注意：Moshi 的 Any 适配把所有数字读成 Double。
                Row((m["t_ns"] as? Number)?.toLong() ?: 0L, frame)
            }
    }

    private fun List<Row>.types(): List<String> = map { it.frame["t"] as String }

    /**
     * 六个技术 golden（逐字节忠实，绝不编辑）。
     *
     * `demo-debug` 刻意不在此列：它是演示素材，会随 app 分发，必须脱敏后编辑。
     * 拿编辑过的文件当 golden，等于把「编辑成什么样」当成「现实是什么样」。
     * 两类文件的规矩相反，见 `bridge/vectors/README.md`。
     */
    private val cases = listOf("text-short", "tool-call", "thinking", "long-reply", "auth-fail", "interrupt")

    /** 六个 case 都能读出来，且非空。golden 缺失/损坏在这里就红。 */
    @Test
    fun allCasesLoad() {
        cases.forEach { c ->
            val rows = load(c)
            assertTrue("$c 应有帧", rows.isNotEmpty())
        }
    }

    /**
     * catalog 四件套必须在 `init` 帧里：点击式 UI 的地基。
     * 命令/skill 选择器的数据源全靠它；缺任何一个，那两个功能就没有内容可渲染。
     */
    @Test
    fun initFrameCarriesCatalog() {
        val init = load("text-short").first { it.frame["t"] == "init" }.frame
        listOf("cmds", "skills", "tools", "mcp", "agents", "plugins").forEach { k ->
            val v = init[k]
            assertTrue("init 帧缺 catalog 字段 `$k`（点击式 UI 的数据源）", v is List<*>)
        }
        assertTrue("slash_commands 不该为空（远端有 skill/命令）", (init["cmds"] as List<*>).isNotEmpty())
        assertTrue("tools 不该为空", (init["tools"] as List<*>).isNotEmpty())
        // 版本协商要的 capabilities / claude_code_version 在 raw 里：init 是唯一
        // 「丢了就再也拿不到」的 catalog 源，不许只挑几个字段出来。
        val raw = init["raw"] as? Map<*, *>
        assertTrue("init 的剩余字段必须留在 raw（版本协商要用）", raw?.containsKey("capabilities") == true)
    }

    /**
     * 未知帧型上抛为 `ev` 而非丢弃（未知即透传）。
     *
     * bridge 无版本协商，静默丢帧的表现是 UI 永远等不到 result、一直转圈、日志什么都没有。
     */
    @Test
    fun unknownKindsArePassedThroughNotDropped() {
        val evKinds =
            listOf("text-short", "tool-call", "thinking", "long-reply")
                .flatMap { load(it) }
                .filter { it.frame["t"] == "ev" }
                .mapNotNull { it.frame["k"] as? String }
                .toSet()

        // 真数据里出现过的、刻意不特化的类型
        assertTrue(
            "HookEventMessage 应被透传（用户 settings.json 的 hook 会触发）",
            evKinds.any { it.startsWith("sdk:HookEventMessage") },
        )
        assertTrue("未特化的 SystemMessage subtype 应被透传", evKinds.any { it.startsWith("system:") })
        assertTrue("未特化的 stream event 应被透传", evKinds.any { it.startsWith("stream:") })
    }

    /**
     * 未知 delta 子类型必须容忍。真数据里有 4 种：
     * `text_delta` / `input_json_delta` / `thinking_delta` / `signature_delta`。
     * 后两者没有 `text` 字段，无条件取 `delta["text"]` 就是 KeyError。
     * `signature_delta` 走 `ev` 透传，正是这条纪律的体现。
     */
    @Test
    fun signatureDeltaIsToleratedViaPassthrough() {
        // 跨全部 case 找，不绑定到某一个：它出现在 `long-reply` 而非 `thinking`，
        // 「哪个 prompt 触发哪种 delta」是模型行为，不该被测试固化。
        val kinds =
            listOf("text-short", "tool-call", "thinking", "long-reply")
                .flatMap { load(it) }
                .filter { it.frame["t"] == "ev" }
                .mapNotNull { it.frame["k"] as? String }
        assertTrue(
            "signature_delta 应作为 ev 透传（它没有 text 字段，特化处理会 KeyError）。实际 delta 透传：" +
                kinds.filter { it.startsWith("delta:") },
            kinds.contains("delta:signature_delta"),
        )
    }

    /**
     * 每个块必须 `bs` 开、`be` 收。`be` 是「封口 = 承诺」，assembler 只在这里推进 closedKeys。
     *
     * `interrupt` 刻意不在此列，见 [interruptLeavesAnUnclosedBlock]。
     */
    @Test
    fun everyBlockStartHasMatchingStop() {
        listOf("text-short", "tool-call", "thinking", "long-reply").forEach { c ->
            val t = load(c).types()
            assertEquals("$c 的 bs 与 be 数量应相等", t.count { it == "bs" }, t.count { it == "be" })
        }
    }

    /**
     * 中断会留下一个永远不封口的块：`bs`=2 而 `be`=1。
     *
     * 这是真实行为，但它约束 assembler：只有 `content_block_stop` 才把块 key 推进 `closedKeys`，
     * 那个块会永远停在 open 状态。若 UI 靠块封口决定「这段文本定稿了、可以切 MarkdownText 渲染」，
     * 中断过的会话里会永远挂着一个开口块 ⇒ assembler 必须以 `res` 帧兜底强制封口所有开口块。
     */
    @Test
    fun interruptLeavesAnUnclosedBlock() {
        val t = load("interrupt").types()
        val bs = t.count { it == "bs" }
        val be = t.count { it == "be" }
        assertTrue("中断 case 应出现未封口的块（bs=$bs be=$be）", bs > be)
        assertEquals("但仍必须以 res 收口——否则 UI 永远转圈", "res", t.last())
    }

    /**
     * 中断的收口语义：`ok=false` + `why=aborted_streaming`。
     * 「停止生成」按钮的 UI 状态全靠这两个字段区分「用户主动停」与「出错了」。
     */
    @Test
    fun interruptResultCarriesAbortedStreaming() {
        val res = load("interrupt").last { it.frame["t"] == "res" }.frame
        assertEquals("中断后 ok 应为 false", false, res["ok"])
        assertEquals("终止原因应为 aborted_streaming", "aborted_streaming", res["why"])
    }

    /**
     * 块 key `m#i` 必须能从帧本身构造出来：「`at` 全文覆盖」的前提。
     *
     * `i` 每条消息重置（tool-call 一个 turn 内 `i=0` 出现两次、指两个不同的块），
     * 所以 `at` 帧必须带 `message_id`，否则没有任何键指明它覆盖哪个块。
     */
    @Test
    fun everyBlockFrameCarriesAConstructibleKey() {
        val keyed = setOf("bs", "d", "td", "ij", "be", "at", "tt", "tu")
        listOf("text-short", "tool-call", "thinking", "long-reply", "interrupt").forEach { c ->
            load(c).map { it.frame }.filter { it["t"] in keyed }.forEach { fr ->
                assertTrue("$c: ${fr["t"]} 帧缺 m（message_id），块归属无法构造", (fr["m"] as? String)?.isNotBlank() == true)
                assertTrue("$c: ${fr["t"]} 帧缺 i", fr["i"] != null)
            }
        }
    }

    /**
     * delta 拼接 == `at`/`tt` 全文，逐字相等。
     *
     * 这是「delta 丢了不用补、`at` 全文无条件覆盖」的依据：两者不等的话就得做 CRC + repair。
     * 5 个 case 全部逐字相等，包括 interrupt（480 == 480 字符）。
     *
     * 与 pytest 侧 `test_delta_concat_equals_full_text` 同源、独立复算。
     */
    @Test
    fun deltaConcatEqualsFullText() {
        listOf("text-short", "tool-call", "thinking", "long-reply", "interrupt").forEach { c ->
            val deltas = mutableMapOf<String, StringBuilder>()
            val full = mutableMapOf<String, String>()
            load(c).map { it.frame }.forEach { fr ->
                val key = "${fr["m"]}#${fr["i"]}"
                when (fr["t"]) {
                    "d", "td", "ij" -> deltas.getOrPut(key) { StringBuilder() }.append(fr["x"] as? String ?: "")
                    "at", "tt" -> full[key] = fr["x"] as? String ?: ""
                }
            }
            var compared = 0
            full.forEach { (key, text) ->
                val joined = deltas[key]?.toString() ?: return@forEach
                compared++
                assertEquals("$c: 块 $key 的 delta 拼接 ≠ 全文：「at 全文覆盖」不再成立", text, joined)
            }
            assertTrue("$c: 一个块都没比到，测试形同虚设", compared > 0)
        }
    }

    /**
     * `tr.ok` 与 `res.ok` 同名同极性。
     *
     * 反极性（`err=bool(is_error)`）的话，读的人以为拿到 `ok`，工具失败会被读成成功（任何 `?: true` 兜底都会踩）。
     */
    @Test
    fun toolResultUsesOkNotReversedErr() {
        val tr = load("tool-call").first { it.frame["t"] == "tr" }.frame
        assertEquals("tool_result 应带 ok（与 res.ok 同极性）", true, tr["ok"])
        assertTrue("不许再出现反极性的 err 字段", !tr.containsKey("err"))
    }

    /** 工具调用链完整：`tu`（调用）必有对应 `tr`（结果）。 */
    @Test
    fun toolUseHasResult() {
        val rows = load("tool-call")
        val useIds = rows.filter { it.frame["t"] == "tu" }.mapNotNull { it.frame["id"] as? String }
        val resIds = rows.filter { it.frame["t"] == "tr" }.mapNotNull { it.frame["id"] as? String }
        assertTrue("tool-call case 应有工具调用", useIds.isNotEmpty())
        assertTrue("每个 tool_use 应有对应的 tool_result：use=$useIds res=$resIds", resIds.containsAll(useIds))
    }

    /**
     * `res.ok` 取自 `!is_error`，绝不看 `subtype`：
     * 未登录时 `ResultMessage.subtype` 仍是 `'success'`，看 subtype 会把认证失败当成功。
     */
    @Test
    fun resultOkComesFromIsErrorNotSubtype() {
        listOf("text-short", "tool-call", "thinking", "long-reply").forEach { c ->
            val res = load(c).last { it.frame["t"] == "res" }.frame
            assertEquals("$c 应成功收口", true, res["ok"])
            assertTrue("$c 的 res 应带 sid", (res["sid"] as? String)?.isNotBlank() == true)
        }
    }

    /**
     * 认证失败：`res.ok` 必须为 false，「看 `is_error` 不看 `subtype`」的真数据证明。
     *
     * 未登录时 `ResultMessage.subtype` 仍然是 `'success'`；换成看 subtype 的话这条会红。
     */
    @Test
    fun authFailureIsDetectedViaIsErrorNotSubtype() {
        val rows = load("auth-fail")
        val err = rows.firstOrNull { it.frame["t"] == "err" }
        assertEquals("应产出 err 帧且 code 为 authentication_failed", "authentication_failed", err?.frame?.get("code"))
        val res = rows.last { it.frame["t"] == "res" }.frame
        assertEquals("认证失败时 res.ok 必须为 false（subtype 仍是 'success'，不可信）", false, res["ok"])
    }

    /**
     * thinking 内容最多是摘要，拿不到原文；不开 `display` 就一个字都没有。
     *
     * `ThinkingConfig.display` 默认是 `omitted`（取值只有 `summarized`/`omitted`，没有 `full`），
     * 加 `display:"summarized"` 后才有 7 个 td / 560 字符。界面的 thinking 折叠按摘要设计。
     */
    @Test
    fun thinkingDeltasCarrySummarizedText() {
        val td = load("thinking").filter { it.frame["t"] == "td" }
        assertTrue("thinking case 应有 thinking_delta", td.isNotEmpty())
        val chars = td.sumOf { (it.frame["x"] as? String)?.length ?: 0 }
        assertTrue(
            "thinking 内容不该为空：为空说明 display 是 omitted（${td.size} 个 td / $chars 字符）",
            chars > 0,
        )
    }

    /**
     * 流式节奏的基线，缓释器参数的依据。
     *
     * `long-reply`：31 个 text_delta / 2575 字符 / 平均 83 字符每块，块间隔中位 ~472ms
     * （30 个间隔里 29 个 >100ms）⇒ 天然速率约 176 字符/秒。
     *
     * 守住「不是逐 token」：SDK 变成逐字符推送时这条会红，那时缓释器的设计前提要重估。
     */
    @Test
    fun streamingIsChunkedNotPerToken() {
        val deltas = load("long-reply").filter { it.frame["t"] == "d" }
        assertTrue("long-reply 应有多个 text delta", deltas.size >= 5)
        val totalChars = deltas.sumOf { (it.frame["x"] as? String)?.length ?: 0 }
        val avg = totalChars.toDouble() / deltas.size
        assertTrue(
            "平均 ~83 字符/块；若降到 <5 说明 SDK 变成了逐 token 推送，缓释器设计需重估（avg=$avg）",
            avg > 5.0,
        )
    }

    // ======================================================================
    // codec 解析结果 == 上面的裸 map 断言
    // ======================================================================

    /**
     * Kotlin codec 解出来的每一帧，字段值必须与 golden 里的裸 JSON 一致。
     * 红了说明 Kotlin 侧的解析与 Python 参考编码器的产出对不上了。
     */
    @Test
    fun codecDecodesEveryGoldenFrameConsistentlyWithTheRawJson() {
        val codec = BridgeCodec()
        for (c in cases) {
            load(c).forEach { row ->
                val raw = row.frame
                when (val f = codec.decode(raw)) {
                    is BridgeFrame.Unknown ->
                        throw AssertionError("$c: codec 认不出 golden 里的帧型 `${raw["t"]}` —— 两侧实现漂移了")

                    is BridgeFrame.TextDelta -> assertEquals(raw["x"], f.x)
                    is BridgeFrame.AssistantText -> {
                        assertEquals(raw["x"], f.x)
                        assertEquals(raw["m"], f.m)
                        assertEquals((raw["i"] as Number).toInt(), f.i)
                    }
                    is BridgeFrame.ToolResult -> assertEquals("tr.ok 极性必须与 golden 一致", raw["ok"], f.ok)
                    is BridgeFrame.Result -> {
                        assertEquals("res.ok 极性必须与 golden 一致", raw["ok"], f.ok)
                        assertEquals(raw["why"], f.why)
                    }
                    is BridgeFrame.Err -> assertEquals(raw["code"], f.code)
                    is BridgeFrame.BlockStart -> assertEquals(raw["bt"], f.bt)
                    else -> Unit
                }
            }
        }
    }

    /**
     * codec 对未知帧型的行为（不是「golden 里存在 ev 帧」，那是 [unknownKindsArePassedThroughNotDropped]）。
     */
    @Test
    fun codecTurnsGenuinelyUnknownFrameTypesIntoUnknownNotSilentlyDropping() {
        val f = BridgeCodec().decodeOrNull("""{"t":"frame_from_a_newer_daemon","whatever":1}""")
        assertTrue("未知帧型必须落成 Unknown，不许丢也不许崩", f is BridgeFrame.Unknown)
        assertEquals("frame_from_a_newer_daemon", (f as BridgeFrame.Unknown).t)
        assertTrue("原始内容必须留着——否则「远端比我新」这件事就没痕迹了", f.raw.containsKey("whatever"))
    }

    /** `ev`（编码器主动打的透传包）与 `Unknown`（解码器不认识）不是一回事，不许混成一个。 */
    @Test
    fun passthroughEventAndUnknownFrameAreDistinctTypes() {
        val codec = BridgeCodec()
        val ev = codec.decodeOrNull("""{"t":"ev","k":"stream:message_start","raw":{}}""")
        val unknown = codec.decodeOrNull("""{"t":"nope"}""")
        assertTrue("ev 是设计内的透传", ev is BridgeFrame.Event)
        assertTrue("Unknown 是「远端比我新」", unknown is BridgeFrame.Unknown)
    }
}
