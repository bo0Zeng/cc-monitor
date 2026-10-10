package com.ccmonitor.mobile.ui.overview

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.BackendFeed
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * 「对话」这一屏与抽屉「最近」：一台机器上的会话清单 ＋ 需手动的那几条 ＋ 每条此刻的状态字。
 * 全是核心的成品：清单与段头 · 时刻读 `history-list`，需手动读 `sessions-needs`（已排好），状态读会话帧的
 * `activity_text` / `activity_tone`。这里只挑格、按段头连着的几行归成一段（不判天、不排序）。
 */
@OptIn(FlowPreview::class)
class ConversationsViewModel(
    private val backend: BackendFeed,
    private val machine: String,
) : ViewModel() {
    private val list = MutableStateFlow<Fetched<HistoryList.Answer>>(Fetched.Loading)
    private val needs = MutableStateFlow<Fetched<List<SessionsNeeds.Row>>>(Fetched.Loading)

    val state: StateFlow<ConversationsUiState> =
        combine(backend.state, backend.table, list, needs) { link, table, l, n -> view(link, table, l, n, machine) }
            .stateIn(viewModelScope, SharingStarted.Eagerly, ConversationsUiState())

    init {
        backend.ensure()
        viewModelScope.launch { backend.state.filter { it == LinkState.Up }.collect { refetchAll() } }
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

    private fun fetchNeeds() {
        viewModelScope.launch { needs.value = fetched(backend.call(SessionsNeeds.COMMAND, null), SessionsNeeds.COMMAND, SessionsNeeds::of) }
    }

    private fun <T> fetched(
        reply: Reply,
        command: String,
        decode: (Any?) -> T?,
    ): Fetched<T> =
        when (reply) {
            // 同一个 BUILD_ID 下解不出 ＝ 应答缺必填格：同帧上缺格的应答一个说法（详情里是哪一条命令）。
            is Reply.Ok -> decode(reply.data)?.let { Fetched.Got(it) } ?: Fetched.Failed(unreadableReply(command))
            else -> Fetched.Failed(reply.problem(machine) ?: unreadableReply(command))
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
    /** 「已等 3m」；没有起点 ⇒ `null`。 */
    val waitedText: String?,
)

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
    /** 此刻活着 ⇒ 状态字与语气（会话帧）；不在流上 ⇒ `null`（不画）。 */
    val status: Toned?,
)

internal fun view(
    link: LinkState,
    table: SessionTable,
    list: Fetched<HistoryList.Answer>,
    needs: Fetched<List<SessionsNeeds.Row>>,
    machine: String,
): ConversationsUiState {
    if (link is LinkState.Down) return ConversationsUiState(linkProblem = link.problem(machine), connecting = false, listLoading = false)
    if (link == LinkState.Connecting) return ConversationsUiState()
    val answer = (list as? Fetched.Got)?.value
    val labels = answer?.rows?.associate { it.sessionId to it.label }.orEmpty()
    // 段头是核心写好的；连着同一个段头的几行归一段（核心已按时间排好，这里不重排）。
    val sections = mutableListOf<HistorySection>()
    for (r in answer?.rows.orEmpty()) {
        val item = HistoryItem(r.sessionId, r.label, r.atText, r.agentTag, table.sessions[r.sessionId]?.activity)
        val last = sections.lastOrNull()
        if (last != null && last.title == r.sectionText) {
            sections[sections.size - 1] = last.copy(rows = last.rows + item)
        } else {
            sections += HistorySection(r.sectionText, listOf(item))
        }
    }
    return ConversationsUiState(
        connecting = false,
        needs = (needs as? Fetched.Got)?.value.orEmpty().map { NeedsItem(it.sid, labels[it.sid], it.needs.text, it.needs.waitedText) },
        needsProblem = (needs as? Fetched.Failed)?.problem,
        sections = sections,
        listLoading = list is Fetched.Loading,
        listProblem = (list as? Fetched.Failed)?.problem,
        notice = answer?.notice,
        truncated = answer?.truncated == true,
    )
}
