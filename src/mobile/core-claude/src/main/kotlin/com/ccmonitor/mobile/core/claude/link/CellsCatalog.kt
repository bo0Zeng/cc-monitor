package com.ccmonitor.mobile.core.claude.link

/**
 * 格目录（帧命令 `cells-catalog`）：每件成品有哪些格。手机按它判「那一格在不在」，不按版本号猜（`ARCHITECTURE.md` §2.9）。
 */
class CellsCatalog private constructor(
    private val byProduct: Map<String, Set<String>>,
) {
    fun has(cell: Cell): Boolean = byProduct[cell.product]?.contains(cell.path) == true

    /** [wanted] 里目录没有的那几格。 */
    fun missing(wanted: Collection<Cell>): List<Cell> = wanted.filterNot(::has)

    data class Cell(
        val product: String,
        val path: String,
    ) {
        override fun toString(): String = "$product:$path"
    }

    companion object {
        const val COMMAND: String = "cells-catalog"

        /** `{products: [{name, frozen, cells: [{path, kind, type}]}], pending}`；形状不对 ⇒ `null`。 */
        fun of(data: Any?): CellsCatalog? =
            decoding {
                val products = data.asObj().objs("products").need()
                CellsCatalog(
                    products.associate { p ->
                        p.str("name").need() to
                            p
                                .objs("cells")
                                .need()
                                .mapNotNull { it.str("path") }
                                .toSet()
                    },
                )
            }
    }
}

/**
 * 手机读的格（出口声明的那一半：要哪几格）。接上一台就拿格目录核一遍，缺一格 ⇒ 两端契约对不上、不接（`架构.md` D4）。
 * 只登记格目录里有的成品；没进目录的成品（`quota-read` 的行 · `history-list` 的行）在这里登不了，见交回的缺格单。
 */
object MobileCells {
    private fun cells(
        product: String,
        vararg paths: String,
    ) = paths.map { CellsCatalog.Cell(product, it) }

    val READS: List<CellsCatalog.Cell> =
        cells("session_added", "sid", "path", "cwd", "name", "activity", "activity_text", "activity_tone", "agent_kind", "attachable") +
            cells("session_status", "sid", "activity", "activity_text", "activity_tone") +
            cells("session_removed", "sid", "cause") +
            cells("needs_row", "sid", "needs.kind", "needs.text", "needs.tone", "needs.rank", "needs.waitedText") +
            cells("facts", "needs.kind", "needs.text", "needs.tone", "needs.rank", "needs.waitedText", "usage.contextText", "usage.contextTone")
}
