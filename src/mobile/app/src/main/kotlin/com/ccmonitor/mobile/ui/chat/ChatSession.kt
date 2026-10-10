package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.ChatTurnAssembler
import com.ccmonitor.mobile.core.claude.bridge.CommandCatalog
import com.ccmonitor.mobile.core.claude.bridge.FrameRoute
import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.TurnState
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.transport.BlockedSignal
import com.ccmonitor.mobile.core.claude.transport.RateLimitReading
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.claude.transport.WaitingCopy
import com.ccmonitor.mobile.core.claude.util.SmoothRelease
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.yield

/**
 * 「让远端停下」走到了哪一步。
 *
 * 与 [ChatUiState.stoppedByUser]（本地不再显示）是两件事，不能合成一个布尔：
 *
 * | | 按了停止 | 中断投出去了 | 远端确认停了 |
 * |---|---|---|---|
 * | [None] | 否 | — | — |
 * | [Requested] | 是 | 是 | 还没 ⇒ 只能说「正在让远端停下」 |
 * | [Confirmed] | 是 | 是 | 是（`res.terminal_reason == aborted_streaming`） |
 * | [Failed] | 是 | 否 | — ⇒ 退回本地停显示，说明远端可能还在跑 |
 *
 * 注意：[Requested] 与 [Confirmed] 不能合并，否则按钮一按就说「已经停下」，是假报。
 */
sealed interface RemoteStop {
    /** 没在停。 */
    data object None : RemoteStop

    /** 中断已投出，还没等到远端的确认帧。 */
    data object Requested : RemoteStop

    /** 远端确认这一轮停了。判据是下行 `res` 的 `terminal_reason == "aborted_streaming"`，只能来自远端。 */
    data object Confirmed : RemoteStop

    /** 中断没投出去。[why] 是人可读的原因（例如「这条路停不了」）。 */
    data class Failed(
        val why: String,
    ) : RemoteStop

    companion object {
        /** 远端说「这一轮被打断」时 `res.terminal_reason` 的机器码。判据和人话映射共用它。 */
        const val ABORTED_STREAMING = "aborted_streaming"

        /**
         * [ABORTED_STREAMING] 上屏时的人话；机器码本身不上屏。
         * 优先级低于远端自己给的 `humanText`。
         */
        const val ABORTED_HUMAN = "这一轮被打断了"
    }
}

/** 聊天面的一帧 UI 状态。 */
data class ChatUiState(
    val units: List<RenderUnit> = emptyList(),
    val streaming: Boolean = false,
    /**
     * 协议层的轮次状态，只由 `res` 帧决定。
     *
     * 与 [stoppedByUser] 是两个事实：按了停止但对端没发过 `res`，`turn` 就该停在 `Streaming`。
     * 屏上「远端那一轮可能还在跑」就靠这个区分。没有 UI 直接读它，但不能降为私有。
     */
    val turn: TurnState = TurnState.Idle,
    /** 本地停止：按了「停止」而且只能停到本地为止。 */
    val stoppedByUser: Boolean = false,
    /**
     * 让远端停下走到哪一步，见 [RemoteStop]。
     *
     * 接了上行时按停止不停本地显示，要继续收，否则远端那条确认帧到不了屏幕。
     */
    val remoteStop: RemoteStop = RemoteStop.None,
    /** 失败原因，null 表示没失败。判据是 `!ok`，不是 `why` 有没有值。 */
    val failedWhy: String? = null,
    /**
     * 这个会话接了翻历史吗。
     *
     * 必须与 [canLoadOlder] 分开：「没有这个功能」与「有但已经到顶」是两回事，
     * 否则没接翻页的调用方顶部会常驻一条假的「已是最早」。
     */
    val historyPagingAttached: Boolean = false,
    /** 还有更早的对话可载吗（仅在 [historyPagingAttached] 时有意义）。 */
    val canLoadOlder: Boolean = false,
    /** 正在载更早的一段（防重入，也给界面一个「在动」的信号）。 */
    val loadingOlder: Boolean = false,
    /** 翻历史失败的原因。不能静默，也不能因此崩掉页面。 */
    val historyError: String? = null,
    /**
     * 下行断了的原因（null = 没断）。不能静默、不能崩页面。
     *
     * 断网或会话被杀时 `ConnectionDeadException` 会从下行协程里抛出，这里接住它。
     */
    val downlinkError: String? = null,
    /** 这个会话接了上行吗。false = 只回显不真发。 */
    val uplinkAttached: Boolean = false,
    /**
     * 现在能不能重新接上下行（= 当前没在收）。
     *
     * 界面据此给「重新接上」的入口；没有它，「内容接收已结束」或「已停止显示」之后只能退出重进。
     */
    val canReattach: Boolean = false,
    /**
     * 当前权限模式（如 `"default"`），来自 `init.raw.permissionMode`。null = 远端没报。
     *
     * 协议不提供「有哪些可选模式」，所以只显示当前值、不做选择器；硬编码列表会在远端加新模式时谎报。
     */
    val permissionMode: String? = null,
    /** `init` 帧的 catalog 整理成的分区视图（命令/skill/agent/MCP/工具）。没收到 `init` 时为空，不弹空面板。 */
    val catalog: CommandCatalog = CommandCatalog(),
    /**
     * 有消息正在飞。与 [streaming]（轮次状态）是两件事：
     * 只看后者的话，`res` 到达之后再发消息就没有「停止」键了。
     */
    val sending: Boolean = false,
    /**
     * 当前流式块已经露出了多少个字符。
     *
     * 逐字上屏与一次性刷出最终文本相同，只有这个读数序列能区分两者（长度 ≥ 3 且严格递增）。
     * 它量的是状态，不是屏幕。不放进 `contentDescription`：TalkBack 会每拍念一次数字。
     */
    val revealedChars: Int = 0,
    /** Claude 在等人回应。null = 没有等待态（含已作废、已过期）。来自 [SessionSignals]，见 [attachSignals]。 */
    val waiting: WaitingNotice? = null,
    /**
     * 配额。null = 这条对话还没收到过 `rate_limit_event`。
     *
     * 注意：[RateLimitReading.windows] 为空时不许在屏上编百分比，有的远端版本根本不发 `unifiedWindows`。
     */
    val quota: RateLimitReading? = null,
    /**
     * 「它想做的一件事被挡住了」（下行 `system/permission_denied`），不是等待态。
     * 这条帧未在真实会话里复现过，帧不来就什么都不显示。
     */
    val blocked: BlockedSignal? = null,
)

/**
 * 等待态在聊天屏上要说的话。
 *
 * 三件套：需手动条（[headline] + [detail]）、输入框禁用（[blocksSending]）、
 * 输入框上方那句（[WaitingCopy.SENDING_NOW_ANSWERS_THE_QUESTION]，与模态拒绝路径共用常量）。
 *
 * @param reason 拦或不拦的理由：哪条帧、什么时刻、什么值。供判据读，也是排查时唯一有用的东西。
 */
data class WaitingNotice(
    val headline: String,
    val detail: String,
    val blocksSending: Boolean,
    val reason: String,
)

/**
 * 一条对话的聊天状态，数据源是一条 `Flow<BridgeFrame>`。
 *
 * 缓释：[SmoothRelease] 不另存正文，只回答「已到达的正文此刻该露出多少」；
 * 正文只在 [ChatTurnAssembler] 里，保证正文只存一份（`at` 全文覆盖时两份必然打架）。
 * 帧的路由一律问 [ChatTurnAssembler.route]，这里不再判一遍：
 * 否则子 agent 的 `at` 会被拿去覆盖主流缓冲，已露出的字当场消失。
 *
 * 它不是 `ViewModel`：下行不能随屏幕生死。持有者是应用级的 [ChatController]，
 * 离开聊天屏后照样收，`res` 到了才算这一轮完；后台完成推送和前台服务都靠这一点。
 * 代价是不会自己消失，释放点见 [ChatController.release]。
 *
 * @param uplink 上行出口。null = 只回显不真发。
 */
class ChatSession(
    private val uplink: UplinkSink? = null,
    /** 本对话的协程作用域，由 [ChatController] 给，随应用而不是随屏幕。测试可注入自己的 scope。 */
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + kotlinx.coroutines.Dispatchers.Main.immediate),
    /** 墙钟。做成参数是为了让判据把时钟推过等待态的新鲜度阈值，不用 `Thread.sleep`。 */
    private val clock: () -> Long = System::currentTimeMillis,
) {
    /** 已经排了一次发布还没发出去（见 [schedulePublish]）。 */
    private var publishScheduled = false

    /**
     * 整份渲染列表被重建了多少次。
     *
     * 不能数收集器收到几次：`StateFlow` 会合并，那样量的是观察者，不是干的活。
     */
    @Volatile
    internal var publishCount = 0
        private set

    private val assembler = ChatTurnAssembler()
    private val release = SmoothRelease(chunkChars = CHAT_CHUNK, intervalMs = CHAT_INTERVAL_MS)

    private val _state = MutableStateFlow(ChatUiState())
    val state: StateFlow<ChatUiState> = _state.asStateFlow()

    /** 正在流式的那个块的 key。只有它需要缓释，已完成的块直接全量显示。 */
    private var streamingKey: String? = null

    private var feedJob: Job? = null
    private var tickJob: Job? = null

    /**
     * 此刻还在收下行吗。显式记，不从 `Job.isActive` 推：
     * 协程跑到自己的 `finally` 时 `isActive` 仍是 true，那次 `publish()` 会算出「还在收」，
     * 「重新接上」的入口就恰好在最需要时不出现。
     */
    private var feeding = false

    /** 本地停止标记。没有它，`finally` 里的 `publish()` 会把 `streaming` 翻回 true。 */
    private var stoppedByUser = false

    /** 远端那一轮的停止进度。见 [RemoteStop]。 */
    private var remoteStop: RemoteStop = RemoteStop.None

    // ---- 跨通路信号（SessionSignals）--------------------------------------------

    /** 接上的信号汇。null = 没接，本类行为不受影响。 */
    private var signals: SessionSignals? = null

    /** 这条对话在信号汇里的 sid。与 [signals] 同生共死。 */
    private var signalSessionId: String? = null

    /** 盯着信号汇的协程。见 [attachSignals]。 */
    private var signalJob: Job? = null

    /**
     * 按过「我知道，还是发」。
     *
     * 拦是「默认不发」，不是「不能发」。一旦立起就不再自动落下：等待态的读数可能是纯告知框、
     * 也可能是停更的状态文件，自动落回会把人重新关进刚走出来的死胡同。
     */
    private var waitingOverridden = false

    /** 最近一条配额读数。 */
    private var quota: RateLimitReading? = null

    /** 最近一条「它被挡住了」（下行 `system/permission_denied`）。 */
    private var blocked: BlockedSignal? = null

    /**
     * 把这条对话接上跨通路信号汇。不接就没有，行为与不接时逐字相同。
     *
     * 等待态不是聊天这条通路产的：聊天屏起的 claude 的 pidfile 里没有 `status` / `waitingFor` /
     * `statusUpdatedAt`，`status == "waiting"` 在本通路上不会出现。唯一产方是总览通路，本类只消费。
     * 用进程级的信号汇而不是直接注入总览的数据源：避免点对点接线，也避免手机上多一条常连。
     */
    fun attachSignals(
        bus: SessionSignals,
        sessionId: String,
    ) {
        signalJob?.cancel()
        signals = bus
        signalSessionId = sessionId
        // 信号变了要重算 UI 态；只存引用不收集的话，等待态只会在碰巧有别的 publish 时上屏。
        signalJob = scope.launch { bus.signals.collect { publish() } }
    }

    /**
     * 本地消息（乐观回显）。
     *
     * @param anchor 加入时 assembler 已有多少个单元，用来插回原本的时间位置；prepend 时随之平移，见 [shiftLocalAnchors]。
     */
    private data class LocalMessage(
        val id: String,
        val text: String,
        var anchor: Int,
        var delivery: DeliveryState?,
        /** 失败原因贴在这条消息上，不用全屏单槽：单槽会在「A 失败、B 成功」后被擦掉。 */
        var error: String? = null,
        /**
         * 钉在最前面，不随 prepend 后移。
         *
         * `start(prompt = …)` 插的是产生这份 transcript 的原始提问，翻多少历史都该在最前面；
         * [send] 插的是此刻打的字，翻进更早的历史时必须跟着后移。
         */
        val pinnedToStart: Boolean = false,
    )

    private val localMessages = mutableListOf<LocalMessage>()

    /**
     * `echoesBack=true` 路径下在飞、且还没进 [localMessages] 的消息。
     *
     * 成功后由回声帧上屏、失败才补进列表，在飞期间锚点已定。期间发生 prepend 也要平移它们，
     * 否则失败落屏时会浮到打字时已在屏上的回答之前。重试的消息已在 [localMessages] 里，不重复登记。
     */
    private val inFlightEcho = mutableListOf<LocalMessage>()

    /** catalog 只随 `init` 变，而 `init` 到了就不再变，按它缓存，不在每次 `publish()` 重建。 */
    private var catalogSource: BridgeFrame.Init? = null
    private var catalogCache = CommandCatalog()

    private fun cachedCatalog(): CommandCatalog {
        val init = assembler.init
        if (init !== catalogSource) {
            catalogSource = init
            catalogCache = CommandCatalog.from(init)
        }
        return catalogCache
    }

    /** 本地 id 用单调计数铸，不从内容派生。 */
    private var localSeq = 0

    /** 起过一次就不再重入，`stop()` 之后也不行。 */
    private var started = false

    // ---- 上滑翻历史 --------------------------------------------------------------

    /** 取更早一段的加载器。返回空或 `null` = 到顶了。 */
    private var olderLoader: (suspend () -> List<BridgeFrame>?)? = null
    private var historyPagingAttached = false
    private var canLoadOlder = false
    private var loadingOlder = false
    private var historyError: String? = null

    /** 下行断线原因（见 [ChatUiState.downlinkError]）。 */
    private var downlinkError: String? = null

    /**
     * 本地改过的权限模式；null = 沿用 `init` 帧报的那个。
     *
     * 协议还没有会话中途改模式的上行动作，所以它恒为 null；留着让 `publish()` 里的取值链完整。
     */
    private val permissionMode: String? = null

    /** 连续拉到「整页都是已载过的」的次数。见 [loadOlder]。 */
    private var emptyPages = 0

    /**
     * 在飞的发送数。既是重入门，也是 [ChatUiState.sending] 的来源。
     *
     * 不能扫 `localMessages` 找 SENDING：`echoesBack = true` 时那条消息成功前不在列表里。
     */
    private var inFlight = 0

    /**
     * 接上翻历史。不接就没有（[ChatUiState.canLoadOlder] 默认 false），
     * 否则没接翻页的调用方会在上滑时无限请求。
     */
    fun attachHistoryPaging(loader: suspend () -> List<BridgeFrame>?) {
        olderLoader = loader
        historyPagingAttached = true
        canLoadOlder = true
        publish()
    }

    /**
     * 上滑到顶时调。
     *
     * 防重入（[loadingOlder]）；加载器返回空就关掉 [canLoadOlder]，显示「已是最早」；
     * 失败也在 `finally` 里复位，否则一次网络抖动就把翻页永久卡在「正在载入」。
     */
    fun loadOlder() {
        val loader = olderLoader ?: return
        if (loadingOlder || !canLoadOlder) return
        loadingOlder = true
        // 必须在 publish 之前清：状态条里错误优先级更高，不清的话重试时仍显示错误条。
        historyError = null
        publish()
        scope.launch {
            try {
                val older = loader()
                // 判据是真的多出了单元，不只是 loader 返回了东西；一直返回重叠内容时会无限重拉。
                if (older.isNullOrEmpty()) {
                    // loader 自己说没有了，这才是确定的到顶
                    canLoadOlder = false
                    emptyPages = 0
                } else {
                    // 两个单位不能混用：`prependFrames` 返回新增的 key 数，`anchor` 是 `units()` 的下标。
                    // 正文为空的块、没见过工具身份的块不出单元，两者不等；混用会把用户的提问挤到回答之后。
                    val unitsBefore = assembler.units().size
                    val added = assembler.prependFrames(older)
                    if (added > 0) {
                        shiftLocalAnchors(assembler.units().size - unitsBefore)
                        emptyPages = 0
                    } else {
                        // `added == 0` 不等于到顶：按行切页时最后一页常常整页落在接缝上、全是重叠。
                        // 也不能无限重试，给一个连续空页的上限。
                        emptyPages++
                        if (emptyPages >= MAX_EMPTY_PAGES) canLoadOlder = false
                    }
                }
            } catch (e: CancellationException) {
                // 取消不是失败，原样上抛
                throw e
            } catch (
                @Suppress("TooGenericExceptionCaught") e: Exception,
            ) {
                // 翻页失败不能崩页面，也不能静默（否则只剩一个不动的「正在载入」）。
                historyError = e.message ?: e.javaClass.simpleName
            } finally {
                loadingOlder = false
                publish()
            }
        }
    }

    /**
     * @param frames 帧源。
     * @param smooth 是否开缓释节拍。UI 测试关掉它可瞬时出全文。
     */
    fun start(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
        prompt: String? = null,
    ) {
        // 判据是 [started] 不是 `feedJob != null`：`stop()` 会把 feedJob 置 null，
        // 旋转屏幕让 `LaunchedEffect` 重跑时就会再插一条 prompt，重复 key 会让 LazyColumn 崩。
        if (started) return
        started = true
        // prompt 是产生这份 transcript 的原始提问，钉在最前面
        prompt?.let { addLocal(it, pinnedToStart = true) }
        attachFeed(frames, smooth)
    }

    /**
     * 把下行重新接上。
     *
     * 两种情形要用到它：远端 tail 到了 `timeout` 上限干净退出（没有异常、界面不再更新）；
     * 按过「停止」之后（`started` 不复位，[start] 不会再起）。
     * 重挂目前从头重读，不做 offset 续传。
     *
     * @param frames 新的帧源，由调用方重新构造。
     * @return `false` = 当前还在收，什么都没做（幂等，否则会开出第二条流、内容双份）。
     */
    fun reattach(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
    ): Boolean {
        if (feeding) return false
        // 重挂就是要再看：清掉上一次的停止、断线结论，否则界面停在旧状态
        stoppedByUser = false
        downlinkError = null
        // 停止结论说的是上一条流里那一轮，重挂之后不再适用
        remoteStop = RemoteStop.None
        attachFeed(frames, smooth)
        publish()
        return true
    }

    /**
     * 开屏与「重新接上内容」走同一个入口：第一次是 [start]，之后是 [reattach]。
     *
     * `ChatRoute` 的「重新接上」按钮重跑整条开屏序列（连接门、账号门、幂等重建管道、挂下行），
     * 不只挂下行：管道已死时只重挂会去 tail 一个不再增长的文件，按钮点得动而一个字都收不到。
     * 「起过没有」是本类的事实（[started] 私有），不让调用方各自维护一份布尔。
     */
    fun openOrReattach(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
    ) {
        if (started) {
            reattach(frames, smooth)
            return
        }
        // 上一趟的失败结论（账号门那句、接不上主机）不能留到这一趟，
        // 否则选好账号回来后管道起来了，「这台机器还没选账号」还挂着。
        downlinkError = null
        start(frames, smooth)
        // 清了要发出去：`start()` 自己不 publish，而门刚修好时可能一帧都还没来。
        publish()
    }

    private fun attachFeed(
        frames: Flow<BridgeFrame>,
        smooth: Boolean,
    ) {
        feeding = true
        feedJob =
            scope.launch {
                // 分两类接：取消原样放行；真异常（断网、会话被杀）变成 [ChatUiState.downlinkError]，
                // 不接的话异常从 scope.launch 逃出去会崩掉整个 app。收尾在 finally 里。
                try {
                    frames.collect { onFrame(it) }
                } catch (e: CancellationException) {
                    throw e // 取消不是错误，别当成断线报出去
                } catch (
                    @Suppress("TooGenericExceptionCaught") e: Exception,
                ) {
                    // 下行断了要说出来，但不崩页面；已收到的内容留在屏上。
                    downlinkError = e.message?.takeIf { it.isNotBlank() } ?: e.javaClass.simpleName
                } finally {
                    // 流结束（正常或被停）：停节拍、吐净积压。
                    // 不能只在收到 `res` 时停节拍：没有 `res` 收尾的流会漏一个 10Hz 常驻协程。
                    tickJob?.cancel()
                    tickJob = null
                    release.flush()
                    // 干净结束也要说出来：远端 tail 到 `timeout` 上限会正常退出，没有异常也没有 `res`，
                    // 不说的话界面一直显示「生成中」且不再更新。对用户是同一件事（这个对话现在
                    // 收不到东西），所以复用 downlinkError 这一条通道。
                    feeding = false
                    if (!stoppedByUser && downlinkError == null) downlinkError = "内容接收已结束"
                    publish()
                }
            }

        if (smooth) {
            tickJob =
                scope.launch {
                    // 条件循环而非 `while (true)`：喂完且积压清空就自然结束
                    while (feedJob?.isActive == true || release.backlog > 0) {
                        delay(CHAT_INTERVAL_MS)
                        if (release.tick().isNotEmpty()) publish()
                    }
                }
        }
    }

    /**
     * 停止。
     *
     * - 接了 [UplinkSink]：投一条 `interrupt`，下行继续收。停没停的证据是下行那条
     *   `res`（`terminal_reason=aborted_streaming`），停掉下行就永远看不见它。
     * - 没接：[stopLocally]，界面说明远端那一轮可能还在跑。
     *
     * 注意：`stop()` 返回、`streaming` 翻 false、输入框回来，这些在中断根本没发出去时也成立；
     * 停住与否只认远端的字节，见 [RemoteStop.Confirmed]。
     */
    fun stop() {
        val sink = uplink ?: return stopLocally()
        // 再点一次 = 不等了：不发第二条中断，退回本地停显示。
        // 不能直接 return：远端一直不确认时屏幕会永远停在「正在让远端停下…」，没有动作能离开。
        // 退回本地后那句「远端那一轮可能还在跑」此刻字面为真。
        if (remoteStop is RemoteStop.Requested) return stopLocally()
        remoteStop = RemoteStop.Requested
        publish() // 立刻上屏「正在让远端停下…」，晚一个循环会像没点着
        scope.launch {
            val outcome =
                runCatching { sink.interrupt() }
                    .getOrElse { e ->
                        // 取消不是失败
                        if (e is CancellationException) throw e
                        // sink 的契约是不抛异常，实现违约也不该弄崩页面
                        SendOutcome.Rejected(e.message ?: e.javaClass.simpleName)
                    }
            if (outcome is SendOutcome.Rejected) {
                // 送不出去 ⇒ 至少把显示停住，并说出原因。先记原因再 stopLocally，否则那次 publish 发的是旧状态。
                remoteStop = RemoteStop.Failed(outcome.reason)
                stopLocally()
            } else {
                publish()
            }
        }
    }

    /** 只把显示停住。没接上行、或中断投不出去时走这里。 */
    private fun stopLocally() {
        stoppedByUser = true
        feeding = false
        feedJob?.cancel()
        tickJob?.cancel()
        tickJob = null
        release.flush()
        publish()
    }

    /** 发送。乐观回显：点了就上屏，不等对端。没接 [UplinkSink] 时只上屏。 */
    fun send(text: String) = submit(text, override = false)

    /**
     * 「我知道，还是发」：等待态下随时能用的出路，不依赖等待态自己解除。
     *
     * 等待态读数可能误判（纯告知框也报 `dialog open`、没人检查 `statusUpdatedAt`、
     * 状态文件停更分不清是没变化还是进程卡住），只靠「解除后自动恢复」会成为死胡同。
     * 走这条发出去的消息照常按成功路径处理。按过一次后本对话不再拦，见 [waitingOverridden]。
     */
    fun sendAnyway(text: String) = submit(text, override = true)

    private fun submit(
        text: String,
        override: Boolean,
    ) {
        // 在飞门放在这里而不靠按钮 disabled：`send()` 是公开 API。
        // 用独立计数器而不扫 `localMessages`：`echoesBack=true` 时那条消息成功前不在列表里，
        // 扫列表什么都拦不住，`sending` 也会是 false（连停止键都不出现）。
        // 两道门写在一个 if 里是 detekt `ReturnCount` 的要求。
        if (text.isBlank() || inFlight > 0) return
        if (override) waitingOverridden = true
        // 拦的是上行，不是屏幕：被拦的消息 `sink.send` 一次都不调。
        if (blockedByWaiting() != null) {
            rejectForWaiting(text)
            return
        }
        // 上一轮的停止结论不能挂到下一轮。清在按下发送时，不等下一个 `res`：从这一刻起「那一轮」已经不是它了。
        remoteStop = RemoteStop.None
        val sink = uplink
        // 对端会回显时不做乐观回显，等回声帧上屏：回声帧里认领不出我们的 localId，宁可慢一个往返也不显示两遍。
        if (sink != null && sink.echoesBack) {
            deliver(LocalMessage(nextLocalId(), text, assembler.units().size, null), sink)
            return
        }
        val msg = addLocal(text)
        publish()
        if (sink == null) return // 没接上行 ⇒ 只回显
        deliver(msg, sink)
    }

    /**
     * 此刻该不该拦上行。null = 不拦（没有等待态、已过期、已作废、按过「还是发」）。
     *
     * 判定条件是 `status == "waiting"`（信号汇里存的就是它），不是 `waitingFor != null`：
     * 老版本 CC 不写 `waitingFor`，拿它当判据会恒不触发。
     */
    private fun blockedByWaiting(): WaitingNotice? = waitingNoticeNow()?.takeIf { it.blocksSending }

    /**
     * 把等待态算成屏上那几句话。null = 此刻没有等待态。
     *
     * [WaitingNotice.blocksSending] 为 false 时也要返回：「读数过期所以不拦」也得说出来。
     */
    private fun waitingNoticeNow(): WaitingNotice? {
        val bus = signals ?: return null
        val sid = signalSessionId ?: return null
        val gate = bus.waitingGate(sid, clock())
        val signal = gate.signal ?: return null
        val blocks = gate.blocks && !waitingOverridden
        val why =
            when {
                waitingOverridden -> OVERRIDDEN_WHY
                gate.degradedWhy != null -> gate.degradedWhy
                else -> BLOCKING_WHY
            }
        return WaitingNotice(
            headline = WaitingCopy.headlineFor(signal.waitingFor),
            detail = WaitingCopy.ANSWER_IT_ON_THE_COMPUTER,
            blocksSending = blocks,
            reason = "${signal.source}@${signal.observedAtMs} waitingFor=${signal.waitingFor} · $why",
        )
    }

    /**
     * 等待态下的那次发送：上屏、判失败、给理由，`sink.send` 一次都不调。
     *
     * 用可重试的 [DeliveryState.FAILED] 而不是 `FAILED_PERMANENT`：这是「现在别发」，不是「重试也没用」。
     */
    private fun rejectForWaiting(text: String) {
        val msg = addLocal(text)
        msg.delivery = DeliveryState.FAILED
        // 与模态拒绝路径共用同一个文案常量：说两句不同的话会像两种故障。
        msg.error = WaitingCopy.SENDING_NOW_ANSWERS_THE_QUESTION
        publish()
    }

    /**
     * 重试一条失败的消息。用同一个 [LocalMessage.id]，key 不变，列表里不多出第二条
     * （重复 key 会让 `LazyColumn` 抛异常）。
     */
    fun retry(localId: String) {
        val sink = uplink ?: return
        // 与 send() 同一道在飞门（并发的两次写入会让模态探测看不到对方正在写的屏），
        // 也走同一道等待门（否则重试照样把话写上去）。合成一个 if 是 detekt `ReturnCount` 的要求。
        if (inFlight > 0 || blockedByWaiting() != null) return
        val msg = localMessages.firstOrNull { it.id == localId } ?: return
        if (msg.delivery != DeliveryState.FAILED) return // 永久失败、正在发的不重试
        deliver(msg, sink)
    }

    /**
     * 投递一条消息。上屏时机只由 [UplinkSink.echoesBack] 决定，send 和 retry 两个入口无从分歧。
     *
     * 回显路径：成功交给回声帧显示（重试的那条要从列表撤下，否则同一句显示两遍）；
     * 失败必须上屏，否则写的那段话连同失败一起静默消失。
     */
    private fun deliver(
        msg: LocalMessage,
        sink: UplinkSink,
    ) {
        val echoes = sink.echoesBack
        msg.delivery = DeliveryState.SENDING
        inFlight++
        // 只登记还不在 localMessages 里的；重试的消息已在列表里，重复登记会平移两次
        if (echoes && localMessages.none { it.id == msg.id }) inFlightEcho += msg
        publish() // 无条件 publish：`sending` 要立刻为 true，否则回显路径下连停止键都不出现
        scope.launch {
            val outcome =
                try {
                    runCatching { sink.send(SendRequest(msg.id, msg.text)) }
                        .getOrElse { e ->
                            if (e is CancellationException) throw e
                            // sink 的契约是不抛异常，实现违约也不该弄崩页面
                            SendOutcome.Rejected(e.message ?: e.javaClass.simpleName)
                        }
                } finally {
                    // 计数器在 finally 里减：协程被取消也要放行，否则发送被永久锁死
                    inFlight--
                    // 按同一性删，不按值：`LocalMessage` 是 data class 且有 var 字段，值相等会删错条
                    inFlightEcho.removeAll { it === msg }
                }
            msg.delivery =
                when (outcome) {
                    // Accepted 只说明已发出、还没看到对面反应，不是「已送达」；见 [DeliveryState.SENT_UNCONFIRMED]。
                    SendOutcome.Accepted -> DeliveryState.SENT_UNCONFIRMED
                    is SendOutcome.Rejected ->
                        if (outcome.retryable) DeliveryState.FAILED else DeliveryState.FAILED_PERMANENT
                }
            msg.error = (outcome as? SendOutcome.Rejected)?.reason
            if (echoes) {
                if (outcome == SendOutcome.Accepted) {
                    localMessages.removeAll { it.id == msg.id } // 重试成功：撤下本地那条，等回声
                } else if (localMessages.none { it.id == msg.id }) {
                    localMessages += msg // 失败必须上屏
                }
            }
            publish()
        }
    }

    /**
     * 本地 key：单调序号，前缀是具名常量 [LOCAL_KEY_PREFIX]。
     * 不用内容哈希：同样的话说两遍会被合并，重试时还会随文本漂移。
     */
    private fun nextLocalId() = "$LOCAL_KEY_PREFIX${localSeq++}"

    private fun addLocal(
        text: String,
        pinnedToStart: Boolean = false,
    ): LocalMessage {
        val msg = LocalMessage(nextLocalId(), text, assembler.units().size, null, pinnedToStart = pinnedToStart)
        localMessages += msg
        return msg
    }

    /**
     * prepend 之后本地消息的锚点整体后移，钉在最前面的除外。
     *
     * 不平移，此刻打的那句会浮到新载进的历史之前；一律平移，原始提问会被推到历史之后。
     */
    private fun shiftLocalAnchors(added: Int) {
        // 两个集合都要平移：回显路径下在飞的消息不在 localMessages 里
        (localMessages + inFlightEcho).forEach { if (!it.pinnedToStart) it.anchor += added }
    }

    private fun onFrame(frame: BridgeFrame) {
        assembler.feed(frame)
        // 路由结论只来自 assembler：子 agent 的帧在这里被挡掉，不会污染主流缓释
        val route = assembler.route(frame)
        if (route is FrameRoute.MainBlock) {
            when (frame) {
                is BridgeFrame.TextDelta -> {
                    switchStreamTo(route.key)
                    release.offer(frame.x)
                }
                // 全文覆盖：保留已露出的部分，只把剩下的排队（不倒回去重放已读过的）
                is BridgeFrame.AssistantText -> {
                    switchStreamTo(route.key)
                    release.replaceAll(frame.x)
                }
                else -> Unit
            }
        }
        if (frame is BridgeFrame.Result) {
            // 一轮收口：把剩下的全露出来。但不停 tickJob，同一条流里还可能有下一轮。
            release.flush()
            streamingKey = null
            confirmRemoteStopIfThisIsTheProof(frame)
        }
        consumeCrossScreenSignals(frame)
        schedulePublish()
    }

    /**
     * 把聊天通路上产的跨屏信号投进信号汇。
     *
     * - 配额：`rate_limit_event` → [BridgeFrame.RateLimit]。
     * - 「它被挡住了」：`system/permission_denied`，经帧编码器的兜底分支成为 `Event("system:permission_denied")`。
     *   形状为 `{"type":"system","subtype":"permission_denied","tool_name":…,"tool_use_id":…,"message":…}`；
     *   这条帧未在真实会话里复现过，帧不来就什么都不显示。
     */
    private fun consumeCrossScreenSignals(frame: BridgeFrame) {
        val sid = signalSessionId
        when {
            frame is BridgeFrame.RateLimit -> {
                val reading = RateLimitReading.parse(frame.raw, clock())
                // 解不出来就不覆盖上一条真读数
                if (reading != null) {
                    quota = reading
                    sid?.let { signals?.publishRateLimit(it, frame.raw, clock()) }
                }
            }

            frame is BridgeFrame.Event && frame.k == EVENT_PERMISSION_DENIED -> {
                val now = clock()
                blocked =
                    BlockedSignal(
                        toolName = frame.raw?.get("tool_name") as? String,
                        // 远端那句话原样带回：我们读不懂每一种拦截
                        humanText = frame.raw?.get("message") as? String,
                        observedAtMs = now,
                    )
                sid?.let {
                    signals?.publishBlocked(it, blocked?.toolName, blocked?.humanText, now)
                }
            }

            else -> Unit
        }
    }

    /**
     * 「远端真的停了」只能由远端产出的字节来说。
     *
     * 只从 [RemoteStop.Requested] 转移：`aborted_streaming` 只说明这一轮被打断，不说明是被这边打断
     * （也可能是电脑上按的停止）。别人打断的那种由 [ChatUiState.failedWhy] 的人话映射说出来。
     * 状态机里没有任何一条边由本地动作走到 [RemoteStop.Confirmed]。
     */
    private fun confirmRemoteStopIfThisIsTheProof(frame: BridgeFrame.Result) {
        if (remoteStop !is RemoteStop.Requested) return
        if (frame.why != RemoteStop.ABORTED_STREAMING) return
        remoteStop = RemoteStop.Confirmed
    }

    /**
     * 把同一批帧的多次 publish 合并成一次。
     *
     * [publish] 重建整份渲染列表，代价随对话长度线性增长（几千个单元时单次约 0.3ms，手机更慢），
     * 而流式一轮有几百个增量帧，都在主线程上。逐字上屏的节奏由 [SmoothRelease] 的节拍决定，
     * 不由帧决定，所以合并不改变屏上节奏。
     *
     * 只有 [onFrame] 走这条。发送、停止、翻历史这些用户动作的回应仍立即 [publish]。
     */
    private fun schedulePublish() {
        if (publishScheduled) return
        publishScheduled = true
        scope.launch {
            // 让出一次：同一批到达的帧在这次 yield 之前全部处理完，共用这一次发布
            yield()
            publishScheduled = false
            publish()
        }
    }

    /** 换到新块时，上一个块先全量露出，否则它会永远停在半截。 */
    private fun switchStreamTo(key: String) {
        if (streamingKey != key) {
            release.flush()
            release.replaceAll("")
            streamingKey = key
        }
    }

    /**
     * 起管道失败也要说出来。复用 [ChatUiState.downlinkError] 而不新加字段：
     * 对用户而言「管道没起来」与「下行断了」是同一件事，分两个字段会出现两条并列的错误横幅。
     */
    fun reportStartFailure(reason: String) {
        downlinkError = reason
        publish()
    }

    /**
     * 把已知的 `terminal_reason` 机器码换成人话，不认识的返回 null（原样显示机器码）。
     *
     * 机器码难看，但能被搜索、能贴给开发者；笼统的「出错了」会把线索抹掉。
     * 人话的正规来源是 [TurnState.Done.humanText]，这里只补它为空的那几个洞，别长成翻译表。
     */
    private fun humanTextFor(why: String?): String? =
        when (why) {
            RemoteStop.ABORTED_STREAMING -> RemoteStop.ABORTED_HUMAN
            else -> null
        }

    private fun publish() {
        publishCount++
        val shownLen = release.released.length
        val assembled = assembler.units()
        // 本地消息按加入时 assembler 有多少单元放回原位（prepend 时锚点已随之平移）
        val merged = ArrayList<RenderUnit>(assembled.size + localMessages.size)
        var next = 0
        localMessages.forEach { m ->
            val at = m.anchor.coerceIn(0, assembled.size)
            while (next < at) merged += assembled[next++]
            merged += RenderUnit.UserText(m.id, m.text, null, m.delivery, m.error)
        }
        while (next < assembled.size) merged += assembled[next++]

        val units =
            merged.map { u ->
                // 只截当前流式的那个块；其余全量
                if (u.key == streamingKey && u is RenderUnit.AssistantMarkdown) {
                    u.copy(markdown = u.markdown.take(shownLen))
                } else {
                    u
                }
            }
        val done = assembler.turn as? TurnState.Done
        _state.value =
            ChatUiState(
                units = units.filter { it !is RenderUnit.AssistantMarkdown || it.markdown.isNotEmpty() },
                // 下行死了就不再显示「生成中」，否则输入行只剩停止键、再也发不出消息。
                // 只动 streaming，`turn` 是协议事实，没收到 `res` 就如实停在 Streaming。
                // 判据是 `turn is Streaming` 而不是 `done == null`：Idle 的 done 也是 null，
                // 刚起的管道会被判成生成中 ⇒ 第一句发不出去 ⇒ `res` 永不到 ⇒ 死锁。
                streaming = !stoppedByUser && downlinkError == null && assembler.turn is TurnState.Streaming,
                turn = assembler.turn,
                stoppedByUser = stoppedByUser,
                remoteStop = remoteStop,
                historyPagingAttached = historyPagingAttached,
                canLoadOlder = canLoadOlder,
                loadingOlder = loadingOlder,
                historyError = historyError,
                downlinkError = downlinkError,
                canReattach = !feeding,
                uplinkAttached = uplink != null,
                // `init.raw` 里只有单数的 `permissionMode`（如 "default"），没有可选列表。
                // 本地改过用本地的，否则用远端报的当前模式。
                permissionMode = permissionMode ?: assembler.init?.raw?.get("permissionMode") as? String,
                // catalog 与 permissionMode 同源（都来自 `init`）；按 `init` 缓存，流式期间每拍都会跑到这里。
                catalog = cachedCatalog(),
                sending = inFlight > 0,
                revealedChars = shownLen,
                // 等待态来自跨通路信号汇（见 [attachSignals]）
                waiting = waitingNoticeNow(),
                quota = quota,
                blocked = blocked,
                // 判据是 `!ok`：`terminal_reason` 为空时 `why` 根本不在帧里，看 why 会把失败当正常结束。
                failedWhy = failedWhyFor(done),
            )
    }

    /**
     * 这一轮没正常结束时显示的那句话。null = 没失败。
     *
     * 取值链顺序不能换：`humanText`（远端给的人话，如 `Not logged in · Please run /login`）
     * → `why` 的人话映射 → `why` 机器码 → `lastError.code` → 兜底。
     * 单独抽出来是 detekt `CyclomaticComplexMethod` 的要求。
     */
    private fun failedWhyFor(done: TurnState.Done?): String? =
        done
            ?.takeIf { !it.ok }
            ?.let {
                it.humanText ?: humanTextFor(it.why) ?: it.why ?: assembler.lastError?.code ?: "未知原因"
            }

    /**
     * 释放本对话。只有 [ChatController] 该调它：屏幕关掉不算结束，
     * 由 controller 按对话的生命周期来收，离开 app 后回复完成推送才成立。
     */
    fun close() {
        feedJob?.cancel()
        tickJob?.cancel()
        // 信号汇是进程级的，对话收掉不把自己那条摘掉就是泄漏
        signalJob?.cancel()
        signalSessionId?.let { signals?.forget(it) }
        scope.cancel()
    }

    companion object {
        /** `system/permission_denied` 经帧编码器兜底分支之后的帧名。 */
        const val EVENT_PERMISSION_DENIED = "system:permission_denied"

        /** [WaitingNotice.reason] 里「按过『我知道，还是发』所以不拦」那一格。 */
        const val OVERRIDDEN_WHY = "用户按过「我知道，还是发」"

        /** [WaitingNotice.reason] 里「读数够新、正在拦」那一格。 */
        const val BLOCKING_WHY = "读数够新 ⇒ 拦上行"

        /**
         * 本地消息 key 的前缀。与 `ChatTurnAssembler.KEY_PREFIX`（`"f:"`）和 classifier 的
         * `<uuid>#<i>` 三个空间互不相交；写成常量，不撞是代码性质而不是运行时约定。
         */
        const val LOCAL_KEY_PREFIX = "local#"

        /** 缓释窗口：每 100ms 最多放 16 个字符。单位是字符数和毫秒。 */
        const val CHAT_CHUNK = 16
        const val CHAT_INTERVAL_MS = 100L

        /** 连续多少页一条都没新增就认定到顶（防 loader 一直返回重叠内容导致死循环）。 */
        const val MAX_EMPTY_PAGES = 3
    }
}
