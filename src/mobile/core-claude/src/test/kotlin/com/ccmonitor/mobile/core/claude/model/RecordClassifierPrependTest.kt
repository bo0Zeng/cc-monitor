package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * [RecordClassifier.classifyPrepend]：翻更早历史时的向前插入路径。
 *
 * 核心断言是「分两段载 == 一次全载」，同时覆盖去重、顺序、边界三件事，纯 JVM 就能跑；
 * prepend 改坏了它会立刻红，而不是等到真机上「上滑之后有条工具卡永远转圈」。
 *
 * 数据用本机真实 JSONL，不手编：手编样例跟着理解走，不跟着现实走。找不到就跳过并说出来。
 */
class RecordClassifierPrependTest {
    /**
     * 取本机尽量大的一个真实会话：分页的价值恰恰在大会话上。
     *
     * 上限是 [MAX_BYTES]，逐行流式解析（`useLines`）而不是 `readLines()` 一次读进内存：
     * Gradle test 默认 512m 堆，几百 MB 的会话整读会 OOM。
     */
    private fun realSession(): List<JsonlRecord>? {
        val roots =
            listOf(
                File(System.getProperty("user.home"), ".claude/projects"),
                File(System.getProperty("user.home"), ".config/claude/projects"),
            )
        val jsonl =
            roots
                .asSequence()
                .filter { it.isDirectory }
                .flatMap { it.walkTopDown().maxDepth(3) }
                .filter { it.isFile && it.name.endsWith(".jsonl") && it.length() in MIN_BYTES..MAX_BYTES }
                .sortedByDescending { it.length() }
                .firstOrNull() ?: return null
        val records = jsonl.bufferedReader().useLines { lines -> lines.mapNotNull { JsonlParser.parse(it) }.toList() }
        return MainBranch.resolve(records).takeIf { it.size >= MIN_RECORDS }
    }

    /** 手编的最小样例：只用来把「分两段 == 一次」这条规则本身钉住（不替代真数据那条）。 */
    private fun synthetic(): List<JsonlRecord> {
        val records = mutableListOf<JsonlRecord>()
        var prev: String? = null
        repeat(SYNTHETIC_TURNS) { i ->
            val callUuid = "u-call-$i"
            records +=
                JsonlRecord.Assistant(
                    uuid = callUuid,
                    parentUuid = prev,
                    blocks =
                        listOf(
                            ContentBlock.Text("第 $i 轮的回答"),
                            ContentBlock.ToolUse("toolu_$i", "Bash", mapOf("cmd" to "echo $i")),
                        ),
                    model = "m",
                    // 注意：必须给 timestamp。`MainBranch.extractBranchRecord` 见到 null 就整条丢弃，
                    // resolve 返回空列表，样例静默变成 0 条。
                    timestamp = "2026-08-01T00:0$i:00Z",
                    sessionId = null,
                    isApiError = false,
                )
            val resUuid = "u-res-$i"
            records +=
                JsonlRecord.User(
                    uuid = resUuid,
                    parentUuid = callUuid,
                    blocks = listOf(ContentBlock.ToolResult("toolu_$i", "输出 $i", false)),
                    isMeta = false,
                    timestamp = "2026-08-01T00:0$i:30Z",
                    sessionId = null,
                    cwd = null,
                )
            prev = resUuid
        }
        return MainBranch.resolve(records)
    }

    private fun assertTwoPhaseEqualsSinglePhase(
        all: List<JsonlRecord>,
        splitAt: Int,
        label: String,
    ) {
        val older = all.subList(0, splitAt)
        val newer = all.subList(splitAt, all.size)

        val oneShot = RecordClassifier.classify(all)
        val twoPhase =
            RecordClassifier.classifyPrepend(
                RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, newer, collectOrphans = true),
                older,
            )

        assertEquals("$label：单元数应相等", oneShot.size, twoPhase.units.size)
        oneShot.zip(twoPhase.units).forEachIndexed { i, (a, b) ->
            assertEquals("$label：第 $i 个单元的 key 不同", a.key, b.key)
            assertEquals("$label：第 $i 个单元不相等（key=${a.key}）", a, b)
        }
    }

    // ---- 核心断言 ---------------------------------------------------------------

    /** 手编样例：分两段载与一次全载逐项相等。 */
    @Test
    fun twoPhaseLoadEqualsSinglePhaseOnSyntheticSession() {
        val all = synthetic()
        assertTrue("前提：样例要够长才切得开", all.size >= MIN_SPLIT * 2)
        assertTwoPhaseEqualsSinglePhase(all, all.size / 2, "合成会话")
    }

    /**
     * 真实会话上的同一条断言，在多个切点上验，不是只切一刀。
     *
     * 只切一刀很容易恰好避开「接缝正好落在 tool_use 和它的 tool_result 之间」那个要命的情况。
     */
    @Test
    fun twoPhaseLoadEqualsSinglePhaseOnARealSessionAtManySplitPoints() {
        val all = realSession()
        // 用 Assume 而不是 `return`：没数据时报告成 skipped，不伪装成 passed。
        assumeTrue("本机找不到合适的真实会话 JSONL（这条依赖真数据）", all != null)
        requireNotNull(all)
        assertTrue("前提：真会话记录数 ${all.size}", all.size >= MIN_RECORDS)

        var checked = 0
        for (splitAt in MIN_SPLIT until all.size step (all.size / SPLIT_SAMPLES).coerceAtLeast(1)) {
            assertTwoPhaseEqualsSinglePhase(all, splitAt, "真会话 split=$splitAt")
            checked++
        }
        assertTrue("必须真的验了多个切点，实际 $checked", checked >= 3)
    }

    // ---- 三件必须做对的事，各钉一条 ----------------------------------------------

    /**
     * 孤儿 tool_result 必须被回填：这是「分两段 == 一次」最容易失败的地方。
     *
     * 窗口起点切在 `tool_use` 与它的 `tool_result` 之间时，新段里的结果找不到它的调用。
     * 查不到就丢弃的话，上滑把老段载进来之后那张卡永远 pending。
     */
    @Test
    fun anOrphanToolResultIsBackfilledOnceItsToolUseIsPrepended() {
        val all = synthetic()
        // 切在第一个 tool_use 之后、它的 tool_result 之前
        val newerFirst = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, all.subList(1, all.size), collectOrphans = true)
        assertTrue(
            "前提：这个切点必须真的产生孤儿结果，否则这条测试什么都没测",
            newerFirst.orphanResults.isNotEmpty(),
        )

        val merged = RecordClassifier.classifyPrepend(newerFirst, all.subList(0, 1))
        val firstCall = merged.units.filterIsInstance<RenderUnit.ToolCall>().first()
        assertFalse("老段载进来之后，那张卡不该还 pending", firstCall.pending)
        assertEquals("结果文本要回填对", "输出 0", firstCall.resultText)
    }

    /**
     * 索引平移：`toolIndexById` 存的是下标，前面插了 k 条之后旧下标全部 +k。
     *
     * 忘了平移的表现是迟到的 tool_result 回填到了错误的那张卡上：
     * 显示错乱但不崩，最难发现。
     */
    @Test
    fun toolIndicesAreShiftedSoLateResultsStillLandOnTheRightCard() {
        val all = synthetic()
        val splitAt = all.size / 2
        val merged =
            RecordClassifier.classifyPrepend(
                RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, all.subList(splitAt, all.size), collectOrphans = true),
                all.subList(0, splitAt),
            )
        merged.toolIndexById.forEach { (id, idx) ->
            val unit = merged.units[idx]
            assertTrue("toolIndexById[$id]=$idx 指向的不是 ToolCall 而是 $unit", unit is RenderUnit.ToolCall)
            assertEquals("下标指错了卡", id, (unit as RenderUnit.ToolCall).toolUseId)
        }
    }

    /**
     * 边界去重：翻页区间与已载区间在接缝处必然重叠（按行边界取的字节区间会多带一条）。
     *
     * 去重按 uuid 而不是按位置：位置在 prepend 之后全变了。
     * 不去重的话 `LazyColumn(items(key))` 撞 key 当场抛异常（列表 key 必须唯一）。
     */
    @Test
    fun overlappingRangesAreDedupedByUuidNotByPosition() {
        val all = synthetic()
        val splitAt = all.size / 2
        val newer = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, all.subList(splitAt, all.size), collectOrphans = true)
        // 故意让老段与新段重叠两条（真实字节区间就是这样多带的）
        val olderWithOverlap = all.subList(0, splitAt + 2)

        val merged = RecordClassifier.classifyPrepend(newer, olderWithOverlap)
        val keys = merged.units.map { it.key }
        assertEquals("重叠的记录不许产生重复 key：$keys", keys.size, keys.toSet().size)
        assertEquals("去重之后应与一次全载等价", RecordClassifier.classify(all).size, merged.units.size)
    }

    /** 老段全都已经载过 ⇒ 原样返回，不做无谓重建。 */
    @Test
    fun prependingOnlyAlreadyKnownRecordsIsANoop() {
        val all = synthetic()
        val state = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, all)
        val again = RecordClassifier.classifyPrepend(state, all)
        assertTrue("完全重叠应原样返回同一个状态", again === state)
    }

    /**
     * 孤儿挂起是 opt-in：不翻历史的调用方（阅读面）不该被塞一份只写不读的驻留。
     * 这条同时钉住「默认关」，默认换成 true 时会红。
     */
    @Test
    fun orphanCollectionIsOptInSoTheReadingPanePaysNothing() {
        val all = synthetic()
        val tail = all.subList(1, all.size)
        val off = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, tail)
        val on = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, tail, collectOrphans = true)

        assertTrue("前提：这个切点确实会产生孤儿", on.orphanResults.isNotEmpty())
        assertTrue("默认不该挂起任何孤儿", off.orphanResults.isEmpty())
        assertEquals("开不开都不该影响渲染结果", off.units, on.units)
    }

    private companion object {
        const val MIN_BYTES = 60_000L

        /** 上限 300MB：要让大会话（几百 MB）也进得来。 */
        const val MAX_BYTES = 300_000_000L
        const val MIN_RECORDS = 40
        const val MIN_SPLIT = 4
        const val SPLIT_SAMPLES = 6
        const val SYNTHETIC_TURNS = 8
    }
}
