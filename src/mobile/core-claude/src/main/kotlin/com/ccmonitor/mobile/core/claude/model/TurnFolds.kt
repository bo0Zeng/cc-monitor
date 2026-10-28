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
        /** 它在哪一段过程里（那一段的 `after`；点开了才画出来）；露着的 ⇒ `null`。 */
        val fold: String? = null,
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
        val cur: FoldSegment? = head(units, folds, byAfter)
        cur?.let { out += StreamItem.Process(it, it.after in open) }
        walk(out, units, 0, cur, byAfter, open)
        return out
    }

    /**
     * 从上一份往下重折：[prev] 是拿 [prevUnits] · [prevFolds] · 同一份 [open] 折出来的。它最后一行过程之前的那几项原样留着（同一个实例），
     * 只从那一段的头往下折 —— 前提是那一段之前的单元没换（同一个实例）、分段没变；不满足 ⇒ 整份折。结果与 [fold] 一样。
     * 分段在时间上一段接一段、不交叠：最上面那一段（`after` 还没拉到的）不会因为尾巴上来了新的而换。
     */
    fun foldTail(
        prev: List<StreamItem>,
        prevUnits: List<RenderUnit>,
        prevFolds: List<FoldSegment>,
        units: List<RenderUnit>,
        turns: List<TurnRow>,
        open: Set<String>,
    ): List<StreamItem> {
        val folds = turns.flatMap { it.folds }
        val k = prev.indexOfLast { it is StreamItem.Process }
        val after = (prev.getOrNull(k) as? StreamItem.Process)?.fold?.after ?: return fold(units, turns, open)
        // 那一段在分段里排第几：它之前的分段一样、它自己还在（字可以变）。
        val at = prevFolds.indexOfFirst { it.after == after }
        val same = at >= 0 && folds.size > at && folds[at].after == after && (0 until at).all { folds[it] == prevFolds[it] }
        // 那一段从哪张卡之后起：`after` 那一条的最后一张卡之后；那之前的单元要原样。
        val j = if (same) units.indexOfLast { it.sourceUuid == after } + 1 else 0
        val kept = j in 1..prevUnits.size && prevUnits.getOrNull(j)?.sourceUuid != after && (0 until j).all { units[it] === prevUnits[it] }
        if (!kept) return fold(units, turns, open)
        val byAfter = folds.associateBy { it.after }
        val cur = folds[at]
        val out = ArrayList<StreamItem>(prev.size + units.size - prevUnits.size + 1)
        out.addAll(prev.subList(0, k))
        out += StreamItem.Process(cur, cur.after in open)
        walk(out, units, j, cur, byAfter, open)
        return out
    }

    /** 从第 [from] 张卡起往下折进 [out]（[start] ＝ 那张卡之前生效的那一段）。 */
    private fun walk(
        out: MutableList<StreamItem>,
        units: List<RenderUnit>,
        from: Int,
        start: FoldSegment?,
        byAfter: Map<String, FoldSegment>,
        open: Set<String>,
    ) {
        var cur = start
        for (i in from until units.size) {
            val u = units[i]
            val id = u.sourceUuid
            // 一条记录的头一张卡：段到 `until` 为止；又是另一段的 `after` · 本机还没进记录的那一句 ⇒ 也到此为止。
            val firstOfRecord = i == 0 || units[i - 1].sourceUuid != id
            if (cur != null && firstOfRecord && ends(cur, id, byAfter)) cur = null
            if (cur == null || cur.after in open) out += StreamItem.Card(u, cur?.after)
            // 这一条的最后一张卡之后：它是某一段的 `after` ⇒ 插那一行、往下的卡归那一段。
            val next = byAfter[id].takeIf { i == units.lastIndex || units[i + 1].sourceUuid != id }
            if (next != null) {
                cur = next
                out += StreamItem.Process(next, next.after in open)
            }
        }
    }

    /** 卡 [key] 收在哪一段过程里（那一段的 `after`）；露着 · 不在 ⇒ `null`。跳到一条之前先展开它（记成手动开）。 */
    fun owner(
        units: List<RenderUnit>,
        turns: List<TurnRow>,
        key: String,
    ): String? {
        val all = turns.flatMapTo(HashSet()) { t -> t.folds.map { it.after } }
        return fold(units, turns, all).firstNotNullOfOrNull { (it as? StreamItem.Card)?.takeIf { c -> c.unit.key == key }?.fold }
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
