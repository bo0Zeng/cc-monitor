package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

/** 增量分类必须与全量分类逐步等价，且 append-only 常态复用旧 unit（不重建）。 */
class RecordClassifierIncrementalTest {
    private fun user(
        vararg blocks: ContentBlock,
        uuid: String,
        meta: Boolean = false,
    ) = JsonlRecord.User(uuid, null, blocks.toList(), meta, null, null, null)

    private fun assistant(
        vararg blocks: ContentBlock,
        uuid: String,
    ) = JsonlRecord.Assistant(uuid, null, blocks.toList(), "m", null, null, false)

    /** 逐个前缀喂增量，每一步断言 == 对该前缀的全量分类。 */
    private fun assertIncrementalMatchesFull(records: List<JsonlRecord>) {
        var state = RecordClassifier.IncrementalState.EMPTY
        for (n in 1..records.size) {
            val prefix = records.subList(0, n)
            state = RecordClassifier.classifyIncremental(state, prefix)
            assertEquals("前缀 $n 增量应等于全量", RecordClassifier.classify(prefix), state.units)
        }
    }

    @Test fun appendOnlyMatchesFull() {
        assertIncrementalMatchesFull(
            listOf(
                user(ContentBlock.Text("q1"), uuid = "u1"),
                assistant(ContentBlock.Text("a1"), uuid = "a1"),
                user(ContentBlock.Text("q2"), uuid = "u2"),
                assistant(ContentBlock.Text("a2"), uuid = "a2"),
            ),
        )
    }

    @Test fun lateToolResultReconcilesIncrementally() {
        // tool_use 先出现（pending），中间夹一条 user，最后 tool_result 迟到回填——增量须与全量一致。
        assertIncrementalMatchesFull(
            listOf(
                assistant(ContentBlock.ToolUse("t1", "Bash", mapOf("command" to "ls")), uuid = "a1"),
                user(ContentBlock.Text("meanwhile"), uuid = "u1"),
                user(ContentBlock.ToolResult("t1", "out", false), uuid = "u2"),
            ),
        )
    }

    @Test fun branchFlipFallsBackToFull() {
        // prev onMain=[u1,a1]；翻转后=[u1,a2]（第 2 条不同 uuid）→ 前缀不匹配 → 回退全量，结果正确。
        val prev =
            RecordClassifier.classifyIncremental(
                RecordClassifier.IncrementalState.EMPTY,
                listOf(user(ContentBlock.Text("q"), uuid = "u1"), assistant(ContentBlock.Text("old"), uuid = "a1")),
            )
        val flipped = listOf(user(ContentBlock.Text("q"), uuid = "u1"), assistant(ContentBlock.Text("new"), uuid = "a2"))
        val result = RecordClassifier.classifyIncremental(prev, flipped)
        assertEquals(RecordClassifier.classify(flipped), result.units)
        assertEquals("new", (result.units[1] as RenderUnit.AssistantMarkdown).markdown)
    }

    @Test fun emptyInputMatchesFull() {
        val s = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, emptyList())
        assertEquals(emptyList<RenderUnit>(), s.units)
    }

    @Test fun noUnitRecordsInSequenceMatchFull() {
        // meta user 产 0 unit，但其 uuid 仍进 onMainUuids → 测 startIdx（索引 onMainRecords）与 units 下标脱耦。
        assertIncrementalMatchesFull(
            listOf(
                user(ContentBlock.Text("hi"), uuid = "u1"),
                user(ContentBlock.Text("injected"), uuid = "m1", meta = true), // 跳过、0 unit
                assistant(ContentBlock.Text("reply"), uuid = "a1"),
            ),
        )
    }

    @Test fun flipThenAppendStillMatches() {
        var state = RecordClassifier.IncrementalState.EMPTY
        val v1 = listOf(user(ContentBlock.Text("q"), uuid = "u1"), assistant(ContentBlock.Text("old"), uuid = "a1"))
        state = RecordClassifier.classifyIncremental(state, v1)
        val flipped = listOf(user(ContentBlock.Text("q"), uuid = "u1"), assistant(ContentBlock.Text("new"), uuid = "a2"))
        state = RecordClassifier.classifyIncremental(state, flipped) // 前缀不匹配 → 回退全量
        val appended = flipped + user(ContentBlock.Text("more"), uuid = "u2")
        state = RecordClassifier.classifyIncremental(state, appended) // 翻转后再走增量
        assertEquals(RecordClassifier.classify(appended), state.units)
    }

    @Test fun unchangedUnitsReusedByIdentity() {
        val r1 =
            listOf(
                user(ContentBlock.Text("q1"), uuid = "u1"),
                assistant(ContentBlock.Text("a1"), uuid = "a1"),
            )
        val s1 = RecordClassifier.classifyIncremental(RecordClassifier.IncrementalState.EMPTY, r1)
        val r2 = r1 + assistant(ContentBlock.Text("a2"), uuid = "a2")
        val s2 = RecordClassifier.classifyIncremental(s1, r2)
        assertSame("旧 unit0 应复用", s1.units[0], s2.units[0])
        assertSame("旧 unit1 应复用", s1.units[1], s2.units[1])
        assertEquals(3, s2.units.size)
    }
}
