package com.ccmonitor.mobile.ui.claude

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.catalog.lightForStatus
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.model.JsonlParser
import com.ccmonitor.mobile.core.claude.model.JsonlRecord
import com.ccmonitor.mobile.core.claude.model.MainBranch
import com.ccmonitor.mobile.core.claude.model.RecordClassifier
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.UsageSummary
import com.ccmonitor.mobile.core.claude.model.codexSessionMetaCwd
import com.ccmonitor.mobile.core.claude.model.groupTools
import com.ccmonitor.mobile.core.claude.model.withUuid
import com.ccmonitor.mobile.core.claude.transport.JsonlFrame
import com.ccmonitor.mobile.core.claude.transport.ResumeOffset
import com.ccmonitor.mobile.core.claude.transport.SkeletonRecord
import com.ccmonitor.mobile.core.claude.transport.SkeletonScan
import com.ccmonitor.mobile.core.claude.transport.TailTransport
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.sample
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** 重算触发窗口（ms）。IDLE：一簇行落定后算一次，避免初始灌入 O(n²)；MAX：连续流至少每这么久刷一次。 */
private const val RECOMPUTE_IDLE_MS = 90L
private const val RECOMPUTE_MAX_MS = 800L

/** 每个 path 的 tail 续传缓存条目上限，超出逐出最旧。 */
private const val TAIL_CACHE_CAP = 8

/** 有界首载的窗口大小：最近 N 条主线记录的正文。更早的历史靠上滑翻页或「载完整会话」。 */
private const val WINDOW_RECORDS = 200

// 窗口首载字节封顶：最近 WINDOW_RECORDS 条里有巨型记录时，首载不至于吃几十 MB。只约束首载起点，
// tail 续接与 windowed 语义不受影响。见 [SkeletonScan.windowStartOffset]。
private const val WINDOW_MAX_BYTES = 2_000_000L

/**
 * 每次上滑再往前载多少条主线记录。
 *
 * `LazyColumn` 在 prepend 后按 key 锚回滚动位置，但只在锚点附近一个窗口内成立
 * （`NearestItemsSlidingWindowSize = 30` / `NearestItemsExtraItemCount = 100`）：
 * 单次 prepend 不超过 100 个渲染单元必定锚定成功，130 个以上必定跳。
 *
 * 注意：这里按 record 切，渲染单位是 unit（`uuid#blockIndex`），一条 record 会展开成多个 unit，
 * 所以还要 [MAX_PREPEND_UNITS] 按 unit 数再封一道。
 */
private const val PAGE_RECORDS = 40

/**
 * 单次 prepend 的渲染单元数上限，超过它 `LazyColumn` 的 key 锚定就不保证（见 [PAGE_RECORDS]）。
 * 那两个 Compose 常量是 `internal const`、可能随版本变，所以写成自己的不变量。
 */
internal const val MAX_PREPEND_UNITS = 100

/** 单次翻页的字节封顶：一页里出现巨型 Write 也不至于拉一个大块。 */
private const val PAGE_MAX_BYTES = 2L * 1024 * 1024

/**
 * 单个会话文件的续传状态：records、增量分类状态、已消费字节 offset 跨 tailFlow 重启存活，
 * 切 tab 或断线重连后从 offset 续传（`tail -c +offset`），不重下整份，也能复用分类前缀。
 */
private class TailState {
    val records = ArrayList<JsonlRecord>()
    var classifyState = RecordClassifier.IncrementalState.EMPTY
    var offsetBytes = 0L
    var cwd: String? = null // 会话工作目录，取自 JSONL 的 cwd 字段，供 resume

    // 是否走了骨架＋窗口有界首载：是则 records 已是主线，重算时不再 MainBranch.resolve。
    var windowed = false

    // 下面几格给上滑翻历史用：骨架只在首载扫一次，之后每翻一页只做算术（`SkeletonScan.rangeBefore`）再取一段字节。

    /** 已载正文的起始字节。翻一页就往前移一段；0 = 已经载到文件头。 */
    var windowStart = 0L

    /** 首载时扫到的主线骨架，留着供翻页复用，不重扫。 */
    var mainSkeleton: List<SkeletonRecord> = emptyList()

    /** 主线 uuid 集合，翻页解析时用来滤掉 off-main 记录。 */
    var mainUuids: Set<String> = emptySet()

    /** 已翻到最早（`rangeBefore` 返回空区间）。UI 据此显示「已是最早」而不是无限转圈。 */
    var historyExhausted = false

    /** 待处理的「再往前载一段」请求。与 [forceFull] 同一套约定：只置 flag，由 tail 循环执行。 */
    var pendingOlderPage = false

    // 强制全量（「载完整会话」设）：跳过骨架、从 byte 0 全量首载。设了就一直保持。
    var noSkeleton = false

    // 待处理的「载完整会话」请求。这里只置 flag，reset 推迟到新 tailFlow 起点（那时旧 tail 循环已 cancelAndJoin）；
    // 在别处 reset 会被旧循环紧随的 records.add/offset 更新覆盖，新 flow 见 offset>0 只载尾部，漏掉全史。
    var forceFull = false

    /** 截断/重写检测命中 → 复位为 full re-read（保留 [noSkeleton] 用户选择）。 */
    fun reset() {
        records.clear()
        classifyState = RecordClassifier.IncrementalState.EMPTY
        offsetBytes = 0L
        cwd = null
        windowed = false
        pendingOlderPage = false // 截断 reset 后窗口会重建，陈旧的翻页请求不该作用在新窗口上
        windowStart = 0L
        mainSkeleton = emptyList()
        mainUuids = emptySet()
        historyExhausted = false
    }
}

/** 已连接的 Claude 阅读面状态。 */
sealed interface ClaudeReadState {
    data object Loading : ClaudeReadState

    data object Empty : ClaudeReadState

    data class Error(
        val message: String,
        // 错误是否因底层 SSH 连接死（tail EOF 或 exec 抛 ConnectionDeadException）：true 由 UI 驱动 SSH 重连，
        // false 只重挂 tail（retry）。
        val connectionDead: Boolean = false,
        // 断连时带上断开前最后一屏（数据一直在 tailStates 缓存里）：UI 保留内容并加顶部断开条，
        // 重挂 tail 后即续渲。空表示首载即断，UI 回退居中提示。
        val lastUnits: List<RenderUnit> = emptyList(),
        val lastPath: String? = null,
        val windowed: Boolean = false,
        val usage: UsageSummary? = null,
    ) : ClaudeReadState

    data class Ready(
        val units: List<RenderUnit>,
        val path: String,
        // 是否有界窗口首载：true 表示还有更早历史未载，UI 显「载完整会话」。
        val windowed: Boolean = false,
        // 本会话（或已载窗口）的用量汇总；null 表示没有 usage 记录。
        val usage: UsageSummary? = null,
        // 已翻到最早。UI 据此显示「已是最早」而不是无限转圈。
        val historyExhausted: Boolean = false,
    ) : ClaudeReadState
}

/** 一个可选会话（jsonl）。[label] 是可读标题（会话目录的回退链）。 */
data class JsonlSession(
    val path: String,
    val label: String,
)

/** 当前会话的 resume 目标：[sessionId] 是会话 UUID（jsonl 文件名去后缀），[cwd] 是会话工作目录（取自 JSONL cwd 字段）。 */
data class ResumeTarget(
    val sessionId: String,
    val cwd: String,
)

/**
 * 阅读面的数据层：会话发现，以及 tail → 解析 → 主分支 → 分类的管线。
 *
 * `state` 用 `stateIn(WhileSubscribed(5_000))`：切走阅读 tab 5s 内 tail 续命，回来不重 tail；
 * 超 5s 无订阅才取消上游（execStream 的 awaitClose 杀远端 tail）。
 * channel 由 Composable 传入，每次 exec 重新解析连接，所以跟随重连。VM 不持 Android/Compose 引用，可在 JVM 上测。
 */
@OptIn(FlowPreview::class, ExperimentalCoroutinesApi::class)
class ReadingPaneViewModel(
    private val channel: RemoteCommandChannel,
    /**
     * 同一条连接的一次性执行（带退出码和 stderr），与 [channel] 并列。截断检测（`wc -c`）走它：
     * 「文件读不到」与「文件真是 0 字节」处置相反，只看 stdout 分不出来。
     *
     * 注意：必填、不可空，理由见 `prepareTailState`。
     */
    private val executor: RemoteExecutor,
    cwdArg: String?,
    claudeDirArg: String? = null, // 上游解析好的 agent 配置目录前缀（主机设置优先于 app 默认）；空或 null 用内置默认
    // 钉住任意历史 JSONL（历史页「点击=阅读」传入）：非空则首载即 tail 该 path（窗口首载与「载完整会话」照常），
    // 下拉并入钉住项（label=pinnedLabel）。空或 null 时自动发现最新或活动会话。
    pinnedPathArg: String? = null,
    pinnedLabelArg: String? = null,
    // 本会话的 agent 种类，决定 parser / usage / locator 与排序策略。
    agentKindArg: AgentKind = AgentProfile.DEFAULT.kind,
    // CPU 管线必须单线程：onEach(records.add) 与 map(resolve/classify 遍历 records) 被 debounce 拆到不同协程，
    // 多线程会并发访问 records/classifyState，抛 ConcurrentModificationException。limitedParallelism(1) 把二者串行。
    private val cpu: CoroutineDispatcher = Dispatchers.Default.limitedParallelism(1),
) : ViewModel() {
    private val cwd: String? = cwdArg?.ifBlank { null } // 空串哨兵（Koin params 不传 null）→ null

    /** [channel] 的别名：在 `channelFlow` 里 `channel` 会被 ProducerScope 的同名成员遮蔽。 */
    private val remoteChannel: RemoteCommandChannel get() = channel

    // 按种类取的 SPI 一律问档案（`AgentProfile.of(kind)`），本文件不出现种类字面。locator 须先于 claudeDir 初始化。
    private val agentKind: AgentKind = agentKindArg
    private val profile = AgentProfile.of(agentKind)
    private val parser = profile.recordParser
    private val usageAggregator = profile.usageAggregator
    private val locator = profile.sessionLocator

    // Codex 没有 parentUuid，不能走 MainBranch/骨架链：按文件序排，并补合成 uuid 保 unitKey 唯一。
    private val usesParentUuidChain = profile.usesParentUuidChain

    private val claudeDir: String = locator.resolveAgentDir(claudeDirArg, null) // 按种类解析的 agent 配置目录前缀
    private val pinnedPath: String? = pinnedPathArg?.ifBlank { null } // 空串哨兵 → null
    private val pinnedLabel: String? = pinnedLabelArg?.ifBlank { null }

    /** 钉住项的下拉条目；label 缺时回退 sid 前 8 位（历史页总会传 title，这里是防御）。 */
    private fun pinnedSession(path: String): JsonlSession = JsonlSession(path, pinnedLabel ?: locator.sessionIdOf(path).take(8))

    // 有钉住项时初值就是它，tailFlow 直接 tail 它。
    private val _selectedPath = MutableStateFlow<String?>(pinnedPath) // null 表示自动取最新
    val selectedPath: StateFlow<String?> = _selectedPath.asStateFlow()

    // 有钉住项时首帧下拉先只放它，发现列表异步并入，发现失败也保底这一项。
    private val _sessions = MutableStateFlow(pinnedPath?.let { listOf(pinnedSession(it)) } ?: emptyList())
    val sessions: StateFlow<List<JsonlSession>> = _sessions.asStateFlow()

    // 当前 tail 会话的 resume 目标：cwd 一从 JSONL 解出即置；path 变或未知时为 null。
    private val _resumeTarget = MutableStateFlow<ResumeTarget?>(null)
    val resumeTarget: StateFlow<ResumeTarget?> = _resumeTarget.asStateFlow()

    // 发现列表里活会话的 sid 集（pidfile 判活快照，[refreshSessions] 每轮刷新），供「▶ 续接」守卫：
    // 对活会话再起 `--resume` 会让两个 Claude 进程双写同一 JSONL。与历史页的 `!live` 守卫同义。
    // 探测失败时为空集，守卫尽力而为。
    private val _liveSessionIds = MutableStateFlow<Set<String>>(emptySet())
    val liveSessionIds: StateFlow<Set<String>> = _liveSessionIds.asStateFlow()

    // 每个发现会话的状态灯（由 pidfile status 派生，[refreshSessions] 在每次 tailFlow 起点刷新）。
    // 不在表内的 sid 由调用方按 [SessionLight.Stopped] 处理（没有活 pidfile）。
    private val _sessionLights = MutableStateFlow<Map<String, SessionLight>>(emptyMap())
    val sessionLights: StateFlow<Map<String, SessionLight>> = _sessionLights.asStateFlow()

    // 会话目录：判活与可读标题（无 pidfile 时兜底最近 mtime 列表）。哪种 agent 配哪个实现，问档案的 `newSessionCatalog`。
    private val catalog: SessionCatalog = profile.newSessionCatalog(channel, claudeDir)

    // 标题探针的异步补全；每次 refresh 取消上一个，防叠加。
    private var titleJob: kotlinx.coroutines.Job? = null

    /**
     * 跑会话发现：返回默认 tail 目标并刷新下拉列表。分两段，标题不卡渲染：
     * 先走快路径 [ClaudeSessionCatalog.liveSessionRefs] 定 tail 目标（首个活动会话；没有就取最新 mtime），
     * 用占位名即时填下拉；再异步跑标题探针补真标题。
     * 非钉住模式在每次 tailFlow 起点调；钉住模式由 init 调一次，只为填下拉。两处都并入钉住项。
     */
    private suspend fun refreshSessions(): String? {
        val refs = catalog.liveSessionRefs(cwd)
        // 活会话 sid 集，覆盖式快照（见 _liveSessionIds）。
        _liveSessionIds.value = refs.filter { it.live }.mapTo(HashSet()) { it.sessionId }
        // 每个 sid 的状态灯快照。
        _sessionLights.value = refs.associate { it.sessionId to lightForStatus(it.status, it.live) }
        // 占位可读名（pidfile name ?? sid8）即刻上屏；tail 目标随即返回，不等标题探针。
        _sessions.value = mergePinned(refs.map { JsonlSession(it.path, it.provisionalTitle) })
        titleJob?.cancel()
        titleJob =
            viewModelScope.launch {
                val labeled =
                    try {
                        catalog.labeledSessions(refs)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Exception) {
                        return@launch // 标题探针失败 → 保留占位名（不覆盖）
                    }
                _sessions.value = mergePinned(labeled.map { JsonlSession(it.path, it.title) })
            }
        return refs.firstOrNull()?.path
    }

    /** 把钉住项并入发现列表，钉住项在前；发现列表已含该 path 就原样返回（那边的标题来自探针，更准）。 */
    private fun mergePinned(discovered: List<JsonlSession>): List<JsonlSession> {
        val p = pinnedPath ?: return discovered
        if (discovered.any { it.path == p }) return discovered
        return listOf(pinnedSession(p)) + discovered
    }

    init {
        // 钉住模式：tail 目标已定，这里跑一次发现只为填下拉（仍可切到活动或最近会话）；发现失败就只留钉住项。
        if (pinnedPath != null) {
            viewModelScope.launch {
                try {
                    refreshSessions()
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // 列表命令失败 → 保底 pinned 单项（初值已就位），阅读不受影响。
                }
            }
        }
    }

    // 断线自愈：retry() 自增 tick，combine 让 flatMapLatest 以同一 path 重跑 tailFlow，从缓存 offset 续传。
    private val retryTicks = MutableStateFlow(0)

    val state: StateFlow<ClaudeReadState> =
        combine(_selectedPath, retryTicks) { sel, _ -> sel }
            .flatMapLatest { sel -> tailFlow(sel) } // selectedPath 变 / retry → 取消旧 tail、起新 tail
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ClaudeReadState.Loading)

    // 最近一次成功渲染的 [ClaudeReadState.Ready]（[recomputeAndSend] 写）。断连 Error 据此带上 lastUnits，
    // 重订阅时据此跳过 Loading，连接回来不闪空屏。写在 cpu 单 worker、读在 flow 上下文，只需可见性：
    // 读到旧值至多多显一帧旧内容。
    @Volatile
    private var lastReady: ClaudeReadState.Ready? = null

    /** 构造「断连但保留断开前内容」的错误态，内容取自 [lastReady]；没有就为空，UI 回退居中提示。 */
    private fun connectionLostError(message: String): ClaudeReadState.Error {
        val lr = lastReady
        return ClaudeReadState.Error(
            message,
            connectionDead = true,
            lastUnits = lr?.units ?: emptyList(),
            lastPath = lr?.path,
            windowed = lr?.windowed ?: false,
            usage = lr?.usage,
        )
    }

    fun selectSession(path: String) {
        // 切到别的会话就清掉最后一屏，否则新会话首载或断连时会显示上一个会话的内容。重选同一会话不清。
        if (path != lastReady?.path) lastReady = null
        _selectedPath.value = path
    }

    /** 一键重连：重挂 tail，复用 TailState 缓存从 offset 续传。 */
    fun retry() {
        retryTicks.value += 1
    }

    /** 「载完整会话」：放弃有界窗口，从 byte 0 全量载 [path]。只置 forceFull，reset 推迟到新 tailFlow 起点。 */
    fun loadFullSession(path: String) {
        viewModelScope.launch {
            // 不在此 reset：旧 tailFlow 尚未取消，它的 tail 循环会紧接着重填 offset。
            // reset 在新 tailFlow 起点做（见 prepareTailState）。
            withContext(cpu) { stateFor(path).forceFull = true }
            // 钉住该会话：直接指定 path，免得 retry 在 selectedPath=null 下重新发现 latest 载错会话。
            _selectedPath.value = path
            retryTicks.value += 1 // 强制重触发 tailFlow（即便 path 未变）
        }
    }

    /**
     * 上滑增量翻历史：翻多远下多远，不像 [loadFullSession] 那样全有或全无。大会话可达数百 MB，全量重下不可用。
     *
     * 与 [loadFullSession] 同一套约定：只置 flag，取字节由 tail 循环用它自己的 channel 做，
     * 不在别处直接拿 channel 与 tail 循环抢同一条连接。
     */
    fun loadOlderHistory(path: String) {
        viewModelScope.launch {
            withContext(cpu) {
                val st = stateFor(path)
                if (st.windowed && !st.historyExhausted) st.pendingOlderPage = true
            }
            // 必须钉住该会话，理由同 [loadFullSession]：自动发现模式下 `selected == null`，新 flow 会重新发现 latest，
            // 期间别的会话变活或 mtime 变头就会静默切过去，本会话的 `pendingOlderPage` 悬着无人消费。
            // 翻页触发得更频繁，更容易撞上。
            _selectedPath.value = path
            retryTicks.value += 1 // 触发 tail 循环去执行
        }
    }

    // 每个 path 的续传状态（本 VM 存活期内跨 tab 和重连复用）。LinkedHashMap 的插入序做 LRU。只在 cpu 单 worker 访问。
    private val tailStates = LinkedHashMap<String, TailState>()

    /** 取/建 [path] 的 [TailState] 并标记最近使用；超 [TAIL_CACHE_CAP] 逐出最旧。仅在 cpu 单 worker 调（无竞态）。 */
    private fun stateFor(path: String): TailState {
        val st = tailStates.remove(path) ?: TailState()
        tailStates[path] = st // 重插到末尾 = 最近使用
        while (tailStates.size > TAIL_CACHE_CAP) {
            tailStates.remove(tailStates.keys.iterator().next()) // 逐出最旧
        }
        return st
    }

    /**
     * 执行一次待处理的翻页请求。取消原样上抛；其他失败保留 flag 待重试，不静默（否则上滑后什么都没发生也没有提示）。
     */
    private suspend fun runOlderPageIfPending(
        srcChannel: RemoteCommandChannel,
        path: String,
        st: TailState,
    ) = withContext(cpu) {
        runCatching { loadOlderPage(srcChannel, path, st) }
            .onFailure {
                if (it is CancellationException) throw it
                st.pendingOlderPage = true
            }
    }

    /**
     * 取/建 [path] 的 [TailState] 并做首载准备，全在 cpu 单 worker 上，与上一个 tailFlow 的收尾串行：
     * 消费 forceFull（此时旧 flow 已 join，可以安全 reset）→ 截断检测（续传前 `wc -c` 看文件是否变短）→
     * 骨架窗口有界首载（失败或非 fresh 时落回全量；失败清掉半填充；取消重抛）。
     */
    private suspend fun prepareTailState(
        srcChannel: RemoteCommandChannel,
        path: String,
    ): TailState =
        withContext(cpu) {
            val s = stateFor(path)
            if (s.forceFull) {
                s.reset()
                s.noSkeleton = true // 跳骨架、从 0 全量
                s.forceFull = false
            }
            if (s.offsetBytes > 0) {
                // 当前大小小于 offset 说明文件被截断或重写（/compact），复位从头读。
                // wc 失败（null）时不复位、沿用旧 offset：最坏注入几行垃圾，解析为 Unknown 被滤掉。
                // 注意：[executor] 必填。写成可空、「没给就跳过」的话，调用方漏传时截断检测会永久静默失效而测试全绿，
                // 所以缺了就在构造期报错。
                val size = ResumeOffset.remoteSize(executor, path)
                if (size != null && size < s.offsetBytes) s.reset()
            }
            // 重挂之前清掉上次断网留下的那条 tail。放在 offset 判断之外：僵尸是上次那条没死透，与这次从哪续无关；
            // 写进那个 if 里，首载（offset==0，恰是重连后最常见的情形）就永远不清。
            // 注意：这里不要再套 `withContext(Dispatchers.IO)`，`execCapture` 内部已切 IO，多套一层会让测试的虚拟时间失效。
            ResumeOffset.killStaleTail(executor, path)
            // 骨架窗口首载靠 parentUuid 链，只有 Claude 走；Codex 跳过（st 仍 fresh，从 0 全量 tail）。
            // 失败或取消时清掉半填充的 records，免得下次 tail 从 0 重下叠加；取消必须重抛。
            if (usesParentUuidChain) {
                runCatching { skeletonWindowLoad(srcChannel, path, s) }
                    .onFailure {
                        if (!s.windowed) s.reset()
                        if (it is CancellationException) throw it
                    }
            }
            s
        }

    /**
     * 给解析后的记录定渲染 id。Codex 记录多半没有 uuid，按文件序补合成 uuid（`cx-<index>`，append-only 下稳定），
     * 保 unitKey 唯一；否则多条 null uuid 撞 `"_#0"`，LazyColumn 的 key 重复会崩。Claude 恒有 uuid，不动。
     */
    private fun withRenderId(
        rec: JsonlRecord,
        index: Int,
    ): JsonlRecord = if (!usesParentUuidChain && rec.uuid == null) rec.withUuid("cx-$index") else rec

    /** 从记录取会话 cwd（供 resume）。Claude 取 User.cwd；Codex 取 session_meta.cwd（Codex 的 User 没有 cwd）。 */
    private fun cwdFromRecord(rec: JsonlRecord): String? =
        if (usesParentUuidChain) (rec as? JsonlRecord.User)?.cwd?.takeIf { it.isNotBlank() } else codexSessionMetaCwd(rec)

    private fun tailFlow(selected: String?): Flow<ClaudeReadState> =
        channelFlow {
            // channelFlow 的 ProducerScope 也暴露一个名为 `channel` 的成员（SendChannel）→ 会遮蔽 VM 的
            // RemoteCommandChannel 属性；用 srcChannel 别名指向后者（经 [remoteChannel]，不在这里写带标签的 this），避免歧义。
            val srcChannel = remoteChannel
            // 只有首载（没有历史内容）才发 Loading；重订阅或重连时有 lastReady，跳过 Loading，保留断开前内容不闪空屏。
            if (lastReady == null) send(ClaudeReadState.Loading)
            // 非钉住模式每次起点都刷新灯和活会话集，包括手选了会话的情形；否则手选后灯与守卫冻结，
            // 会放行对新变活会话的 --resume 双写。不加定时器，只随 tailFlow 重启（retry / 切 tab / 重订阅）刷；
            // selected!=null 时返回值只用于刷新、不改 tail 目标。钉住模式读的是历史会话，不重新发现。
            val discovered = if (pinnedPath == null) refreshSessions() else null
            val path = selected ?: discovered // selectedPath==null 时取活动会话，没有就取最新 mtime
            if (path.isNullOrBlank()) {
                send(ClaudeReadState.Empty)
                return@channelFlow
            }
            // 每个 path 的续传状态跨 tailFlow 重启存活。取/建与截断检测都在 cpu 单 worker 上，与上一次 tailFlow 的收尾串行。
            val st = prepareTailState(srcChannel, path)
            // 有待处理的翻页请求就先补一段更早的（在 tail 循环之前，与首载同位置、同 channel）
            if (st.pendingOlderPage) runOlderPageIfPending(srcChannel, path, st)
            // resume 目标：sessionId 由文件名算出；cwd 已缓存就立即可 resume，否则等记录解出。
            // `_resumeTarget` 的写统一放 cpu 单 worker，与记录循环里的写全序串行。
            val sessionId = locator.sessionIdOf(path) // 按种类：Claude 取文件名 stem，Codex 取末尾 UUID
            withContext(cpu) { _resumeTarget.value = st.cwd?.let { ResumeTarget(sessionId, it) } }
            var lastCount = -1
            val trigger = MutableSharedFlow<Unit>(extraBufferCapacity = 256, onBufferOverflow = BufferOverflow.DROP_OLDEST)

            suspend fun recomputeAndSend() {
                val n = st.records.size
                if (n == lastCount) return // 记录数没变 → 跳过（去 merge 双触发的重复）
                lastCount = n
                st.classifyState =
                    RecordClassifier.classifyIncremental(
                        st.classifyState,
                        resolvedRecords(st, usesParentUuidChain),
                        // 窗口首载必然把一部分 tool_use 切在窗口外，对应的 tool_result 成了孤儿；
                        // 只有会翻页的会话才挂起它们（见 classifyIncremental）。
                        collectOrphans = st.windowed,
                    )
                // 连续工具分组是视图变换，作用在分类输出上，不碰 classifyState 缓存，增量复用不受影响。
                // 用量从全部累积记录聚合（Claude 按 requestId 取 MAX；Codex 累计 token_count）。
                val ready =
                    ClaudeReadState.Ready(
                        groupTools(st.classifyState.units),
                        path,
                        // 翻到最早之后就不再是「有界窗口」了 —— 否则 UI 继续显示「↑ 载完整会话」，
                        // 点了会从 byte 0 重下一遍已经全在内存里的东西
                        st.windowed && !st.historyExhausted,
                        usageAggregator.aggregate(st.records),
                        historyExhausted = st.historyExhausted,
                    )
                lastReady = ready // 记住最后一屏，供断连保留内容与重订阅跳过 Loading
                send(ready)
            }

            // 有缓存记录就先渲染一次，切 tab 回来立即显示旧内容再续传。
            if (st.records.isNotEmpty()) withContext(cpu) { recomputeAndSend() }

            // 重算消费者：debounce 在一簇行落定后算一次，避免初始灌入 O(n²)；sample 让连续流也至少每 MAX 刷一次。
            // 二者 merge，recomputeAndSend 内按记录数去重。
            val recompute =
                launch(cpu) {
                    merge(trigger.debounce(RECOMPUTE_IDLE_MS), trigger.sample(RECOMPUTE_MAX_MS))
                        .collect { recomputeAndSend() }
                }
            val transport = TailTransport(srcChannel, path, st.offsetBytes)
            try {
                withContext(cpu) {
                    transport
                        .frames()
                        .filterIsInstance<JsonlFrame.Line>()
                        .collect { frame ->
                            val rec = withRenderId(parser.parse(frame.raw), st.records.size) // 解析，Codex 补合成 uuid
                            st.records.add(rec) // 逐行累积（cpu 单 worker，不丢）
                            st.offsetBytes = transport.currentOffset // 逐行推进 resume 点，与 records.add 原子（同 worker）
                            if (st.cwd == null) {
                                // 首个带 cwd 的记录定会话工作目录，供 resume。
                                cwdFromRecord(rec)?.let {
                                    st.cwd = it
                                    _resumeTarget.value = ResumeTarget(sessionId, it)
                                }
                            }
                            trigger.tryEmit(Unit)
                        }
                }
            } finally {
                recompute.cancelAndJoin()
                withContext(cpu) { recomputeAndSend() } // 最终 flush：断线前最后一批也渲染（debounce 可能没来得及）
            }
            // tail -F 是无限流，正常返回就是连接断了（execStream EOF）：标 connectionDead，应驱动 SSH 重连而不是 retry；
            // 带上断开前最后一屏，UI 保留内容加断开条，重连后自动续渲。
            send(connectionLostError("连接已断开"))
        }.catch { e ->
            if (e is CancellationException) throw e
            emit(catchStateFor(e)) // 分支收进 [catchStateFor]，压 tailFlow 复杂度
        }

    /**
     * tailFlow `.catch` 的错误态映射：ConnectionDeadException（连接不存在或未连上）→ connectionDead=true，并保留断开前内容；
     * 其他传输或解析错误 → connectionDead=false（可 retry），不保留内容（不是连接问题，旧内容可能已失真）。
     */
    private fun catchStateFor(e: Throwable): ClaudeReadState =
        if (e is ConnectionDeadException) {
            connectionLostError(e.message ?: "连接已断开")
        } else {
            ClaudeReadState.Error(e.message ?: e.toString(), connectionDead = false)
        }
}

/**
 * 给分类用的已解析主线记录。窗口模式下 [TailState.records] 已是主线（按骨架的 uuid 过滤），
 * 不再 MainBranch.resolve：父记录在窗口外会被误判为新根。全量模式照常 resolve。
 *
 * 窗口模式也按 uuid 保首见去重并滤掉无 uuid 的记录，与 [MainBranch.resolve] 的输出语义一致：
 * tail -F 检测到截断（/compact 原地重写）会从头重放同 uuid 的记录，骨架缺 END 行时也会重读末行；
 * 重复记录产出重复 `unitKey`，LazyColumn `items(key)` 会抛 IllegalArgumentException 直接崩。
 * 保首见对 append-only 输入仍前缀稳定，[RecordClassifier.classifyIncremental] 的增量复用不破。
 */
private fun resolvedRecords(
    st: TailState,
    canMainBranch: Boolean,
): List<JsonlRecord> =
    if (st.windowed || !canMainBranch) {
        // 窗口模式 / Codex（无 parentUuid）：文件序 + 按 uuid 保首见去重（Codex 记录已在累积时补合成 uuid，故不被滤掉）。
        val seen = HashSet<String>(st.records.size * 2)
        st.records.filter { r ->
            val id = r.uuid ?: return@filter false
            seen.add(id)
        }
    } else {
        MainBranch.resolve(st.records)
    }

/**
 * 把这一页裁到 [MAX_PREPEND_UNITS] 个渲染单元以内，保留靠近当前窗口的那一截（列表尾部）。
 * 只按 record 数封顶不够：一条 record 会展开成多个 unit，而 `LazyColumn` 的锚定窗口按 unit 算（见 [PAGE_RECORDS]）。
 */
internal fun trimToPrependBudget(older: List<JsonlRecord>): List<JsonlRecord> {
    if (RecordClassifier.classify(older).size <= MAX_PREPEND_UNITS) return older
    var lo = 1
    var hi = older.size
    // 二分找「最多能保留多少条尾部记录而不超预算」
    while (lo < hi) {
        val mid = (lo + hi + 1) / 2
        if (RecordClassifier.classify(older.takeLast(mid)).size <= MAX_PREPEND_UNITS) lo = mid else hi = mid - 1
    }
    return older.takeLast(lo)
}

/**
 * 再往前载一段。
 *
 * [skeletonWindowLoad] 回答「首载从哪到 EOF」，这里回答「再往前一段是哪一段」。两者用同一份骨架
 * （[TailState.mainSkeleton] 首载时留下），翻页不重扫，比 [ReadingPaneViewModel.loadFullSession] 省的就是这个。
 *
 * 1. 只取被请求的字节区间（`rangeContentCommand` = `tail -c +N | head -c LEN`），不读到 EOF。
 * 2. 往前插：新记录接在 [TailState.records] 前面，用 [RecordClassifier.classifyPrepend] 增量合并
 *    （边界去重、索引平移、孤儿 tool_result 回填）。
 * 3. 到顶要说出来：空区间置 [TailState.historyExhausted]，UI 显示「已是最早」。
 */
private suspend fun loadOlderPage(
    channel: RemoteCommandChannel,
    path: String,
    st: TailState,
) {
    if (!st.windowed || st.historyExhausted) {
        st.pendingOlderPage = false
        return
    }

    val range = SkeletonScan.rangeBefore(st.mainSkeleton, st.windowStart, PAGE_RECORDS, PAGE_MAX_BYTES)
    if (range.isEmpty) {
        st.historyExhausted = true
        st.pendingOlderPage = false
        return
    }

    // 只取这一段字节，不读到 EOF
    val buf = java.io.ByteArrayOutputStream()
    channel.exec(SkeletonScan.rangeContentCommand(path, range)).collect { buf.write(it) }

    val older = parseOlderRecords(buf.toString("UTF-8"), st)

    // 注意：取完字节才清 flag。取字节途中被取消（切 tab 超时、retry、SSH 掉线）时 flag 还在，下次会重试；
    // 提前清掉的话请求就静默丢了，而 UI 侧的 key 都没变，不会重触发。
    st.pendingOlderPage = false
    if (older.isEmpty()) {
        st.windowStart = range.start
        if (range.start <= 0L) st.historyExhausted = true
        return
    }

    // 按渲染单元数封顶（见 [MAX_PREPEND_UNITS]）：超了只留靠近当前窗口的那一截，其余留给下一页。
    val kept = trimToPrependBudget(older)
    val newStart =
        if (kept.size == older.size) {
            range.start
        } else {
            // 只吃了一部分 ⇒ 窗口起点停在被保留的第一条上，剩下的下次再翻
            kept.firstOrNull()?.uuid?.let { u -> st.mainSkeleton.firstOrNull { it.uuid == u }?.byteOffset } ?: range.start
        }
    st.windowStart = newStart
    if (newStart <= 0L) st.historyExhausted = true

    st.records.addAll(0, kept)
    // 增量向前插，不整份重算；大会话上整份重算每翻一页都要重跑全部记录
    st.classifyState = RecordClassifier.classifyPrepend(st.classifyState, kept)
}

/**
 * 从翻页取回的字节里解出主线上、且还没载过的记录。
 * 去重按 uuid 而不是按位置：prepend 之后位置全变；按行边界取的区间在接缝处必然多带几条已载过的。
 */
private fun parseOlderRecords(
    text: String,
    st: TailState,
): List<JsonlRecord> {
    val known = HashSet<String>(st.records.size * 2).apply { st.records.forEach { r -> r.uuid?.let { add(it) } } }
    val older = ArrayList<JsonlRecord>()
    val emitted = HashSet<String>()
    text.lineSequence().forEach { line ->
        if (line.isBlank()) return@forEach
        val rec = JsonlParser.parse(line)
        val u = rec.uuid ?: return@forEach
        if (u in st.mainUuids && u !in known && emitted.add(u)) older.add(rec)
    }
    return older
}

/**
 * 骨架＋窗口有界首载。远端 awk 抽小字段骨架（几 KB）→ 算主线 → 只载最近 [WINDOW_RECORDS] 条主线正文的字节范围。
 * 成功：填 [TailState.records]，offset 置为骨架时刻的 EOF，tail 从那里接手新行。
 * 失败（awk 不可用或骨架为空）：直接返回，st 仍 fresh，调用方从 0 全量 tail。
 * 只在 fresh st 上、cpu 单 worker 调（早于 tail 循环，改 st.records 无竞态）。
 */
private suspend fun skeletonWindowLoad(
    channel: RemoteCommandChannel,
    path: String,
    st: TailState,
) {
    if (st.records.isNotEmpty() || st.offsetBytes != 0L || st.noSkeleton) return // 非 fresh/用户选了全量 → 跳过（沿用缓存/全量）
    // 1. 骨架扫描（缓冲全部输出——小，几 KB）
    val skelOut = StringBuilder()
    channel.exec(SkeletonScan.skeletonCommand(path)).collect { skelOut.append(String(it, Charsets.UTF_8)) }
    val res = SkeletonScan.parse(skelOut.toString())
    if (res.records.isEmpty()) return // awk 不可用/空 → fallback 全量
    val mainSkel = SkeletonScan.mainBranchSkeleton(res.records)
    if (mainSkel.isEmpty()) return
    val start = SkeletonScan.windowStartOffset(mainSkel, WINDOW_RECORDS, res.eofBytes, WINDOW_MAX_BYTES)
    val mainUuids = HashSet<String>(mainSkel.size * 2).apply { mainSkel.forEach { add(it.uuid) } }

    // 2. 窗口正文 [start, EOF]——缓冲全部字节再一次性解码（防多字节 UTF-8 跨 chunk 截断）。窗口有界（最近 N 条）。
    val buf = java.io.ByteArrayOutputStream()
    channel.exec(SkeletonScan.windowContentCommand(path, start)).collect { buf.write(it) }
    val emitted = HashSet<String>()
    buf.toString("UTF-8").lineSequence().forEach { line ->
        if (line.isBlank()) return@forEach
        val rec = JsonlParser.parse(line)
        val u = rec.uuid
        if (u != null && u in mainUuids && emitted.add(u)) st.records.add(rec) // 只留主线记录（范围内含 off-main 过滤掉）
    }
    st.offsetBytes = res.eofBytes // tail 从骨架时刻的 EOF 续接（不用窗口读时 EOF，避免扫描↔读之间新行丢失）
    // 留下翻页要用的三样，骨架不再重扫
    st.windowStart = start
    st.mainSkeleton = mainSkel
    st.mainUuids = mainUuids
    // start>0 才是真有界（还有更早历史，UI 显「载完整」）；start==0 时窗口已是全量，windowed=false，不显多余按钮、走 resolve。
    st.windowed = start > 0L
    // resume 用的 cwd 从窗口内（最近的）记录取。
    st.cwd = st.records.firstNotNullOfOrNull { r -> (r as? JsonlRecord.User)?.cwd?.takeIf { it.isNotBlank() } } ?: st.cwd
}
