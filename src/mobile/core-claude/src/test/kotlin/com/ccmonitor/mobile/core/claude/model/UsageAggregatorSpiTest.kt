package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * [UsageAggregatorSpi] per-kind 用量。Claude 复用 aggregateUsage（requestId MAX）；
 * Codex 读 event_msg/token_count 的 info（末条=会话总）——sums 用 total_token_usage、当前占用用 last_token_usage、
 * 上限用 model_context_window。fixtures 按真 ~/.codex 结构（含 last_token_usage + model_context_window）。
 */
class UsageAggregatorSpiTest {
    private fun tokenCount(
        input: Long,
        output: Long,
        cached: Long,
        reasoning: Long,
        total: Long,
        lastTotal: Long = total,
        window: Long = 258_400,
    ) = JsonlRecord.Unknown(
        "event_msg",
        rawJson =
            """{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":""" +
                """{"input_tokens":$input,"cached_input_tokens":$cached,"output_tokens":$output,""" +
                """"reasoning_output_tokens":$reasoning,"total_tokens":$total},""" +
                // last_token_usage=本轮增量（真机非零；此处 input=lastTotal 使真实轮非全零→被计数，见 codexSkipsAllZero）。
                """"last_token_usage":{"input_tokens":$lastTotal,"cached_input_tokens":0,"output_tokens":0,""" +
                """"reasoning_output_tokens":0,"total_tokens":$lastTotal},"model_context_window":$window}}}""",
    )

    @Test fun codexMapsLastTokenCountEvent() {
        // sums 用末条 total_token_usage；input 去缓存对齐 Claude 列义。
        val recs =
            listOf(
                tokenCount(100, 10, 50, 5, 110, lastTotal = 60),
                tokenCount(200, 20, 100, 10, 220, lastTotal = 80),
            )
        val s = CodexUsageAggregator.aggregate(recs)
        assertEquals(100, s.input) // 200 input_tokens − 100 cached = 新鲜输入
        assertEquals(20, s.output)
        assertEquals(100, s.cacheRead)
        assertEquals(10, s.reasoningOutput)
        assertEquals(80, s.lastContextTokens) // last_token_usage.total（当前占用），非累计 220
        assertEquals(258_400L, s.contextWindow) // Long? → 需 L 后缀（否则 Int/Long 装箱不等）
        assertEquals(2, s.requests) // token_count 事件数
        assertEquals(null, s.lastModel)
        assertEquals(0, s.cacheWrite5m)
    }

    @Test fun codexOccupancyUsesLastNotCumulative() {
        // 注意：total_token_usage.total 是全会话累计（单调增、误用会卡 100%），当前占用须读 last_token_usage.total。
        val recs =
            listOf(
                tokenCount(100, 10, 50, 5, 110, lastTotal = 60),
                tokenCount(2000, 200, 1500, 100, 2200, lastTotal = 700),
            )
        val s = CodexUsageAggregator.aggregate(recs)
        assertEquals(700, s.lastContextTokens) // 不是累计 2200
        assertEquals(500, s.input) // 2000 − 1500 缓存
        assertEquals(1500, s.cacheRead)
    }

    @Test fun codexTokenCountMissingSubmapsGraceful() {
        // token_count 事件但 info 缺 total_/last_token_usage / window → 不崩、降级 0/null（Codex 格式会变）。
        val rec = JsonlRecord.Unknown("event_msg", rawJson = """{"type":"event_msg","payload":{"type":"token_count","info":{}}}""")
        val s = CodexUsageAggregator.aggregate(listOf(rec))
        assertEquals(0, s.input)
        assertEquals(0, s.lastContextTokens)
        assertEquals(null, s.contextWindow)
        // 无可用 last_token_usage（缺或全零）→ 不计一次请求。
        assertEquals(0, s.requests)
    }

    @Test fun codexSkipsAllZeroNoOpEventsInCount() {
        // last_token_usage 全 0 的 no-op 事件（如会话起始 turn_context 前）不计 requests，与后端同；真实轮照计。sums 取末条累计不受影响。
        val allZero =
            JsonlRecord.Unknown(
                "event_msg",
                rawJson =
                    """{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":""" +
                        """{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":0},""" +
                        """"last_token_usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":0}}}}""",
            )
        // 全零 no-op 出现在会话起始（turn_context 前、total 累计=0）；末条恒真实轮（total=累计真值）。
        val recs = listOf(allZero, tokenCount(100, 10, 50, 5, 110, lastTotal = 60))
        val s = CodexUsageAggregator.aggregate(recs)
        assertEquals("只计非全零轮（起始全零跳、1 真实计）", 1, s.requests)
        assertEquals("sums 取末条真实轮累计 total、不受起始全零影响", 50, s.input) // 100−50
    }

    @Test fun codexNoTokenCountGivesEmpty() {
        val other = JsonlRecord.Unknown("event_msg", rawJson = """{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t"}}""")
        assertEquals(UsageSummary.EMPTY, CodexUsageAggregator.aggregate(listOf(other, JsonlRecord.Unknown("session_meta"))))
    }

    @Test fun codexBadRawJsonDoesNotThrow() {
        assertEquals(UsageSummary.EMPTY, CodexUsageAggregator.aggregate(listOf(JsonlRecord.Unknown("event_msg", rawJson = "not json"))))
    }

    @Test fun claudeDelegatesToAggregateUsage() {
        val recs =
            listOf(
                JsonlRecord.Assistant("u1", null, emptyList(), "claude-opus", null, null, false, "end_turn", false, Usage(100, 50, 0, 0)),
            )
        assertEquals(aggregateUsage(recs), ClaudeUsageAggregator.aggregate(recs))
    }

    @Test fun claudeReasoningOutputStaysZero() {
        // Claude 无 reasoning 维，这一格恒 0。
        val recs = listOf(JsonlRecord.Assistant("u", null, emptyList(), "sonnet", null, null, false, "end_turn", false, Usage(10, 5, 0, 0)))
        assertEquals(0, ClaudeUsageAggregator.aggregate(recs).reasoningOutput)
    }

    /** 按 kind 经 `AgentProfile.of(kind).usageAggregator` 取到对应聚合器。 */
    @Test fun selectorRoutesPerKind() {
        assertEquals(ClaudeUsageAggregator, AgentProfile.of(AgentKind.ClaudeCode).usageAggregator)
        assertEquals(CodexUsageAggregator, AgentProfile.of(AgentKind.Codex).usageAggregator)
    }
}
