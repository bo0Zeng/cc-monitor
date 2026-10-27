package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.FoldSegment
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.core.wire.gen.TurnPart
import com.ccmonitor.mobile.core.wire.gen.TurnRow

/** 流里的一项：一张卡，或一段过程的那一行。 */
sealed interface StreamItem {
    val key: String

    data class Card(
        val unit: RenderUnit,
    ) : StreamItem {
        override val key: String get() = unit.key
    }

    /** 一段过程（[open] ＝ 这一段点开了：段里的卡接在它后面）。 */
    data class Process(
        val fold: FoldSegment,
        val open: Boolean,
    ) : StreamItem {
        override val key: String get() = "proc#${fold.after}"
    }
}

/**
 * 过程照后端 `history-turns` 的分段折（大折叠，同桌面 `turn-fold.ts`）：每段 `after` 那一条的卡后面插一行过程，
 * 管到 `until` 那一条为止（`until` 没有 ⇒ 到尾）的卡收进去；哪几条露着、怎么分段、行上的字全是后端给的，手机不按卡型判、不自己划段。
 * 开合是「在看什么」：按 `after` 记在调用方，这里只照它排（[open] 里有 ⇒ 段里的卡接在过程行后面）。
 */
object TurnFolds {
    fun fold(
        units: List<RenderUnit>,
        turns: List<TurnRow>,
        open: Set<String>,
    ): List<StreamItem> {
        val folds = turns.flatMap { it.folds }
        if (folds.isEmpty()) return units.map { StreamItem.Card(it) }
        val byAfter = folds.associateBy { it.after }
        val out = ArrayList<StreamItem>(units.size)
        var cur: FoldSegment? = head(units, folds, byAfter)
        cur?.let { out += StreamItem.Process(it, it.after in open) }
        for ((i, u) in units.withIndex()) {
            val id = u.sourceUuid
            // 一条记录的头一张卡：段到 `until` 为止；又是另一段的 `after` · 本机还没进记录的那一句 ⇒ 也到此为止。
            val firstOfRecord = i == 0 || units[i - 1].sourceUuid != id
            if (cur != null && firstOfRecord && ends(cur, id, byAfter)) cur = null
            if (cur == null || cur.after in open) out += StreamItem.Card(u)
            // 这一条的最后一张卡之后：它是某一段的 `after` ⇒ 插那一行、往下的卡归那一段。
            val next = byAfter[id].takeIf { i == units.lastIndex || units[i + 1].sourceUuid != id }
            if (next != null) {
                cur = next
                out += StreamItem.Process(next, next.after in open)
            }
        }
        return out
    }

    /** 只拉了后半截：最上面那一段的 `after` 还没拉到 ⇒ 过程行画在已拉到的最上面、开头那几张卡归它。 */
    private fun head(
        units: List<RenderUnit>,
        folds: List<FoldSegment>,
        byAfter: Map<String, FoldSegment>,
    ): FoldSegment? {
        val top = units.firstOrNull()?.sourceUuid ?: return null
        val present = units.mapNotNullTo(HashSet()) { it.sourceUuid }
        val seg = folds.firstOrNull { it.after !in present && (it.until == null || it.until in present) } ?: return null
        return seg.takeUnless { ends(it, top, byAfter) }
    }

    /** 记录 [id]（`null` ＝ 本机还没进记录的那一句）不归 [seg]：它是那一段的 `until`，或另一段的 `after`。 */
    private fun ends(
        seg: FoldSegment,
        id: String?,
        byAfter: Map<String, FoldSegment>,
    ): Boolean = id == null || id == seg.until || id in byAfter

    /** 过程行的两行：第一行 plain 那几截（过程 · 计数），第二行 now · need · fail 那几截（行首；右端那一截另画）。分行只看语气。 */
    fun lines(fold: FoldSegment): Pair<List<TurnPart>, List<TurnPart>> = fold.parts.partition { it.tone == Tone.PLAIN }
}
