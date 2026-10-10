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
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.ccmonitor.mobile.MainActivity
import com.ccmonitor.mobile.core.claude.link.TurnEnd
import com.ccmonitor.mobile.core.claude.link.TurnEnds
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.countsAsActive
import com.ccmonitor.mobile.core.ui.copy.copyText
import com.ccmonitor.mobile.link.HostBackend
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ui.chat.ChatController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.koin.android.ext.android.inject

/**
 * SSH 保活前台服务。会话连上时启动，挂一条常驻通知降低后台被杀的概率（部分 ROM 仍会杀，靠重连兜底）。
 * 没有活动会话也没有在飞轮次时自停；网络恢复时调 `probeAndReconnectStale()`。
 *
 * 连接期间，对每台连着的机器收它常驻流上的 `turn_end` 帧（[HostBackends]，与「对话」一屏同一条流）：
 * app 在后台（[AppForeground]）时，一轮完成就发一条本地通知。判哪条算一轮结束在后端，折成一轮一条在 [TurnEnds]。
 */
class SshKeepAliveService : Service() {
    private val manager: SshConnectionManager by inject() // 只用会话计数、通道工厂和探活重连，不管连接生死

    /** 断连接经网关：[onTaskRemoved] 走 [HostConnector.disconnectAll]。 */
    private val connector: HostConnector by inject()

    /** 每台一条常驻流；完成通知读它的 `turn_end`。 */
    private val backends: HostBackends by inject()

    /** 存活判据要问它有没有在飞轮次，见 [onCreate]。 */
    private val chatController: ChatController by inject()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var netCallback: ConnectivityManager.NetworkCallback? = null

    /** 每台主机一个完成通知的收听者，key 是连接 id（＝ hostId）。 */
    private val watchers = mutableMapOf<String, Job>()

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
                // 哪些主机起 watcher，用与保活计数同一个谓词 countsAsActive()（名义状态；流断了由 [HostBackends] 自己重接）。
                val toWatch =
                    map.entries
                        .filter { it.value.countsAsActive() }
                        .map { it.key }
                        .toSet()
                toWatch.forEach { id ->
                    if (watchers[id]?.isActive != true) watchers[id] = watchHost(id)
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

    // ─── 一轮完成的通知 ───

    /** 收一台的 `turn_end`：常驻流没接就接上（断了它自己重接），折成一轮一条，app 在后台才发。 */
    private fun watchHost(hostId: String): Job {
        val backend = backends.of(hostId)
        backend.ensure()
        return scope.launch {
            TurnEnds.rounds(backend.turnEnds).collect { round ->
                if (!AppForeground.isForeground) notifyRoundDone(hostId, round, backend)
            }
        }
    }

    /**
     * 每条会话一个通知 id：同一会话的多次完成复用同一个 id（替换，不刷屏）。从 [DONE_NOTIF_ID] 往上分配。
     * 注意：这个 map 只增不删，`size` 单调才保证 id 唯一。
     */
    private val doneNotifIds = mutableMapOf<String, Int>()

    private fun doneNotifIdFor(slot: String): Int = doneNotifIds.getOrPut(slot) { DONE_NOTIF_ID + doneNotifIds.size }

    private fun notifyRoundDone(
        hostId: String,
        round: TurnEnd,
        backend: HostBackend,
    ) {
        // 标题里的 {tab}：会话名（流上宣告时带的），没有就用机器名。
        val tab =
            backend.table.value.sessions[round.sid]
                ?.name
                ?: manager.sessions.value[hostId]?.label
                ?: hostId
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
                .setContentTitle(copyText("turnNotify.finish.title", "tab" to tab))
                .setContentText(copyText("turnNotify.finish.body"))
                .setSmallIcon(android.R.drawable.stat_notify_chat)
                .setAutoCancel(true)
                .setContentIntent(tap)
                .build()
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(doneNotifIdFor("$hostId/${round.sid}"), n)
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
