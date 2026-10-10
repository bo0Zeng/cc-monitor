package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.ui.common.chatHolder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * 「这条对话跑在哪台主机上」的具名载体。
 *
 * Koin 的 `parametersOf` 按类型取，两个裸 `String` 一起传会静默错配。
 */
@JvmInline
value class HostId(
    val value: String,
)

/**
 * 「是哪个对话」的具名载体。
 *
 * 不用裸 `String`：`ParametersHolder.getOrNull` 游标优先、仍然位置敏感，裸字符串参数会静默错配。
 */
@JvmInline
value class ChatSessionKey(
    val value: String,
) {
    companion object {
        /** 没指定时的默认对话，debug 重放屏只有这一个对话。 */
        const val DEFAULT = "default"
    }
}

/**
 * Claude 自己的对话编号（`--session-id` 给的 uuid）的具名载体。
 *
 * 与 [ChatSessionKey] 不同：那个是本进程里的 `<hostId>/<sid>`，这个是 daemon 与 pidfile 都认的 sid。
 * 跨通路信号汇按它索引，拿错了信号恒为 null 且不报错。
 */
@JvmInline
value class ClaudeSessionId(
    val value: String,
)

/**
 * 应用级的对话持有者（单例，见 `di/AppModule.kt`）。
 *
 * 下行活在这里而不在 ViewModel 里：离开聊天屏不断流，离开 app 后回复完成照样能推送。
 * 应用级的东西不会自己消失，所以有 [release]，并由 [MAX_SESSIONS] 兜上限：
 * 超出时逐出最旧且不在飞的那个，逐出会通报，见 [evictedCount]。
 */
class ChatController(
    private val scopeFactory: () -> CoroutineScope = {
        CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    },
    /**
     * 逐出通报口，生产侧在 `AppModule` 里接日志。
     *
     * 做成回调而非直接调 `android.util.Log`：JVM 单测下没开 `returnDefaultValues`，直接调 `Log` 会抛「not mocked」。
     */
    private val onEvicted: (key: String, total: Int) -> Unit = { _, _ -> },
    /**
     * 对话建起来就 `retain` 那条连接，收掉时 `release`。
     * 只要这两个动作，所以收窄接口而不是整个 `SshConnectionManager`。
     */
    private val connections: ConnectionHolder? = null,
    /**
     * 跨通路信号汇，null 表示不接（debug 重放屏）。
     *
     * 生产侧是进程级单例：总览通路往里投等待态，聊天通路往里投配额与「被挡住了」，两个屏都从它取。
     */
    private val signals: SessionSignals? = null,
) {
    /** key → 它持有的 hostId。`release` 时要按这张表把持有还回去。 */
    private val heldHosts = mutableMapOf<String, String>()
    private val sessions = LinkedHashMap<String, ChatSession>()

    /** 每个 session 一个盯着它 state 的协程，用来刷新 [hasInFlightTurn]；与 [sessions] 同生共死。 */
    private val watchers = mutableMapOf<String, Job>()

    /** 跑 [watchers] 的作用域，与各 session 的 scope 分开：收掉一个对话不影响别的。 */
    private val ownScope: CoroutineScope by lazy { scopeFactory() }

    private val _hasInFlightTurn = MutableStateFlow(false)

    /**
     * 此刻有没有对话还在生成；前台保活服务据此决定存活。
     *
     * 惰性连接下没有活动会话是常态，所以不能按会话数判。
     * 判据用 [ChatUiState.streaming] 而非 [ChatUiState.turn]：后者是协议事实，没收到 `res` 就一直停在 `Streaming`，
     * 下行断了也不翻，拿它判存活会永远不放手。
     */
    val hasInFlightTurn: StateFlow<Boolean> = _hasInFlightTurn.asStateFlow()

    /** 被上限逐出的对话数。 */
    var evictedCount: Int = 0
        private set

    /**
     * 取这个对话的 [ChatSession]，幂等：同一个 key 永远拿到同一个实例，离屏再回来下行没断过。
     *
     * 连接持有挂在对话上，不挂在屏幕上：否则终端 tab 关光时持有归零，连接被回收，连带杀掉 bridge，
     * 而那条管道还在远端跑着。
     *
     * @param key 对话标识，不同屏幕、不同 VM 实例之间必须一致。
     * @param uplink 上行出口，只在首次创建时生效。
     * @param hostId 这条对话跑在哪台主机上；给了就 `retain`。
     */
    fun sessionFor(
        key: String,
        uplink: UplinkSink? = null,
        hostId: String? = null,
        claudeSessionId: String? = null,
    ): ChatSession {
        sessions[key]?.let { return it }
        evictIfNeeded()
        val created = ChatSession(uplink, scopeFactory())
        // 聊天屏不直连总览通路，两边只经信号汇交换。键用 Claude 的 sid 而非本类的 key，daemon 那侧认的是 sid。
        val bus = signals
        if (bus != null && claudeSessionId != null) created.attachSignals(bus, claudeSessionId)
        // 对话建起来就持有那条连接，直到 [release] 才放手。
        hostId?.let {
            connections?.retain(it, chatHolder(key))
            heldHosts[key] = it
        }
        sessions[key] = created
        watchers[key] = ownScope.launch { created.state.collect { refreshInFlight() } }
        return created
    }

    /**
     * 结束一个对话并回收它。屏幕关掉不算结束，只有用户离开这个对话才调。
     *
     * @return 是否真的收掉了；false 表示本来就没有。
     */
    fun release(key: String): Boolean {
        watchers.remove(key)?.cancel()
        val s = sessions.remove(key) ?: return false
        heldHosts.remove(key)?.let { connections?.release(it, chatHolder(key)) }
        s.close()
        refreshInFlight()
        return true
    }

    /** 全部收掉（应用退出 / 断开全部）。 */
    fun releaseAll() {
        watchers.values.forEach { it.cancel() }
        watchers.clear()
        heldHosts.forEach { (k, host) -> connections?.release(host, chatHolder(k)) }
        heldHosts.clear()
        sessions.values.forEach { it.close() }
        sessions.clear()
        refreshInFlight()
    }

    /**
     * 一次算清全部 session，重算 [hasInFlightTurn]。
     *
     * 每个 session 的 state 一变就由 [watchers] 调到这里，否则标志会过期。
     */
    private fun refreshInFlight() {
        _hasInFlightTurn.value = sessions.values.any { it.state.value.streaming }
    }

    /**
     * 上限逐出，在飞的绝不逐。
     *
     * 全都在飞时宁可超上限：掐掉一个正在生成的对话比多占一点内存糟得多。
     */
    private fun evictIfNeeded() {
        if (sessions.size < MAX_SESSIONS) return
        val victim = sessions.entries.firstOrNull { !it.value.state.value.streaming } ?: return
        watchers.remove(victim.key)?.cancel()
        heldHosts.remove(victim.key)?.let { connections?.release(it, chatHolder(victim.key)) }
        sessions.remove(victim.key)?.close()
        evictedCount++
        // 逐出必须通报，否则「对话怎么没了」无从查起。
        onEvicted(victim.key, evictedCount)
    }

    companion object {
        /** 同时持有的对话数上限，与阅读面的 `TAIL_CACHE_CAP` 同量级。 */
        const val MAX_SESSIONS = 8
    }
}

/**
 * 对话持有连接所需的两个动作。
 *
 * 生产实现是 `SshConnectionManager::retain` / `::release`（见 `di/AppModule.kt`）；
 * 做成接口让对话持有与连接编排分开。
 */
interface ConnectionHolder {
    fun retain(
        hostId: String,
        holder: String,
    )

    fun release(
        hostId: String,
        holder: String,
    )
}
