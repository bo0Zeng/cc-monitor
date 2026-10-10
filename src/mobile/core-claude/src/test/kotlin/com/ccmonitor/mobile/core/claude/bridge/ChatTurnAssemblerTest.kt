package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.model.RenderUnit
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [ChatTurnAssembler]：把帧流装配成渲染单元（第二个生产者）。
 *
 * 喂的全是 `bridge/vectors/` 里的真录制，不写手编样例：手编样例跟着理解走，不跟着现实走。
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ChatTurnAssemblerTest {
    private fun vectorsDir(): File =
        File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    private fun rows(case: String): List<ReplayRow> =
        ReplayVectors.parse(File(vectorsDir(), "$case.frames.ndjson").readLines().asSequence())

    private fun assemble(case: String): ChatTurnAssembler =
        ChatTurnAssembler().also { a -> rows(case).forEach { a.feed(it.frame) } }

    // ---- at/tt 全文无条件覆盖 delta -------------------------

    /**
     * `at` 到达后，正文必须是 `at` 的全文，不是 delta 拼出来的。
     *
     * 造一段 delta 与全文不一致的流（模拟丢包/乱序），断言以 `at` 为准。
     * 若实现写成「delta 优先」或「拼接」，这里会立刻红。
     */
    @Test
    fun authoritativeFullTextOverridesWhateverDeltasBuilt() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.TextDelta("msg_1", 0, "丢了一半的"))
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "完整的正文在这里"))

        val unit = a.units().filterIsInstance<RenderUnit.AssistantMarkdown>().single()
        assertEquals("at 是权威，delta 只是过程", "完整的正文在这里", unit.markdown)
    }

    /** `at` 先到、delta 后到（乱序）时，仍以 `at` 为准：覆盖是无条件的。 */
    @Test
    fun lateArrivingDeltasDoNotCorruptTheAuthoritativeText() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "权威全文"))
        a.feed(BridgeFrame.TextDelta("msg_1", 0, "迟到的碎片"))

        assertEquals(
            "权威全文",
            a
                .units()
                .filterIsInstance<RenderUnit.AssistantMarkdown>()
                .single()
                .markdown,
        )
    }

    // ---- res 强制封口 --------------------------------------

    /**
     * `interrupt` golden 有个块永远等不到 `be`（bs=2 / be=1）。
     *
     * 只等 `be` 的实现会让那个块挂着「生成中」直到页面被杀掉。
     * 这里用真数据断言：`res` 到了之后，开着的块必须是 0。
     */
    @Test
    fun resultForciblyClosesTheBlockThatNeverGotItsBlockEnd() {
        val frames = rows("interrupt").map { it.frame }
        val bs = frames.count { it is BridgeFrame.BlockStart }
        val be = frames.count { it is BridgeFrame.BlockEnd }
        assertTrue("前提：interrupt golden 里确实有块没收口（bs=$bs be=$be）", bs > be)

        val a = assemble("interrupt")
        assertEquals("res 之后不许还有开着的块", 0, a.openBlockCount)
        assertTrue("turn 必须已收口", a.turn is TurnState.Done)
        assertFalse("中断的 turn 是失败", (a.turn as TurnState.Done).ok)
        assertEquals("aborted_streaming", (a.turn as TurnState.Done).why)
    }

    /** 封口发生在 `res` 那一刻：之前必须还是 Streaming（否则「强制封口」无从谈起）。 */
    @Test
    fun blocksStayOpenUntilResultArrives() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.BlockStart("msg_1", 0, "text", null, null))
        assertEquals(1, a.openBlockCount)
        assertTrue(a.turn is TurnState.Streaming)

        a.feed(BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null))
        assertEquals(0, a.openBlockCount)
    }

    // ---- 两个生产者的 key 空间不许相交 ------------------------------

    /**
     * 两个生产者的 key 必须不相交：`RecordClassifier`（`uuid#i`）与这里（`f:<m>#i`）。
     *
     * 不能靠「帧侧 key 以 `msg_` 开头」：`auth-fail` golden 的 `message_id` 是
     * `ffe57315-4539-469a-b287-976c20cda069`，SDK 出错时用的就是 UUID，与 classifier 的形状一模一样。
     * 所以遍历全部 7 个 vector（含那条 UUID 的），靠的是显式前缀这个代码性质，而不是远端 id 长什么样。
     */
    @Test
    fun keysFromBothProducersNeverCollide() {
        val allCases = listOf("text-short", "tool-call", "thinking", "auth-fail", "interrupt", "long-reply", "demo-debug")
        var sawUuidShapedMessageId = false
        val uuidShape = Regex("^[0-9a-f]{8}-[0-9a-f]{4}-")

        allCases.forEach { case ->
            val keys = assemble(case).units().map { it.key }
            assertTrue("$case 应产出 key", keys.isNotEmpty())
            keys.forEach { k ->
                assertTrue("$case: 帧侧 key 必须带显式前缀，实际 $k", k.startsWith(ChatTurnAssembler.KEY_PREFIX))
                assertFalse("$case: 带了前缀就不可能落进 uuid# 空间：$k", uuidShape.containsMatchIn(k))
            }
            if (rows(case).mapNotNull { (it.frame as? BridgeFrame.AssistantText)?.m }.any { uuidShape.containsMatchIn(it) }) {
                sawUuidShapedMessageId = true
            }
        }

        assertTrue(
            "前提：至少有一个 case 的 message_id 是 UUID 形——正是它让「靠形状区分」不成立",
            sawUuidShapedMessageId,
        )
    }

    // ---- 未知帧不丢弃 -----------------------------------------------------

    /** `ev` 与未知帧型都要留下痕迹：静默丢弃会让「远端升级了协议」在手机上毫无迹象。 */
    @Test
    fun unknownAndPassthroughFramesAreKeptNotDropped() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.Event("stream:message_start", null, mapOf("x" to 1)))
        a.feed(BridgeFrame.Unknown("brand_new_from_the_future", mapOf("t" to "brand_new_from_the_future")))

        assertEquals("两条都要留下", 2, a.unhandled.size)
        assertTrue(a.unhandled.any { it is BridgeFrame.Unknown })
    }

    /** 真数据核对：`long-reply` 的 `ev` 帧一条都不许丢。 */
    @Test
    fun realGoldenPassthroughCountIsPreserved() {
        val frames = rows("long-reply").map { it.frame }
        val evCount = frames.count { it is BridgeFrame.Event }
        assertTrue("前提：golden 里确实有 ev 帧", evCount > 0)
        assertEquals(evCount, assemble("long-reply").unhandled.count { it is BridgeFrame.Event })
    }

    // ---- 子 agent：collect-but-skip ------------------------------------------

    /**
     * `p`（parent_tool_use_id）非空的帧不许进主流，但也不许丢。
     *
     * 进主流 = 子 agent 干的活整段插进主对话，看起来像主 agent 说的。丢掉 = Task 工具的过程彻底看不见。
     * 所以是 collect-but-skip，与 `RecordClassifier` 对 sidechain 的处置一致。
     *
     * 六个 golden 都没有子 agent，所以这里是手造帧；`bridge/PROTOCOL.md` 的字段表记的是录到过的，因此也没有 `p`。
     */
    @Test
    fun subagentOutputIsCollectedButKeptOutOfTheMainFlow() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "主 agent 说的"))
        a.feed(BridgeFrame.AssistantText("msg_2", 0, "toolu_parent", "子 agent 说的"))

        val texts = a.units().filterIsInstance<RenderUnit.AssistantMarkdown>().map { it.markdown }
        assertEquals("主流里只许有主 agent 的正文", listOf("主 agent 说的"), texts)
        assertEquals("子 agent 的产物必须被收着", 1, a.sidechain["toolu_parent"]?.size)
    }

    // ---- 工具调用链 -----------------------------------------------------------

    /** 真数据：`tool-call` golden 的 `tu`→`tr` 必须合并成一张有结果的卡。 */
    @Test
    fun toolUseAndResultMergeIntoOneCompletedCall() {
        val calls = assemble("tool-call").units().filterIsInstance<RenderUnit.ToolCall>()
        assertTrue("golden 里有工具调用", calls.isNotEmpty())
        val done = calls.first { it.resultText != null }
        assertFalse("拿到结果就不该再 pending", done.pending)
        assertTrue("入参应来自 tu 的权威 inp", done.input.isNotEmpty())
    }

    /**
     * `tr` 先于 `tu` 到达（乱序）时必须回填到那张卡上，不是「留个痕」就算完。
     *
     * 只把孤儿 `tr` 丢进 `unhandled` 的话，`tu` 后到时那张卡照样 `pending=true`：一次失败的工具被显示成「进行中」。
     * 只断言 `unhandled.size == 1` 验不出后续会不会对上。
     */
    @Test
    fun toolResultArrivingBeforeItsToolUseIsBackfilledOntoTheCard() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.ToolResult("toolu_x", null, ok = false, x = "失败了", truncatedFullLength = null))
        a.feed(BridgeFrame.ToolUse("msg_1", 0, null, "toolu_x", "Bash", mapOf("cmd" to "ls")))
        a.feed(BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null))

        val call = a.units().filterIsInstance<RenderUnit.ToolCall>().single()
        assertEquals("先到的结果必须落到卡上", "失败了", call.resultText)
        assertTrue("失败必须显示成失败，不是「进行中」", call.isError)
        assertFalse("拿到结果就不该再 pending", call.pending)
    }

    /** 连 `id` 都没有的 `tr` 无处可挂：那才该留痕（不丢弃）。 */
    @Test
    fun toolResultWithoutAnIdIsKeptAsUnhandled() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.ToolResult(null, null, ok = true, x = "无主结果", truncatedFullLength = null))
        assertEquals(1, a.unhandled.size)
    }

    // ---- 支持往前插入 ----------------------------------

    /**
     * 顺序与身份分离 ⇒ 往顶部插一批更老的是 O(n) 插入，不是重建整个状态。
     *
     * 钉住的是设计：assembler 若被写成 append-only，这里会红。
     */
    @Test
    fun orderSupportsPrependingOlderUnitsWithoutRebuilding() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText("msg_new", 0, null, "新的"))
        a.prependOrder(listOf("${ChatTurnAssembler.KEY_PREFIX}msg_old#0"))
        a.feed(BridgeFrame.AssistantText("msg_old", 0, null, "老的"))

        assertEquals(
            "prepend 过的 key 必须排在前面",
            listOf("老的", "新的"),
            a.units().filterIsInstance<RenderUnit.AssistantMarkdown>().map { it.markdown },
        )
    }

    /** 重复 prepend 不许产生重复 key：上滑翻历史必然与已加载部分重叠。 */
    @Test
    fun prependingAnAlreadyKnownKeyDoesNotDuplicateIt() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "一"))
        a.prependOrder(listOf("${ChatTurnAssembler.KEY_PREFIX}msg_1#0", "${ChatTurnAssembler.KEY_PREFIX}msg_1#0"))

        val keys = a.units().map { it.key }
        assertEquals("key 重复会让 LazyColumn 当场抛异常", keys.size, keys.toSet().size)
    }

    // ---- init 与错误 ----------------------------------------------------------

    /** catalog 必须被解出来：它是命令/skill 选择器的地基。 */
    @Test
    fun initCatalogIsParsedFromRealGolden() {
        val init = assertNotNull(assemble("text-short").init).let { assemble("text-short").init!! }
        assertTrue("工具清单不该为空", init.tools.isNotEmpty())
        assertNotNull("cwd 应存在", init.cwd)
    }

    /** 认证失败：`err` 帧必须被抓住，且 `res.ok` 为 false（`subtype` 那时仍是 success）。 */
    @Test
    fun authFailureSurfacesBothErrorFrameAndFailedResult() {
        val a = assemble("auth-fail")
        assertNotNull("err 帧必须被记住", a.lastError)
        assertEquals("authentication_failed", a.lastError?.code)
        assertFalse("res.ok 必须是 false", (a.turn as TurnState.Done).ok)
    }

    // ---- 重放 ------------------------------------------------------------------

    /** 六个技术 case 全部可重放，且帧数与 golden 行数一致（`__meta__` 不算帧）。 */
    @Test
    fun allTechnicalCasesReplayWithFrameCountMatchingGolden() =
        runTest {
            for (case in listOf("text-short", "tool-call", "thinking", "auth-fail", "interrupt", "long-reply")) {
                val expected = rows(case)
                val got = ReplayTransport(expected).frames().toList()
                assertEquals("$case 重放帧数应与 golden 一致", expected.size, got.size)
                assertTrue("$case 不该出现未知帧型", got.none { it is BridgeFrame.Unknown })
            }
        }

    /** `__meta__` 行不是帧：当成帧解析会得到一个 Unknown 混进对话里。 */
    @Test
    fun metaRowIsNotParsedAsAFrame() {
        val raw = File(vectorsDir(), "text-short.frames.ndjson").readLines()
        assertTrue("前提：首行是 __meta__", raw.first().contains("__meta__"))
        assertEquals("解析出的行数应比文件少 1（少的正是 __meta__）", raw.count { it.isNotBlank() } - 1, rows("text-short").size)
    }

    /** 首帧不该等待：`t_ns` 是相对录制起点的绝对时刻，不是间隔。 */
    @Test
    fun replayDoesNotWaitBeforeTheFirstFrame() =
        runTest {
            val t0 = testScheduler.currentTime
            ReplayTransport(listOf(ReplayRow(5_000_000_000L, BridgeFrame.BlockEnd("m", 0)))).frames().toList()
            assertEquals("首帧不许有等待", t0, testScheduler.currentTime)
        }

    /** [Pacing.None] 给 Compose UI 测试用：完全不睡。 */
    @Test
    fun pacingNoneSkipsAllWaiting() =
        runTest {
            val t0 = testScheduler.currentTime
            ReplayTransport(rows("long-reply"), Pacing.None).frames().toList()
            assertEquals(t0, testScheduler.currentTime)
        }

    /** [Pacing.Original] 下，虚拟时间推进量应≈录制总时长：证明节奏是真按 `t_ns` 走的。 */
    @Test
    fun pacingOriginalAdvancesVirtualTimeByTheRecordedDuration() =
        runTest {
            val r = rows("long-reply")
            val expectedMs = (r.last().tNs - r.first().tNs) / 1_000_000
            val t0 = testScheduler.currentTime
            ReplayTransport(r, Pacing.Original).frames().toList()
            val actual = testScheduler.currentTime - t0
            assertTrue(
                "虚拟时间应≈录制时长（期望 ~${expectedMs}ms，实际 ${actual}ms）",
                actual in (expectedMs - r.size)..expectedMs,
            )
        }

    /** 坏行不许让整条流断掉，也不许静默变成空帧。 */
    @Test
    fun malformedLinesAreSkippedNotCrashing() {
        val parsed = ReplayVectors.parse(sequenceOf("", "不是 JSON", """{"t_ns":1,"frame":{"t":"be","m":"m","i":0}}"""))
        assertEquals(1, parsed.size)
        assertNull(BridgeCodec().decodeOrNull("半行 {"))
    }

    // ---- 已知的几种错误行为，各钉一条 ----------------

    /**
     * 多轮对话里，用户消息必须按到达顺序与助手块交错。
     *
     * 写成 `userTexts + 块` 的话，所有用户消息被顶到全部助手块之前：第二轮的提问排在第一轮回答的前面。
     * `demo-debug` 的 `res.turns=8`，多轮是常态；golden 里没有 `ut` 只是因为录到的 content 恰好都是 list。
     */
    @Test
    fun userMessagesInterleaveWithAssistantBlocksInArrivalOrder() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "第一轮回答"))
        a.feed(BridgeFrame.UserText(null, "第二轮提问"))
        a.feed(BridgeFrame.AssistantText("msg_2", 0, null, "第二轮回答"))

        assertEquals(
            "用户消息不许被顶到最前面",
            listOf("第一轮回答", "第二轮提问", "第二轮回答"),
            a.units().map {
                when (it) {
                    is RenderUnit.AssistantMarkdown -> it.markdown
                    is RenderUnit.UserText -> it.text
                    else -> "?"
                }
            },
        )
    }

    /** 用户消息也走同一条 order ⇒ 它同样支持往前插入。 */
    @Test
    fun userMessageKeysAlsoLiveInTheSharedOrderSoTheyCanBePrepended() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.UserText(null, "新的提问"))
        val olderKey = "${ChatTurnAssembler.KEY_PREFIX}msg_old#0"
        a.prependOrder(listOf(olderKey))
        a.feed(BridgeFrame.AssistantText("msg_old", 0, null, "更早的回答"))

        assertEquals(
            listOf("更早的回答", "新的提问"),
            a.units().map { if (it is RenderUnit.UserText) it.text else (it as RenderUnit.AssistantMarkdown).markdown },
        )
    }

    /**
     * 权威帧必须能纠正块类型；delta 只能建议。
     *
     * 判成 `if (kind != it.kind && kind != Kind.TEXT)` 的话 TEXT 永远纠正不了：`bs(bt="tool_use")` 后来了
     * `at(x="其实是正文")`，产出的是一张 `name="?" / pending=true` 的永久转圈卡，正文不显示。
     */
    @Test
    fun authoritativeFrameCanCorrectABlockTypeGuessedFromBlockStart() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.BlockStart("msg_1", 0, "tool_use", null, null))
        a.feed(BridgeFrame.AssistantText("msg_1", 0, null, "其实是正文"))

        val units = a.units()
        assertEquals("应当渲染成正文，而不是一张转圈的工具卡", 1, units.size)
        assertEquals("其实是正文", (units.single() as RenderUnit.AssistantMarkdown).markdown)
    }

    /** 从没见过工具身份的 TOOL 块不许渲染：否则是一张 `name="?"` 的永久转圈卡。 */
    @Test
    fun aToolBlockWithNoIdentityIsNotRenderedAsAnEmptySpinnerCard() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.InputJsonDelta("msg_1", 0, """{"partial":"""))
        assertTrue("没有 id / name 的工具块不该上屏：${a.units()}", a.units().isEmpty())
    }

    /**
     * `m` 缺失时，两条不同消息不许撞进同一个 key。
     *
     * key 若是 `"${'$'}{m ?: "?"}#${'$'}i"`，两条 `at(m=null, i=0)` 只剩一个 unit，第一条正文永久丢失。
     * 可达：编码器在第一个 `message_start` 之前没有 message id。
     */
    @Test
    fun blocksWithMissingMessageIdDoNotOverwriteEachOther() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.AssistantText(null, 0, null, "第一条"))
        a.feed(BridgeFrame.AssistantText(null, 0, null, "第二条"))

        val texts = a.units().filterIsInstance<RenderUnit.AssistantMarkdown>().map { it.markdown }
        assertEquals("两条都必须留下", listOf("第一条", "第二条"), texts)
    }

    /**
     * 路由判据只有一处：[ChatTurnAssembler.route]。
     *
     * ViewModel 若自己再判一遍「这帧是什么、key 是什么」，assembler 把子 agent 的 `at` 分流进 sidechain、
     * ViewModel 却拿它去覆盖主流缓冲，已读的正文当场倒退。这条钉住「结论由 assembler 出」。
     */
    @Test
    fun routeIsTheSingleSourceOfTruthForWhereAFrameBelongs() {
        val a = ChatTurnAssembler()
        val main = BridgeFrame.AssistantText("msg_1", 0, null, "主流")
        val sub = BridgeFrame.AssistantText("msg_2", 0, "toolu_parent", "子 agent")

        assertEquals(FrameRoute.MainBlock("${ChatTurnAssembler.KEY_PREFIX}msg_1#0"), a.route(main))
        assertEquals(FrameRoute.Sidechain("toolu_parent"), a.route(sub))
        assertTrue(
            "res 不属于任何块",
            a.route(BridgeFrame.Result("s", true, null, 1, null, null, null, null, null, null, null)) is FrameRoute.Session,
        )
    }

    /** `tr` 的归属靠 tool_use_id 反查：它没有 (m,i) 坐标。 */
    @Test
    fun toolResultRoutesToTheBlockItsToolUseCreated() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.ToolUse("msg_1", 0, null, "toolu_a", "Read", emptyMap()))
        val tr = BridgeFrame.ToolResult("toolu_a", null, ok = true, x = "内容", truncatedFullLength = null)
        assertEquals(FrameRoute.MainBlock("${ChatTurnAssembler.KEY_PREFIX}msg_1#0"), a.route(tr))
    }

    // ---- prependFrames：向前插入 ------------------------------

    /**
     * 分两段载 == 一次全载：与 `RecordClassifier` 同一条核心断言。
     *
     * 两个生产者都过这条，「两个生产者、一个渲染器」才成立。用真 golden 的帧流切开验，不是手编样例。
     */
    @Test
    fun twoPhaseLoadEqualsSinglePhaseOnRealFrames() {
        listOf("tool-call", "thinking", "long-reply", "demo-debug").forEach { case ->
            val frames = rows(case).map { it.frame }
            val splitAt = frames.size / 2

            val oneShot = ChatTurnAssembler().also { a -> frames.forEach { a.feed(it) } }.units()
            val twoPhase =
                ChatTurnAssembler()
                    .also { a ->
                        frames.drop(splitAt).forEach { a.feed(it) }
                        a.prependFrames(frames.take(splitAt))
                    }.units()

            assertEquals("$case：单元数应相等", oneShot.size, twoPhase.size)
            oneShot.zip(twoPhase).forEachIndexed { i, (x, y) ->
                assertEquals("$case：第 $i 个单元的 key 不同", x.key, y.key)
                assertEquals("$case：第 $i 个单元不相等（key=${x.key}）", x, y)
            }
        }
    }

    /** 重叠区间不许产生重复 key（真实字节区间按行边界取，接缝必然多带几条）。 */
    @Test
    fun prependingAnOverlappingRangeDoesNotDuplicateKeys() {
        val frames = rows("tool-call").map { it.frame }
        val splitAt = frames.size / 2
        val a = ChatTurnAssembler()
        frames.drop(splitAt).forEach { a.feed(it) }
        a.prependFrames(frames.take(splitAt + 3)) // 故意多带 3 条

        val keys = a.units().map { it.key }
        assertEquals("重叠不许产生重复 key：$keys", keys.size, keys.toSet().size)
        assertEquals(
            "去重后应与一次全载等价",
            ChatTurnAssembler().also { x -> frames.forEach { x.feed(it) } }.units().size,
            keys.size,
        )
    }

    /**
     * 老段自带的 `res` 不许改写当前轮次状态。
     *
     * 更早那一段必然有它自己的 `res`（那轮早结束了）。照常 feed 会让 turn 退回成老的 Done，
     * 并把正在流式的块强制封口：上滑看历史，正在生成的回答当场停住。
     */
    @Test
    fun prependingOlderFramesDoesNotEndTheCurrentTurn() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.BlockStart("msg_now", 0, "text", null, null))
        a.feed(BridgeFrame.TextDelta("msg_now", 0, "正在说……"))
        assertTrue("前提：此刻这一轮还没结束", a.turn is TurnState.Streaming)
        assertEquals("前提：有块开着", 1, a.openBlockCount)

        a.prependFrames(
            listOf(
                BridgeFrame.AssistantText("msg_old", 0, null, "很早以前的回答"),
                BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null),
            ),
        )

        assertTrue("老段的 res 不许让当前轮次收口", a.turn is TurnState.Streaming)
        assertEquals("正在流式的块不许被老段强制封口", 1, a.openBlockCount)
        assertEquals(
            "老段的正文应排在前面",
            "很早以前的回答",
            (a.units().first() as RenderUnit.AssistantMarkdown).markdown,
        )
    }

    /**
     * `ut` 帧在接缝重叠时不许渲染两次。
     *
     * `ut` 是协议里唯一没有身份的帧（只有正文，没有 uuid/message_id）；拿到达序号当 key 的话，
     * 老段与新段重叠的那条用户消息会出现两遍。golden 里一个 `ut` 帧都没有，别的测试看不见这个洞。
     */
    @Test
    fun overlappingUserTextFramesAreNotRenderedTwice() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.UserText(null, "接缝上的那句"))
        a.feed(BridgeFrame.AssistantText("msg_new", 0, null, "新回答"))

        // 老段的末尾按行边界必然多带上「接缝上的那句」
        val added =
            a.prependFrames(
                listOf(
                    BridgeFrame.UserText(null, "更早的提问"),
                    BridgeFrame.UserText(null, "接缝上的那句"),
                ),
            )

        val texts = a.units().filterIsInstance<RenderUnit.UserText>().map { it.text }
        assertEquals("接缝那句不许出现两遍", listOf("更早的提问", "接缝上的那句"), texts)
        assertEquals("只该新增 1 条", 1, added)
        val keys = a.units().map { it.key }
        assertEquals("key 不许重复", keys.size, keys.toSet().size)
    }

    /** 老段全是已载过的 ⇒ 返回 0，上层据此判「真的到顶了」。 */
    @Test
    fun prependingOnlyKnownFramesReportsZeroSoCallersCanStop() {
        val a = ChatTurnAssembler()
        val frames = rows("tool-call").map { it.frame }
        frames.forEach { a.feed(it) }
        assertEquals("全是已知的 ⇒ 新增 0", 0, a.prependFrames(frames))
    }

    /**
     * prepend 老段不许打散正在流式的块。
     *
     * 老段里 `m == null` 的块会抬高 `lastNullIndex`；之后实时流里同样 `m == null` 的 `i=0` 再来就满足
     * `i <= lastNullIndex` → 开新分代 → 正在流的块当场换 key 分裂，缓释器随即清零重放 = 已读正文倒退。
     * prepend 期间必须用独立的分代空间。
     */
    @Test
    fun prependingBlocksWithoutMessageIdDoesNotSplitTheLiveStreamingBlock() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.TextDelta(null, 0, "正在流的"))
        val liveKey = a.units().single().key

        a.prependFrames(
            listOf(
                BridgeFrame.AssistantText(null, 0, null, "老段第一条"),
                BridgeFrame.AssistantText(null, 0, null, "老段第二条"),
            ),
        )
        a.feed(BridgeFrame.TextDelta(null, 0, "内容继续"))

        val live = a.units().last()
        assertEquals("正在流的块不许换 key", liveKey, live.key)
        assertEquals("内容应继续累加而不是被分裂重来", "正在流的内容继续", (live as RenderUnit.AssistantMarkdown).markdown)
    }

    /** 老段自带的 `init` 不许覆盖当前 catalog。 */
    @Test
    fun prependingDoesNotOverwriteTheCurrentInitCatalog() {
        val a = ChatTurnAssembler()
        a.feed(BridgeFrame.Init("now", "opus", "/now", listOf("a"), emptyList(), listOf("Bash"), emptyList(), emptyList(), emptyList(), null))
        a.prependFrames(
            listOf(
                BridgeFrame.Init("old", "sonnet", "/old", listOf("z"), emptyList(), emptyList(), emptyList(), emptyList(), emptyList(), null),
            ),
        )
        assertEquals("catalog 必须还是当前这份", "/now", a.init?.cwd)
    }

    /**
     * 子 agent 的流式增量不许污染主流。
     *
     * golden 里没有 Task 子 agent 的样本（工具名只有 Bash/Edit/Read），所以用构造输入，
     * 字段名与形状照真实的 `stream_event` 行（`parent_tool_use_id` 与 `event` 同级）。
     *
     * 判据落在路由结论上，不是「帧里有没有 p 字段」：带 `p` 的增量必须判成 sidechain，否则上层切流时
     * 会把主块已露出的正文清零重来，而子 agent 的权威全文 `at` 因为带 `p` 被分流走 ⇒ 那个块永远得不到纠正。
     */
    @Test
    fun aSubagentDeltaIsRoutedToTheSidechainNotTheMainBlock() {
        val asm = ChatTurnAssembler()
        // 主流那条消息先露出一段正文
        asm.feed(BridgeFrame.TextDelta(m = "m-main", i = 0, x = "主流已经露出来的正文", p = null))
        val mainRoute = asm.route(BridgeFrame.TextDelta(m = "m-main", i = 0, x = "更多", p = null))
        assertTrue("前提：主流的增量走主流块", mainRoute is FrameRoute.MainBlock)

        // 子 agent 的增量：同样是 delta，只是带 p
        val subRoute = asm.route(BridgeFrame.TextDelta(m = "m-sub", i = 0, x = "子 agent 的字", p = "toolu_x"))
        assertTrue("带 p 的增量必须分流走，否则会清掉主块已露出的正文：$subRoute", subRoute is FrameRoute.Sidechain)
        assertEquals("要认得出是哪个子 agent", "toolu_x", (subRoute as FrameRoute.Sidechain).parentToolUseId)
    }

    /** 五个流式帧都要能分流：漏一个，那一种增量就照样污染主流。 */
    @Test
    fun allFiveStreamingFrameTypesCanBeRoutedToTheSidechain() {
        val asm = ChatTurnAssembler()
        val frames =
            listOf<BridgeFrame>(
                BridgeFrame.BlockStart(m = "m", i = 0, bt = "text", id = null, name = null, p = "toolu_x"),
                BridgeFrame.TextDelta(m = "m", i = 0, x = "a", p = "toolu_x"),
                BridgeFrame.ThinkingDelta(m = "m", i = 0, x = "b", p = "toolu_x"),
                BridgeFrame.InputJsonDelta(m = "m", i = 0, x = "c", p = "toolu_x"),
                BridgeFrame.BlockEnd(m = "m", i = 0, p = "toolu_x"),
            )
        for (f in frames) {
            assertTrue("${f.t} 带 p 时必须分流走", asm.route(f) is FrameRoute.Sidechain)
        }
        // 前提断言：这五个确实是不同的帧型，否则上面那圈没验到五种
        assertEquals("前提：五个帧型各不相同", 5, frames.map { it.t }.toSet().size)
    }

    /** 不带 `p` 的走主流（主流消息里 `p` 恒为 null）。 */
    @Test
    fun aMainFlowStreamingFrameStillRoutesToTheMainBlock() {
        val asm = ChatTurnAssembler()
        for (
        f in
        listOf<BridgeFrame>(
            BridgeFrame.BlockStart(m = "m", i = 0, bt = "text", id = null, name = null),
            BridgeFrame.TextDelta(m = "m", i = 0, x = "a"),
            BridgeFrame.BlockEnd(m = "m", i = 0),
        )
        ) {
            assertTrue("${f.t} 不带 p ⇒ 主流", asm.route(f) is FrameRoute.MainBlock)
        }
    }
}
