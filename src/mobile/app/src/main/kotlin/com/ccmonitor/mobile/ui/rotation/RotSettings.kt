package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.RotEdit
import com.ccmonitor.mobile.core.claude.link.RotSegment
import com.ccmonitor.mobile.core.claude.link.wire.RotCell
import com.ccmonitor.mobile.core.claude.link.wire.RotFrom
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.claude.link.wire.RotMark
import com.ccmonitor.mobile.core.claude.link.wire.RotRow
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.copy.copyText

/*
 * 轮换屏上画什么、交什么（桌面 `rot-board.ts` 同一处）：格 · 记号 · 底行全照那台的回包，这里只挑出来排；
 * 手势只交松手的地方（第几段 · 拖到第几格 · 松在哪一刻）与原样打的字，值 · 继承 · 写法 · 钟点怎么认 · 能不能用都在那台。
 */

/** 一天的分钟。 */
internal const val DAY_MIN = 1440L

/** 视图里这个号那一行（号列表外 ⇒ `null`）。 */
internal fun rowOf(
    view: RotationViewReply,
    account: String,
): RotRow? {
    val i = view.accounts.indexOfFirst { it.account == account }
    return if (i in 0 until view.listed) view.board.rows.getOrNull(i) else null
}

/** 号名旁的记号（那台照声明给的：字形 · 虚线框 · 那一句）。 */
internal fun marksOf(
    view: RotationViewReply,
    account: String,
): List<RotMark> = rowOf(view, account)?.marks.orEmpty()

/** 编辑小窗里的格：那台只回此刻该出的；挂在别格下的（`under`）缩进。 */
internal fun paneCells(row: RotRow): List<Pair<RotCell, Boolean>> = row.cells.map { it to (it.under != null) }

/** 分段那一窗画不画段：后面那窗同第一窗 ⇒ 只出［同 · 另设］那一对钮。 */
internal fun segsShown(cell: RotCell): Boolean = !(cell.same != null && cell.from == RotFrom.SAME)

/**
 * 编辑小窗底行：「改的是策略「X」 · 改一格就存成新版」接那台写的「N 个会话还在用旧版」（有 ⇒ 带［都载入新版］）或「没有会话在用」。
 */
internal fun paneFoot(view: RotationViewReply): Pair<String, Boolean> {
    val older = view.hints.firstOrNull { it.hint.k == RotHintKind.OLDER }
    val tail =
        when {
            older != null -> listOf(older.text)
            view.strategy?.inUse.isNullOrEmpty() -> listOf(copyText("rot.pane.noUse"))
            else -> emptyList()
        }
    val parts = listOf(copyText("rot.pane.foot", "name" to view.strategy?.id.orEmpty())) + tail
    return parts.joinToString(copyText("kit.text.sep")) to (older != null)
}

/** 一段一行交上来的三格 ⇒ 改分段的意图（[at] ＝ 第几段；新加的那一行 ⇒ `null`）：钟点交原样的字（认不认得是那台的）；有一格空着或上限不是数 ⇒ `null`（不交）。 */
internal fun segEdit(
    cell: RotCell,
    row: Int?,
    at: Int?,
    from: String,
    to: String,
    v: String,
): Map<String, Any?>? {
    val (a, b) = from.trim() to to.trim()
    if (a.isEmpty() || b.isEmpty()) return null
    val n = v.trim().toDoubleOrNull() ?: return null
    return RotEdit.seg(cell.key, row, cell.window.orEmpty(), at, RotSegment(a, b, n))
}

/** 数格里的数照它的样子写（整数不带小数点）。 */
internal fun numText(v: Double): String = if (v == Math.floor(v) && !v.isInfinite()) v.toLong().toString() else v.toString()
