package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.Anchor
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.RawPage
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.HistoryLines
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.HistoryTail
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.wire.gen.FactsReply
import com.ccmonitor.mobile.core.wire.gen.Held
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.TailHow
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * 一条会话的正文跟读（[ChatSession] 持有；名字带 `Locked` 的在它的锁里调）：手上读到哪（续点 · 尾段起点 · 下一行的行号）、
 * 认记录文件 · 读最后一屏 · 交续点对齐那台 · 按行号补缺 · 往上翻 · 从一处命中读起。读到的记录排进 [body]，排进了就叫一声 [took]；
 * 没读成的那一句照 [failSession] · [readFailed] 上屏。
 */
internal class ChatRecord(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
    private val lock: Mutex,
    private val cache: ChatCache,
    private val reads: ChatReads,
    private val body: RecordUnits,
    private val state: MutableStateFlow<ChatUiState>,
    private val sid: () -> String?,
    /** 排进了 [body]（分段要不要重问由持有者记）。 */
    private val took: () -> Unit,
    private val publishLocked: () -> Unit,
    private val failSession: (String?) -> Unit,
    private val readFailed: (String?) -> Unit,
    /** 从一处命中读起：命中那一页的第一张卡（落点）。 */
    private val landed: (String?) -> Unit,
) {
    /** 这条会话的记录文件；还没挂上 ⇒ `null`。 */
    var path: String? = null

    /** 已读到的最早一行的字节起点；已有的最新一行之后那一行的行号。 */
    var head = 0L
        private set
    private var nextSeq = 0L

    /** 手上的一页：从 [start] 字节起到 [end] 为止，几条记录（[body] 里几条）· 几行（尾段要几行照它数）。 */
    private class Span(
        val start: Long,
        var end: Long,
        var records: Int,
        var lines: Int,
    )

    /** 手上那几页，从早到晚；跟着尾巴时最前面的整页丢（[trimLocked]）。 */
    private val spans = ArrayDeque<Span>()

    /** 缓存里最早那一页从哪个字节起（往上翻读回丢掉的那几页时缓存里已有，不再存）；还没存过 ⇒ `null`。 */
    private var cachedFrom: Long? = null

    /** 手上几行（尾段要几行、往上翻要几行照它；还没读 ⇒ 一屏）。 */
    private fun tailLines(): Int = spans.sumOf { it.lines }.coerceAtLeast(ChatSession.PAGE_LINES)

    private fun spansOf(pages: List<RawPage>): List<Span> =
        pages.map { pg -> Span(pg.start, pg.end, pg.rows.count { (it as? Map<*, *>)?.get("record") != null }, pg.rows.count { (it as? Map<*, *>)?.get("end") != null }) }

    /** 末尾接上几行（[end] 是接完之后的字节终点，不知道 ⇒ 照前一页的）：末页不满一屏就接在它上面，满了另起一页。 */
    private fun appendSpan(
        end: Long?,
        records: Int,
        lines: Int,
    ) {
        val last = spans.lastOrNull()
        if (last != null && last.lines < ChatSession.PAGE_LINES) {
            last.end = end ?: last.end
            last.records += records
            last.lines += lines
        } else {
            val start = last?.end ?: head
            spans.addLast(Span(start, end ?: start, records, lines))
        }
    }

    /** 整份换成 [pages] 那几页。 */
    private fun resetSpans(pages: List<RawPage>) {
        spans.clear()
        spans.addAll(spansOf(pages))
    }

    /**
     * 跟着尾巴（持有者判视口贴着底）：手上多过「留几页」⇒ 最前面的整页丢，已读到的最早一行挪到下一页的起点（往上翻照它读回）。
     */
    fun trimLocked() {
        val keep = ChatSession.KEEP_PAGES * ChatSession.PAGE_LINES
        var lines = spans.sumOf { it.lines }
        var dropped = false
        while (spans.size > 1 && lines - spans.first().lines >= keep) {
            val gone = spans.removeFirst()
            lines -= gone.lines
            body.dropFirst(gone.records)
            head = spans.first().start
            dropped = true
        }
        if (dropped) state.value = state.value.copy(canLoadOlder = head > 0L)
    }

    /** 已有最新一行之后的字节（续读从这里接）；不知道 ⇒ `null`（下次从头读）。手上的记录是哪份文件的（[anchorPath]）· 那台给的记号（[anchorMark]，原样交回）。 */
    private var anchorOffset: Long? = null
    private var anchorPath: String? = null
    private var anchorMark: String = ""

    /** 会话被顶替、换一条记录读起：手上读到哪全清。 */
    fun reset() {
        path = null
        head = 0L
        nextSeq = 0L
        spans.clear()
        cachedFrom = null
        anchorOffset = null
        anchorPath = null
    }

    /**
     * 这条会话的记录文件（＋ 为认它已经问到的会话事实）。会话表里有 ⇒ 用它（目录也照它）；上次读过的那份（续点里记着）⇒ 照它接：
     * 离线时也挂得上，网回来（换了一条流）就整份对齐，目录等读 chip 那一趟事实时取；都没有 ⇒ [byId]。[fresh]：刚起的那条（见 [freshPath]）。
     */
    suspend fun recordOf(
        s: String,
        fresh: Boolean,
    ): Pair<String, FactsReply?>? {
        if (fresh) return freshPath(s) to null
        link.table.value.live[s]?.let { live ->
            if (state.value.cwd == null) live.cwd?.let { state.value = state.value.copy(cwd = it) }
            live.path?.let { return it to null }
        }
        anchorPath?.let { return it to null }
        return byId(s)
    }

    /** 只有会话 id：问那台 `history-facts {sid}`，它在自己的记录树里找那份记录（回包的 `path`）；找不到 · 问不到 ⇒ 那台写好的那一句上屏。 */
    private suspend fun byId(s: String): Pair<String, FactsReply>? {
        val r = link.call(HistoryFacts.COMMAND, HistoryFacts.bySid(s), READ_MS)
        if (r !is Reply.Ok) return null.also { failSession(r.problem(machine, HistoryFacts.COMMAND, link.offline.value)?.text) }
        val f = HistoryFacts.of(r.data)
        val p = f?.path ?: return null.also { failSession(unreadableReply(machine, HistoryFacts.COMMAND, r.data.toString()).text) }
        return p to f
    }

    /**
     * 刚起的那条的记录路径：第一句进去之前记录还没落盘，那台宣告这条会话时不带路径；记录出现时那台先推一帧带路径的状态帧
     * （在那一行的 line 帧之前）⇒ 等会话表里那一条有了路径。不设期限、不去清单里按点找：会话在，记录迟早来。
     */
    private suspend fun freshPath(s: String): String = link.table.mapNotNull { it.live[s]?.path }.first()

    /** 往上翻一段：尾段多要一屏，读到的接在前面（跟着尾巴时丢掉的那几页也照这样读回）。 */
    fun loadOlder() {
        val p = path ?: return
        val s = sid() ?: return
        if (state.value.loadingOlder || head <= 0L) return
        state.value = state.value.copy(loadingOlder = true, historyError = null)
        scope.launch {
            val want = lock.withLock { tailLines() } + ChatSession.PAGE_LINES
            val got =
                reads.ask(HistoryTail.COMMAND, HistoryTail.args(p, want))?.let { data ->
                    reads.parsed(HistoryTail.COMMAND, data, HistoryTail::of)?.let { t -> reads.range(p, t.splitAt, head, data)?.let { t to it } }
                }
            lock.withLock {
                if (got == null) {
                    state.value = state.value.copy(loadingOlder = false)
                    return@withLock
                }
                val (t, read) = got
                head = minOf(head, t.splitAt)
                take(read.records) { body.prepend(it) }
                spans.addAll(0, spansOf(read.pages))
                // 读回的是丢掉的那几页（缓存里已有）⇒ 只存比缓存最早那一页还早的。
                val fresh = cachedFrom?.let { c -> read.pages.filter { it.end <= c } } ?: read.pages
                if (fresh.isNotEmpty()) {
                    cache.prepend(s, fresh)
                    cachedFrom = fresh.first().start
                }
                state.value = state.value.copy(loadingOlder = false, canLoadOlder = head > 0L)
                publishLocked()
            }
        }
    }

    /** 接上一样；记录有变 ⇒ `true`。从头重读了 ⇒ 先 [reread]（读过的那一代的分段作废）。 */
    suspend fun stepLocked(
        p: String,
        e: Event,
        reread: () -> Unit,
    ): Boolean =
        when (e) {
            is Event.Line -> lineLocked(p, e.line)
            is Event.From -> e.seq <= nextSeq || fillLocked(p, e.seq)
            // 换了一条流：交手上那一份问一次，断开那一段怎么补那台判（续得上就只读那一截）。
            Event.Reconnected -> reloadLocked(p)
            // 从头重读了：手上那一份不交（续点作废），读最后一屏、整份重来（同桌面那一 tab 整份重来）。
            Event.Reread -> {
                anchorOffset = null
                reread()
                reloadLocked(p)
            }
        }

    /**
     * 上次读到的（缓存里的页原样，交给与线上同一个解码器）先上屏 ＋ 续点。有一页解不出 ⇒ 当没有缓存（照那台重读）。
     * 从一处命中打开（[at]）⇒ 缓存里那一屏不上。
     */
    suspend fun restoreLocked(
        s: String,
        at: Long?,
    ) {
        val cached = cache.load(s)?.takeIf { at == null } ?: return publishLocked()
        val got = cached.pages.map { HistoryRead.decodeRows(it.rows) ?: return publishLocked() }.flatten()
        take(got) { body.reset(it) }
        head = cached.pages.first().start
        cachedFrom = head
        resetSpans(cached.pages)
        cached.anchor?.let {
            anchorPath = it.path
            anchorOffset = it.offset
            anchorMark = it.mark
            nextSeq = it.seq
        }
        state.value = state.value.copy(canLoadOlder = head > 0L)
        publishLocked()
    }

    /** 一行：比已有的旧 ⇒ 重叠、不要；比已有的新、中间缺着 ⇒ 先补上。末页与续点跟着落盘。 */
    private suspend fun lineLocked(
        p: String,
        line: RecordLine,
    ): Boolean {
        if (line.seq > nextSeq && !fillLocked(p, line.seq)) return false
        if (line.seq < nextSeq) return false
        line.record?.let { take(listOf(it)) { rs -> body.append(rs) } }
        appendSpan(line.byteOffset, if (line.record != null) 1 else 0, 1)
        nextSeq = line.seq + 1
        anchorOffset = line.byteOffset
        sid()?.let { s -> cache.append(s, listOf(HistoryRead.row(line.byteOffset, line.raw)), line.byteOffset?.let { Anchor(p, it, nextSeq, anchorMark) }) }
        return true
    }

    /**
     * 对齐那台：交手上那一份（续点）问 `history-tail`，怎么接那台判、第一页随回包来。续得上 ⇒ 从续点往下读到 `end`、接在后面；
     * 没交 · 变短 · 被改写 ⇒ 读最后 [lines] 行（缺省：手上那几行）、整份重来；缺得比一屏多 ⇒ 那台不读那一截，这里读最后那几行、整份重来。
     * 没读成 ⇒ `false`（那一句已上屏），已有的不动。
     */
    suspend fun reloadLocked(
        p: String,
        lines: Int = tailLines(),
    ): Boolean {
        val s = sid() ?: return false
        val held = anchorOffset?.takeIf { !body.isEmpty() }?.let { o -> anchorPath?.let { Held(it, nextSeq, o, anchorMark) } }
        val data = reads.ask(HistoryTail.COMMAND, HistoryTail.args(p, lines, held)) ?: return false
        val t = reads.parsed(HistoryTail.COMMAND, data, HistoryTail::of) ?: return false
        val anchor = Anchor(t.held.path, t.held.nextByte, t.held.next, t.held.mark)
        val ok =
            when {
                t.how == TailHow.RESUME && held != null ->
                    reads.range(p, held.nextByte, t.end, data)?.also { read ->
                        take(read.records) { body.append(it) }
                        spans.addAll(spansOf(read.pages))
                        cache.append(s, read.pages.flatMap { it.rows }, anchor)
                    } != null
                // 缺得多那一形那台没读那一截：最后一屏这里自己读。
                else ->
                    reads.range(p, t.splitAt, t.end, data.takeIf { t.how != TailHow.HANDOFF })?.also { read ->
                        head = t.splitAt
                        take(read.records) { body.reset(it) }
                        // 一页都没有（尾段是空的）也记一页空的：页锚留着，下次先上缓存时「还能往上翻」照样对。
                        val pages = read.pages.ifEmpty { listOf(RawPage(t.splitAt, t.end, emptyList())) }
                        resetSpans(pages)
                        cache.replace(s, pages, anchor)
                        cachedFrom = t.splitAt
                    } != null
            }
        if (ok) heldLocked(anchor)
        return ok
    }

    /** 挂上那一下读正文：从一处命中打开 ⇒ 从那里读（[jumpLocked]），否则读最后一屏。 */
    suspend fun loadLocked(
        p: String,
        at: Long?,
    ): Boolean = if (at == null) reloadLocked(p) else jumpLocked(p, at)

    /** 读成了一段、拿到那台给的续点：记下、上屏。 */
    private fun heldLocked(anchor: Anchor) {
        nextSeq = anchor.seq
        anchorOffset = anchor.offset
        anchorPath = anchor.path
        anchorMark = anchor.mark
        state.value = state.value.copy(canLoadOlder = head > 0L, failedWhy = null, historyError = null)
        publishLocked()
    }

    /**
     * 从一处命中读起：第一问 `history-read {at}` 读命中那一页、先上屏（落点是它的第一张卡）；再往下读到尾，交读到的行数问 `history-tail` 拿续点。
     * 尾段起点正好是 [at]、终点正好是读到的那里 ⇒ 读过的就是尾段，照它记续点、换缓存；对不上（这当中记录变了）⇒ 照那几行读尾段（[reloadLocked]）。
     * 之后往上翻、跟流、断线重接都照旧：尾段就是从命中那一行起的那几行（手上那几页）。没读成 ⇒ `false`（那一句已上屏）。
     */
    private suspend fun jumpLocked(
        p: String,
        at: Long,
    ): Boolean {
        var first = true
        val read =
            reads.range(p, at, Long.MAX_VALUE) { rs ->
                if (first) {
                    head = at
                    take(rs) { body.reset(it) }
                    landed(body.units().firstOrNull()?.key)
                    publishLocked()
                    first = false
                } else {
                    take(rs) { body.append(it) }
                }
            } ?: return false
        // 读到的可计行（残尾那一行没有 `end`，不算）。
        resetSpans(read.pages)
        val lines = spans.sumOf { it.lines }
        val t = reads.ask(HistoryTail.COMMAND, HistoryTail.args(p, lines))?.let { reads.parsed(HistoryTail.COMMAND, it, HistoryTail::of) } ?: return false
        if (t.splitAt != at || t.end != read.pages.lastOrNull()?.end) {
            anchorOffset = null
            return reloadLocked(p, lines)
        }
        val anchor = Anchor(t.held.path, t.held.nextByte, t.held.next, t.held.mark)
        sid()?.let { cache.replace(it, read.pages, anchor) }
        cachedFrom = at
        heldLocked(anchor)
        return true
    }

    /**
     * 补上 `[nextSeq, until)`：缺得不多按行号取（`history-lines`）；缺得多（超过一屏）⇒ 整份重读最后一屏、更早的往上翻再取。
     * 没补成 ⇒ `false`，已有的不动（下一行来了再补）。
     */
    private suspend fun fillLocked(
        p: String,
        until: Long,
    ): Boolean {
        if (until - nextSeq > ChatSession.PAGE_LINES) return reloadLocked(p, ChatSession.PAGE_LINES)
        var more = true
        while (more && nextSeq < until) {
            val r = link.call(HistoryLines.COMMAND, HistoryLines.args(p, nextSeq, until), READ_MS)
            val page =
                (r as? Reply.Ok)?.let { HistoryLines.of(it.data) }
                    ?: return false.also { readFailed(r.problem(machine, HistoryLines.COMMAND, link.offline.value)?.text ?: unreadableReply(machine, HistoryLines.COMMAND, r.toString()).text) }
            take(page.records) { body.append(it) }
            more = page.next > nextSeq && !page.eof
            appendSpan(page.end, page.records.size, (page.next - nextSeq).coerceAtLeast(0).toInt())
            nextSeq = maxOf(nextSeq, page.next)
            // 续点跟到这一段最后一个可计行之后；每格带它那一行之后的字节落末页。
            anchorOffset = page.end
            val anchor = anchorPath?.takeIf { it == p }?.let { Anchor(it, page.end, nextSeq, anchorMark) }
            sid()?.let { s -> cache.append(s, page.raw.mapIndexed { i, r -> HistoryRead.row(page.ends.getOrNull(i), r) }, anchor) }
        }
        nextSeq = maxOf(nextSeq, until)
        return true
    }

    /** 排进 [body]（[put] 是接后 · 接前 · 整份换哪一样），再叫一声 [took]。 */
    private inline fun take(
        rs: List<LineRecord>,
        put: (List<LineRecord>) -> Unit,
    ) {
        put(rs)
        took()
    }

    private companion object {
        const val READ_MS = ChatReads.READ_MS
    }
}

/** 一条会话流上来的两样：一行 · 进名单那一格（从第几行起上流）。 */
internal sealed interface Event {
    data class Line(
        val line: RecordLine,
    ) : Event

    data class From(
        val seq: Long,
    ) : Event

    /** 换了一条流（`CoreLink.epoch` 变了）：断开那段的行没收到。 */
    data object Reconnected : Event

    /** 那份记录被那台从头重读了（会话表里那一条换了一代）：读过的那一代作废。 */
    data object Reread : Event
}
