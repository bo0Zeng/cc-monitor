package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.wire.RotFormCell
import com.ccmonitor.mobile.core.claude.link.wire.RotSeg
import com.ccmonitor.mobile.core.claude.link.wire.RotTryUse

/*
 * 设置是策略自己的形状（`settings`）：这里只照声明取、放（同桌面 `rot-board.ts` 的 rowsOf · withRows · valueOf · put）。
 * `scope: all` ⇒ `settings[key]`；`scope: each` ⇒ `settings.accounts[i][key]`（`inherit` 且号没另设 ⇒ 跟总体）。改一格 ⇒ 整份新设置交那台（那台照声明校验）。
 * 不判、不排：先后 · 能不能用都是那台的。
 */

internal typealias Obj = Map<String, Any?>

@Suppress("UNCHECKED_CAST")
internal fun Any?.asObjOrNull(): Obj? = this as? Map<String, Any?>

/** 号列表那一排（`settings.accounts`）。 */
internal fun rowsOf(settings: Any?): List<Obj> = (settings.asObjOrNull()?.get("accounts") as? List<*>)?.mapNotNull { it.asObjOrNull() }.orEmpty()

/** 换掉号列表那一排，别的原样（整份新的）。 */
internal fun withRows(
    settings: Any?,
    rows: List<Obj>,
): Obj = (settings.asObjOrNull() ?: emptyMap()) + ("accounts" to rows)

/** 拖号牌：第 [from] 张松在第 [to] 张上 ⇒ 排到它前头（同桌面）。没动 ⇒ `null`。 */
internal fun moved(
    settings: Any?,
    from: Int,
    to: Int,
): Obj? {
    val rows = rowsOf(settings)
    if (to < 0 || to == from || from !in rows.indices) return null
    val next = rows.toMutableList()
    val m = next.removeAt(from)
    next.add(if (to > from) to - 1 else to, m)
    return withRows(settings, next)
}

/** 移出第 [i] 个号。 */
internal fun removed(
    settings: Any?,
    i: Int,
): Obj = withRows(settings, rowsOf(settings).filterIndexed { k, _ -> k != i })

/** 加一个号（排在最后）。 */
internal fun added(
    settings: Any?,
    account: String,
): Obj = withRows(settings, rowsOf(settings) + mapOf("id" to account))

/** 一格此刻的值 ＋ 是不是跟着总体（画虚线）。 */
internal data class CellValue(
    val v: Any?,
    val inherited: Boolean,
)

internal fun valueOf(
    settings: Any?,
    cell: RotFormCell,
    row: Int?,
): CellValue {
    val all = settings.asObjOrNull() ?: emptyMap()
    val total = if (all.containsKey(cell.key)) all[cell.key] else cell.default
    if (row == null) return CellValue(total, false)
    val r = rowsOf(settings).getOrNull(row) ?: emptyMap()
    return when {
        r.containsKey(cell.key) -> CellValue(r[cell.key], false)
        cell.inherit == true -> CellValue(total, true)
        else -> CellValue(cell.default, false)
    }
}

/** 放回一格（整份新设置）；号那一行 [unset] ⇒ 删掉那一格（回到跟总体）。可空的格放 `null` 是「空的那一项」，不是删。 */
internal fun put(
    settings: Any?,
    cell: RotFormCell,
    row: Int?,
    v: Any?,
    unset: Boolean = false,
): Obj {
    val all = settings.asObjOrNull() ?: emptyMap()
    if (row == null) return all + (cell.key to v)
    val rows = rowsOf(settings).map { it.toMutableMap() }
    val r = rows.getOrNull(row) ?: return all
    if (unset) r.remove(cell.key) else r[cell.key] = v
    return withRows(all, rows)
}

/** 24 小时条：某一窗那一组段（号没另设、跟总体 ⇒ 总体那一组，[CellDay.follows] 为真）。 */
internal data class CellDay(
    val segs: List<RotSeg>,
    val follows: Boolean,
)

internal fun dayOf(
    settings: Any?,
    cell: RotFormCell,
    row: Int?,
    window: String,
): CellDay {
    val total = settings.asObjOrNull()?.get(cell.key).asObjOrNull() ?: emptyMap()
    val own = row?.let { rowsOf(settings).getOrNull(it)?.get(cell.key).asObjOrNull() } ?: emptyMap()
    val mine = if (row == null) total[window] else own[window]
    val follows = row != null && cell.inherit == true && mine !is List<*>
    val raw =
        (
            if (mine is List<*>) {
                mine
            } else if (follows) {
                total[window] as? List<*>
            } else {
                null
            }
        ).orEmpty()
    return CellDay(raw.mapNotNull(::segOf), follows)
}

/** 刷完那一组放回那一窗（空 ⇒ 删那一窗；号那一行全空 ⇒ 删那一格）。 */
internal fun putDay(
    settings: Any?,
    cell: RotFormCell,
    row: Int?,
    window: String,
    segs: List<RotSeg>,
): Obj {
    val total = settings.asObjOrNull()?.get(cell.key).asObjOrNull() ?: emptyMap()
    val own = row?.let { rowsOf(settings).getOrNull(it)?.get(cell.key).asObjOrNull() } ?: emptyMap()
    val base = (if (row == null) total else own).toMutableMap()
    if (segs.isEmpty()) base.remove(window) else base[window] = segs.map { mapOf("from" to it.from, "to" to it.to, "v" to it.v) }
    return put(settings, cell, row, base, unset = row != null && base.isEmpty())
}

private fun segOf(x: Any?): RotSeg? {
    val m = x.asObjOrNull() ?: return null
    val from = (m["from"] as? Number)?.toLong()
    val to = (m["to"] as? Number)?.toLong()
    val v = (m["v"] as? Number)?.toDouble()
    return if (from != null && to != null && v != null) RotSeg(from, to, v) else null
}

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
