package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
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

/** 板上一台此刻：那台最后一份整份（还没读到过 ⇒ `null`）· 流断着没有。断着的那台不进数。 */
data class MachineNeeds(
    val list: NeedsList? = null,
    val down: Boolean = false,
) {
    /** 进数的那几项（断着 ⇒ 不进）。 */
    val counted: Int get() = if (down) 0 else list?.items?.size ?: 0

    /** 会外那一截读不出（那台那一句）；断着不说。 */
    val unreadable: String? get() = list?.manualUnreadable?.takeIf { !down }
}

/**
 * 跨会话的需手动板（进程级一份）：对此刻起着的每条常驻流（[sources]，不为它另连）各持那台最后一份 `sessions-needs` 整份。
 * 什么时候问那一台：那条流一挂上来 · 换了一条流 · 那台推 `changed {needs | plan | manual}` · 那台会话表里谁的点变了（去抖合成一次）；
 * 别处答完 / 登完也可以叫它重问（[reask]）。总览照它画、聊天屏右上那颗入口数它、聊天屏那一块需手动读它里面那一条 —— 别处都不再自己问。
 * 判 · 排 · 写字全在那台；这里只记、只把几台的条数相加。
 */
@OptIn(FlowPreview::class)
class NeedsBoard(
    sources: Flow<Map<String, NeedsSource>>,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow<Map<String, MachineNeeds>>(emptyMap())

    /** 按台（键 ＝ 那台的 id）。 */
    val state: StateFlow<Map<String, MachineNeeds>> = _state.asStateFlow()

    private val wakes = java.util.concurrent.ConcurrentHashMap<String, Channel<Unit>>()
    private val jobs = java.util.concurrent.ConcurrentHashMap<String, Job>()

    init {
        scope.launch {
            sources.collect { now ->
                for ((id, src) in now) if (!jobs.containsKey(id)) jobs[id] = watch(id, src)
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
                    .map { it is LinkState.Up }
                    .distinctUntilChanged()
                    .collect { up -> put(id) { it.copy(down = !up) } }
            }
            launch {
                merge(
                    src.pushes.filter { it.topic in TOPICS }.map { },
                    src.link.table
                        .map { t -> t.faces.mapValues { it.value.dot } }
                        .distinctUntilChanged()
                        .drop(1)
                        .map { },
                    src.link.epoch
                        .drop(1)
                        .map { },
                ).collect { wake.trySend(Unit) }
            }
            wake.receiveAsFlow().debounce(DEBOUNCE_MS).collect { fetch(id, src.link) }
        }
    }

    private suspend fun fetch(
        id: String,
        link: CoreLink,
    ) {
        val r = link.call(SessionsNeeds.COMMAND, null, READ_MS)
        // 读不出 ⇒ 旧的一份照留（下一次变化再问）。
        val list = (r as? Reply.Ok)?.let { SessionsNeeds.list(it.data) } ?: return
        put(id) { it.copy(list = list) }
    }

    private fun put(
        id: String,
        f: (MachineNeeds) -> MachineNeeds,
    ) = _state.update { it + (id to f(it[id] ?: MachineNeeds())) }

    companion object {
        /** 推送 · 会话帧连着来时合成一次重问。 */
        const val DEBOUNCE_MS = 300L
        private const val READ_MS = 15_000L

        /** 那台这几样变了 ⇒ 整份可能变了。 */
        private val TOPICS = setOf("needs", "plan", "manual")
    }
}
