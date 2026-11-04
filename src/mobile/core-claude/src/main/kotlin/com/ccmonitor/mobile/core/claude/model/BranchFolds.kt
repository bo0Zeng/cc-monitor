package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.BranchRun

/**
 * 一个会话的主线外清单（Esc 回退掉、还留在文件里的那几条记录的 `id`）与后端分好的段：段里那几条 ＝ `off[at..at + n]`，框头照段的 `said`。
 * 冷读（`history-branch` 一页一页）与流上 `session_branch` 一帧同一种接法（[put]）；手机不分段、不数数，只照它记账。
 */
data class BranchList(
    val off: List<String>,
    val runs: List<BranchRun>,
) {
    /**
     * 接一截：前 [at] 条照旧、其后换成 [ids]；起在 [at] 之前的段照旧，其后的换成 [more]（`at` 恒是段首，不并段）。
     * 冷读的一页 ＝ 从第 `from` 条起的那一截，流上一帧 ＝ 从 `at` 起的那一截。
     */
    fun put(
        at: Long,
        ids: List<String>,
        more: List<BranchRun>,
    ): BranchList = BranchList(off.take(at.toInt()) + ids, runs.filter { it.at < at } + more)

    companion object {
        val EMPTY = BranchList(emptyList(), emptyList())
    }
}

/**
 * 流照主线外清单收框（同桌面 `branch-fold.ts` 的折法）：流里相邻、记录 id 落在同一段里的卡收进一个框（[StreamItem.Branch]）；
 * 不在清单里的卡 · 过程行 · 换了一段的卡断框。框收着 ⇒ 段里的卡不画；点开（[open] 里有那一段的段首条 id）⇒ 卡接在框头后面、带 `branch`。
 */
object BranchFolds {
    fun fold(
        stream: List<StreamItem>,
        list: BranchList,
        open: Set<String>,
    ): List<StreamItem> {
        if (list.runs.isEmpty()) return stream
        // 段首不在手上那份清单里（还没接到）⇒ 这一段不画。
        val runOf = HashMap<String, BranchRun>()
        for (r in list.runs) {
            if (r.at >= list.off.size) continue
            for (id in list.off.drop(r.at.toInt()).take(r.n.toInt())) runOf[id] = r
        }
        val out = ArrayList<StreamItem>(stream.size)
        var cur: BranchRun? = null
        var head: String? = null
        for (item in stream) {
            val r = (item as? StreamItem.Card)?.unit?.sourceUuid?.let(runOf::get)
            if (r == null) {
                cur = null
                out += item
                continue
            }
            if (r !== cur) {
                cur = r
                head = list.off[r.at.toInt()]
                out += StreamItem.Branch(run = head, said = r.said, open = head in open, first = item.key)
            }
            if (head in open) out += item.copy(branch = head)
        }
        return out
    }
}
