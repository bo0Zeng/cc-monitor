package com.ccmonitor.mobile.ui.overview

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.HistoryAnnotate
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.deployFix
import com.ccmonitor.mobile.core.claude.link.wire.Annotated
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.MachineFixButton
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.notice
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.FlowPreview
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
 * 「对话」这一屏与抽屉「最近」：一台机器上的会话清单 ＋ 需手动的那几条 ＋ 每条此刻的状态字。
 * 全是核心的成品：清单与段头 · 时刻读 `history-list`，需手动读 `sessions-needs`（已排好），状态读那台给的整份脸
 * （`face`：会话帧上最新那一份，没有就清单那一行的）。这里只挑格、按段头连着的几行归成一段（不判天、不排序）。
 * 清单先上本地缓存（上次那一份 `rows[]` 原样，离线也看得到），连上以后照那台给的新的那一份覆盖、再存下。
 * 隐藏谁由那台判（行上 `hidden` 那一格）：「显示已隐藏」只换问 `history-list` 的那个参数；改单个那一格走 `history-annotate`。
 */
@OptIn(FlowPreview::class)
class ConversationsViewModel(
    private val backend: ResidentFeed,
    private val machine: String,
    private val cache: ListCache = ListCache.None,
    /** 给这台装 / 更新后端（判定与做法在核心；包里那几格由 App 解出来交过去）。 */
    private val deployBackend: () -> Unit = {},
) : ViewModel() {
    private val list = MutableStateFlow<Fetched<HistoryListPage>>(Fetched.Loading)
    private val needs = MutableStateFlow<Fetched<List<NeedsRow>>>(Fetched.Loading)
    private val cached = MutableStateFlow<HistoryListPage?>(null)
    private val showHidden = MutableStateFlow(false)

    val state: StateFlow<ConversationsUiState> =
        combine(backend.state, backend.table, list, needs, combine(cached, backend.deploying, showHidden, ::Triple)) { link, table, l, n, (c, d, h) -> view(machine, link, table, l, n, needsAt, c, d, h) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConversationsUiState())

    init {
        viewModelScope.launch { cached.value = cache.rows().takeIf { it.isNotEmpty() }?.let(HistoryList::ofRows) }
        backend.ensure()
        viewModelScope.launch {
            backend.state
                .map { it is LinkState.Up }
                .distinctUntilChanged()
                .filter { it }
                .collect { refetchAll() }
        }
        viewModelScope.launch {
            merge(backend.changes.filter { it in LIST_KINDS }, backend.changed.filter { it.topic in LIST_TOPICS }.map { it.topic })
                .debounce(LIST_DEBOUNCE_MS)
                .collect { fetchList() }
        }
        viewModelScope.launch {
            backend.changes
                .filter { it in NEEDS_KINDS }
                .debounce(NEEDS_DEBOUNCE_MS)
                .collect { fetchNeeds() }
        }
    }

    /** 重试：流断了 ⇒ 重接；流通着 ⇒ 两样都重问。 */
    fun retry() {
        if (backend.state.value is LinkState.Down) backend.reconnect() else refetchAll()
    }

    /** ［安装］/［更新］/［核对版本］：核心点名的那一种修法。 */
    fun deploy() = deployBackend()

    /** 过滤「显示已隐藏」：换了就照新的参数重问（屏上先照开关挑手上那一份的行）。 */
    fun showHidden(on: Boolean) {
        if (showHidden.value == on) return
        showHidden.value = on
        fetchList()
    }

    /**
     * 改单个隐藏那一格（[HistoryAnnotate.hide]），改成了就重问清单。成了 ⇒ 改之前那一格（撤销原样交回）；没成 ⇒ 那一句。
     * 不挂 [viewModelScope]：撤销在应用根部的反馈里跑，这一屏可能已经关了。
     */
    suspend fun setHide(
        sid: String,
        pin: Hide?,
    ): Fetched<Annotated> {
        val got = fetched(backend.call(HistoryAnnotate.COMMAND, HistoryAnnotate.hide(sid, pin)), HistoryAnnotate.COMMAND, HistoryAnnotate::of)
        if (got is Fetched.Got) fetchList()
        return got
    }

    private fun refetchAll() {
        fetchList()
        fetchNeeds()
    }

    private fun fetchList() {
        // 手机只连这一台、直接问它的常驻：不带 origin / listing ⇒ 那台自己的清单并上它的注解（`history_list.rs` 头注）。
        val wanted = showHidden.value
        viewModelScope.launch {
            val reply = backend.call(HistoryList.COMMAND, HistoryList.args(wanted))
            val got = fetched(reply, HistoryList.COMMAND, HistoryList::of)
            // 问的路上开关又换了 ⇒ 这一份作废（换的那一下已经照新的参数另问了）。
            if (wanted != showHidden.value) return@launch
            list.value = got
            // 存的是那台交来的那几行原样（按 sid 一行一格），不是这里挑过的格。
            if (got is Fetched.Got && reply is Reply.Ok) {
                HistoryList.rawRows(reply.data)?.let { raw ->
                    cache.save(
                        got.value.rows
                            .map { it.sessionId }
                            .zip(raw),
                    )
                }
            }
        }
    }

    /** 最近一份 `sessions-needs` 收到的本机时刻（「已等」从这里接着走）。 */
    @Volatile private var needsAt = 0L

    private fun fetchNeeds() {
        viewModelScope.launch {
            val got = fetched(backend.call(SessionsNeeds.COMMAND, null), SessionsNeeds.COMMAND, SessionsNeeds::of)
            needsAt = monoNow()
            needs.value = got
        }
    }

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

    private companion object {
        val LIST_KINDS = setOf("session_added", "session_removed")

        /** 推帧主题：某条会话隐藏那一格变了（那台判的；重问清单）。 */
        val LIST_TOPICS = setOf("session_hidden")
        val NEEDS_KINDS = setOf("session_added", "session_status", "session_removed")
        const val LIST_DEBOUNCE_MS = 1_000L
        const val NEEDS_DEBOUNCE_MS = 300L
    }
}

/** 一问的三态。 */
sealed interface Fetched<out T> {
    data object Loading : Fetched<Nothing>

    data class Got<T>(
        val value: T,
    ) : Fetched<T>

    data class Failed(
        val problem: Problem,
    ) : Fetched<Nothing>
}

/** 这一屏要画的全部。 */
data class ConversationsUiState(
    /** 流走不通 ⇒ 那一句（整屏只说这一句 ＋［重试］）；通着 ⇒ `null`。 */
    val linkProblem: Problem? = null,
    /** 通着、但要人动手的那一句（那台后端旧了：照用对得上的那几条，屏上排在需手动最前）；没有 ⇒ `null`。 */
    val linkNotice: Problem? = null,
    /** 成品给的「装一次会好」那一颗（问题条上给［安装］/［更新］）；没给 ⇒ `null`。 */
    val fix: MachineFixButton? = null,
    /** 装 / 更新后端这一趟走到哪了（核心写好的话）；没装过 ⇒ `null`。 */
    val deploying: Deploying? = null,
    val connecting: Boolean = true,
    val needs: List<NeedsItem> = emptyList(),
    val needsProblem: Problem? = null,
    val sections: List<HistorySection> = emptyList(),
    val listLoading: Boolean = true,
    val listProblem: Problem? = null,
    /** 注解没并上的那句话（核心写好）。 */
    val notice: String? = null,
    val truncated: Boolean = false,
    /** 过滤「显示已隐藏」开着没有。 */
    val showHidden: Boolean = false,
) {
    /** 抽屉「最近」：清单最前面那几条（核心已按最后活动排好），隐藏的不列（开着「显示已隐藏」也不列）。 */
    val recent: List<HistoryItem> get() = sections.flatMap { it.rows }.filter { it.hidden == null }
}

/** 需手动的一条。 */
data class NeedsItem(
    val sessionId: String,
    /** 那台写好的标题（同清单那一行）。 */
    val label: String,
    /** 「等批准 · Bash」。 */
    val text: String,
    /** 那台答出那一刻已等了多久（毫秒）；没有起点 ⇒ `null`。 */
    val waitedMs: Long?,
    /** 收到那份的本机时刻（[monoNow] 那一种钟）。 */
    val receivedAt: Long,
    /** 状态点（那台给的脸）。 */
    val dot: SessionDot = SessionDot.NEEDS_YOU,
) {
    /** 此刻（[now]，同一种钟）已等多久：那台给的 ＋ 本机从收到起走过的（屏上每秒重画这一格，不重问那台）。 */
    fun waitedAt(now: Long): String? = DurationFormat.waited(waitedMs, receivedAt, now)
}

/** 本机单调钟（毫秒）：「已等」接着走表用，不受改系统时间影响。 */
internal fun monoNow(): Long = System.nanoTime() / NANOS_PER_MS

private const val NANOS_PER_MS = 1_000_000L

/** 清单里一段：段头（核心写的「今天」「昨天」…）＋ 那几行。 */
data class HistorySection(
    val title: String?,
    val rows: List<HistoryItem>,
)

data class HistoryItem(
    val sessionId: String,
    val label: String,
    val atText: String?,
    val agentTag: String?,
    /** 状态字与语气：那台给的脸的短名（流上有它 ⇒ 流上最新那一份，否则清单那一行的）；那台断着 ⇒ 离线那一句。 */
    val status: Toned,
    val dot: SessionDot,
    /** 隐藏 ⇒ 那台写好的那一句（为什么隐藏）；不隐藏 ⇒ `null`。 */
    val hidden: String? = null,
)

internal fun view(
    /** 这台在屏上的名字（断着时每行那一句里称呼它）。 */
    machine: String,
    link: LinkState,
    table: SessionTable,
    list: Fetched<HistoryListPage>,
    needs: Fetched<List<NeedsRow>>,
    needsAt: Long = 0,
    /** 本地缓存里上次那一份；那台这一份还没到（没连上 · 在问 · 没问成）时先画它。 */
    cached: HistoryListPage? = null,
    deploying: Deploying? = null,
    /** 过滤「显示已隐藏」：关着 ⇒ 行上带隐藏那一格的不画（那台这一份本来就不给；上次缓存里的照这一格挑）。 */
    showHidden: Boolean = false,
): ConversationsUiState {
    val answer = (list as? Fetched.Got)?.value ?: cached
    if (link is LinkState.Down) {
        return ConversationsUiState(
            linkProblem = link.problem(),
            fix = link.deployFix(),
            deploying = deploying,
            connecting = false,
            listLoading = false,
            sections = sectionsOf(cached, null, machine, showHidden),
            showHidden = showHidden,
        )
    }
    if (link == LinkState.Connecting) {
        return ConversationsUiState(deploying = deploying, sections = sectionsOf(cached, table, machine, showHidden), showHidden = showHidden)
    }
    return ConversationsUiState(
        connecting = false,
        linkNotice = (link as? LinkState.Up)?.notice(),
        fix = link.deployFix(),
        deploying = deploying,
        needs = (needs as? Fetched.Got)?.value.orEmpty().map { NeedsItem(it.sid, it.label, it.needs.text, it.needs.waitedMs, needsAt, it.face.dot) },
        needsProblem = (needs as? Fetched.Failed)?.problem,
        sections = sectionsOf(answer, table, machine, showHidden),
        listLoading = list is Fetched.Loading,
        listProblem = (list as? Fetched.Failed)?.problem,
        notice = answer?.notice,
        truncated = answer?.truncated == true,
        showHidden = showHidden,
    )
}

/**
 * 清单 ⇒ 几段：段头是核心写好的；连着同一个段头的几行归一段（核心已按时间排好，这里不重排）。
 * 每行的脸：流上有它 ⇒ 流上最新那一份，否则清单那一行的。[table] 为 `null` ＝ 那台断着：那台说「看不见时照旧作数」的（已结束 · 记录已不在）照画，
 * 其余点画说不清、字写那台离线（断前的会话表与缓存里的状态都可能过时）。
 */
private fun sectionsOf(
    answer: HistoryListPage?,
    table: SessionTable?,
    machine: String,
    showHidden: Boolean,
): List<HistorySection> {
    val sections = mutableListOf<HistorySection>()
    val offline = Toned(copyText("newSession.machine.offline", "machine" to machine), Tone.PLAIN)
    for (r in answer?.rows.orEmpty()) {
        if (r.hidden != null && !showHidden) continue
        val face = if (table == null) r.face.takeIf { it.settled } else table.faces[r.sessionId] ?: r.face
        val (status, dot) = if (face == null) offline to SessionDot.UNKNOWN else Toned(face.name, face.tone) to face.dot
        val item = HistoryItem(r.sessionId, r.label, r.atText, r.agentTag, status, dot, r.hidden?.said)
        val last = sections.lastOrNull()
        if (last != null && last.title == r.sectionText) {
            sections[sections.size - 1] = last.copy(rows = last.rows + item)
        } else {
            sections += HistorySection(r.sectionText, listOf(item))
        }
    }
    return sections
}
