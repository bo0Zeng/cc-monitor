package com.ccmonitor.mobile.service

import android.app.ForegroundServiceStartNotAllowedException
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.IBinder
import android.os.SystemClock
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.ccmonitor.mobile.MainActivity
import com.ccmonitor.mobile.agent.agentKindOrDefault
import com.ccmonitor.mobile.core.claude.agent.AgentDirSettingKey
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionCatalog
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.transport.AlphaUnavailable
import com.ccmonitor.mobile.core.claude.transport.DaemonLocator
import com.ccmonitor.mobile.core.claude.transport.DaemonTurnEndSource
import com.ccmonitor.mobile.core.claude.transport.JsonlFrame
import com.ccmonitor.mobile.core.claude.transport.ResumeOffset
import com.ccmonitor.mobile.core.claude.transport.TailTransport
import com.ccmonitor.mobile.core.claude.transport.TurnEndDebouncer
import com.ccmonitor.mobile.core.claude.transport.TurnEndRoute
import com.ccmonitor.mobile.core.claude.transport.agentKindFromWire
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.core.ssh.commandExecutor
import com.ccmonitor.mobile.core.ssh.countsAsActive
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ui.chat.ChatController
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import org.koin.android.ext.android.inject

/**
 * SSH 保活前台服务。会话连上时启动，挂一条常驻通知降低后台被杀的概率（部分 ROM 仍会杀，靠 tmux 重连兜底）。
 * 没有活动会话也没有在飞轮次时自停；网络恢复时调 `probeAndReconnectStale()`。
 *
 * 连接期间，对每台有活动会话的主机起一个 turn-end watcher：app 在后台（[AppForeground]）时，
 * agent 完成一轮就发一条本地通知。纯客户端，不经云。
 *
 * 完成通知优先吃 α（守护进程的 `turn_end` 帧），α 用不了才回落 β（tail 会话 JSONL，逐行用
 * [AgentProfile.turnEndDetector] 判轮次）。β 的 tail 目标来自 [ClaudeSessionCatalog.scanLiveSessions]
 * 的活动集（pidfile 判活），每个活会话一条 tail，上限 [MAX_WATCH_TAILS]，规则见 [watchTargets]。
 *
 * 分工：同一轮多帧折成一个事件在 `TurnEndDebouncer`；帧到「哪条会话、哪个文件」在 `TurnEndRoute`；
 * 探测、能力门控、起流在 `DaemonTurnEndSource`；走哪条路、多久再试在 [TurnEndPathArbiter]；
 * 发不发、新基线是什么在 [TurnEndNotifyDecision]。
 *
 * 两条路发通知的唯一出口是 [settleRound]，共用 [baselineByPath] 这一个跨路、跨重连的去重账本。
 */
class SshKeepAliveService : Service() {
    private val manager: SshConnectionManager by inject() // 只用会话计数、通道工厂和探活重连，不管连接生死

    /** 断连接经网关：[onTaskRemoved] 走 [HostConnector.disconnectAll]。 */
    private val connector: HostConnector by inject()
    private val hostRepo: HostRepository by inject() // 取每台主机的 agent 目录
    private val settingsRepo: SettingsRepository by inject() // 应用级默认 agent 目录

    /** 存活判据要问它有没有在飞轮次，见 [onCreate]。 */
    private val chatController: ChatController by inject()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var netCallback: ConnectivityManager.NetworkCallback? = null

    /** 每台主机一个 turn-end watcher，key 是连接 id。 */
    private val watchers = mutableMapOf<String, Job>()

    /**
     * 每个通知槽（[TurnEndRoute.Done.slotKey]：一般是 JSONL path，α 拿不到 path 时是 `sid:<sid>`）已基线或通知过的
     * end_turn uuid。跨 watcher 重启和重连存活，只在主线程访问：重连后断网期间跑完的新一轮与基线不同，补发一次；
     * 见过的不重发。两条路共用，理由见 [TurnEndNotifyDecision]。
     * 注意：值为 `null` 是合法的（见过这个槽、当时没读出 uuid），判见没见过一律用 `containsKey`。
     */
    private val baselineByPath = mutableMapOf<String, String?>()

    /**
     * 每个 JSONL path 的 tail 续传字节 offset，跨 watcher 重启和重连存活，只在主线程访问。
     * 重 tail 从这里续，不从 0 重下整份（大会话十几 MB）。
     * 与 [baselineByPath] 在 debounce 结算时一起提交，不逐行推进，保证 offset 不超过已结算的位置：
     * tail 在 debounce 窗口内被取消时两者都不动，下次从上次结算点重读，uuid 去重，不漏 turn-end。
     * 文件被截短（/compact 重写，size < offset）时复位到 0（见 `ResumeOffset.remoteSize`）。
     */
    private val offsetByPath = mutableMapOf<String, Long>()

    @Volatile private var started = false // startForeground 成功前不许 stopSelf，否则会崩

    override fun onBind(intent: Intent?): IBinder? = null

    // 从最近任务里划走 = 用完了：断开全部连接、收掉全部对话、停服务，免得划走后还常驻耗电。
    // 只有划走触发；普通返回和退到后台仍按活动会话保活。
    override fun onTaskRemoved(rootIntent: Intent?) {
        runCatching { connector.disconnectAll() }
        // 对话是应用级的，不随屏幕消失，得有人收；没有这里，对话只会攒到上限被逐出。
        runCatching { chatController.releaseAll() }
        stopFor(KeepAliveStopReason.TaskRemoved)
        super.onTaskRemoved(rootIntent)
    }

    override fun onCreate() {
        super.onCreate()
        createChannel()
        registerNetwork()
        scope.launch {
            // 存活判据是「有活动会话或有在飞轮次」。连接是惰性的，活动会话为 0 是常态；只看会话数的话服务会当场自停，
            // 离开 app 等回复时通知永远不来，也没有任何地方报错。在飞轮次由应用级的 `ChatController` 持有，
            // 不能由本服务的 watcher 持有（watcher 自己也挂在活动会话上）。
            combine(manager.sessions, chatController.hasInFlightTurn) { _, inFlight ->
                manager.activeCount() to inFlight
            }.collectLatest { (n, inFlight) ->
                // 判决在 [KeepAlivePolicy]，和 [onTimeout] 走同一道闸。
                val reason = KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.NoWork, hasWork = n > 0 || inFlight)
                if (reason != null) {
                    // startForeground 之后才许自停，否则会 "did not call startForeground" 崩溃。
                    if (started) stopFor(reason)
                } else {
                    notify(n)
                }
            }
        }
        // watcher 管理。用 collect 不用 collectLatest：watcher 要跨 sessions 变化存活。
        scope.launch {
            manager.sessions.collect { map ->
                // 哪些主机起 watcher，用与保活计数同一个谓词 countsAsActive()。这是会话的名义状态，不当底层存活判据：
                // 半开时 status 还是 CONNECTED，watcher 的 tail 只会失败、下一轮自愈；网络恢复时真探测重连，
                // status 一翻，这里就撤或重建 watcher。agent 会话是否活着另由 pidfile 判。
                val toWatch =
                    map.entries
                        .filter { it.value.countsAsActive() }
                        .map { it.key }
                        .toSet()
                toWatch.forEach { id ->
                    if (watchers[id]?.isActive != true) watchers[id] = scope.launch { watchHost(id) }
                }
                watchers.entries.filter { it.key !in toWatch }.map { it.key }.forEach { id ->
                    watchers.remove(id)?.cancel()
                }
            }
        }
    }

    /**
     * `startForeground` 不裸调：Android 12+ 在后台起前台服务、Android 15+ `dataSync` 额度用尽时，系统都抛
     * `ForegroundServiceStartNotAllowedException`，冲出 `onStartCommand` 进程就崩。
     *
     * 接住之后不只吞掉，否则服务起不来却没有任何地方报错：记下 [KeepAliveStopFact]、打一条 `Log.w`、停掉自己。
     * 注意：`started` 只在成功之后置真，失败后置真会让观察者放开自停闸。
     * 通知在 `try` 外先建好：建通知自己抛的 `IllegalStateException` 不该被认成「系统不许起」。
     */
    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        val notification = buildNotification(manager.activeCount().coerceAtLeast(1))
        try {
            ServiceCompat.startForeground(
                this,
                NOTIF_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC, // API34+ 必须显式带类型
            )
        } catch (e: IllegalStateException) {
            // 不认识的 IllegalStateException 原样上抛，见 [KeepAlivePolicy.isStartRefusal]。
            if (!KeepAlivePolicy.isStartRefusal(e, Build.VERSION.SDK_INT)) throw e
            Log.w(TAG, "startForeground 被系统拒绝 → 记账并停：${e.message}")
            stopFor(KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.StartRefused, hasWork = hasWork()))
            return START_NOT_STICKY
        }
        started = true
        // 自停型服务用 START_NOT_STICKY：进程被杀后不带 null intent 重启，免得和 startForeground 竞态。
        return START_NOT_STICKY
    }

    /**
     * Android 15 的 `dataSync` 额度到点，立刻停。
     *
     * 本服务是 `dataSync` 型，targetSdk 高于 35。系统允许一个 app 的 `dataSync` 服务 24 小时内合计跑 6 小时，
     * 到点调 `Service.onTimeout(int, int)`；此时服务已不算前台服务，只有几秒可以 `stopSelf()`，不停就抛
     * `RemoteServiceException: A foreground service of type dataSync did not stop within its timeout`。
     * 真机上可以用 `adb shell am compat enable FGS_INTRODUCE_TIME_LIMITS <pkg>` 加
     * `adb shell device_config put activity_manager data_sync_fgs_timeout_duration <ms>` 缩短额度来验。
     *
     * 两参的 `onTimeout` 是 API 35 新增（单参那支是 API 34 给 `shortService` 用的，本服务不是那个型），
     * 所以挂 `@RequiresApi`；低版本上它只是个没人调的方法。基类实现为空，不调 `super`。
     *
     * 这一支绝不能看现在有没有活：「没活才停」的条件在有轮次在飞时不停，几秒后就崩，而跑满 6 小时的服务多半正有活。
     * `hasWork` 照实传给 [KeepAlivePolicy]，由它判照停。
     */
    @RequiresApi(Build.VERSION_CODES.VANILLA_ICE_CREAM)
    override fun onTimeout(
        startId: Int,
        fgsType: Int,
    ) {
        Log.w(TAG, "前台服务额度到点（startId=$startId fgsType=$fgsType）→ 立刻停")
        stopFor(KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.SystemTimeout, hasWork = hasWork()))
    }

    /** 有活 = 有活动会话或有在飞轮次，与 [onCreate] 里的判据相同。 */
    private fun hasWork(): Boolean = manager.activeCount() > 0 || chatController.hasInFlightTurn.value

    /** 停服务的唯一出口：先把为什么停记下来，再停。`reason == null` 表示这回不该停，什么也不做。 */
    private fun stopFor(reason: KeepAliveStopReason?) {
        if (reason == null) return
        KeepAliveStopFact.record(reason)
        stopSelf()
    }

    // ─── 后台 turn-end watcher ───

    /**
     * 监视一台主机：周期性经 catalog 解析活动会话集，每个活会话维持一条 tail；新会话补 tail，会话死了撤 tail，
     * tail 瞬时失败下一轮自愈。主机断开时本协程被取消，全部 tail 跟着取消。
     */
    private suspend fun watchHost(connId: String) =
        coroutineScope {
            // 通道工厂在没有活连接时抛 ConnectionDeadException，绝不给空流：空流会让 scanLiveSessions 返回可信的空集，
            // watchTargets 就会撤光活 tail。抛出后走下面的 runCatching，整轮跳过。
            val channel = manager.commandChannel(connId)
            // 同一条连接上带退出码的一次性执行，截断检测用。
            val executor = manager.commandExecutor(connId)
            // connId 就是 hostId。读库失败必须兜住：异常会沿 coroutineScope 杀掉整个 watcher，而 sessions 不变时没人重启它，
            // 通知静默死。失败时退回该种类的内置默认目录；取消照样上抛。
            val host = runCatching { hostRepo.get(connId) }.onFailure { if (it is CancellationException) throw it }.getOrNull()
            // 种类读不出来走档案的缺省档；定位器、目录、catalog 都问档案。
            val kind = host?.agentKindOrDefault() ?: AgentProfile.DEFAULT.kind
            val profile = AgentProfile.of(kind)
            val locator = profile.sessionLocator
            val agentDir =
                runCatching {
                    locator.resolveAgentDir(host?.claudeDir, appDefaultAgentDir(profile))
                }.onFailure { if (it is CancellationException) throw it }
                    .getOrElse {
                        Log.w(TAG, "host=$connId 读取 agentDir 失败，回退内置默认: ${it.message}")
                        locator.defaultAgentDir
                    }
            val catalog: SessionCatalog = profile.newSessionCatalog(channel, agentDir)
            val tails = mutableMapOf<String, Job>() // path → tail job，只在本 watcher 协程（主线程）访问
            // 这台主机上谁发完成通知。
            val arbiter = TurnEndPathArbiter()
            // α 的 `turn_end` 不是每种 agent 都发（见 [AlphaUnavailable.NotClaudeHost]），不发的直接钉在 β 上、不探。
            if (!profile.daemonStreamReportsTurnEnd) {
                arbiter.onUnavailable(AlphaUnavailable.NotClaudeHost, now())
                Log.i(TAG, "host=$connId 这台配的是 ${profile.displayName}，α 不发它的 turn_end ⇒ 固定走 β")
            }
            // α 一死就立刻叫醒发现循环，不等满 REDISCOVER_MS，那段空窗会漏通知。
            val wake = Channel<Unit>(Channel.CONFLATED)
            var alpha: Job? = null
            while (isActive) {
                if (alpha?.isActive != true && arbiter.shouldTryAlpha(now())) {
                    alpha =
                        launch {
                            // α 抛了意外异常也不许上抛：coroutineScope 会让整个 watcher 跟着死，sessions 不变时没人重建，
                            // 通知静默死。取消照样上抛。
                            runCatching { runAlpha(channel, connId, daemonPath(), arbiter, kind, wake) }
                                .onFailure {
                                    if (it is CancellationException) throw it
                                    Log.w(TAG, "host=$connId α 这一趟抛了：${it::class.simpleName}: ${it.message} ⇒ 回落 β、按退避再试")
                                }
                        }
                }
                // cwd=null：不按项目过滤。scanLiveSessions 只在列 jsonl 失败时抛，scan=null 整轮跳过、保留现有 tail；
                // 存活探测失败只把 probeOk 置 false，由 watchTargets 决定不撤。
                val scan = runCatching { catalog.scanLiveSessions(cwd = null) }.getOrNull()
                if (scan != null) {
                    // α 在供货且过了交接宽限（`alphaCoversTurnEnd`）→ targets 为空、cancelStale 为真，β 的 tail 全撤。
                    val plan = watchTargets(scan, alphaCoversTurnEnd = arbiter.alphaCoversTurnEnd(now()))
                    // runCatching：tail 的瞬时错误不许上抛取消整个 watcher。
                    reconcileTails(connId, plan, tails) { path -> launch { runCatching { tailAndNotify(channel, executor, path, kind) } } }
                }
                // 平时等 REDISCOVER_MS；α 掉线时提前醒来，当场重建 β tail。
                withTimeoutOrNull(REDISCOVER_MS) { wake.receive() }
            }
        }

    /** 照 [plan] 撤旧 tail、补新 tail；[startTail] 起一条 tail 协程。只在 watcher 协程里调。 */
    private fun reconcileTails(
        connId: String,
        plan: WatchPlan,
        tails: MutableMap<String, Job>,
        startTail: (String) -> Job,
    ) {
        // 活会话超上限被截掉时记日志，不静默丢。只会在 β 档出现。
        if (plan.droppedLive > 0) {
            Log.w(TAG, "host=$connId 活动 Claude 会话超上限 $MAX_WATCH_TAILS，${plan.droppedLive} 条未监视 turn-end")
        }
        // 只在活动集可信时撤 tail；探测抖动时撤，会误杀活 tail。
        if (plan.cancelStale) {
            tails.keys.filter { it !in plan.targets }.forEach { path -> tails.remove(path)?.cancel() }
        }
        // 新目标或 tail 已死 → 重新 tail。
        plan.targets.forEach { path ->
            if (tails[path]?.isActive != true) tails[path] = startTail(path)
        }
    }

    /**
     * α 这条路跑一趟：接上就一直收，判不可用就报给 [TurnEndPathArbiter] 并叫醒 β。
     *
     * `finally` 必不可少：无论怎么结束（流自己结束没说原因、我们抛了意外异常），只要 arbiter 还以为 α 在供货，
     * β 就永远起不来，通知没了也没有线索。所以没说原因也当它死了。
     * 被取消不算：那是整个 watcher 要走，β 也一起走。
     */
    private suspend fun runAlpha(
        channel: RemoteCommandChannel,
        connId: String,
        daemonPath: String,
        arbiter: TurnEndPathArbiter,
        hostKind: AgentKind,
        wake: Channel<Unit>,
    ) {
        try {
            DaemonTurnEndSource(channel, daemonPath)
                // 时钟用 elapsedRealtime：单调，且算设备睡眠的时间（墙钟会跳，nanoTime 不算睡眠）。
                .events(nowMs = SystemClock::elapsedRealtime)
                .collect { ev ->
                    when (ev) {
                        is DaemonTurnEndSource.Event.Engaged -> {
                            arbiter.onEngaged(now())
                            Log.i(TAG, "host=$connId α 接上（${ev.hello.buildId}）⇒ 完成通知改吃 turn_end 帧；命令：${ev.command}")
                        }

                        is DaemonTurnEndSource.Event.Round ->
                            settleRound(
                                slotKey = ev.done.slotKey,
                                path = ev.done.path,
                                uuid = ev.done.uuid,
                                // α 说了哪种 agent 就按它的，没说才用这台主机配的（`agentKindFromWire(null)` 会当成 Claude）。
                                kind = ev.done.agentKindWire?.let { agentKindFromWire(it) } ?: hostKind,
                                // `--tail-only` 不回放历史，首轮照发
                                replaysHistory = false,
                            )

                        is DaemonTurnEndSource.Event.Unavailable -> {
                            arbiter.onUnavailable(ev.why, now())
                            Log.w(TAG, "host=$connId α 用不了（${ev.why}）⇒ 回落 β，${arbiter.backoffMs()}ms 后再试：${ev.detail}")
                        }
                    }
                }
        } finally {
            if (arbiter.alphaEngaged) {
                arbiter.onUnavailable(AlphaUnavailable.StreamDied, now())
                Log.w(TAG, "host=$connId α 没说原因就停了 ⇒ 当它死了、回落 β（兜底）")
            }
            wake.trySend(Unit)
        }
    }

    /**
     * 两条路唯一的发完成通知出口。判决在 [TurnEndNotifyDecision]；这里只做带副作用的三件事：写账本、过前台门、发通知。
     * 前台时不发，但基线照样推进，否则切回后台会收到刚在屏上看完的那一轮。
     *
     * @param replaysHistory 这条路挂上去会不会先收到一串历史轮次：β 为 true，α 为 false。
     */
    private fun settleRound(
        slotKey: String,
        path: String?,
        uuid: String?,
        kind: AgentKind,
        replaysHistory: Boolean,
    ): Boolean {
        val outcome =
            TurnEndNotifyDecision.decide(
                previousBaseline = baselineByPath[slotKey],
                baselineKnown = baselineByPath.containsKey(slotKey),
                settledUuid = uuid,
                replaysHistory = replaysHistory,
            )
        if (outcome.writeBaseline) baselineByPath[slotKey] = outcome.newBaseline
        if (outcome.notify && !AppForeground.isForeground) notifyAgentDone(slotKey, path, kind)
        return outcome.notify
    }

    /** α 要问的远端程序：占位名 ⇒ `DaemonLocator` 按候选表找（这一条 α 流随「一轮完成」那一路换到常驻流上时删）。 */
    private fun daemonPath(): String = DaemonLocator.UNSET_PLACEHOLDER

    /** 单调、算睡眠时间的时钟，arbiter 与 `DaemonTurnEndSource` 共用。 */
    private fun now(): Long = SystemClock.elapsedRealtime()

    /**
     * 这种 agent 的目录认不认设置里的应用级默认，问档案。
     * `when` 不带 `else`：[AgentDirSettingKey] 加一个值，这里就编译不过，不会静默走缺省。
     * 终端 VM 里有一份同样的判定，两边各自注入 settingsRepo。
     */
    private suspend fun appDefaultAgentDir(profile: AgentProfile): String? =
        when (profile.appDefaultDirSetting) {
            AgentDirSettingKey.AppDefaultClaudeDir -> settingsRepo.getDefaultClaudeDir()
            null -> null // 没有应用级默认，只认主机覆盖和内置默认
        }

    /**
     * tail 一个 JSONL 检测 turn-end。O(1) 内存：只滚动记住最近一条 end_turn 的 uuid。
     * 从 [offsetByPath] 续传；断线期间的 turn-end 落在 offset 之后，重连仍读得到，与基线不同就补发；uuid 去重不重发。
     * 文件被截短时复位到 0 重建。
     */
    @OptIn(FlowPreview::class)
    private suspend fun tailAndNotify(
        channel: RemoteCommandChannel,
        // 截断检测要分清 `wc` 读不到文件和文件真是 0 字节（处置相反），所以用带退出码的执行。与 [channel] 是同一条连接。
        executor: RemoteExecutor,
        path: String,
        kind: AgentKind,
    ) {
        // 解析器和 turn-end 探测器按 agent 种类问档案。
        val profile = AgentProfile.of(kind)
        val parser = profile.recordParser
        val detector = profile.turnEndDetector
        // 从上次 offset 续接。续接前查截断：size < offset（/compact 原地重写）→ offset 和基线一起复位。
        var start = offsetByPath[path] ?: 0L
        if (start > 0) {
            val size = ResumeOffset.remoteSize(executor, path)
            // wc 失败（null）→ 沿用旧 offset，不误复位；最坏读进几行残片，解析成 Unknown 被滤掉。
            if (size != null && size < start) {
                start = 0L
                offsetByPath.remove(path)
                baselineByPath.remove(path) // 文件已重写，旧基线未必还在；首轮重新吞历史
            }
        }
        // 清掉上次断网留下的僵尸 tail。放在 `start > 0` 之外：僵尸与从哪个 offset 续无关，重连后首次挂 tail（start==0）
        // 最常见。尽力而为，失败不阻塞；兜底是 tail 自带的 `timeout`。
        ResumeOffset.killStaleTail(executor, path)
        var current: String? = baselineByPath[path] // 截断复位之后读；重连时从既有基线起算
        val transport = TailTransport(channel, path, start)
        transport
            .frames()
            .filterIsInstance<JsonlFrame.Line>()
            // offset 在 map 里（flowOn 上游，与写 currentOffset 同线程）配对进数据，免得跨线程读非 volatile 的 Long。
            .map { parser.parse(it.raw) to transport.currentOffset } // 解析在 Default，历史回放不卡主线程
            .flowOn(Dispatchers.Default)
            .onEach { (r, _) ->
                // offset 不在这里提交，要与基线一起提交（见 collect）。
                detector.turnEndUuid(r)?.let { current = it }
            }.debounce(TurnEndDebouncer.SETTLE_MS) // 历史回放密集，静默后结算一次；实时 turn-end 稀疏，各结算一次。与 α 同一个静默窗
            .collect { (_, off) ->
                // replaysHistory = true：首次见此 path 吞掉全部历史。
                settleRound(slotKey = path, path = path, uuid = current, kind = kind, replaysHistory = true)
                // offset 与基线同段提交，见 [offsetByPath]。
                offsetByPath[path] = off
            }
    }

    /**
     * 每个通知槽一个完成通知 id：多个活会话各占一个槽，同一会话的多次 turn-end 复用同一个 id（替换，不刷屏）。
     * 从 [DONE_NOTIF_ID] 往上分配。
     *
     * 注意：这个 map 只增不删，`size` 单调才保证 id 唯一；要加淘汰就得换成单调计数器，否则会重发一个仍在用的 id。
     * 两条路对同一会话用同一个 key：交接重叠期同一轮两条路都发时，后一条替换前一条。
     * 代价：α 先在 path 未知时发过一条（key=`sid:…`）、之后 path 到了再发，这条会话会占两个槽。
     */
    private val doneNotifIds = mutableMapOf<String, Int>()

    private fun doneNotifIdFor(slotKey: String): Int = doneNotifIds.getOrPut(slotKey) { DONE_NOTIF_ID + doneNotifIds.size }

    private fun notifyAgentDone(
        slotKey: String,
        path: String?,
        kind: AgentKind,
    ) {
        // 副文：Claude 从路径取项目编码目录名（…/projects/<enc>/<id>.jsonl）；Codex 路径按日期分区、没有 /projects/，
        // 用产品名。产品名问档案。
        val agentName = AgentProfile.of(kind).displayName
        // α 可能还不知道 path（null），也退回产品名。
        val project =
            path
                .orEmpty()
                .substringAfterLast("/projects/", "")
                .substringBefore('/')
                .ifBlank { agentName }
        val tap =
            PendingIntent.getActivity(
                this,
                1,
                Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
        val n =
            NotificationCompat
                .Builder(this, DONE_CHANNEL_ID)
                .setContentTitle("$agentName 完成一轮")
                .setContentText("$project · 点开查看")
                .setSmallIcon(android.R.drawable.stat_notify_chat)
                .setAutoCancel(true)
                .setContentIntent(tap)
                .build()
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(doneNotifIdFor(slotKey), n)
    }

    private fun notify(n: Int) =
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(NOTIF_ID, buildNotification(n))

    private fun buildNotification(n: Int): Notification {
        val tap =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
        return NotificationCompat
            .Builder(this, CHANNEL_ID)
            .setContentTitle("Aterm")
            .setContentText("$n 个活动 SSH 会话")
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setContentIntent(tap)
            .build()
    }

    private fun createChannel() {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.createNotificationChannel(NotificationChannel(CHANNEL_ID, "SSH 会话保活", NotificationManager.IMPORTANCE_LOW))
        // 完成通知渠道：DEFAULT 出声并进状态栏，不强弹 heads-up。
        nm.createNotificationChannel(NotificationChannel(DONE_CHANNEL_ID, "Claude 任务完成", NotificationManager.IMPORTANCE_DEFAULT))
    }

    private fun registerNetwork() {
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val cb =
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    manager.probeAndReconnectStale() // 网络恢复 → 重连僵死会话
                }
            }
        runCatching {
            cm.registerDefaultNetworkCallback(cb)
            netCallback = cb
        }
    }

    override fun onDestroy() {
        watchers.values.forEach { it.cancel() }
        watchers.clear()
        netCallback?.let { cb -> runCatching { (getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager).unregisterNetworkCallback(cb) } }
        scope.cancel()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "SshKeepAlive"
        private const val CHANNEL_ID = "ssh_keepalive"
        private const val NOTIF_ID = 1001
        private const val DONE_CHANNEL_ID = "claude_done"
        private const val DONE_NOTIF_ID = 1002 // 完成通知 id 的分配基址（见 doneNotifIdFor）
        private const val REDISCOVER_MS = 20_000L // 重新解析活动会话集的周期
    }
}

/**
 * 前台保活服务为什么停了，记成事实而不是文案。系统逼停（[SystemTimeout]、[StartRefused]）时，
 * 看到的只是「保活没了」，有了这个界面才能说出原因。
 * 顺序有意义：前两档是我们自己决定停的，后两档是系统逼停的。
 */
enum class KeepAliveStopReason {
    /** 没有活动会话，也没有在飞轮次。 */
    NoWork,

    /** 任务从最近任务里被划走了。 */
    TaskRemoved,

    /** Android 15 的 `dataSync` 额度（24 小时合计 6 小时）到点，系统调了 [SshKeepAliveService.onTimeout]。 */
    SystemTimeout,

    /** `startForeground` 被系统拒了（Android 12+ 后台起前台服务，或 Android 15+ 额度用尽），见 [SshKeepAliveService.onStartCommand]。 */
    StartRefused,
}

/**
 * 最近一次前台保活服务为什么停了，进程级。只在本进程内有效，不落盘，进程被杀后归 `null`。
 */
object KeepAliveStopFact {
    @Volatile
    private var _last: KeepAliveStopReason? = null

    /** 最近一次停的因由；`null` = 本进程内还没停过。 */
    val last: KeepAliveStopReason?
        get() = _last

    internal fun record(reason: KeepAliveStopReason) {
        _last = reason
    }

    /** 测试用：清回「没停过」。 */
    internal fun reset() {
        _last = null
    }
}

/**
 * 「该不该停」和「这个异常认不认」的唯一判决处（纯函数，无 Android 依赖）。
 * 判断住在 `Service` 的系统回调里，那些回调在 JVM 单测里碰不到，所以把会写错的那部分搬到这里。
 */
internal object KeepAlivePolicy {
    /**
     * 这个因由当下该不该停；返回 `null` = 不停。
     * 只有 [KeepAliveStopReason.NoWork] 看 `hasWork`（没活才停）；其余三档不看，照停：
     * 额度到点几秒内不停就崩，前台没起成留着也会被系统收走，划走任务就是用完了。
     * `when` 不带 `else`：加一档因由时编译不过，逼人决定它看不看 `hasWork`。
     */
    fun stopReasonFor(
        candidate: KeepAliveStopReason,
        hasWork: Boolean,
    ): KeepAliveStopReason? =
        when (candidate) {
            KeepAliveStopReason.NoWork -> candidate.takeIf { !hasWork }
            KeepAliveStopReason.SystemTimeout -> candidate
            KeepAliveStopReason.StartRefused -> candidate
            KeepAliveStopReason.TaskRemoved -> candidate
        }

    /**
     * `startForeground` 抛的 `IllegalStateException` 认不认成「系统不许起」。
     *
     * `IllegalStateException` 底下有：`ServiceStartNotAllowedException`（API 31）及其子类
     * `BackgroundServiceStartNotAllowedException`、`ForegroundServiceStartNotAllowedException`（31）、
     * `ForegroundServiceTypeException`（34，含 `Invalid…` / `Missing…`）；另有 `StartForegroundCalledOnStoppedServiceException`（34）。
     * 只认 `ForegroundServiceStartNotAllowedException`。类型异常说明清单里的型写错了，是构建期的错，必须在开发时崩；
     * 对已停服务调 `startForeground` 是生死竞态，吞了就永远查不到。两者原样上抛。
     *
     * 整族异常都是 API 31+，而 minSdk 是 26：类型判断关在 [Api31] 里，靠 `sdkInt` 短路不让它在低版本被加载。
     * 注意：JVM 单测里这个异常类是桩、造不出实例，「真异常认得出」只能在真机上验。
     */
    fun isStartRefusal(
        error: Throwable,
        sdkInt: Int,
    ): Boolean = startRefusalIsPossible(sdkInt) && Api31.isRefusal(error)

    /**
     * 「系统不许起」这族异常在这个系统上存不存在（API 31 才引入）。
     * 单独成一个函数，版本门才可测：单测手上只有普通 `IllegalStateException`，`Api31.isRefusal` 对它恒为 false，
     * 门写在 [isStartRefusal] 里的话摘掉它测试照样绿。
     */
    fun startRefusalIsPossible(sdkInt: Int): Boolean = sdkInt >= Build.VERSION_CODES.S

    /** API 31 才有的类型判断关在这里，低版本上不加载，免得 `NoClassDefFoundError`。 */
    @RequiresApi(Build.VERSION_CODES.S)
    private object Api31 {
        fun isRefusal(error: Throwable): Boolean = error is ForegroundServiceStartNotAllowedException
    }
}
