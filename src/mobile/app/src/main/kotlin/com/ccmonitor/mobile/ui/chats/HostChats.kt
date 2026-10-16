package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.cache.LocalCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.CanDelete
import com.ccmonitor.mobile.core.claude.link.wire.ForkedFrom
import com.ccmonitor.mobile.core.claude.link.wire.HistoryGroup
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.HistoryRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.notice
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.common.HostLink
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * 一台机器的会话清单：问 `history-list`（带这一屏的筛选）· 本地缓存 · 会话帧上的状态。会话屏每台一份、抽屉「最近」一份。
 * 缓存是没筛选的那一份 `rows[]` 原样：只在没筛选时画、只存没筛选时问到的；连上以后照那台新的那一份覆盖。
 */
@OptIn(FlowPreview::class)
class HostChats(
    val hostId: String,
    /** 这台在屏上的名字。 */
    val machine: String,
    val link: CoreLink,
    /** 清单该重问了（会话增删 · 隐藏那一格变了）。 */
    listChanged: Flow<Unit>,
    deploying: StateFlow<Deploying?>,
    private val reconnect: () -> Unit,
    /** 给这台装 / 更新后端（判定与做法在核心）。 */
    val deploy: () -> Unit,
    private val cache: ListCache,
    private val scope: CoroutineScope,
) {
    private val list = MutableStateFlow<Fetched<HistoryListPage>>(Fetched.Loading)
    private val cached = MutableStateFlow<HistoryListPage?>(null)
    private val asked = MutableStateFlow(ListAsk())

    val state: StateFlow<HostSnap> =
        combine(link.state, link.table, list, combine(cached, deploying, asked, ::Triple)) { l, t, got, (c, d, a) ->
            HostSnap(hostId, machine, l, t, got, c, d, a)
        }.stateIn(scope, SharingStarted.Eagerly, HostSnap(hostId, machine, link.state.value, link.table.value, Fetched.Loading, null, null, ListAsk()))

    init {
        scope.launch { cached.value = cache.rows().takeIf { it.isNotEmpty() }?.let(HistoryList::ofRows) }
        scope.launch {
            link.state
                .map { it is LinkState.Up }
                .distinctUntilChanged()
                .filter { it }
                .collect { fetch() }
        }
        scope.launch { listChanged.debounce(LIST_DEBOUNCE_MS).collect { fetch() } }
    }

    /** 换筛选：换了才重问（问的路上又换了 ⇒ 那一份作废）。 */
    fun ask(a: ListAsk) {
        if (asked.value == a) return
        asked.value = a
        list.value = Fetched.Loading
        fetch()
    }

    /** ［重试］：流断了 ⇒ 重接；通着 ⇒ 重问。 */
    fun retry() {
        if (link.state.value is LinkState.Down) reconnect() else fetch()
    }

    fun fetch() {
        val wanted = asked.value
        scope.launch {
            val reply = link.call(HistoryList.COMMAND, HistoryList.args(wanted))
            val got = fetched(reply, HistoryList.COMMAND, HistoryList::of)
            if (wanted != asked.value) return@launch
            list.value = got
            if (got is Fetched.Got && reply is Reply.Ok && wanted == ListAsk()) {
                HistoryList.rawRows(reply.data)?.let { raw -> cache.save(got.value.rows.map { it.sessionId }.zip(raw)) }
            }
        }
    }

    /** 问这台一条命令、照 [decode] 解回包。 */
    suspend fun <T> call(
        command: String,
        args: Map<String, Any?>?,
        decode: (Any?) -> T?,
    ): Fetched<T> = fetched(link.call(command, args), command, decode)

    private fun <T> fetched(
        reply: Reply,
        command: String,
        decode: (Any?) -> T?,
    ): Fetched<T> =
        when (reply) {
            // 同一个 BUILD_ID 下解不出 ＝ 应答缺必填格：同帧上缺格的应答一个说法（详情里是哪一条命令）。
            is Reply.Ok -> decode(reply.data)?.let { Fetched.Got(it) } ?: Fetched.Failed(unreadableReply(machine, command, reply.data.toString()))
            else -> Fetched.Failed(reply.problem(machine, command) ?: unreadableReply(machine, command, reply.toString()))
        }

    companion object {
        /** 会话帧里叫清单重问的那几种（会话增删）。 */
        val LIST_KINDS = setOf("session_added", "session_removed")

        /** 推帧主题：某条会话隐藏那一格变了（那台判的）。 */
        val LIST_TOPICS = setOf("session_hidden")
        const val LIST_DEBOUNCE_MS = 1_000L
    }
}

/** 一台此刻手上的全部：流 · 会话表 · 清单这一问 · 缓存 · 装到哪一步 · 带的筛选。 */
data class HostSnap(
    val hostId: String,
    val machine: String,
    val link: LinkState,
    val table: SessionTable,
    val list: Fetched<HistoryListPage>,
    val cached: HistoryListPage?,
    val deploying: Deploying?,
    val ask: ListAsk,
)

/** 会话屏上的一行（各台并成一列之后，每行记着是哪台的）。 */
data class ChatRow(
    val hostId: String,
    val sessionId: String,
    val label: String,
    /** 并几台时的排序键（那台给的）。 */
    val at: Long,
    val atText: String?,
    val section: String?,
    val agentTag: String?,
    /**
     * 状态字与语气：活着 ⇒ 会话帧那一句；离开活了 ⇒ `session_state` 那一句；流上没有 ⇒ 清单 `status` 那一格的名字（已结束 · 状态不明）；
     * 清单说活着、流上还没报到 · 认不出 ⇒ `null`（不画）。
     */
    val status: Toned?,
    /** 状态点（[SessionDot.of]，只照格选）；认不出 ⇒ `null`。 */
    val dot: SessionDot? = null,
    /** 隐藏 ⇒ 那台写好的那一句（为什么隐藏）；不隐藏 ⇒ `null`。 */
    val hidden: String? = null,
    val starred: Boolean = false,
    /** 目录短名（那台写的）。 */
    val project: String? = null,
    val projectPath: String = "",
    /** 按目录那一形里属于哪一组（`groups[].key`）。 */
    val group: String = "",
    /** 分叉那一句（[forkLine]）；不是分叉来的 ⇒ `null`。 */
    val fork: String? = null,
    val canDelete: CanDelete = CanDelete.YES,
    /** 机器小标签：勾上的不止一台、又不在当前机器上 ⇒ 那台的名字；否则 `null`。 */
    val machineTag: String? = null,
)

/** 一台在屏上要画的：行 · 组 · 顶上那一条 · 注解没并上那一句 · 截断 · 还在读。 */
data class HostPart(
    val hostId: String,
    val machine: String,
    val rows: List<ChatRow>,
    val groups: List<HistoryGroup>,
    val bar: HostBar?,
    val notice: String?,
    val truncated: Boolean,
    val loading: Boolean,
    /** 装 / 更新后端这一趟走到哪了（条上没画它时单独一行）。 */
    val deploying: Deploying? = null,
)

/**
 * 一台顶上那一条：[offline] ＝ 那台断着（虚线条 ＋［重新连接］），否则红条 ＋［重试］［复制详情］；
 * [fix] 是核心点名「装一次会好」的那一种。
 */
data class HostBar(
    val hostId: String,
    val problem: Problem,
    val offline: Boolean,
    val fix: Fix? = null,
    val deploying: Deploying? = null,
)

/**
 * 一台 ⇒ 要画的那一份。[ssh] 是这台 SSH 那一层（接不上 ⇒ 那一句当这台的条）。
 * 带了筛选那台又断着 ⇒ 那一句是「离线 · 未列出 / 未搜索」，不画行（缓存里是没筛选的那一份）。
 */
internal fun hostPart(
    s: HostSnap,
    ssh: HostLink? = null,
): HostPart {
    val plain = s.ask == ListAsk()
    val down = s.link as? LinkState.Down
    val got = (s.list as? Fetched.Got)?.value
    val answer = if (down != null) s.cached.takeIf { plain } else got ?: s.cached.takeIf { plain }
    val bar =
        when {
            ssh is HostLink.Failed -> HostBar(s.hostId, Problem(ssh.why, ""), offline = true)
            down != null && !plain -> HostBar(s.hostId, Problem(downLine(s.machine, s.ask), ""), offline = true)
            down != null -> HostBar(s.hostId, down.problem(), offline = !down.needsHand, down.fix, s.deploying)
            s.list is Fetched.Failed -> HostBar(s.hostId, s.list.problem, offline = false)
            s.link is LinkState.Up -> s.link.notice()?.let { HostBar(s.hostId, it, offline = false, s.link.fix, s.deploying) }
            else -> null
        }
    val table = s.table.takeIf { down == null }
    return HostPart(
        hostId = s.hostId,
        machine = s.machine,
        rows = answer?.rows.orEmpty().map { r -> rowOf(s.hostId, s.machine, r, table) },
        groups = answer?.groups.orEmpty(),
        bar = bar,
        notice = answer?.notice,
        truncated = answer?.truncated == true,
        loading = down == null && ssh !is HostLink.Failed && s.list is Fetched.Loading && answer == null,
        deploying = s.deploying,
    )
}

/** 带了筛选那台又断着的那一句：在搜 ⇒ 未搜索；只筛 ⇒ 未列出。 */
private fun downLine(
    machine: String,
    ask: ListAsk,
): String = if (ask.query != null) copyText("history.search.machineDown", "machine" to machine) else copyText("history.list.machineDown", "machine" to machine)

/** [table] 为 `null` ＝ 那台断着：此刻的状态谁都说不清 ⇒ 点画说不清、字写那台离线。 */
private fun rowOf(
    hostId: String,
    machine: String,
    r: HistoryRow,
    table: SessionTable?,
): ChatRow {
    val live = table?.sessions?.get(r.sessionId)
    val fate = table?.fates?.get(r.sessionId)
    val (status, dot) =
        if (table == null) {
            Toned(copyText("newSession.machine.offline", "machine" to machine), Tone.PLAIN) to SessionDot.UNKNOWN
        } else {
            statusOf(live?.activity, fate?.text, r.status) to SessionDot.of(live, fate, r.status)
        }
    return ChatRow(
        hostId = hostId,
        sessionId = r.sessionId,
        label = r.label,
        at = r.at,
        atText = r.atText,
        section = r.sectionText,
        agentTag = r.agentTag,
        status = status,
        dot = dot,
        hidden = r.hidden?.said,
        starred = r.starred,
        project = r.projectName,
        projectPath = r.projectPath,
        group = r.group,
        fork = forkLine(r.forkedFrom, r.forkedFromSessionId),
        canDelete = r.can.delete,
    )
}

/** 分叉那一句：父会话在清单里 ⇒ 带它的标题；只知道是分叉来的 ⇒ 「分叉自其他会话」；不是分叉来的 ⇒ `null`。 */
internal fun forkLine(
    from: ForkedFrom?,
    fromSessionId: String?,
): String? =
    when {
        from != null -> copyText("mobChats.fork.from", "label" to from.label)
        fromSessionId != null -> copyText("history.row.orphan")
        else -> null
    }

/** 一行的状态字：先会话帧、再去向那一句、再清单那一格的名字（文案表 `sessionState.*`，与桌面同一句）。 */
private fun statusOf(
    live: Toned?,
    fate: String?,
    listed: String?,
): Toned? =
    live ?: fate?.let { Toned(it, Tone.PLAIN) } ?: when (listed) {
        "ended" -> Toned(copyText("sessionState.ended.name"), Tone.PLAIN)
        "unknown" -> Toned(copyText("sessionState.unseen.name"), Tone.PLAIN)
        else -> null
    }

/** 一台的清单接到那台的常驻流上（[HostBackends]）、缓存接本地库；会话屏与抽屉「最近」同一种接法。 */
fun hostChats(
    backends: HostBackends,
    cache: LocalCache,
): (HostName, CoroutineScope) -> HostChats =
    { h, scope ->
        val feed = backends.of(h.id).also { it.ensure() }
        HostChats(
            hostId = h.id,
            machine = h.label,
            link = feed,
            listChanged = merge(feed.changes.filter { it in HostChats.LIST_KINDS }, feed.changed.filter { it.topic in HostChats.LIST_TOPICS }).map { },
            deploying = feed.deploying,
            reconnect = feed::reconnect,
            deploy = { backends.deploy(h.id) },
            cache = cache.list(h.id),
            scope = scope,
        )
    }
