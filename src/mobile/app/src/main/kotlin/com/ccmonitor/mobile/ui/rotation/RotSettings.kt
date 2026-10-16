package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.wire.RotTryUse

/*
 * 轮换屏上的几何：分钟 ↔ 横坐标、松手落在第几段。设置那几格的值 · 继承 · 写法都在那台（`rotation-view` 的 `board` · `rotation-settings` 的意图）。
 */

/** 一天的分钟。 */
internal const val DAY_MIN = 1440L

/** 拖的时候按半小时吸附。 */
internal const val SNAP_MIN = 30L

/** 横坐标（0–1）⇒ 一天里的分钟（吸附到半小时）。只是几何。 */
internal fun minuteAt(fraction: Float): Long = (Math.round(fraction.coerceIn(0f, 1f) * DAY_MIN / SNAP_MIN.toDouble()) * SNAP_MIN).coerceIn(0, DAY_MIN)

/**
 * 拖号名到最上那条、松在时刻 [t] ⇒ 排到第几段（此刻那段是第 0 位，之后每段换号往后数一位，同桌面）。只是几何：能不能用、跳不跳由那台推演答。
 */
internal fun placeAt(
    uses: List<RotTryUse>,
    now: Long,
    t: Long,
): Int = uses.count { it.from in (now + 1) until t } + 1

/** 数格里的数照它的样子写（整数不带小数点）。 */
internal fun numText(v: Double): String = if (v == Math.floor(v) && !v.isInfinite()) v.toLong().toString() else v.toString()
