package com.ccmonitor.mobile.ui.chats

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.HistorySearchMerge
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.ListSort
import com.ccmonitor.mobile.core.claude.link.wire.HistoryGroup
import com.ccmonitor.mobile.core.claude.link.wire.Merged
import com.ccmonitor.mobile.core.claude.link.wire.MergedRow
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.common.HostLink
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus

/** 机器表里的一台：id ＋ 屏上的名字。 */
data class HostName(
    val id: String,
    val label: String,
)

/** 看法：按时间（一列行）· 按目录（一个目录一行）。 */
enum class ChatsView { TIME, DIR }

/** 这一屏的筛选：看法 · 去掉了哪几台（缺省机器表里全勾）· 时间 · 排序 · 显示已隐藏。改一项马上重问。 */
data class Filters(
    val view: ChatsView = ChatsView.TIME,
    val unchecked: Set<String> = emptySet(),
    val withinDays: Int? = null,
    val sort: ListSort? = null,
    val hidden: Boolean = false,
) {
    /** 生效着的筛选（机器 · 时间 · 排序 · 显示已隐藏；看法不算）。 */
    val active: Boolean get() = unchecked.isNotEmpty() || withinDays != null || sort != null || hidden

    fun ask(query: String?): ListAsk = ListAsk(query, withinDays, sort, hidden)
}

/** 筛选抽屉里机器那一勾：名字 · 勾没勾 · 断没断。 */
data class HostChoice(
    val id: String,
    val label: String,
    val checked: Boolean,
    val offline: Boolean,
)

/** 一个目录一行（按目录）：那台的组 ＋ 是哪台的 ＋ 机器小标签。 */
data class ChatGroup(
    val hostId: String,
    val group: HistoryGroup,
    val machineTag: String?,
) {
    /** 展开着的组记在屏上用的键（同名目录在两台是两行）。 */
    val id: String get() = "$hostId\n${group.key}"
}

/** 按天连着的一段：段头（那台写的）＋ 那几行。 */
data class ChatSection(
    val title: String?,
    val rows: List<ChatRow>,
)

/** 搜内容合好的一行：哪台的（按 `origin` 对回）＋ 机器小标签 ＋ 那台写好的那一行。 */
data class ContentRow(
    val hostId: String,
    val machineTag: String?,
    val row: MergedRow,
)

/**
 * 搜内容「{q}」的结果：[searching] 还在问；[summary] 合好的那一句（没合成 ⇒ `null`）；[lines] 没答的台 · 读不动几份 · 只按标题的那几家；
 * [problem] 合并那一问没成。
 */
data class ContentUi(
    val query: String,
    val searching: Boolean = true,
    val summary: String? = null,
    val rows: List<ContentRow> = emptyList(),
    val lines: List<String> = emptyList(),
    val problem: Problem? = null,
)

/** 列表空着时说哪一句。 */
enum class Empty { NONE, NO_SESSIONS, NO_MATCH }

/** 会话屏要画的全部（几台并好）。 */
data class ChatsUi(
    val filters: Filters = Filters(),
    val query: String? = null,
    val choices: List<HostChoice> = emptyList(),
    val sections: List<ChatSection> = emptyList(),
    val groups: List<ChatGroup> = emptyList(),
    /** 各台顶上那一条（照机器表先后）。 */
    val bars: List<HostBar> = emptyList(),
    /** 各台装到哪一步（条上没画的那几台）。 */
    val deploying: List<HostPart> = emptyList(),
    val notices: List<String> = emptyList(),
    val truncated: Boolean = false,
    /** 一行都还没有、又有台在读 ⇒ 骨架。 */
    val skeleton: Boolean = true,
    val empty: Empty = Empty.NONE,
    /** 搜内容那一份（点了「搜内容「{q}」」之后；换词就收掉）。 */
    val content: ContentUi? = null,
) {
    val rows: List<ChatRow> get() = sections.flatMap { it.rows }

    /** 展开一组：并好的那一列里属于它的那几行（次序照并好的）。 */
    fun rowsOf(g: ChatGroup): List<ChatRow> = rows.filter { it.hostId == g.hostId && it.group == g.group.key }
}

/**
 * 会话屏：机器表里勾上的每台各一份 [HostChats]，几列按 `at` 并成一列、按目录时各台的组按 `order` 并（各台各自排好，只并、不另判；
 * 同值照机器表先后）。筛选改一项各台马上重问；搜标题停手 [QUERY_DEBOUNCE_MS] 再问。手指按着 / 在滚时对外那一份冻住（[hold]）。
 */
@OptIn(FlowPreview::class)
class ChatsBoard(
    /** 当前机器（它的行不写机器小标签）。 */
    private val current: String,
    hosts: Flow<List<HostName>>,
    private val make: (HostName, CoroutineScope) -> HostChats,
    scope: CoroutineScope? = null,
) : ViewModel() {
    private val scope = scope ?: viewModelScope
    private val filters = MutableStateFlow(Filters())
    private val query = MutableStateFlow<String?>(null)
    private val ssh = MutableStateFlow<Map<String, HostLink>>(emptyMap())
    private val table = MutableStateFlow<List<HostName>>(emptyList())
    private val boards = MutableStateFlow<List<HostChats>>(emptyList())
    private val jobs = mutableMapOf<String, Pair<HostChats, CoroutineScope>>()
    private val content = MutableStateFlow<ContentUi?>(null)
    private var contentJob: Job? = null
    private val held = MutableStateFlow(false)
    private val _ui = MutableStateFlow(ChatsUi())
    private var latest = ChatsUi()

    /** 对外那一份：[hold] 与 [release] 之间不换。 */
    val ui: StateFlow<ChatsUi> = _ui.asStateFlow()

    /** 此刻勾上的几台（屏上每台建一条 SSH 持有）。 */
    val checked: StateFlow<List<HostChats>> = boards.asStateFlow()

    init {
        this.scope.launch { hosts.collect { table.value = it } }
        this.scope.launch { combine(table, filters) { t, f -> t.filter { it.id !in f.unchecked } }.collect(::keep) }
        this.scope.launch {
            combine(filters, query.debounce { if (it == null) 0 else QUERY_DEBOUNCE_MS }) { f, q -> f.ask(q) }.collect { a -> boards.value.forEach { it.ask(a) } }
        }
        this.scope.launch {
            boards
                .map { bs -> if (bs.isEmpty()) flowOf(emptyList()) else combine(bs.map { it.state }) { it.toList() } }
                .collectLatest { snaps ->
                    combine(snaps, ssh, filters, combine(query, content, ::Pair), table) { s, l, f, (q, c), t ->
                        chatsView(s.map { hostPart(it, l[it.hostId]) }, current, t, f, q).copy(content = c)
                    }.collect { v ->
                        latest = v
                        if (!held.value) _ui.value = v
                    }
                }
        }
    }

    private fun keep(want: List<HostName>) {
        val ids = want.map { it.id }.toSet()
        (jobs.keys - ids).forEach { id -> jobs.remove(id)?.second?.cancel() }
        val ask = filters.value.ask(query.value)
        boards.value =
            want.map { h ->
                jobs
                    .getOrPut(h.id) {
                        val s = scope + SupervisorJob(scope.coroutineContext[Job])
                        make(h, s).also { it.ask(ask) } to s
                    }.first
            }
    }

    fun filters(change: (Filters) -> Filters) = filters.update(change)

    /** 搜标题：空串 ⇒ 收起（`null`）。 */
    fun query(q: String?) {
        val next = q?.takeIf { it.isNotBlank() }
        if (next != query.value) {
            contentJob?.cancel()
            content.value = null
        }
        query.value = next
    }

    /**
     * 搜内容：勾上的每台各问 `history-search`，答了的那几台回包里的 `sessions` 原样交一台 `history-search-merge`
     * （当前机器答了就是它，否则第一台答了的；它自己写 [HistorySearchMerge.LOCAL]、别的台写 hostId），照合好的排。
     */
    fun searchContent() {
        val q = query.value ?: return
        val hosts = boards.value
        contentJob?.cancel()
        content.value = ContentUi(q)
        contentJob =
            scope.launch {
                val per = hosts.map { h -> async { h to h.searchContent(q) } }.awaitAll()
                content.value = contentOf(q, per, current)
            }
    }

    /** 这台 SSH 那一层此刻的样子（屏上每台一条持有）。 */
    fun ssh(
        hostId: String,
        link: HostLink,
    ) = ssh.update { it + (hostId to link) }

    fun chats(hostId: String): HostChats? = jobs[hostId]?.first

    /** 手指按着 / 在滚：对外那一份冻住。 */
    fun hold() {
        held.value = true
    }

    /** 松手：换成最新的那一份。 */
    fun release() {
        held.value = false
        _ui.value = latest
    }

    companion object {
        const val QUERY_DEBOUNCE_MS = 300L
    }
}

private fun summaryOf(
    q: String,
    r: Merged,
): String =
    if (r.truncated) {
        copyText("history.search.summaryTruncated", "q" to q, "n" to r.totalHits, "m" to r.sessionCount)
    } else {
        copyText("history.search.summary", "q" to q, "n" to r.totalHits, "m" to r.sessionCount)
    }

/** 各台没答上的那几句 · 读不动几份 · 只按标题的那几家。 */
private fun contentLines(per: List<Pair<HostChats, Searched>>): List<String> =
    buildList {
        per.forEach { (h, s) ->
            when (s) {
                Searched.Down -> add(copyText("history.search.machineDown", "machine" to h.machine))
                is Searched.Failed -> add(s.problem.text)
                is Searched.Got -> Unit
            }
        }
        val got = per.mapNotNull { it.second as? Searched.Got }
        got.sumOf { it.reply.unreadable }.takeIf { it > 0 }?.let { add(copyText("history.search.unreadable", "n" to it)) }
        got.flatMap { it.reply.skipped }.distinct().forEach { add(copyText("history.search.skipped", "agent" to it)) }
    }

/** 各台答回来的 ⇒ 合一份（合并那一问交一台）。 */
private suspend fun contentOf(
    q: String,
    per: List<Pair<HostChats, Searched>>,
    current: String,
): ContentUi {
    val answered = per.mapNotNull { (h, s) -> (s as? Searched.Got)?.let { h to it } }
    val lines = contentLines(per)
    val merger = answered.firstOrNull { it.first.hostId == current }?.first ?: answered.firstOrNull()?.first ?: return ContentUi(q, searching = false, lines = lines)
    val machines = answered.map { (h, got) -> (if (h === merger) HistorySearchMerge.LOCAL else h.hostId) to got.raw }
    val tagged = per.size > 1
    return when (val m = merger.call(HistorySearchMerge.COMMAND, HistorySearchMerge.args(machines), HistorySearchMerge::of)) {
        is Fetched.Got -> {
            val r = m.value
            val byId = per.associate { it.first.hostId to it.first.machine }
            ContentUi(
                query = q,
                searching = false,
                summary = summaryOf(q, r),
                rows =
                    r.sessions.map { row ->
                        val id = row.origin ?: merger.hostId
                        ContentRow(id, byId[id].takeIf { tagged && id != current }, row)
                    },
                lines = lines,
            )
        }
        is Fetched.Failed -> ContentUi(q, searching = false, lines = lines, problem = m.problem)
        Fetched.Loading -> ContentUi(q, lines = lines)
    }
}

/**
 * 几台 ⇒ 一屏。[parts] 照机器表先后（只有勾上的）；[table] 是整张机器表（筛选抽屉里那一列）。
 * 机器小标签：勾上的不止一台、行又不在 [current] 上 ⇒ 写那台的名字。
 */
internal fun chatsView(
    parts: List<HostPart>,
    current: String,
    table: List<HostName>,
    filters: Filters,
    query: String?,
): ChatsUi {
    val multi = parts.size > 1
    val tag = { p: HostPart -> p.machine.takeIf { multi && p.hostId != current } }
    val rows = mergeBy(parts.map { p -> p.rows.map { it.copy(machineTag = tag(p)) } }) { it.at }
    val sections = mutableListOf<ChatSection>()
    for (r in rows) {
        val last = sections.lastOrNull()
        if (last != null && last.title == r.section) sections[sections.size - 1] = last.copy(rows = last.rows + r) else sections += ChatSection(r.section, listOf(r))
    }
    val groups = mergeBy(parts.map { p -> p.groups.map { ChatGroup(p.hostId, it, tag(p)) } }) { it.group.order }
    val down = parts.filter { it.bar?.offline == true }.map { it.hostId }.toSet()
    // 机器表还没读到（一台都没有、又没去勾）也算在读。
    val loading = parts.any { it.loading } || (parts.isEmpty() && !filters.active)
    return ChatsUi(
        filters = filters,
        query = query,
        choices = table.map { HostChoice(it.id, it.label, it.id !in filters.unchecked, it.id in down) },
        sections = sections,
        groups = groups,
        bars = parts.mapNotNull { it.bar },
        deploying = parts.filter { it.bar == null && it.deploying != null },
        notices = parts.mapNotNull { it.notice }.distinct(),
        truncated = parts.any { it.truncated },
        skeleton = rows.isEmpty() && loading,
        empty =
            when {
                rows.isNotEmpty() || loading -> Empty.NONE
                filters.active || query != null -> Empty.NO_MATCH
                else -> Empty.NO_SESSIONS
            },
    )
}

/** 各台各自按 [key] 倒序排好的几列 ⇒ 一列（同值照台的先后，稳定）。只并、不另判。 */
internal fun <T> mergeBy(
    lists: List<List<T>>,
    key: (T) -> Long,
): List<T> {
    val out = ArrayList<T>(lists.sumOf { it.size })
    val at = IntArray(lists.size)
    while (true) {
        var best = -1
        for (i in lists.indices) {
            val x = lists[i].getOrNull(at[i]) ?: continue
            if (best < 0 || key(x) > key(lists[best][at[best]])) best = i
        }
        if (best < 0) return out
        out += lists[best][at[best]++]
    }
}
