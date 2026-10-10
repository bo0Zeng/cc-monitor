package com.ccmonitor.mobile.core.claude.transport

/**
 * 配额读数：`rate_limit_event` 帧解出来的、能上屏的东西。
 *
 * 同一个东西有两套键名，两套都要认，只认一套的实现在另一套素材上恒 null 且不报错：
 *
 * | 来源 | `resetsAt` | `rateLimitType` | 百分比 |
 * |---|---|---|---|
 * | CLI stream-json | `resetsAt` | `rateLimitType` | `unifiedWindows` |
 * | SDK 信封 | `resets_at` | `rate_limit_type` | `utilization`（常为 null）＋ 内层 `raw` 里一份 camelCase |
 *
 * 注意：SDK 信封的 `rate_limit_info.raw` 里带着一份 camelCase 镜像，只认 camelCase 的实现也能从那里拿到值。
 * 我们不依赖那份镜像，外层 snake_case 要能自己读出来。
 *
 * 「没有百分比」和「百分比是 0」是两件事：[windows] 为空时 UI 不许显示任何百分比或进度条。
 */
data class RateLimitReading(
    /** `allowed` / `rejected` / … 原始机器码，不翻译（翻译在 UI 那层，只翻已知的）。 */
    val status: String?,
    /** `five_hour` / … 主限流窗口的名字。 */
    val limitType: String?,
    /** 主窗口重置的墙钟秒（epoch）。null = 流上没说。 */
    val resetsAtEpochSec: Long?,
    /**
     * 各个窗口的用量，按窗口名排序。可能为空：空 = 流上没给百分比。
     */
    val windows: List<QuotaWindow>,
    /** 本地看到这条读数的时刻（同 [WaitingSignal.observedAtMs] 的口径）。 */
    val observedAtMs: Long,
) {
    /**
     * 用量最高的那个窗口，「还剩多少」说的是它。空 ⇒ null。
     * 是用量最高的，不是第一个：常见读数里字典序第一恰好也是用量最高，换成取第一个不容易看出来。
     */
    val tightest: QuotaWindow? get() = windows.maxByOrNull { it.utilization }

    companion object {
        /** `rate_limit_info` 在两套信封里都叫这个名字。 */
        private const val INFO = "rate_limit_info"

        /**
         * 解一条 `rl` 帧的 `raw`。解不出来回 null（调用方据此不覆盖上一条真读数）。
         *
         * @param raw `BridgeFrame.RateLimit.raw` —— 即整行减去 `type`，含 `rate_limit_info`。
         */
        fun parse(
            raw: Map<String, Any?>?,
            nowMs: Long,
        ): RateLimitReading? {
            val info = raw?.get(INFO) as? Map<*, *> ?: return null
            // SDK 那套把 camelCase 原文塞在内层 `raw` 里；两层都要看。
            val inner = info["raw"] as? Map<*, *>
            return RateLimitReading(
                status = str(info, "status") ?: inner?.let { str(it, "status") },
                limitType = pick(info, inner, "rateLimitType", "rate_limit_type"),
                resetsAtEpochSec = pickNum(info, inner, "resetsAt", "resets_at"),
                windows = windows(info, inner),
                observedAtMs = nowMs,
            )
        }

        /**
         * 取百分比，三个来源按可信度排：
         * ① `unifiedWindows`（逐窗口带 `utilization`）；
         * ② 内层 `raw.unifiedWindows`（SDK 信封把 camelCase 原文放这儿）；
         * ③ 顶层单个 `utilization`（SDK 那套的形状，多数时候是 null）。
         */
        private fun windows(
            info: Map<*, *>,
            inner: Map<*, *>?,
        ): List<QuotaWindow> {
            val unified =
                (info["unifiedWindows"] ?: info["unified_windows"]) as? Map<*, *>
                    ?: (inner?.get("unifiedWindows") ?: inner?.get("unified_windows")) as? Map<*, *>
            if (unified != null) {
                return unified.entries
                    .mapNotNull { (k, v) -> window(k as? String, v as? Map<*, *>) }
                    .sortedBy { it.name }
            }
            val flat = num(info["utilization"]) ?: inner?.let { num(it["utilization"]) } ?: return emptyList()
            val name = pick(info, inner, "rateLimitType", "rate_limit_type") ?: UNNAMED_WINDOW
            return listOf(
                QuotaWindow(name, flat, pickNum(info, inner, "resetsAt", "resets_at")),
            )
        }

        private fun window(
            name: String?,
            body: Map<*, *>?,
        ): QuotaWindow? {
            if (name == null || body == null) return null
            val used = num(body["utilization"]) ?: return null
            return QuotaWindow(name, used, num(body["resetsAt"] ?: body["resets_at"])?.toLong())
        }

        private fun str(
            m: Map<*, *>,
            k: String,
        ): String? = m[k] as? String

        private fun pick(
            info: Map<*, *>,
            inner: Map<*, *>?,
            camel: String,
            snake: String,
        ): String? = str(info, camel) ?: str(info, snake) ?: inner?.let { str(it, camel) ?: str(it, snake) }

        private fun pickNum(
            info: Map<*, *>,
            inner: Map<*, *>?,
            camel: String,
            snake: String,
        ): Long? =
            (num(info[camel]) ?: num(info[snake]) ?: inner?.let { num(it[camel]) ?: num(it[snake]) })?.toLong()

        private fun num(v: Any?): Double? = (v as? Number)?.toDouble()

        /** 流上给了百分比却没给窗口名时的占位名。 */
        const val UNNAMED_WINDOW = "current"
    }
}

/**
 * 一个限流窗口的用量。
 *
 * @param utilization 已用比例，不是剩余。注意：它会超过 1（超额时 `status` 是 `"rejected"`）；
 *   在这里或 UI 上加 `coerceAtMost(1.0)` 会把「已经超了」压成「刚好用完」，产品上要说的话不一样。
 */
data class QuotaWindow(
    val name: String,
    val utilization: Double,
    val resetsAtEpochSec: Long?,
)
