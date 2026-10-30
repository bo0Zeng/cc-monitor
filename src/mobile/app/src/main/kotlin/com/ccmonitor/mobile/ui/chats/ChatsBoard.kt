package com.ccmonitor.mobile.ui.chats

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.ColumnFace
import com.ccmonitor.mobile.core.claude.link.HistorySearch
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.ListSort
import com.ccmonitor.mobile.core.claude.link.SearchMerge
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.ChatsColumnAsk
import com.ccmonitor.mobile.core.wire.gen.ColumnAt
import com.ccmonitor.mobile.core.wire.gen.ColumnHost
import com.ccmonitor.mobile.core.wire.gen.ColumnRow
import com.ccmonitor.mobile.core.wire.gen.EmptyFace
import com.ccmonitor.mobile.core.wire.gen.HistoryGroup
import com.ccmonitor.mobile.core.wire.gen.Merged
import com.ccmonitor.mobile.core.wire.gen.MergedRow
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.unreadableReply
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

    /** 按组分段只在按时间、没筛选也没搜标题时：组是用户摆的，筛着 · 搜着看的是命中的那几条。 */
    fun banded(query: String?): Boolean = view == ChatsView.TIME && !active && query == null
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

/**
 * 一组一段（`groups-read` 那一组，一台一段）：段头是那台写好的组名 [title] 与计数那几句 [heads]，段里是组员（清单里有的照清单那一行）。
 * [machineTag] 同行上的机器小标签。
 */
data class GroupBand(
    val hostId: String,
    val groupId: String,
    val title: String,
    val heads: List<String>,
    val rows: List<ChatRow>,
    val machineTag: String? = null,
) {
    /** 屏上用的键（同一组在两台是两段）。 */
    val id: String get() = "$hostId\n$groupId"
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
    /** 按天那几段（分了组 ⇒ 已在固定 · 组那几段里的不再重出）。 */
    val sections: List<ChatSection> = emptyList(),
    val groups: List<ChatGroup> = emptyList(),
    /** 固定的那一段（各台照机器表先后接）：只在按时间、没筛选也没搜标题时有。 */
    val pinned: List<ChatRow> = emptyList(),
    /** 一组一段（各台照机器表先后接）：同 [pinned]。 */
    val bands: List<GroupBand> = emptyList(),
    /** 各台顶上那一条（照机器表先后）。 */
    val bars: List<HostBar> = emptyList(),
    /** 各台装到哪一步（条上没画的那几台）。 */
    val deploying: List<HostPart> = emptyList(),
    val notices: List<String> = emptyList(),
    val truncated: Boolean = false,
    /** 一行都还没有、又有台在读（或首屏那一句核心还没交）⇒ 骨架。 */
    val skeleton: Boolean = true,
    val empty: Empty = Empty.NONE,
    /** [Empty.NO_SESSIONS] 时写的那一句（核心照各台的成品与报完了没有合的，桌面首屏同一份）：各台都报完才是「无会话」。 */
    val emptyFace: EmptyFace? = null,
    /** 搜内容那一份（点了「搜内容「{q}」」之后；换词就收掉）。 */
    val content: ContentUi? = null,
) {
    val rows: List<ChatRow> get() = sections.flatMap { it.rows }

    /** 展开一组：并好的那一列里属于它的那几行（次序照并好的）。 */
    fun rowsOf(g: ChatGroup): List<ChatRow> = rows.filter { it.hostId == g.hostId && it.group == g.group.key }
}

/**
 * 会话屏：机器表里勾上的每台各一份 [HostChats]，几列并成一列、切段、按目录时各台的组并成一列都由核心做（[ColumnFace]，本机后端并历史页那一份），
 * 这里照它摆。筛选改一项各台马上重问；搜标题停手 [QUERY_DEBOUNCE_MS] 再问。手指按着 / 在滚时对外那一份冻住（[hold]）。
 */
@OptIn(FlowPreview::class)
class ChatsBoard(
    /** 当前机器（它的行不写机器小标签）。 */
    private val current: String,
    hosts: Flow<List<HostName>>,
    private val make: (HostName, CoroutineScope) -> HostChats,
    /** 几台搜内容的结果合成一份（嵌的 Rust 核心那一步，[SearchMerge]）。 */
    private val merge: (String) -> String,
    scope: CoroutineScope? = null,
    /** 首屏那一句（`HostBackends.empty`）；还没交 ⇒ `null`。 */
    private val empty: Flow<EmptyFace?> = flowOf(null),
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
                    combine(snaps, ssh, filters, combine(query, content, ::Pair), combine(table, empty, ::Pair)) { s, l, f, (q, c), (t, e) ->
                        chatsView(s.map { hostPart(it, l[it.hostId]) }, current, t, f, q, e).copy(content = c)
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
     * 搜内容：勾上的每台各问 `history-search`，答了的那几台回包原样交核心合（[SearchMerge]，本机后端扇出完调的同一份），照合好的排；
     * 不交哪一台当信差。
     */
    fun searchContent() {
        val q = query.value ?: return
        val hosts = boards.value
        contentJob?.cancel()
        content.value = ContentUi(q)
        contentJob =
            scope.launch {
                val per = hosts.map { h -> async { h to h.searchContent(q) } }.awaitAll()
                content.value = contentOf(q, per, current, merge)
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

/** 没答上的那几台各一句 · 合好的那一份里读不动几份 · 只按标题的那几家。 */
private fun contentLines(
    per: List<Pair<HostChats, Searched>>,
    r: Merged?,
): List<String> =
    buildList {
        per.forEach { (h, s) ->
            when (s) {
                Searched.Down -> add(copyText("history.search.machineDown", "machine" to h.machine))
                is Searched.Failed -> add(s.problem.text)
                is Searched.Got -> Unit
            }
        }
        r?.unreadable?.takeIf { it > 0 }?.let { add(copyText("history.search.unreadable", "n" to it)) }
        r?.skipped?.forEach { add(copyText("history.search.skipped", "agent" to it)) }
    }

/** 各台答回来的 ⇒ 核心合一份。 */
private fun contentOf(
    q: String,
    per: List<Pair<HostChats, Searched>>,
    current: String,
    merge: (String) -> String,
): ContentUi {
    val answered = per.mapNotNull { (h, s) -> (s as? Searched.Got)?.let { h.hostId to it.raw } }
    if (answered.isEmpty()) return ContentUi(q, searching = false, lines = contentLines(per, null))
    val byId = per.associate { it.first.hostId to it.first.machine }
    val input = SearchMerge.input(q, answered)
    val r =
        SearchMerge.of(merge(input))
            ?: return ContentUi(q, searching = false, lines = contentLines(per, null), problem = unreadableReply(byId[current] ?: current, HistorySearch.COMMAND, input))
    val tagged = per.size > 1
    return ContentUi(
        query = q,
        searching = false,
        summary = r.summary,
        rows =
            r.sessions.map { row ->
                val id = row.origin ?: current
                ContentRow(id, byId[id].takeIf { tagged && id != current }, row)
            },
        // 核心解不开哪台那一份 ⇒ 那台一句（同「答了读不出」那一说法）。
        lines = contentLines(per, r) + r.down.map { unreadableReply(byId[it.origin] ?: it.origin, HistorySearch.COMMAND, it.said).text },
    )
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
    /** 首屏那一句（核心交的；还没交 ⇒ `null`：不说「无会话」，照在读画骨架）。 */
    face: EmptyFace?,
): ChatsUi {
    val multi = parts.size > 1
    val tag = { p: HostPart -> p.machine.takeIf { multi && p.hostId != current } }
    val (pinned, bands) = if (filters.banded(query)) bandsOf(parts, tag) else emptyList<ChatRow>() to emptyList()
    val placed = (pinned + bands.flatMap { it.rows }).map { it.hostId to it.sessionId }.toSet()
    val none = parts.all { it.rows.isEmpty() } && placed.isEmpty()
    // 并 · 切段 · 组的先后都在核心；交回第几台第几项，这里照它摆那台那一行。
    val ask = ChatsColumnAsk(parts.map { p -> ColumnHost(p.rows.map { ColumnRow(it.at, it.section, (it.hostId to it.sessionId) in placed) }, p.groups.map { it.order }) })
    val column = ColumnFace.of(ask)
    val row = { a: ColumnAt -> parts[a.host.toInt()].let { p -> p.rows[a.at.toInt()].copy(machineTag = tag(p)) } }
    val sections = column.sections.map { s -> ChatSection(s.title, s.rows.map(row)) }
    val groups = column.groups.map { a -> parts[a.host.toInt()].let { p -> ChatGroup(p.hostId, p.groups[a.at.toInt()], tag(p)) } }
    val down = parts.filter { it.bar?.offline == true }.map { it.hostId }.toSet()
    // 机器表还没读到（一台都没有、又没去勾）也算在读。
    val loading = parts.any { it.loading } || (parts.isEmpty() && !filters.active)
    val empty = emptyOf(none, loading, filters.active || query != null, face)
    return ChatsUi(
        filters = filters,
        query = query,
        choices = table.map { HostChoice(it.id, it.label, it.id !in filters.unchecked, it.id in down) },
        sections = sections,
        groups = groups,
        pinned = pinned,
        bands = bands,
        bars = parts.mapNotNull { it.bar },
        deploying = parts.filter { it.bar == null && it.deploying != null },
        notices = parts.flatMap { listOfNotNull(it.notice) + it.groupLines }.distinct(),
        truncated = parts.any { it.truncated },
        skeleton = none && empty == Empty.NONE,
        empty = empty,
        emptyFace = face,
    )
}

/**
 * 空着时说哪一句：有行 · 在读 ⇒ 不说；带筛选 · 在搜 ⇒ 无匹配；没筛选 ⇒ 照核心那一句（各台报完才是「无会话」），它还没交 ⇒ 不说（当在读）。
 */
private fun emptyOf(
    none: Boolean,
    loading: Boolean,
    asked: Boolean,
    face: EmptyFace?,
): Empty =
    when {
        !none || loading -> Empty.NONE
        asked -> Empty.NO_MATCH
        face == null -> Empty.NONE
        else -> Empty.NO_SESSIONS
    }

/** 固定那一段与各组那几段（各台照机器表先后接）：段与行带上那台的机器小标签。 */
private fun bandsOf(
    parts: List<HostPart>,
    tag: (HostPart) -> String?,
): Pair<List<ChatRow>, List<GroupBand>> =
    parts.flatMap { p -> p.pinned.map { it.copy(machineTag = tag(p)) } } to
        parts.flatMap { p -> p.bands.map { b -> b.copy(machineTag = tag(p), rows = b.rows.map { it.copy(machineTag = tag(p)) }) } }
