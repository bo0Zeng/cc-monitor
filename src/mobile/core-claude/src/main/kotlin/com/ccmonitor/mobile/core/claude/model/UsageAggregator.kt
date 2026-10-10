package com.ccmonitor.mobile.core.claude.model

/**
 * 上下文上限（token）。基准按 model（带 `[1m]`/`-1m` → 1M，否则 200K）；但 1M 是 beta flag、常不写进 model id，
 * 所以用 [observed] 兜底：观测值超基准就升 1M 档，超 1M 用观测值，不显示 >100%。是近似，UI 标估算。
 */
fun contextLimitFor(
    model: String?,
    observed: Long = 0,
): Long {
    val base = if (isOneMillionModel(model)) 1_000_000L else 200_000L
    return when {
        observed > 1_000_000L -> observed
        observed > base -> 1_000_000L
        else -> base
    }
}

/** model id 是否带 1M 上下文标记。 */
private fun isOneMillionModel(model: String?): Boolean {
    val m = model?.lowercase() ?: return false
    return m.contains("[1m]") || m.contains("-1m") || m.contains(" 1m")
}

/** 一个会话（或已载窗口）的用量汇总，只读 `Assistant.usage`。不含钱：app 不估价，用量屏的数据来自后端 `quota-read`。 */
data class UsageSummary(
    val requests: Int,
    val input: Long,
    val output: Long,
    val cacheWrite5m: Long,
    val cacheWrite1h: Long,
    val cacheRead: Long,
    val lastContextTokens: Long,
    val lastModel: String?,
    // Codex 的 reasoning token；Claude 没有这一维，恒 0。
    val reasoningOutput: Long = 0,
    // Codex 的显式上下文窗口（token_count.info.model_context_window）。Claude 为 null，走 contextLimitFor 估；
    // 有真值时应优先用它，否则无 model 名会误套 200K/1M 档。
    val contextWindow: Long? = null,
) {
    companion object {
        val EMPTY = UsageSummary(0, 0, 0, 0, 0, 0, 0, null)
    }
}

/**
 * 聚合用量（纯函数），按 requestId 分组：一条 API 响应被拆成多条 assistant 记录。
 * 同一 requestId 的 `output_tokens` 是流式的，前几条是占位小值，终结条才是真实总量；input/cache 组内恒定。
 * 所以每组逐字段取 MAX，取首条会少计 output。null requestId 回退 uuid，各自一组。
 * [lastContextTokens]=最近一组请求的 `input+cacheRead+cacheCreation`。
 */
fun aggregateUsage(records: List<JsonlRecord>): UsageSummary {
    val perRequest = groupUsageByRequest(records) // 每组 = (逐字段 MAX 有效 usage, model)，保首现顺序
    if (perRequest.isEmpty()) return UsageSummary.EMPTY

    var input = 0L
    var output = 0L
    var w5 = 0L
    var w1 = 0L
    var read = 0L
    for ((u, _) in perRequest) {
        // 只填了总 cacheCreation（5m/1h 皆 0）的记录按 5m 计（默认缓存 TTL）。
        val cw5 = if (u.cacheCreation5m == 0L && u.cacheCreation1h == 0L && u.cacheCreation > 0L) u.cacheCreation else u.cacheCreation5m
        val cw1 = u.cacheCreation1h
        input += u.input
        output += u.output
        w5 += cw5
        w1 += cw1
        read += u.cacheRead
    }
    val last = perRequest.last().first
    return UsageSummary(
        requests = perRequest.size,
        input = input,
        output = output,
        cacheWrite5m = w5,
        cacheWrite1h = w1,
        cacheRead = read,
        lastContextTokens = last.input + last.cacheRead + last.cacheCreation,
        lastModel = perRequest.last().second,
    )
}

/** 按 requestId 分组（null→uuid→记录身份回退，LinkedHashMap 保首现顺序），每组逐字段 MAX。返回 (有效 usage, model)。 */
private fun groupUsageByRequest(records: List<JsonlRecord>): List<Pair<Usage, String?>> {
    val groups = LinkedHashMap<Any, MutableList<Usage>>()
    val modelOf = LinkedHashMap<Any, String?>()
    for (r in records.filterIsInstance<JsonlRecord.Assistant>()) {
        val u = r.usage ?: continue
        val key: Any = r.requestId ?: r.uuid ?: r
        groups.getOrPut(key) { mutableListOf() }.add(u)
        modelOf.getOrPut(key) { r.model } // 组内 model 恒定，取首现
    }
    return groups.entries.map { (k, us) -> mergeUsageMax(us) to modelOf[k] }
}

/**
 * 一个 requestId 组内逐字段取 MAX：output 流式取到终结真值，input/cache 恒定即取该常量。
 */
private fun mergeUsageMax(us: List<Usage>): Usage =
    Usage(
        input = us.maxOf { it.input },
        output = us.maxOf { it.output },
        cacheCreation = us.maxOf { it.cacheCreation },
        cacheRead = us.maxOf { it.cacheRead },
        cacheCreation5m = us.maxOf { it.cacheCreation5m },
        cacheCreation1h = us.maxOf { it.cacheCreation1h },
    )
