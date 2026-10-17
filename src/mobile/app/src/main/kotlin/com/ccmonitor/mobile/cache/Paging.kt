package com.ccmonitor.mobile.cache

/** 末页长到这么多格就另起一页（每次改末页要整页重写，页小一点写得快）。 */
const val PAGE_ROWS: Int = 60

/**
 * 末尾来了几格（[rows]，`{end, record}` 原样）：接在末页 [last] 后面，满了 [cap] 格另起一页。回要写的那几页（末页 ＋ 新起的）。
 * 每页到哪为止照那几格自己带的 `end`；[end] 是接完之后的字节终点（续点那一格），都不知道 ⇒ 照前一页的。没有末页 ⇒ 从 0 起一页。
 */
fun appendRows(
    last: RawPage?,
    rows: List<Any?>,
    end: Long?,
    cap: Int = PAGE_ROWS,
): List<RawPage> {
    if (rows.isEmpty()) return emptyList()
    val head = last ?: RawPage(0L, 0L, emptyList())
    val out = mutableListOf<RawPage>()
    var cur = head
    for (r in rows) {
        // 末页满了、而且知道它到哪为止（新页的页锚不和它撞）⇒ 另起一页；否则照接在末页上。
        if (cur.rows.size >= cap && cur.end > cur.start) {
            out += cur
            cur = RawPage(cur.end, cur.end, emptyList())
        }
        // 那一格自己带着字节终点（`{end, record}` 的 `end`）⇒ 这一页到它为止。
        val rowEnd = ((r as? Map<*, *>)?.get("end") as? Number)?.toLong()
        cur = cur.copy(rows = cur.rows + listOf(r), end = maxOf(cur.end, rowEnd ?: cur.end))
    }
    out += cur.copy(end = maxOf(cur.end, end ?: cur.end))
    return out
}
