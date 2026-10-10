package com.ccmonitor.mobile.core.claude.model

/**
 * 用量聚合：按 [AgentKind] 把一批记录聚成 [UsageSummary]。实现经 `AgentProfile.usageAggregator` 取。
 * Claude 按 requestId 逐字段取 MAX（output 是流式的）；Codex 读 `token_count` 事件的 `info`（每条带运行总量，取末条）。
 */
interface UsageAggregatorSpi {
    fun aggregate(records: List<JsonlRecord>): UsageSummary
}

/** Claude：即 [aggregateUsage]。 */
object ClaudeUsageAggregator : UsageAggregatorSpi {
    override fun aggregate(records: List<JsonlRecord>): UsageSummary = aggregateUsage(records)
}

/**
 * Codex：读 `event_msg/token_count` 的 `payload.info`，取末条（累计字段单调，末条即会话总）。映射按 Claude 的列义：
 * - `input` = total_token_usage.input_tokens − cached_input_tokens：Codex 的 input 含缓存、Claude 的不含，不减就和 cacheRead 重复计。
 * - `lastContextTokens` = last_token_usage.total_tokens（当前上下文占用）。不是 total_token_usage.total_tokens，
 *   那是全会话累计，用错会让上下文表卡在 100%。
 * - `contextWindow` = model_context_window（真实上限；Codex 没有 model 名，猜不了档）。
 * cache5m/1h=0（Codex 不拆写缓存），requests ≈ token_count 事件数，lastModel=null（model 在 turn_context）。
 */
object CodexUsageAggregator : UsageAggregatorSpi {
    override fun aggregate(records: List<JsonlRecord>): UsageSummary {
        var lastInfo: Map<*, *>? = null
        var count = 0
        for (r in records) {
            val info = tokenCountInfoOf(r) ?: continue
            lastInfo = info // 末条 total_token_usage=会话累计总（sums 用它）；lastInfo 恒更新（全零事件的 total 也是累计、无害）
            // 全零事件（last_token_usage 的 input/cached/output 都是 0，如会话起始那条）不计一次请求，
            // 与后端的请求计数一致。累计和不受影响。
            val last = info["last_token_usage"] as? Map<*, *>
            if (tok(last, "input_tokens") != 0L || tok(last, "cached_input_tokens") != 0L || tok(last, "output_tokens") != 0L) {
                count++
            }
        }
        val info = lastInfo ?: return UsageSummary.EMPTY
        val total = info["total_token_usage"] as? Map<*, *>
        val lastUsage = info["last_token_usage"] as? Map<*, *>
        val cachedInput = tok(total, "cached_input_tokens")
        return UsageSummary(
            requests = count,
            input = (tok(total, "input_tokens") - cachedInput).coerceAtLeast(0), // 去缓存=新鲜输入，对齐 Claude
            output = tok(total, "output_tokens"),
            cacheWrite5m = 0,
            cacheWrite1h = 0,
            cacheRead = cachedInput,
            lastContextTokens = tok(lastUsage, "total_tokens"), // 当前占用（≠ 累计 total）
            lastModel = null, // 在 turn_context、非 token_count 事件
            reasoningOutput = tok(total, "reasoning_output_tokens"),
            contextWindow = (info["model_context_window"] as? Number)?.toLong(),
        )
    }

    private fun tok(
        m: Map<*, *>?,
        k: String,
    ): Long = (m?.get(k) as? Number)?.toLong() ?: 0L

    /** 记录=event_msg/token_count → 返 `payload.info` Map（含 total_/last_token_usage 与 model_context_window），否则 null。 */
    private fun tokenCountInfoOf(r: JsonlRecord): Map<*, *>? =
        codexEventPayload(r)
            ?.takeIf { it["type"] == "token_count" }
            ?.let { it["info"] as? Map<*, *> }
}
