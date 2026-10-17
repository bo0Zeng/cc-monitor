package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.RotEdit
import com.ccmonitor.mobile.core.claude.link.RotSegment
import com.ccmonitor.mobile.core.claude.link.wire.RotCell
import com.ccmonitor.mobile.core.claude.link.wire.RotFrom
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.claude.link.wire.RotMark
import com.ccmonitor.mobile.core.claude.link.wire.RotRow
import com.ccmonitor.mobile.core.claude.link.wire.RotTryUse
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.copy.copyText

/*
 * 轮换屏上画什么、交什么（桌面 `rot-board.ts` 同一处）：格 · 记号 · 底行全照那台的回包，这里只挑出来排；
 * 手势只换成意图（第几段 · 拖完那一串 · 松在第几段），值 · 继承 · 写法 · 能不能用都在那台。
 */

/** 一天的分钟。 */
internal const val DAY_MIN = 1440L

private const val MIN_PER_H = 60

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

/** 一段一行里写的钟点 ⇒ 一天里的分钟（`20:30` · `2030` · `20` 都认，`24:00` ⇒ 1440）；写得不对 ⇒ `null`。只是读字。 */
internal fun minutesOf(text: String): Long? {
    val m = Regex("""^(\d{1,2})(?::?(\d{2}))?$""").matchEntire(text.trim()) ?: return null
    val h = m.groupValues[1].toInt()
    val min = m.groupValues[2].ifEmpty { "0" }.toInt()
    val v = (h * MIN_PER_H + min).toLong()
    return v.takeIf { min < MIN_PER_H && v <= DAY_MIN }
}

/** 一段一行交上来的三格 ⇒ 改分段的意图（[at] ＝ 第几段；新加的那一行 ⇒ `null`）；有一格写得不对 ⇒ `null`（不交）。 */
internal fun segEdit(
    cell: RotCell,
    row: Int?,
    at: Int?,
    from: String,
    to: String,
    v: String,
): Map<String, Any?>? {
    val a = minutesOf(from) ?: return null
    val b = minutesOf(to) ?: return null
    val n = v.trim().toDoubleOrNull() ?: return null
    return RotEdit.seg(cell.key, row, cell.window.orEmpty(), at, RotSegment(a, b, n))
}

/** 会话顺序里松手的那一下：第一格 ⇒ 现在用这个号；别处 ⇒ 本会话先后照拖完那一串。 */
sealed interface RowDrop {
    data class UseNow(
        val account: String,
    ) : RowDrop

    data class Set(
        val next: List<String>,
    ) : RowDrop
}

/** 把第 [i] 行拖到第 [to] 格（同桌面）：没挪 ⇒ `null`；交的那一串是第二格到挪动那一截的尽头。只是几何。 */
internal fun dropRow(
    line: List<String>,
    i: Int,
    to: Int,
): RowDrop? {
    val account = line.getOrNull(i) ?: return null
    if (to !in line.indices || to == i) return null
    if (to == 0) return RowDrop.UseNow(account)
    val next = line.filter { it != account }.toMutableList()
    next.add(to, account)
    return RowDrop.Set(next.subList(1, maxOf(i, to) + 1).toList())
}

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
