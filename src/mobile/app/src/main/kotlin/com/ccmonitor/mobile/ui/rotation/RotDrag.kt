package com.ccmonitor.mobile.ui.rotation

/*
 * 会话顺序改先后的状态机（桌面 `rot-drag.ts` 同一张表）：静止 → 按着（长按）→ 拿起 → 跟手 ⇄ 悬停第一格 → 等回包 → 静止。
 * 只管「在看什么 · 手上正在改的」：拖的是哪一行、空槽在第几格、每一行此刻该让开多少（px）。不判顺序：
 * 哪几行可挪是那台给的 `movable`，松在第几格交给那台认（`rotation-temp {move:{account, to}}`），回包之后整张照那台画。
 * 落点只在可挪那一组（第 0 格在用的号也是落点：松在那里 ＝ 现在换过去）。行的几何由调用方量好交进来。
 */

/** 一行：号 · 能不能拖（那台给的）· 量好的上沿与高（px）。 */
data class DragRow(
    val account: String,
    val movable: Boolean,
    val top: Float,
    val height: Float,
)

/** 松手的结局：交（拖的号 · 松在第几格）· 退回（没挪 · 取消）· 什么都不做（不可挪 · 离线）。 */
sealed interface DragEnd {
    data class Drop(
        val account: String,
        val to: Int,
    ) : DragEnd

    data object Back : DragEnd

    data object None : DragEnd
}

enum class DragPhase { IDLE, LIFTED, WAITING }

/** [lifts] ＝ 此刻能不能拿起（那台离线 ⇒ 不能）。 */
class OrderDrag(
    private val lifts: () -> Boolean,
) {
    var phase: DragPhase = DragPhase.IDLE
        private set
    var from: Int = -1
        private set
    var slot: Int = -1
        private set
    private var rows: List<DragRow> = emptyList()
    private var dy = 0f

    /** 拖的那一行的号（没在拖 ⇒ `null`）。 */
    val account: String? get() = rows.getOrNull(from)?.account

    /** 空槽在第一格（松手 ＝ 现在换过去）。 */
    val first: Boolean get() = phase == DragPhase.LIFTED && slot == 0

    /** 长按拿起第 [i] 行（不可挪 · 离线 · 等回包 ⇒ `false`）。 */
    fun lift(
        rows: List<DragRow>,
        i: Int,
    ): Boolean {
        if (phase == DragPhase.WAITING || rows.getOrNull(i)?.movable != true || !lifts()) return false
        reset()
        this.rows = rows
        from = i
        slot = i
        phase = DragPhase.LIFTED
        return true
    }

    /** 跟手：手指从按下处挪了 [d]（px，纵向）；夹在可挪那一组里，空槽照越过的中线换格。 */
    fun move(d: Float) {
        if (phase != DragPhase.LIFTED) return
        val me = rows[from]
        val last = rows[last()]
        dy = d.coerceIn(rows[0].top - me.top, last.top + last.height - me.height - me.top)
        slot = slotAt(me.top + dy, me.top + me.height + dy)
    }

    /** 第 [j] 行此刻该挪多少（px）：拖的那一行跟手；空槽经过的那几行让一格。 */
    fun offset(j: Int): Float {
        if (phase == DragPhase.IDLE) return 0f
        if (j == from) return dy
        val me = rows[from]
        val next = rows.getOrNull(from + 1) ?: rows.getOrNull(from - 1)
        val gap = next?.let { kotlin.math.abs(it.top - me.top) - (if (it.top > me.top) me.height else it.height) } ?: 0f
        val step = me.height + gap
        return when {
            slot > from && j > from && j <= slot -> -step
            slot < from && j >= slot && j < from -> step
            else -> 0f
        }
    }

    /** 松手：交了 ⇒ 等回包（行留在放下的位置）。 */
    fun release(): DragEnd {
        if (phase != DragPhase.LIFTED) return DragEnd.None
        if (slot == from) {
            reset()
            return DragEnd.Back
        }
        phase = DragPhase.WAITING
        return DragEnd.Drop(account.orEmpty(), slot)
    }

    /** 取消（拖到一半被打断）：退回，不交。 */
    fun cancel() {
        if (phase != DragPhase.WAITING) reset()
    }

    /** 回包到了：回静止，之后照那台重画。 */
    fun settle() = reset()

    private fun last(): Int = rows.indexOfLast { it.movable }.coerceAtLeast(0)

    private fun slotAt(
        top: Float,
        bottom: Float,
    ): Int {
        fun mid(r: DragRow) = r.top + r.height / 2
        var s = from
        for (j in from - 1 downTo 0) if (top < mid(rows[j])) s = j
        for (j in from + 1..last()) if (bottom > mid(rows[j])) s = j
        return s
    }

    private fun reset() {
        phase = DragPhase.IDLE
        from = -1
        slot = -1
        dy = 0f
    }
}
