package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.bridge.ChatTurnAssembler
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * `RenderUnit` 只做 additive-with-default 演进。
 *
 * `RenderUnit` 的实例会从很早录的 golden 重放出来。给某个变体加一个没有默认值的必填字段，
 * 编译期只打红当前的构造点；构造路径上「顺手补一个值」就能编译过、测试也过，但语义悄悄变了：
 * 老素材重放出来的单元带上了一个它录制时根本不存在的字段值。
 *
 * 违反方式与谁守：
 * - 给已有变体加没有默认值的字段 / 删字段 / 改类型 / 删变体：编译器（这里的具名构造同时红）。
 * - 加新变体却不登记：[everyVariantIsConstructibleFromItsRequiredFieldsAlone] 的前提断言。
 * - 加带默认值的字段，但生产者重放老素材时也给它填了值：[replayingTheOldestGoldenNeverPopulatesAnyDefaultedField]。
 *
 * 守不住的：改掉既有可选字段的默认值（如 `null` → `emptyList()`，两边都拿新默认值 ⇒ 全绿）；
 * 字段名与类型都没变、含义变了。
 */
class RenderUnitAdditiveTest {
    private fun vectorsDir(): File =
        File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    /** 仓里的帧 golden，是最老的素材。 */
    private fun oldestCases(): List<File> =
        vectorsDir()
            .listFiles { f -> f.name.endsWith(".frames.ndjson") }
            .orEmpty()
            .sortedBy { it.name }

    /**
     * 最老的 golden 重放出来仍然是完整的渲染单元。
     *
     * 判据写成关系式（每个单元都有 key、key 互不相同、正文非空的那些确实有正文），
     * 不钉具体条数：钉条数是钉素材。
     */
    @Test
    fun theOldestGoldenVectorsStillReplayIntoCompleteRenderUnits() {
        val cases = oldestCases()
        assertTrue("前提：仓里得有 golden 素材", cases.isNotEmpty())

        for (case in cases) {
            val rows = case.bufferedReader().useLines { ReplayVectors.parse(it) }
            assertTrue("「${case.name}」应能解析出帧", rows.isNotEmpty())

            val asm = ChatTurnAssembler()
            rows.forEach { asm.feed(it.frame) }
            val units = asm.units()
            assertTrue("「${case.name}」应装配出渲染单元", units.isNotEmpty())

            // 每个单元都必须有 key：没有 key 的单元进不了 `LazyColumn items(key)`
            assertTrue("「${case.name}」每个单元都要有 key", units.all { it.key.isNotEmpty() })
            // key 互不相同：撞了 `LazyColumn` 当场抛
            assertEquals("「${case.name}」key 不许重复", units.size, units.map { it.key }.toSet().size)
        }
    }

    /**
     * 每个 `RenderUnit` 变体都必须能只用必填字段构造出来。
     *
     * 这是 additive-with-default 的可执行含义：新加的字段必须带默认值，于是不传它也能构造，
     * 老素材重放时走的正是这条路。这条的红法是编译不过：谁给某个变体加了没有默认值的字段，
     * 这个文件当场编译失败，错误信息直接点名哪个变体、缺哪个参数。
     * 加带默认值的字段时这里不用改。
     */
    @Test
    fun everyVariantIsConstructibleFromItsRequiredFieldsAlone() {
        val units: List<RenderUnit> =
            listOf(
                RenderUnit.UserText(key = "k", text = "t", sourceUuid = null),
                RenderUnit.AssistantMarkdown(key = "k", markdown = "m", sourceUuid = null),
                RenderUnit.Thinking(key = "k", text = "t", sourceUuid = null),
                RenderUnit.ToolCall(
                    key = "k",
                    toolUseId = "t1",
                    name = "Bash",
                    input = emptyMap(),
                    resultText = null,
                    isError = false,
                    pending = false,
                    sourceUuid = null,
                ),
                RenderUnit.SlashCommand(key = "k", name = "model", args = "", sourceUuid = null),
                RenderUnit.BashInput(key = "k", command = "ls", sourceUuid = null),
                RenderUnit.BashOutput(key = "k", stdout = "", stderr = "", sourceUuid = null),
                RenderUnit.CompactSummary(key = "k", text = "t", sourceUuid = null),
                RenderUnit.ToolGroup(key = "k", calls = emptyList(), sourceUuid = null),
            )
        // 前提断言：这份清单要真的覆盖到全部变体，否则漏掉的那个加必填字段时没人拦
        val declared =
            RenderUnit::class
                .java.declaredClasses
                .map { it.simpleName }
                .filter { it.isNotEmpty() }
                .toSet()
        val covered = units.map { it::class.java.simpleName }.toSet()
        assertTrue(
            "前提：清单要覆盖全部变体。漏了：${declared - covered}",
            declared.isEmpty() || declared.all { it in covered },
        )
        assertTrue("每个都要造得出来", units.all { it.key.isNotEmpty() })
    }

    // ────────────────────────────────────────────────────────────────────────
    // 老素材重放不许带上带默认值字段的非默认值
    // ────────────────────────────────────────────────────────────────────────

    /**
     * 把一个单元剥回只有必填字段的形状：只用没有默认值的那些参数重造一遍。
     *
     * 这个 `when` 刻意不写 `else`：[RenderUnit] 是 sealed，加了新变体这里当场编译不过。
     * 加带默认值的新字段时这里不用改，所以维护成本是零。
     */
    private fun strippedToRequired(u: RenderUnit): RenderUnit =
        when (u) {
            is RenderUnit.UserText -> RenderUnit.UserText(key = u.key, text = u.text, sourceUuid = u.sourceUuid)
            is RenderUnit.AssistantMarkdown ->
                RenderUnit.AssistantMarkdown(key = u.key, markdown = u.markdown, sourceUuid = u.sourceUuid)
            is RenderUnit.Thinking -> RenderUnit.Thinking(key = u.key, text = u.text, sourceUuid = u.sourceUuid)
            is RenderUnit.ToolCall -> strippedCallToRequired(u)
            is RenderUnit.SlashCommand ->
                RenderUnit.SlashCommand(key = u.key, name = u.name, args = u.args, sourceUuid = u.sourceUuid)
            is RenderUnit.BashInput -> RenderUnit.BashInput(key = u.key, command = u.command, sourceUuid = u.sourceUuid)
            is RenderUnit.BashOutput ->
                RenderUnit.BashOutput(key = u.key, stdout = u.stdout, stderr = u.stderr, sourceUuid = u.sourceUuid)
            is RenderUnit.CompactSummary ->
                RenderUnit.CompactSummary(key = u.key, text = u.text, sourceUuid = u.sourceUuid)
            // 组卡里的每个 call 也要剥：不剥的话组卡里藏着的新字段值会原样通过 equals
            is RenderUnit.ToolGroup ->
                RenderUnit.ToolGroup(key = u.key, calls = u.calls.map { strippedCallToRequired(it) }, sourceUuid = u.sourceUuid)
        }

    private fun strippedCallToRequired(u: RenderUnit.ToolCall): RenderUnit.ToolCall =
        RenderUnit.ToolCall(
            key = u.key,
            toolUseId = u.toolUseId,
            name = u.name,
            input = u.input,
            resultText = u.resultText,
            isError = u.isError,
            pending = u.pending,
            sourceUuid = u.sourceUuid,
        )

    /**
     * 这个类有没有带默认值的构造参数：用 Kotlin 生成默认参数时的调用约定判断：
     * 带默认参数的构造函数会多出一个 `DefaultConstructorMarker` 形参。
     *
     * 用它做命中自证（下面那条判据的前提），不用它做判定：
     * 判定靠 `equals`，这里只回答「刚才那轮重放到底看没看到带可选字段的变体」。
     * 探测器本身由 [theDefaultedFieldDetectorActuallyTellsThemApart] 自检。
     */
    private fun hasDefaultedFields(c: Class<*>): Boolean =
        c.declaredConstructors.any { ctor ->
            ctor.parameterTypes.any { it.name == "kotlin.jvm.internal.DefaultConstructorMarker" }
        }

    private data class HasADefault(
        val a: String,
        val b: String? = null,
    )

    private data class HasNoDefault(
        val a: String,
    )

    /**
     * 探测器自检：[hasDefaultedFields] 真的分得出两者。
     *
     * 不验的话，命中自证可能因为「探测器恒返回 false」而静默失效。
     * 用这里自己的两个小类做样本，不用 [RenderUnit] 的变体：给变体加可选字段时这条不会被误伤。
     */
    @Test
    fun theDefaultedFieldDetectorActuallyTellsThemApart() {
        assertTrue("带默认值的必须被认出来", hasDefaultedFields(HasADefault::class.java))
        assertFalse("不带默认值的不许被认成带", hasDefaultedFields(HasNoDefault::class.java))
    }

    /**
     * 最老的 golden 重放出来的每个单元，都必须等于「只用它的必填字段重造」的那个。
     *
     * 新字段带默认值只是第一步，生产者在重放老素材时也不许去填它：填了，老素材就带上了一个
     * 录制时不存在的值，而编译器和上面两条判据对此全部恒绿。
     * 红了该问的是「这个值是从老帧里读出来的，还是替它编的」。前者要换 golden，后者是违反。
     *
     * 射程：
     * 1. 只覆盖 bridge 帧那条生产线（`ChatTurnAssembler` ← `*.frames.ndjson`）。JSONL 那条
     *    （`RecordClassifier`）刻意不覆盖：老记录里本来就可能带着 `structuredPatch`，套上去是假阳。
     * 2. 改掉既有可选字段的默认值它抓不到（两边都拿新默认值）。
     * 3. 这批 golden 只重放出 `AssistantMarkdown` / `ToolCall` / `Thinking` 三种，真正被检查到可选字段的
     *    只有 `ToolCall`（`structuredPatch` / `patchFilePath`）；`UserText` 的 `delivery` / `deliveryError`
     *    没有 golden 走到（由 UI 层填）。要覆盖得先录一份带用户消息帧的 golden。
     */
    @Test
    fun replayingTheOldestGoldenNeverPopulatesAnyDefaultedField() {
        val cases = oldestCases()
        assertTrue("前提：仓里得有 golden 素材", cases.isNotEmpty())

        val seen = LinkedHashSet<Class<*>>()
        for (case in cases) {
            val rows = case.bufferedReader().useLines { ReplayVectors.parse(it) }
            val asm = ChatTurnAssembler()
            rows.forEach { asm.feed(it.frame) }
            for (u in asm.units()) {
                seen += u.javaClass
                assertEquals(
                    "「${case.name}」重放出来的 ${u.javaClass.simpleName} 带上了带默认值字段的非默认值：" +
                        "additive-with-default 要求老素材重放时那些字段仍是默认值",
                    strippedToRequired(u),
                    u,
                )
            }
        }

        // 命中自证：这轮重放必须真的看到过「带可选字段」的变体，否则上面那圈 assertEquals 是空跑。
        assertTrue(
            "前提：重放结果里得有至少一个带可选字段的变体，否则这条判据什么都没检查。看到的是：" +
                seen.joinToString { it.simpleName },
            seen.any { hasDefaultedFields(it) },
        )
    }
}
