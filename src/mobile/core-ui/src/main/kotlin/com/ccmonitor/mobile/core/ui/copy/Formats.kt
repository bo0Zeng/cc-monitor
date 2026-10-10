package com.ccmonitor.mobile.core.ui.copy

/**
 * 大小（字节 ⇒ 给人看的一格）：与核心 `copy_core::size_text` · 桌面 `format.ts::sizeText` 同一个写法，
 * 三侧对 `tests/__fixtures__/size-text.golden.json`。不满 1 KB 写整数字节；往上按 1024 进位、最多到 TB，一位小数。
 */
object SizeFormat {
    private const val UNITS = 5

    fun text(bytes: Long): String {
        val b = bytes.coerceAtLeast(0)
        var u = 0
        while (u + 1 < UNITS && b >= 1L shl (10 * (u + 1))) u++
        if (u == 0) return unit(0, b.toString())

        fun tenths(k: Int): Long = (b * 10 + (1L shl (10 * k - 1))) / (1L shl (10 * k))
        var t = tenths(u)
        if (t >= 10_240 && u + 1 < UNITS) t = tenths(++u)
        return unit(u, "${t / 10}.${t % 10}")
    }

    /** 第 [u] 档单位写上数（文案键逐档字面量）。 */
    private fun unit(
        u: Int,
        n: String,
    ): String =
        when (u) {
            0 -> copyText("sizeFormat.unit.b", "n" to n)
            1 -> copyText("sizeFormat.unit.kb", "n" to n)
            2 -> copyText("sizeFormat.unit.mb", "n" to n)
            3 -> copyText("sizeFormat.unit.gb", "n" to n)
            else -> copyText("sizeFormat.unit.tb", "n" to n)
        }
}

/**
 * 短时长（文案规范 N5）· 距今 · 会走的钟：与核心 `copy_core::short_duration` / `rel_duration` · 桌面 `duration-format.ts`
 * 同一个写法，三侧对 `tests/__fixtures__/short-duration.golden.json`。核心写好的 `{text, from, to?}` 钟只在这里按此刻填 `{dur}`。
 */
object DurationFormat {
    private const val MINUTE_MS = 60_000L
    private const val HOUR_MIN = 60L
    private const val DAY_SEC = 86_400L

    /** 毫秒 ⇒ `<1s` · `45s` · `12m` · `1h4m` · `3d`（秒以下 · 分钟以下舍去，满 24 小时只写天）。 */
    fun short(ms: Long): String {
        val d = ms.coerceAtLeast(0) / 1_000
        val mins = d / HOUR_MIN
        val h = mins / HOUR_MIN
        val m = mins % HOUR_MIN
        return when {
            d == 0L -> copyText("durationFormat.short.under")
            d < HOUR_MIN -> copyText("durationFormat.short.sec", "n" to d)
            d >= DAY_SEC -> copyText("durationFormat.short.day", "n" to d / DAY_SEC)
            mins < HOUR_MIN -> copyText("durationFormat.short.min", "n" to mins)
            m == 0L -> copyText("durationFormat.short.hour", "n" to h)
            else -> copyText("durationFormat.short.hourMin", "h" to h, "m" to m)
        }
    }

    /** 距今（只写将来，向上取整到分钟，前面 `+`）；到了 ⇒ `null`。 */
    fun ahead(aheadMs: Long): String? {
        if (aheadMs <= 0) return null
        val mins = (aheadMs + MINUTE_MS - 1) / MINUTE_MS
        return copyText("durationFormat.rel.ahead", "dur" to short(mins * MINUTE_MS))
    }

    /** 会走的钟：[text] 里的 `{dur}` 按（[to] 或此刻 [now]）− [from] 填；[from] 比此刻还晚 ⇒ 按 0。 */
    fun clock(
        text: String,
        from: Long,
        to: Long?,
        now: Long,
    ): String = text.replace("{dur}", short((to ?: now) - from))
}
