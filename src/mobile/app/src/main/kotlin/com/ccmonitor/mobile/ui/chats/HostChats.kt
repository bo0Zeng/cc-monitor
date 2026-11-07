package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.cache.LocalCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Groups
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.HistorySearch
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.deployFix
import com.ccmonitor.mobile.core.claude.link.face
import com.ccmonitor.mobile.core.claude.link.retry
import com.ccmonitor.mobile.core.claude.link.seen
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.CanDelete
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.GroupMember
import com.ccmonitor.mobile.core.wire.gen.HistoryGroup
import com.ccmonitor.mobile.core.wire.gen.HistoryRow
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionDot
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.barOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * 一台机器的会话清单：问 `history-list`（带这一屏的筛选）· 本地缓存 · 会话帧上的状态。会话屏每台一份、抽屉「最近」一份。
 * 缓存是没筛选那一问手上那一份（[ListCache]，住嵌的核心）：只在没筛选时画；没筛选那一问带着它的记号问、答交核心换好再画，
 * 那台只给变了的那几行。带筛选的每次整份、不存。
 */
@OptIn(FlowPreview::class)
class HostChats(
    val hostId: String,
    /** 这台在屏上的名字。 */
    val machine: String,
    val link: CoreLink,
    /** 清单该重问了（那台推的 `changed {history_list}`）。 */
    listChanged: Flow<Unit>,
    /** 组那一片该重问了（那台推的 `changed {groups}`）。 */
    groupsChanged: Flow<Unit>,
    deploying: StateFlow<Deploying?>,
    /** 给这台装 / 更新后端（判定与做法在核心）。 */
    val deploy: () -> Unit,
    private val cache: ListCache,
    private val scope: CoroutineScope,
) {
    private val list = MutableStateFlow<Fetched<HistoryListPage>>(Fetched.Loading)
    private val cached = MutableStateFlow<HistoryListPage?>(null)
    private val asked = MutableStateFlow(ListAsk())
    private val groups = MutableStateFlow(GroupsHeld())

    /** 没筛选那一问：读记号 · 问 · 交核心换一趟一趟来，不叠着（叠着就拿旧记号的答盖新的那一份）。 */
    private val syncing = Mutex()

    val state: StateFlow<HostSnap> =
        combine(link.state, link.table, list, combine(cached, deploying, asked, groups) { c, d, a, g -> Tail(c, d, a, g) }) { l, t, got, tail ->
            HostSnap(hostId, machine, l, t, got, tail.cached, tail.deploying, tail.ask, tail.groups.read, tail.groups.problem)
        }.stateIn(scope, SharingStarted.Eagerly, HostSnap(hostId, machine, link.state.value, link.table.value, Fetched.Loading, null, null, ListAsk()))

    init {
        scope.launch { cached.value = syncing.withLock { cache.held() }?.takeIf { !it.order.isNullOrEmpty() }?.let(HistoryList::page) }
        // 接上的每一回（核心那一格，断着漏了的推送跟着补上）：清单 · 组各问一次。
        scope.launch {
            link.seen.collect {
                fetch()
                fetchGroups()
            }
        }
        scope.launch { listChanged.debounce(LIST_DEBOUNCE_MS).collect { fetch() } }
        scope.launch { groupsChanged.debounce(LIST_DEBOUNCE_MS).collect { fetchGroups() } }
    }

    /** 问这台自己那一片组（[Groups]，几台由核心合）：答了 ⇒ 换上；答了读不出 · 那台报错 ⇒ 留着上一份、记下那一句；没答（断着 · 无应答）⇒ 留着上一份（条上已说）。 */
    fun fetchGroups() {
        scope.launch {
            val reply = link.call(Groups.COMMAND, Groups.ARGS)
            when (val got = fetched(reply, Groups.COMMAND, Groups::of)) {
                is Fetched.Got -> groups.value = GroupsHeld(got.value, null)
                is Fetched.Failed -> if (reply !is Reply.TimedOut && reply !is Reply.LinkDown && reply !is Reply.Cancelled) groups.update { it.copy(problem = got.problem) }
                Fetched.Loading -> Unit
            }
        }
    }

    /** 换筛选：换了才重问（问的路上又换了 ⇒ 那一份作废）。 */
    fun ask(a: ListAsk) {
        if (asked.value == a) return
        asked.value = a
        list.value = Fetched.Loading
        fetch()
    }

    /** ［重试］：流断着 ⇒ 重接（接上了照上面那一份重问）；通着 ⇒ 重问（[CoreLink.retry]）。 */
    fun retry() =
        link.retry {
            fetch()
            fetchGroups()
        }

    fun fetch() {
        val wanted = asked.value
        scope.launch {
            val got =
                if (wanted == ListAsk()) {
                    syncing.withLock { synced() }
                } else {
                    fetched(link.call(HistoryList.COMMAND, HistoryList.args(wanted)), HistoryList.COMMAND, HistoryList::of)
                }
            if (wanted != asked.value) return@launch
            list.value = got
        }
    }

    /** 没筛选那一问：带着手上那一份的记号问，答交核心换好（核心存盘）、画换好的整份；答不上 · 读不动 ⇒ 手上那一份不动。 */
    private suspend fun synced(): Fetched<HistoryListPage> {
        val reply = link.call(HistoryList.COMMAND, HistoryList.args(ListAsk(), cache.held()?.held))
        val page = (reply as? Reply.Ok)?.let { cache.keep(it.data) }?.let(HistoryList::page)
        return fetched(reply, HistoryList.COMMAND) { page }
    }

    /** 搜这台的会话内容（[Searched]）。 */
    suspend fun searchContent(q: String): Searched =
        when (val reply = link.call(HistorySearch.COMMAND, HistorySearch.args(q))) {
            is Reply.TimedOut, Reply.LinkDown, is Reply.Cancelled -> Searched.Down
            else ->
                when (val got = fetched(reply, HistorySearch.COMMAND) { d -> (d as? Map<*, *>)?.let { Searched.Got(d) } }) {
                    is Fetched.Got -> got.value
                    is Fetched.Failed -> Searched.Failed(got.problem)
                    Fetched.Loading -> Searched.Down
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
            else -> Fetched.Failed(reply.problem(machine, command, link.offline.value) ?: unreadableReply(machine, command, reply.toString()))
        }

    companion object {
        const val LIST_DEBOUNCE_MS = 1_000L
    }
}

/** 一台搜内容的结局：答了（回包原样，交核心合）· 没答（断着 · 无应答）· 答了读不出。 */
sealed interface Searched {
    data class Got(
        val raw: Any?,
    ) : Searched

    data object Down : Searched

    data class Failed(
        val problem: Problem,
    ) : Searched
}

/** 一台此刻手上的全部：流 · 会话表 · 清单这一问 · 缓存 · 装到哪一步 · 带的筛选。 */
data class HostSnap(
    val hostId: String,
    val machine: String,
    /** 核心交的那一行（通没通 ＋ 那台的状态成品）；还没交过 ⇒ `null`。 */
    val link: LinkLine?,
    val table: SessionBook,
    val list: Fetched<HistoryListPage>,
    val cached: HistoryListPage?,
    val deploying: Deploying?,
    val ask: ListAsk,
    /** 这台自己那一片组（`groups-here` 最近答上的那一份，原样交核心合）；还没答过 ⇒ `null`。 */
    val groups: Any? = null,
    /** 最近那一问组答了读不出 · 那台报错的那一句；答上了 ⇒ `null`。 */
    val groupsProblem: Problem? = null,
)

/** 组那一问手上那一份：最近答上的 ＋ 之后那一问没成的那一句。 */
private data class GroupsHeld(
    val read: Any? = null,
    val problem: Problem? = null,
)

/** [HostChats.state] 后半那几样（`combine` 只收五样）。 */
private data class Tail(
    val cached: HistoryListPage?,
    val deploying: Deploying?,
    val ask: ListAsk,
    val groups: GroupsHeld,
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
    /** 状态字与语气：那台给的脸的短名（会话簿里有它 ⇒ 簿里那一份，否则清单那一行的；那台断着 ⇒ 簿里看不见那一张，见 [rowOf]）。 */
    val status: Toned,
    val dot: SessionDot,
    /** 隐藏 ⇒ 那台写好的那一句（为什么隐藏）；不隐藏 ⇒ `null`。 */
    val hidden: String? = null,
    val starred: Boolean = false,
    /** 目录短名（那台写的）。 */
    val project: String? = null,
    val projectPath: String = "",
    /** 按目录那一形里属于哪一组（`groups[].key`）。 */
    val group: String = "",
    /** 分叉自哪个会话（那台写的 `forkedFromSessionId`，只交核心挂树）；不是分叉来的 ⇒ `null`。 */
    val parent: String? = null,
    /** 它在分叉树里的样子（核心挂好的，[chatsView] 拷进来）。 */
    val nest: Nest = Nest(),
    val canDelete: CanDelete = CanDelete.YES,
    /** 机器小标签：勾上的不止一台、又不在当前机器上 ⇒ 那台的名字；否则 `null`。 */
    val machineTag: String? = null,
)

/**
 * 一行在分叉树里的样子（核心 `nest` 回的，桌面历史页同一套）：[child] 挂在展开了的父下 · [forks] 这一列里有几条分叉自它 ·
 * [forksOpen] 那几条展开着 · [orphan] 是分叉、父不在这一列。
 */
data class Nest(
    val child: Boolean = false,
    val forks: Int = 0,
    val forksOpen: Boolean = false,
    val orphan: Boolean = false,
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
    /** 装 / 更新后端这一趟的结局（条上没画它时单独一行；走到哪一步在成品条那一格里，见 [HostBar]）。 */
    val deploying: Deploying? = null,
    /** 那台成品说离线时名字旁那个词（筛选抽屉照摆）；没离线 ⇒ `null`。 */
    val word: String? = null,
    /** 这台组那一片（交核心几台合一次）；断着 · 还没答过组 ⇒ `null`。 */
    val slice: Slice? = null,
    /** 组那一问没成的那一句（那台写的）。 */
    val groupLines: List<String> = emptyList(),
)

/** 一台组那一片：`groups-here` 回包原样（交核心）· 组员落成行要的那几样（清单里那几行 · 会话簿）。 */
data class Slice(
    val raw: Any?,
    val rows: Map<String, ChatRow>,
    val book: SessionBook,
) {
    /** 组里那一条 ⇒ 屏上那一行：清单里有的照清单那一行，没有的照组员那一行。 */
    fun row(
        hostId: String,
        m: GroupMember,
    ): ChatRow = rows[m.sid] ?: memberRow(hostId, m, book)
}

/**
 * 一台顶上那一条。那台通着或断着时整条是成品条那一格（[barOf]，机器条同一份）：语气 [tone] · 那一句 · 进度 [progress]；
 * 画它的是机器条那一个组件（`MachineBarView`，样子照语气）；成品没点名装 / 更新 ⇒ 按钮是［重试］。
 * 手机这一侧的三样（SSH 那层接不上 · 带筛选时那台断着 ⇒ offline；清单读不出 ⇒ fail）不在成品里，这里照那一侧给。
 * [fix] 是成品给的「装一次会好」那一颗；[edit] 是改哪一台的设置能修好（跳板坏了）⇒ 条下给［编辑］。
 */
data class HostBar(
    val hostId: String,
    val problem: Problem,
    val tone: Tone,
    val fix: MachineFixButton? = null,
    val edit: String? = null,
    val progress: Boolean = false,
)

/** 那台成品说离线时名字旁那个词；没离线 · 没交过 · 没写词 ⇒ `null`。 */
private fun offlineWord(line: LinkLine?): String? {
    val m = line?.machine ?: return null
    return m.word.takeIf { m.offline && it.isNotEmpty() }
}

/** 成品条那一格 ⇒ 顶条；成品不带条 ⇒ `null`。 */
private fun productBar(
    hostId: String,
    line: LinkLine,
): HostBar? = barOf(line)?.let { l -> HostBar(hostId, Problem(l.text, l.detail), l.tone, line.deployFix(), progress = l.progress) }

/**
 * 一台 ⇒ 要画的那一份。[ssh] 是这台 SSH 那一层（接不上 ⇒ 那一句当这台的条）。
 * 带了筛选那台又断着 ⇒ 那一句是「离线 · 未列出 / 未搜索」，不画行（缓存里是没筛选的那一份）。
 */
internal fun hostPart(
    s: HostSnap,
    ssh: HostLink? = null,
): HostPart {
    val plain = s.ask == ListAsk()
    val down = s.link as? LinkLine.Down
    val got = (s.list as? Fetched.Got)?.value
    val answer = if (down != null) s.cached.takeIf { plain } else got ?: s.cached.takeIf { plain }
    val bar =
        when {
            ssh is HostLink.Failed -> HostBar(s.hostId, Problem(ssh.why, ""), Tone.OFFLINE, edit = ssh.edit)
            down != null && !plain -> HostBar(s.hostId, Problem(downLine(s.machine, s.ask), ""), Tone.OFFLINE)
            down != null -> productBar(s.hostId, down)
            s.list is Fetched.Failed -> HostBar(s.hostId, s.list.problem, Tone.FAIL)
            s.link is LinkLine.Up -> productBar(s.hostId, s.link)
            else -> null
        }
    val rows = answer?.rows.orEmpty().map { r -> rowOf(s.hostId, r, s.table) }
    return HostPart(
        hostId = s.hostId,
        machine = s.machine,
        rows = rows,
        groups = answer?.groups.orEmpty(),
        bar = bar,
        notice = answer?.notice,
        truncated = answer?.truncated == true,
        loading = down == null && ssh !is HostLink.Failed && s.list is Fetched.Loading && answer == null,
        deploying = s.deploying,
        word = offlineWord(s.link),
        slice = sliceOf(s, rows, ssh),
        groupLines = listOfNotNull(s.groupsProblem?.text),
    )
}

/** 这台组那一片（交核心合）：断着 · SSH 接不上 · 还没答过 ⇒ 不交（组员的脸会过时，那几条回到按天那几段）。 */
private fun sliceOf(
    s: HostSnap,
    rows: List<ChatRow>,
    ssh: HostLink?,
): Slice? {
    val raw = s.groups ?: return null
    if (s.link is LinkLine.Down || ssh is HostLink.Failed) return null
    return Slice(raw, rows.associateBy { r -> r.sessionId }, s.table)
}

/** 组里那一条清单里没有（窗外 · 被筛掉）⇒ 照组员那一行：那台写的标题 · 脸（流上有它 ⇒ 流上那一份）；能不能删这里说不清。 */
private fun memberRow(
    hostId: String,
    m: GroupMember,
    table: SessionBook,
): ChatRow {
    val face = table.face(m.sid) ?: m.face
    return ChatRow(
        hostId = hostId,
        sessionId = m.sid,
        label = m.facts.title,
        at = m.facts.at ?: 0L,
        atText = null,
        section = null,
        agentTag = null,
        status = Toned(face.name, face.tone),
        dot = face.dot,
        projectPath = m.facts.cwd.orEmpty(),
        canDelete = CanDelete.UNSURE,
    )
}

/** 带了筛选那台又断着的那一句：在搜 ⇒ 未搜索；只筛 ⇒ 未列出。 */
private fun downLine(
    machine: String,
    ask: ListAsk,
): String = if (ask.query != null) copyText("history.search.machineDown", "machine" to machine) else copyText("history.list.machineDown", "machine" to machine)

/**
 * 每行的脸，只照会话簿（核心折好的，桌面同一份折法）：簿里有它 ⇒ 那一张（那台断着时活的 · 可重连的已折成看不见那一张）；
 * 没有 ⇒ 清单那一行的；簿说那台此刻看不见（`unseen`）、清单那一行又不是「看不见时照旧作数」的 ⇒ 簿那一张。
 */
private fun rowOf(
    hostId: String,
    r: HistoryRow,
    book: SessionBook,
): ChatRow {
    val face = book.face(r.sessionId) ?: book.unseen?.takeUnless { r.face.settled } ?: r.face
    return ChatRow(
        hostId = hostId,
        sessionId = r.sessionId,
        label = r.label,
        at = r.at,
        atText = r.atText,
        section = r.sectionText,
        agentTag = r.agentTag,
        status = Toned(face.name, face.tone),
        dot = face.dot,
        hidden = r.hidden?.said,
        starred = r.starred,
        project = r.projectName,
        projectPath = r.projectPath,
        group = r.group,
        parent = r.forkedFromSessionId,
        canDelete = r.can.delete,
    )
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
            // 那台的会话清单动了（会话来去 · 名字 / 状态 · 记录长了 · 注解 · 隐藏规则，那台判的）。
            listChanged = feed.changed.filter { it is Changed.HistoryList }.map { },
            // 那台的组那一片动了（注解里组 · 组员 · 固定变了，那台判的）。
            groupsChanged = feed.changed.filter { it is Changed.Groups }.map { },
            deploying = feed.deploying,
            deploy = { backends.deploy(h.id) },
            cache = cache.list(h.id),
            scope = scope,
        )
    }
