package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Test

/** [aggregateUsage] 纯函数——requestId 去重、cache 分价、上下文窗、未知 model 降级。 */
class UsageAggregatorTest {
    private fun asst(
        uuid: String,
        requestId: String?,
        model: String?,
        usage: Usage?,
    ) = JsonlRecord.Assistant(
        uuid = uuid,
        parentUuid = null,
        blocks = emptyList(),
        model = model,
        timestamp = null,
        sessionId = null,
        isApiError = false,
        usage = usage,
        requestId = requestId,
    )

    private fun usage(
        input: Long = 0,
        output: Long = 0,
        cacheCreation: Long = 0,
        cacheRead: Long = 0,
        c5: Long = 0,
        c1: Long = 0,
    ) = Usage(input, output, cacheCreation, cacheRead, c5, c1)

    @Test
    fun dedupsByRequestId() {
        // 一条响应拆成 3 条 assistant 记录、同 requestId + 逐条重复 usage → 只计一次。
        val u = usage(input = 100, output = 50)
        val recs = listOf(asst("a1", "req1", "sonnet", u), asst("a2", "req1", "sonnet", u), asst("a3", "req1", "sonnet", u))
        val s = aggregateUsage(recs)
        assertEquals("去重后 1 个请求", 1, s.requests)
        assertEquals(100, s.input)
        assertEquals(50, s.output)
    }

    @Test
    fun streamingOutputTakesTerminalNotPlaceholder() {
        // 同一 requestId 的 output 是流式的：占位 9,9,9 → 终结 403，input/cache 恒定。
        // 逐字段取 MAX → output=403、input=100（不是取第一条的 9，也不是 4× 累加）。
        val recs =
            listOf(
                asst("a1", "req1", "sonnet", usage(input = 100, output = 9, cacheRead = 500)),
                asst("a2", "req1", "sonnet", usage(input = 100, output = 9, cacheRead = 500)),
                asst("a3", "req1", "sonnet", usage(input = 100, output = 9, cacheRead = 500)),
                asst("a4", "req1", "sonnet", usage(input = 100, output = 403, cacheRead = 500)),
            )
        val s = aggregateUsage(recs)
        assertEquals("1 个请求", 1, s.requests)
        assertEquals("output 取终结真值、非占位", 403, s.output)
        assertEquals("input 恒定不累加", 100, s.input)
        assertEquals("cacheRead 恒定不累加", 500, s.cacheRead)
        assertEquals("上下文=input+cacheRead+cacheCreation", 600, s.lastContextTokens)
    }

    @Test
    fun sumsAcrossRequests() {
        val s =
            aggregateUsage(
                listOf(
                    asst("a1", "req1", "sonnet", usage(input = 100, output = 10)),
                    asst("a2", "req2", "sonnet", usage(input = 200, output = 20)),
                ),
            )
        assertEquals(2, s.requests)
        assertEquals(300, s.input)
        assertEquals(30, s.output)
    }

    @Test
    fun nullRequestIdFallsBackToUuidEachCounted() {
        // null requestId → 回退 uuid，各留一份（不误合并）。
        val s =
            aggregateUsage(
                listOf(
                    asst("u1", null, "haiku", usage(input = 5)),
                    asst("u2", null, "haiku", usage(input = 7)),
                ),
            )
        assertEquals(2, s.requests)
        assertEquals(12, s.input)
    }

    @Test
    fun cacheSplitCounted() {
        val u = usage(input = 1000, output = 500, cacheCreation = 2000, cacheRead = 10000, c5 = 2000, c1 = 0)
        val s = aggregateUsage(listOf(asst("a1", "req1", "claude-opus-4-8", u)))
        assertEquals(2000, s.cacheWrite5m)
        assertEquals(10000, s.cacheRead)
    }

    @Test
    fun legacyTotalCacheCountedAs5m() {
        // 只填总 cacheCreation 的记录（5m/1h 皆 0）→ 计入 cacheWrite5m。
        val s = aggregateUsage(listOf(asst("a", "r", "sonnet", usage(cacheCreation = 500, c5 = 0, c1 = 0))))
        assertEquals(500, s.cacheWrite5m)
        assertEquals(0, s.cacheWrite1h)
    }

    @Test
    fun unknownModelTokensStillSummed() {
        val s = aggregateUsage(listOf(asst("a", "r", "claude-future-9", usage(input = 100, output = 50))))
        assertEquals("tokens 仍求和", 100, s.input)
    }

    @Test
    fun mixedModelsTokensSummed() {
        val s =
            aggregateUsage(
                listOf(
                    asst("a1", "r1", "sonnet", usage(input = 100)),
                    asst("a2", "r2", "unknownX", usage(input = 200)),
                ),
            )
        assertEquals(300, s.input)
    }

    @Test
    fun lastContextTokensIsLastRequest() {
        val s =
            aggregateUsage(
                listOf(
                    asst("a1", "r1", "sonnet", usage(input = 100, cacheRead = 1000, cacheCreation = 50)),
                    asst("a2", "r2", "sonnet", usage(input = 200, cacheRead = 8000, cacheCreation = 100)),
                ),
            )
        assertEquals("最近请求 input+cacheRead+cacheCreation", 200 + 8000 + 100, s.lastContextTokens)
        assertEquals("sonnet", s.lastModel)
    }

    @Test
    fun emptyAndNoUsage() {
        assertEquals(UsageSummary.EMPTY, aggregateUsage(emptyList()))
        assertEquals("assistant 但 usage=null → EMPTY", UsageSummary.EMPTY, aggregateUsage(listOf(asst("a", "r", "sonnet", null))))
    }

    @Test
    fun ignoresNonAssistantRecords() {
        val recs =
            listOf(
                JsonlRecord.User(
                    uuid = "u",
                    parentUuid = null,
                    blocks = emptyList(),
                    isMeta = false,
                    timestamp = null,
                    sessionId = null,
                    cwd = null,
                ),
                asst("a1", "r1", "sonnet", usage(input = 42)),
            )
        assertEquals(1, aggregateUsage(recs).requests)
        assertEquals(42, aggregateUsage(recs).input)
    }

    @Test
    fun contextLimitDetects1mModels() {
        assertEquals("1M 标记 → 1M 上限", 1_000_000L, contextLimitFor("claude-opus-4-8[1m]"))
        assertEquals(1_000_000L, contextLimitFor("claude-sonnet-5-1m"))
        assertEquals("普通 → 200K 默认", 200_000L, contextLimitFor("claude-sonnet-5"))
        assertEquals(200_000L, contextLimitFor(null))
    }

    @Test
    fun contextLimitInfersFromObserved() {
        // model id 无 1M 标记但观测上下文超 200K → 升 1M 档（避免显 >100%）。
        assertEquals("观测 920K > 200K → 1M", 1_000_000L, contextLimitFor("claude-opus-4-8", 920_000))
        assertEquals("观测 ≤200K → 200K", 200_000L, contextLimitFor("claude-sonnet-5", 150_000))
        assertEquals("观测超 1M → 用观测", 1_500_000L, contextLimitFor("claude-opus-4-8", 1_500_000))
    }
}
