package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.NeedsList
import com.ccmonitor.mobile.ui.chat.needs.monoMs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 一台常驻流：问答那一条 ＋ 它推来的 `changed` 帧。 */
class NeedsSource(
    val link: CoreLink,
    val pushes: Flow<Changed>,
)

/**
 * 板上一台此刻：那台最后一份整份（还没读到过 ⇒ `null`）· 流断着没有 · 最近那一问没问到的结局（问到了 ⇒ `null`）。断着的那台不进数。
 */
data class MachineNeeds(
    val list: NeedsList? = null,
    val down: Boolean = false,
    val failed: Reply? = null,
    /** 最近那一问没问到时那条链上的离线那一句（[CoreLink.offline]，[failed] 是断着时照摆它）。 */
    val offline: String = "",
    /** 收到 [list] 那一刻（本机单调钟）：会走的那一句从它起接着走。 */
    val at: Long = 0L,
) {
    /** 进数的那几项（断着 ⇒ 不进）。 */
    val counted: Int get() = if (down) 0 else list?.items?.size ?: 0

    /** 会外那一截读不出（那台那一句）；断着不说。 */
    val unreadable: String? get() = list?.manualUnreadable?.takeIf { !down }

    /** 整份没问到（那台报错 · 回包解不开 · 无应答）；断着不说（断着那一句另有）。 */
    val unread: Reply? get() = failed?.takeIf { !down }

    /** 还在读：通着、还没问到过、也还没没问到过。 */
    val reading: Boolean get() = !down && list == null && failed == null
}

/**
 * 跨会话的需手动板（进程级一份）：对此刻起着的每条常驻流（[sources]，不为它另连）各持那台最后一份 `sessions-needs` 整份。
 * 什么时候问那一台：那条流一挂上来 · 换了一条流 · 那台推 `changed {needs}`（那台的需手动只这一个铃；去抖合成一次）· 没问到之后退避再问
 * （[RETRY_FIRST_MS] 起、翻倍、封顶；问到了归零）；别处答完 / 登完也可以叫它重问（[reask]）。会话进出需手动由那台照它的活会话表推 `needs`，这里不看点。总览照它画、聊天屏右上那颗入口数它、聊天屏那一块需手动读它里面那一条 —— 别处都不再自己问。
 * 判 · 排 · 写字全在那台；这里只记、只把几台的条数相加。
 */
@OptIn(FlowPreview::class)
class NeedsBoard(
    sources: Flow<Map<String, NeedsSource>>,
    private val scope: CoroutineScope,
    /** 本机单调钟（记收到那一份的时刻）。 */
    private val now: () -> Long = ::monoMs,
) {
    private val _state = MutableStateFlow<Map<String, MachineNeeds>>(emptyMap())

    /** 按台（键 ＝ 那台的 id）。 */
    val state: StateFlow<Map<String, MachineNeeds>> = _state.asStateFlow()

    private val wakes = java.util.concurrent.ConcurrentHashMap<String, Channel<Unit>>()
    private val jobs = java.util.concurrent.ConcurrentHashMap<String, Job>()

    init {
        scope.launch {
            sources.collect { srcs ->
                for ((id, src) in srcs) if (!jobs.containsKey(id)) jobs[id] = watch(id, src)
            }
        }
    }

    /** 各台加起来几项（断着的不算）。 */
    fun total(all: Map<String, MachineNeeds> = _state.value): Int = all.values.sumOf { it.counted }

    /** 那一台重问一次（去抖照常）。 */
    fun reask(hostId: String) {
        wakes[hostId]?.trySend(Unit)
    }

    private fun watch(
        id: String,
        src: NeedsSource,
    ): Job {
        val wake = Channel<Unit>(Channel.CONFLATED).also { wakes[id] = it }
        wake.trySend(Unit)
        return scope.launch {
            launch {
                src.link.state
                    .map { it is LinkLine.Up }
                    .distinctUntilChanged()
                    .collect { up -> put(id) { it.copy(down = !up) } }
            }
            launch {
                merge(
                    src.pushes.filter { it.wholeMayChange() }.map { },
                    src.link.epoch
                        .drop(1)
                        .map { },
                ).collect { wake.trySend(Unit) }
            }
            var backoff = 0L
            var retry: Job? = null
            wake.receiveAsFlow().debounce(DEBOUNCE_MS).collect {
                retry?.cancel()
                if (fetch(id, src.link)) {
                    backoff = 0L
                } else {
                    backoff = if (backoff == 0L) RETRY_FIRST_MS else minOf(backoff * 2, RETRY_MAX_MS)
                    val wait = backoff
                    retry =
                        launch {
                            delay(wait)
                            wake.trySend(Unit)
                        }
                }
            }
        }
    }

    /** 问那一台一次：问到了 ⇒ 换上、`true`；读不出 ⇒ 旧的一份照留、记下那一问的结局、`false`（退避再问）。 */
    private suspend fun fetch(
        id: String,
        link: CoreLink,
    ): Boolean {
        val r = link.call(SessionsNeeds.COMMAND, null, READ_MS)
        val list = (r as? Reply.Ok)?.let { SessionsNeeds.list(it.data) }
        if (list == null) {
            put(id) { it.copy(failed = if (r is Reply.Ok) Reply.Unreadable(r.data.toString()) else r, offline = link.offline.value) }
            return false
        }
        put(id) { it.copy(list = list, failed = null, at = now()) }
        return true
    }

    private fun put(
        id: String,
        f: (MachineNeeds) -> MachineNeeds,
    ) = _state.update { it + (id to f(it[id] ?: MachineNeeds())) }

    companion object {
        /** 推送 · 会话帧连着来时合成一次重问。 */
        const val DEBOUNCE_MS = 300L
        private const val READ_MS = 15_000L

        /** 没问到之后头一回隔多久再问；之后每回翻倍、封顶 [RETRY_MAX_MS]。 */
        const val RETRY_FIRST_MS = 1_000L
        private const val RETRY_MAX_MS = 30_000L

        /** 那台这几样变了 ⇒ 整份可能变了。 */
        private fun Changed.wholeMayChange(): Boolean = this is Changed.Needs
    }
}
