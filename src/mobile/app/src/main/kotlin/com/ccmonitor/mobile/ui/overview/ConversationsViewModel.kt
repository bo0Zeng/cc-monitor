package com.ccmonitor.mobile.ui.overview

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
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
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * 「对话」这一屏与抽屉「最近」：一台机器上的会话清单 ＋ 需手动的那几条 ＋ 每条此刻的状态字。
 * 全是核心的成品：清单与段头 · 时刻读 `history-list`，需手动读 `sessions-needs`（已排好），状态读会话帧的
 * `activity_text` / `activity_tone`。这里只挑格、按段头连着的几行归成一段（不判天、不排序）。
 */
@OptIn(FlowPreview::class)
class ConversationsViewModel(
    private val backend: ResidentFeed,
    private val machine: String,
) : ViewModel() {
    private val list = MutableStateFlow<Fetched<HistoryListPage>>(Fetched.Loading)
    private val needs = MutableStateFlow<Fetched<List<NeedsRow>>>(Fetched.Loading)

    val state: StateFlow<ConversationsUiState> =
        combine(backend.state, backend.table, list, needs) { link, table, l, n -> view(link, table, l, n, needsAt) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConversationsUiState())

    init {
        backend.ensure()
        viewModelScope.launch {
            backend.state
                .map { it is LinkState.Up }
                .distinctUntilChanged()
                .filter { it }
                .collect { refetchAll() }
        }
        viewModelScope.launch {
            backend.changes
                .filter { it in LIST_KINDS }
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

    private fun refetchAll() {
        fetchList()
        fetchNeeds()
    }

    private fun fetchList() {
        // 手机只连这一台、直接问它的常驻：不带 origin / listing ⇒ 那台自己的清单并上它的注解（`history_list.rs` 头注）。
        viewModelScope.launch { list.value = fetched(backend.call(HistoryList.COMMAND, null), HistoryList.COMMAND, HistoryList::of) }
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
    val connecting: Boolean = true,
    val needs: List<NeedsItem> = emptyList(),
    val needsProblem: Problem? = null,
    val sections: List<HistorySection> = emptyList(),
    val listLoading: Boolean = true,
    val listProblem: Problem? = null,
    /** 注解没并上的那句话（核心写好）。 */
    val notice: String? = null,
    val truncated: Boolean = false,
) {
    /** 抽屉「最近」：清单最前面那几条（核心已按最后活动排好）。 */
    val recent: List<HistoryItem> get() = sections.flatMap { it.rows }
}

/** 需手动的一条。 */
data class NeedsItem(
    val sessionId: String,
    /** 清单里那一行的标题；清单里还没有它 ⇒ `null`（屏上不画标题，不拿 sid 顶）。 */
    val label: String?,
    /** 「等批准 · Bash」。 */
    val text: String,
    /** 那台答出那一刻已等了多久（毫秒）；没有起点 ⇒ `null`。 */
    val waitedMs: Long?,
    /** 收到那份的本机时刻（[monoNow] 那一种钟）。 */
    val receivedAt: Long,
    /** 状态点（照核心给的语气）。 */
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
    /**
     * 状态字与语气：活着 ⇒ 会话帧那一句；离开活了 ⇒ `session_state` 那一句；流上没有 ⇒ 清单 `status` 那一格的名字（已结束 · 状态不明）；
     * 清单说活着、流上还没报到 · 认不出 ⇒ `null`（不画）。
     */
    val status: Toned?,
    /** 状态点（[SessionDot.of]，只照格选）；认不出 ⇒ `null`（不画、占位照留）。 */
    val dot: SessionDot? = null,
)

internal fun view(
    link: LinkState,
    table: SessionTable,
    list: Fetched<HistoryListPage>,
    needs: Fetched<List<NeedsRow>>,
    needsAt: Long = 0,
): ConversationsUiState {
    if (link is LinkState.Down) return ConversationsUiState(linkProblem = link.problem(), connecting = false, listLoading = false)
    if (link == LinkState.Connecting) return ConversationsUiState()
    val answer = (list as? Fetched.Got)?.value
    val labels = answer?.rows?.associate { it.sessionId to it.label }.orEmpty()
    // 段头是核心写好的；连着同一个段头的几行归一段（核心已按时间排好，这里不重排）。
    val sections = mutableListOf<HistorySection>()
    for (r in answer?.rows.orEmpty()) {
        val live = table.sessions[r.sessionId]
        val fate = table.fates[r.sessionId]
        val item = HistoryItem(r.sessionId, r.label, r.atText, r.agentTag, statusOf(live?.activity, fate?.text, r.status), SessionDot.of(live, fate, r.status))
        val last = sections.lastOrNull()
        if (last != null && last.title == r.sectionText) {
            sections[sections.size - 1] = last.copy(rows = last.rows + item)
        } else {
            sections += HistorySection(r.sectionText, listOf(item))
        }
    }
    return ConversationsUiState(
        connecting = false,
        linkNotice = (link as? LinkState.Up)?.notice(),
        needs = (needs as? Fetched.Got)?.value.orEmpty().map { NeedsItem(it.sid, labels[it.sid], it.needs.text, it.needs.waitedMs, needsAt, SessionDot.live(it.needs.tone)) },
        needsProblem = (needs as? Fetched.Failed)?.problem,
        sections = sections,
        listLoading = list is Fetched.Loading,
        listProblem = (list as? Fetched.Failed)?.problem,
        notice = answer?.notice,
        truncated = answer?.truncated == true,
    )
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
