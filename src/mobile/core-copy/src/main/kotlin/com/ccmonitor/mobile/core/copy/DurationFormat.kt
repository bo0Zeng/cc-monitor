package com.ccmonitor.mobile.core.copy

/**
 * 短时长 · 距今：核心 `copy_core::short_duration` / `rel_duration` 那一份。核心写好的 `{text, from, to?}` 钟只在这里按此刻填 `{dur}`
 * （同桌面 `duration-format.ts`，clock 段对金样 `tests/__fixtures__/short-duration.golden.json`）。
 */
object DurationFormat {
    /** 毫秒 ⇒ `<1s` · `45s` · `12m` · `1h4m` · `3d`。 */
    fun short(ms: Long): String = CopyCore.shortDuration(ms)

    /** 距今（只写将来，向上取整到分钟，前面 `+`）；到了 ⇒ `null`。 */
    fun ahead(aheadMs: Long): String? = CopyCore.relDuration(aheadMs)

    /**
     * 已等多久（同桌面 `duration-format.ts` 的 `waitedNow`）：那台答出那一刻算好的 [waitedMs] ＋ 本机从收到那份（[receivedAt]）到 [now] 走过的
     * （两段各在一台钟上量，不跨机器减；本机钟往回走按 0）。没有起点 ⇒ `null`。
     */
    fun waited(
        waitedMs: Long?,
        receivedAt: Long,
        now: Long,
    ): String? = waitedMs?.let { short(it + (now - receivedAt).coerceAtLeast(0)) }

    /** 会走的钟：[text] 里的 `{dur}` 按（[to] 或此刻 [now]）− [from] 填；[from] 比此刻还晚 ⇒ 按 0。 */
    fun clock(
        text: String,
        from: Long,
        to: Long?,
        now: Long,
    ): String = text.replace("{dur}", short((to ?: now) - from))
}
