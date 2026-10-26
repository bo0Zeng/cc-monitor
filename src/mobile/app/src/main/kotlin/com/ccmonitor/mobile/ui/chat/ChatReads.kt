package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.RawPage
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply

/**
 * 一条会话读正文的那几问（一问一答 · `history-read` 分页读一段）。没成 ⇒ 那一句交 [failed]（上屏归 [ChatSession]）、回 `null`。
 */
internal class ChatReads(
    private val link: CoreLink,
    /** 这台在手机上的名字（「{machine} 无应答」那几句）。 */
    private val machine: String,
    private val failed: (String?) -> Unit,
) {
    /** 读到的一段：记录 ＋ 每一问的 `rows[]` 原样（一问一页，缓存存它）。 */
    class Read(
        val records: List<LineRecord>,
        val pages: List<RawPage>,
    )

    /** 问一次；没成 ⇒ 那一句、`null`。 */
    suspend fun ask(
        cmd: String,
        args: Map<String, Any?>,
    ): Any? =
        when (val r = link.call(cmd, args, READ_MS)) {
            is Reply.Ok -> r.data
            else -> null.also { failed(r.problem(machine, cmd)?.text) }
        }

    /** 回包照 [of] 解；解不出 ⇒ 「读不懂那台的回包」那一句、`null`。 */
    fun <T> parsed(
        cmd: String,
        data: Any?,
        of: (Any?) -> T?,
    ): T? = of(data) ?: null.also { failed(unreadableReply(machine, cmd, data.toString()).text) }

    /** 一页回包里 [until] 之前的那几行：记录 ＋ `rows[]` 原样（缓存存它）；形状不对 ⇒ `null`。 */
    private class Cut(
        val records: List<LineRecord>,
        val rows: List<Any?>,
        val next: Long,
        val eof: Boolean,
    )

    private fun cut(
        data: Any?,
        until: Long,
    ): Cut? {
        val page = HistoryRead.of(data) ?: return null
        val rows = HistoryRead.rawRows(data)?.filter { ((it as? Map<*, *>)?.get("end") as? Number)?.toLong()?.let { e -> e <= until } ?: true } ?: return null
        return HistoryRead.decodeRows(rows)?.let { Cut(it, rows, page.next, page.eof) }
    }

    /**
     * 读 `[from, until)` 里的记录，一页不够就接着读；每读到一页交 [each]（要先上屏的用它）。[first] 是已经拿到的第一页
     * （`history-tail` 随回包带来的，从 [from] 起）；那一页越过 [until] 的几行不要（往上翻时第一页是整个尾段的）。
     */
    suspend fun range(
        p: String,
        from: Long,
        until: Long,
        first: Any? = null,
        each: (List<LineRecord>) -> Unit = {},
    ): Read? {
        val out = mutableListOf<LineRecord>()
        val pages = mutableListOf<RawPage>()
        var at = from
        var given = first
        while (at < until) {
            val data = given ?: ask(HistoryRead.COMMAND, HistoryRead.args(p, at, until)) ?: return null
            given = null
            val page = parsed(HistoryRead.COMMAND, data) { cut(it, until) } ?: return null
            out += page.records
            each(page.records)
            pages += RawPage(at, minOf(page.next, until), page.rows)
            if (page.eof || page.next <= at) break
            at = page.next
        }
        return Read(out, pages)
    }

    companion object {
        const val READ_MS = 15_000L
    }
}
