package com.ccmonitor.mobile.ui.overview

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.catalog.lightForActivity
import com.ccmonitor.mobile.core.claude.transport.ConversationPager
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationReader
import com.ccmonitor.mobile.core.claude.transport.DaemonHistoryReader
import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.claude.transport.DaemonSignalBridge
import com.ccmonitor.mobile.core.claude.transport.SessionLineage
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * 会话总览面的 VM：把 [DaemonSessionSource] 的快照整理成几个分区。
 *
 * 总览只在看得见时才查，且不为每个会话各挂一条流。[DaemonSessionSource] 是一条 exec 出全部会话的长流，
 * 所以用 `WhileSubscribed`：离开总览面 ⇒ 没有订阅者 ⇒ 流被取消 ⇒ 远端 exec 被杀。
 *
 * 点灯只用 [lightForActivity]，这里不再写一份 `when (activity)`，否则远端加新值时两份词表各错各的。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SessionOverviewViewModel(
    private val source: DaemonSessionSource,
    /**
     * 跨通路信号汇。等待态只能从 daemon 这条通路来（聊天通路的 claude 不写 `status`），本 VM 是它唯一的产方。
     *
     * 刻意没有默认值：忘了接线时聊天屏永远收不到等待态，且没有断言会红，只能靠编译器守。
     *
     * 注意：投递发生在 [state] 的 `combine` 里，而 [state] 是 `WhileSubscribed` 的 ⇒ 没人看总览面时信号不更新。
     * 所以 [com.ccmonitor.mobile.core.claude.transport.SessionSignals.waitingGate] 带新鲜度阈值，过期降级成「提示但不拦」。
     */
    private val signals: SessionSignals,
    /** 读会话首条记录拿 `forkedFrom`。null = 不画血统（血统只是装饰）。 */
    private val history: DaemonHistoryReader? = null,
    /**
     * 历史列表的来源：daemon 的一次性查询面。
     *
     * `null` = 没接翻页。此时不许显示「已是最早」，那会把「没这功能」讲成「已到顶」。
     */
    private val conversations: DaemonConversationReader? = null,
    private val probeTimeoutMs: Long = DEFAULT_PROBE_TIMEOUT_MS,
    /**
     * 本地看到这一帧的时刻，不是「它开始等的时刻」（流上没有后者）。
     *
     * 函数型参数放在最后：调用方用尾随 lambda，往它后面插参数会绑错。
     */
    private val nowMs: () -> Long = System::currentTimeMillis,
) : ViewModel() {
    /**
     * 重连代：每 `+1` 重起一条流。
     *
     * 丢帧和 `fatal` 都靠它：对端每条新流都先做一次全量扫描、重新宣告所有会话，所以重连既是补齐也是重试。
     */
    private val generation = MutableStateFlow(0)

    /** `sid → 父 sid`，由 [DaemonHistoryReader] 在后台逐条填，不阻塞首屏。 */
    private val parents = MutableStateFlow<Map<String, String>>(emptyMap())

    /**
     * 历史翻页的状态，是一条与 [generation] 无关的独立流。
     *
     * 翻页只改这里、不碰 [generation] ⇒ `flatMapLatest` 不会重起 `source.states()`，翻页不打断实时流。
     * 重连一次要重新扛一次全量洪峰，代价很大。
     */
    private val historyPaging = MutableStateFlow(HistoryPaging(attached = conversations != null))

    /** 只读历史那一份（抽屉「最近」读它）。收它不起实时流，实时流只在有人收 [state] 时才起。 */
    val conversationHistory: StateFlow<HistoryPaging> = historyPaging.asStateFlow()

    /** 翻页器。第一次翻页时用 `--list-projects` 的结果建，之后一直复用（游标只前进）。 */
    private var pager: ConversationPager? = null

    /**
     * `--list-projects` 的原表，只用来查「翻到过的项目各有多少条」。不许拿它说「这台机器一共多少条对话」。
     *
     * 注意：这个声明（和 [pager]）必须在 `init` 之前。`init` 里的 [loadOlderConversations] 在
     * `Dispatchers.Main.immediate` 下会同步跑到第一次挂起，途中已经给它赋值；声明若在 `init` 之后，
     * `= emptyList()` 会把值擦掉，表现是第二页起组头的「已显示 / 共」静默消失。
     */
    private var projectTable: List<DaemonConversationCatalog.Project> = emptyList()

    init {
        // 进总览时就查首页，否则列表里只有 tmux 活会话。
        loadOlderConversations()
    }

    val state: StateFlow<SessionOverviewUiState> =
        combine(
            generation.flatMapLatest { source.states(probeTimeoutMs = probeTimeoutMs) },
            parents,
            historyPaging,
        ) { raw, parentMap, paging ->
            // 这里刻意不调 `acknowledgeDegraded()`，见 [reconnect]。
            fillParents(raw)
            autoResyncIfDropped(raw)
            // 总览通路 → 信号汇。吃的是合并快照，分不出是哪种帧带来的，所以走 `onSnapshot` 而不是 `onFrame`。
            DaemonSignalBridge.onSnapshot(signals, raw, nowMs())
            raw.toUiState(parentMap).copy(historyPaging = paging)
        }
            // `states()` 约定探测失败发 fatal 快照，但实现可能违约；违约也要看得见，不能静默空白。
            .catch { e -> emit(SessionOverviewUiState(fatal = e.message ?: e.javaClass.simpleName)) }
            .stateIn(
                scope = viewModelScope,
                started = SharingStarted.WhileSubscribed(STOP_TIMEOUT_MS),
                initialValue = SessionOverviewUiState(loading = true),
            )

    /**
     * 后台补血统：只查还不知道父、且有 `path` 的会话。
     *
     * 每会话一次 exec，靠 [DaemonHistoryReader] 的缓存兜住，否则每次重连都要再读一遍。
     */
    private fun fillParents(raw: DaemonSessionSource.State) {
        val reader = history ?: return
        val pathsBySid =
            raw.sessions.values
                .mapNotNull { s -> s.path?.let { s.sessionId to it } }
                .toMap()
        if (pathsBySid.keys.all { it in probedSids }) return
        // `combine` 每帧都跑，而探针要一整个 SSH 往返；立刻置位，免得同一批被并发查很多遍。
        probedSids += pathsBySid.keys
        viewModelScope.launch {
            val found = reader.parentsFor(pathsBySid)
            if (found.isNotEmpty()) parents.value = parents.value + found
        }
    }

    /** 已经发过探针的 sid（发出去就记，不等结果）。 */
    private val probedSids = HashSet<String>()

    /**
     * 重连：补齐丢失的帧，也是 `fatal` 之后唯一的重试入口。
     * `states()` 发完 fatal 就结束，而 `stateIn(WhileSubscribed)` 不会重启已完成的冷流。
     *
     * 不需要 `State.acknowledgeDegraded()`：新流从新的 `State` 建起，`dropped` 天然归零。
     * 反过来按「上一代丢了几行」去清标志会误判——`dropped` 每条流各自从 0 计，会吞掉重连后新的丢行。
     */
    fun reconnect() {
        probedSids.clear() // 换代重查一遍血统：期间可能有新会话、也可能上次探针整片失败过
        autoResyncUsed = false // 用户介入了 ⇒ 自动重连的额度重新给一次
        generation.value += 1
    }

    /**
     * 再往回翻一页历史对话。滑到底触发，也是历史那半失败之后的重试入口。
     *
     * 只动 [historyPaging]，不碰 [generation]。注意：不要把翻页实现成「重连一下」。
     * 按项目切页、只查被翻到的项目，这件事在 [ConversationPager] 里。
     */
    fun loadOlderConversations() {
        val reader = conversations ?: return // 没接翻页：什么都不做，也不许因此显示「已是最早」
        val now = historyPaging.value
        if (now.loading || now.exhausted) return // 已在翻 / 已到底 ⇒ 不重复发请求
        historyPaging.value = now.copy(loading = true, error = null)
        viewModelScope.launch {
            val p = pager ?: newPager(reader)
            if (p == null) {
                // 问不出来不许表现成没有对话：保留已有行，只加一条错误。
                historyPaging.value =
                    historyPaging.value.copy(loading = false, error = "暂时读不到这台服务器上的对话")
                return@launch
            }
            val page = p.loadNextPage()
            val prev = historyPaging.value
            historyPaging.value =
                prev.copy(
                    rows = prev.rows + page.added.map { it.toConversationRow() },
                    // 组头的数只对真被查过的项目说（`p.fetched`），不能对还没问过的项目报数。
                    pagedProjects = projectTable.filter { it.dirName in p.fetched }.map { it.toSummary() },
                    loading = false,
                    exhausted = page.exhausted,
                    error =
                        page.failedProjects
                            .takeIf { it.isNotEmpty() }
                            // 部分项目查不到 ⇒ 列表不全，必须说出来。
                            ?.let { "有 ${it.size} 个项目这次没读到，列表可能不全" },
                )
        }
    }

    /**
     * 第一次翻页时建翻页器：先问一次 `--list-projects`，拿到项目表再切页。
     *
     * @return `null` = 项目列表都问不出来。与「问出来了但没有项目」不同，后者正常建器、第一页为空、`exhausted = true`。
     */
    private suspend fun newPager(reader: DaemonConversationReader): ConversationPager? {
        val all = reader.projects() ?: return null
        projectTable = all
        // 尾随 lambda 绑的是 [ConversationPager] 的最后一个参数 `fetch`。
        return ConversationPager { dir -> reader.conversations(dir) }
            .also { it.start(all) }
            .also { pager = it }
    }

    /**
     * 丢过帧就自己重连一次，不等用户点；否则「少收了 N 行」会一直挂着而不补。
     *
     * 每次用户介入之间只自动重连一次：拥塞时新流往往立刻又丢帧，再自动重连就是死循环。之后交给「重试」按钮。
     */
    private fun autoResyncIfDropped(raw: DaemonSessionSource.State) {
        if ((raw.dropped ?: 0) <= 0) return
        if (autoResyncUsed) return
        autoResyncUsed = true
        generation.value += 1
    }

    /** 自动重连额度。不能按代号记：每次重连都产生新代号，会变成无限重连链。 */
    private var autoResyncUsed = false

    companion object {
        /** 冷启（SSH + 远端进程启动）比稳态慢得多；总览是用户主动进的，可以多等，且须长于 `states()` 自己的默认值。 */
        const val DEFAULT_PROBE_TIMEOUT_MS = 20_000L

        /** 转屏、短暂切后台不拆流重连，但也不常驻后台。5 秒覆盖配置变更重建。 */
        const val STOP_TIMEOUT_MS = 5_000L
    }
}

/**
 * 一屏的状态。
 *
 * @property needsYou 会话在等用户输入（deferred 批准 / 限流 / 认证失效都落在这里），排在「正在跑」前面。
 * @property superseded 被 `/branch`、`/clear` 顶替的旧 sid。不是死了，单列一区，混进「其余」会像会话崩了。
 * @property degraded 拥塞丢过行 ⇒ 手上数据不完整，必须显示。
 */
data class SessionOverviewUiState(
    val needsYou: List<SessionRow> = emptyList(),
    val running: List<SessionRow> = emptyList(),
    val others: List<SessionRow> = emptyList(),
    val superseded: List<SessionRow> = emptyList(),
    /**
     * 下线了，但服务器说不出为什么（对端没发 `cause`）。
     *
     * 不许并进 [others]，也不许直接消失，否则 `/clear` 之后对话会从总览面无声消失。
     */
    val removedUnknown: List<SessionRow> = emptyList(),
    val degraded: String? = null,
    val fatal: String? = null,
    val loading: Boolean = false,
    /** 还没补齐的累计丢行数。这是结构化事实，[degraded] 是由它派生的文案。 */
    val droppedLines: Int = 0,
    /** 「按项目历史」那一区。翻页的几个状态互斥且联动，收在一个对象里，UI 就读不到自相矛盾的字段。 */
    val historyPaging: HistoryPaging = HistoryPaging(),
) {
    /** 三个活动分区都空（归档不算「有会话」）。 */
    val isEmpty: Boolean get() = needsYou.isEmpty() && running.isEmpty() && others.isEmpty()

    /**
     * 屏幕上真的一条对话都没有。空态文案看这个，不看 [isEmpty]。
     *
     * [removedUnknown] 也要算：空态那一支整张列表不渲染，只剩这几行时它们会被吞掉。
     */
    val isCompletelyEmpty: Boolean get() = isEmpty && historyPaging.rows.isEmpty() && removedUnknown.isEmpty()
}

/**
 * 历史翻页的状态。
 *
 * @property attached 接上翻页没有。`false` ⇒ UI 不许显示「已是最早」。
 * @property exhausted 项目走完了，真的没有更早的了。只在 [attached] 时有意义。
 * @property error 这一页没读全的原因。保留已有 [rows]，失败不清空已翻到的内容。
 * @property pagedProjects 已经翻到过的项目，分区组头靠它说「已显示 {n} / 共 {N}」。
 *   它不是整张项目表：没翻到的项目不许出现在这里。
 */
data class HistoryPaging(
    val rows: List<ConversationRow> = emptyList(),
    val loading: Boolean = false,
    val exhausted: Boolean = false,
    val attached: Boolean = false,
    val error: String? = null,
    val pagedProjects: List<ProjectSummary> = emptyList(),
) {
    /** 还能不能再翻。三条都要满足，少一条就会转着圈但永远没有下一页。 */
    val canLoadMore: Boolean get() = attached && !exhausted && !loading

    /** 该不该显示「已是最早」。没接翻页时恒 false。 */
    val showsOldestReached: Boolean get() = attached && exhausted && rows.isNotEmpty()
}

/**
 * 历史区的一行：磁盘上的一条对话，不一定活着。
 *
 * 与 [SessionRow] 分开：那个是 daemon 说它此刻怎么样；合成一个的话就得为没在跑的对话编一个灯色。
 */
data class ConversationRow(
    val sessionId: String,
    val title: String,
    val cwd: String?,
    val jsonlPath: String?,
    val messageCount: Int,
    val updatedAtMs: Long,
)

/**
 * daemon 查询面的一条 → 一行。
 *
 * 标题回退用 [ClaudeSessionCatalog.readableTitle]，不另写一份。`aiTitle` 常为 `null`，这条回退是常走的路径。
 */
private fun DaemonConversationCatalog.Conversation.toConversationRow(): ConversationRow =
    ConversationRow(
        sessionId = sessionId,
        title = ClaudeSessionCatalog.readableTitle(parts = null, pidfileName = title, sessionId = sessionId),
        cwd = cwd,
        jsonlPath = jsonlPath,
        messageCount = messageCount,
        updatedAtMs = updatedAtMs,
    )

/** 项目 → 组头要的数。不带 `dirName`：它是翻页参数、不是显示名，带进 UI 层只会被误当成项目名。 */
private fun DaemonConversationCatalog.Project.toSummary(): ProjectSummary =
    ProjectSummary(projectPath = projectPath, sessionCount = sessionCount, lastActivityMs = lastActivityMs)

/**
 * 一行。
 *
 * @property attachable attach 进去对人有没有意义，不是「活着吗」。`false` ⇒ 不给 attach / ↗ /「杀死空 tmux」，
 *   但这一行仍要显示：它是一条真实存在的会话。
 */
data class SessionRow(
    val sessionId: String,
    val title: String,
    val cwd: String?,
    val light: SessionLight,
    val attachable: Boolean,
    val waitingFor: String? = null,
    /** 被 `/branch`、`/clear` 顶替，不是死了。这一档不点状态灯，行内记号不能写成「已停」。 */
    val archived: Boolean = false,
    /**
     * 下线了，而服务器说不出是被接替还是真结束了。
     *
     * 不许复用 [archived]：那个是我们知道的事，这个是「不知道」，合成一个就替对端把话说死了。
     * 同样不点状态灯。
     */
    val removalUnknown: Boolean = false,
    /**
     * daemon 判活的可信度：`authoritative`（pidfile + `/proc` 身份）或 `heuristic`（Codex 走 mtime 兜底）。
     *
     * 不改灯（活动档没问题，不确定的是判活），只如实标注，不替对端打包票。
     */
    val livenessUncertain: Boolean = false,
    /** `chat/{sid}` 路由要用：读正文要 JSONL 路径，选对 catalog/定位器要 agent 种类。 */
    val path: String? = null,
    val agentKind: String? = null,
    /**
     * 树里的深度（0 = 根）。血统来自会话首条记录的 `forkedFrom`。
     *
     * 没有分支的对话就是平的一行，`0` 是常态；UI 不为树预留恒空的缩进槽位。
     */
    val depth: Int = 0,
)

internal fun DaemonSessionSource.State.toUiState(parentOf: Map<String, String> = emptyMap()): SessionOverviewUiState {
    // 归档行必须一起参与血统排布：`/branch`、`/clear` 原地换 sid，父进 `superseded`、子留在 `sessions`。
    // 只喂 `sessions` 的话父不在场，子被当成根，树恒是平的。
    val activeIds = sessions.keys
    val laidOut =
        (
            sessions.values.map { it.toRow() } +
                superseded.values.map { it.toRow().copy(archived = true) } +
                removedUnknown.values.map { it.toRow().copy(removalUnknown = true) }
        ).withLineage(parentOf)
    val rows = laidOut.filter { it.sessionId in activeIds }
    // 两个下线分区按行上的标记切，不按「不在活动表里」切，否则第三档会被一起扫进归档区。
    val supersededRows = laidOut.filter { it.archived && it.sessionId !in activeIds }
    val unknownRows = laidOut.filter { it.removalUnknown && it.sessionId !in activeIds }

    // 缩进只在父就在同一分区里时才画，否则子行缩进而上面没有父。
    // 父子状态通常不同，所以总览面上常看不到树；树状视图属于历史页。
    fun List<SessionRow>.indentOnlyWithinThisSection(): List<SessionRow> {
        val here = mapTo(HashSet()) { it.sessionId }
        return map { if (parentOf[it.sessionId] in here) it else it.copy(depth = 0) }
    }
    return SessionOverviewUiState(
        needsYou = rows.filter { it.light == SessionLight.WaitingInput }.indentOnlyWithinThisSection(),
        running = rows.filter { it.light == SessionLight.Working }.indentOnlyWithinThisSection(),
        // 「其余」用减法而不是列举：`SessionLight` 加一个值时，列举会让一批会话凭空消失。
        others =
            rows
                .filter { it.light != SessionLight.WaitingInput && it.light != SessionLight.Working }
                .indentOnlyWithinThisSection(),
        superseded = supersededRows.indentOnlyWithinThisSection(),
        removedUnknown = unknownRows.indentOnlyWithinThisSection(),
        // 文案在 app，不在 core。
        degraded = dropped?.let { "少收了 $it 行，这份列表可能不全" },
        droppedLines = dropped ?: 0,
        fatal = fatal?.let { describeFatal(it, cause) },
        loading = false,
    )
}

/** 按 `State.cause` 的类型给 `fatal` 分档。流正常结束（对端退出）不是「连不上」。 */
private fun describeFatal(
    raw: String,
    cause: Throwable?,
): String =
    when {
        cause is ConnectionDeadException -> "还没连上这台服务器"
        // 连上过、拿到过完整列表、对端退出了。
        raw.contains(ENDED_MARKER) -> "服务器上的助手已经退出，这份列表不再更新"
        // 兜底不透传原串：core 的 fatal 里带远端命令行、绝对路径、方法名，`daemon`/`exec`/`SSH` 不许上屏。
        else -> "暂时读不到服务器上的对话列表"
    }

/** core 侧那句「daemon 流已结束（对端退出）」的识别锚。 */
private const val ENDED_MARKER = "已结束"

private fun DaemonSessionSource.Session.toRow(): SessionRow =
    SessionRow(
        sessionId = sessionId,
        title = ClaudeSessionCatalog.readableTitle(parts = null, pidfileName = name, sessionId = sessionId),
        cwd = cwd,
        // 归档行不在这里点灯，状态由 `archived` 表达。
        //
        // 灯只读活动档，不读 `status`、也不拿它兜底：两者不一一对应；`status` 缺席保留旧值而 `activity`
        // 缺席即清空，兜底会把过期的 `status` 重新点亮；也不为不发 `activity` 的对端留回落分支。
        light = lightForActivity(activity),
        attachable = attachable,
        waitingFor = waitingFor,
        livenessUncertain = livenessConfidence == LIVENESS_HEURISTIC,
        path = path,
        agentKind = agentKind,
    )

/** wire 的 `liveness_confidence` 取值之一：靠 mtime 猜的（Codex 路径），不是权威判活。 */
private const val LIVENESS_HEURISTIC = "heuristic"

/**
 * 按 `forkedFrom` 血统重排并标深度。
 *
 * 排序交给 [SessionLineage]（环、父不在场都处理，一条都不丢）；这里只把深度贴回行上，并保持它给的顺序。
 */
private fun List<SessionRow>.withLineage(parentOf: Map<String, String>): List<SessionRow> {
    if (parentOf.isEmpty()) return this // 常态：没有分支 ⇒ 一行不动
    val byId = associateBy { it.sessionId }
    return SessionLineage
        .arrange(map { it.sessionId }, parentOf)
        .mapNotNull { node -> byId[node.sessionId]?.copy(depth = node.depth) }
}
