package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.NeedsFace
import com.ccmonitor.mobile.core.claude.link.seen
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.NeedsMerged
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
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 一台常驻流：问答那一条 ＋ 它推来的 `changed` 帧 ＋ 它在核心里那条流的编号（逐台问一圈时交核心）。 */
class NeedsSource(
    val link: CoreLink,
    val pushes: Flow<Changed>,
    val handle: suspend () -> Long,
)

/**
 * 板此刻：板上有哪几台（起着的常驻流）· 最近那一圈核心合好的那一份（还没问完过一圈 ⇒ `null`）· 那一圈问了哪几台 · 此刻流断着的那几台 ·
 * 收到那一份那一刻（本机单调钟）。
 */
data class NeedsNow(
    val on: Set<String> = emptySet(),
    val merged: NeedsMerged? = null,
    val asked: Set<String> = emptySet(),
    val down: Set<String> = emptySet(),
    /** 收到 [merged] 那一刻：会走的那一句从它起接着走。 */
    val at: Long = 0L,
) {
    /** 还在读：板上有它、通着、还没在哪一圈里问过它。 */
    fun reading(id: String): Boolean = id in on && id !in down && id !in asked

    /** 答了：那一圈问了它、核心合好的那一份里它不在没答上那几台里。 */
    fun answered(id: String): Boolean = id in asked && merged?.down?.none { it.origin == id } == true
}

/**
 * 跨会话的需手动板（进程级一份）：对此刻起着的每条常驻流（[sources]，不为它另连）问一圈 `sessions-needs`、合成一份 —— 逐台问与合都在核心
 * （[NeedsFace]，`needs_all`），这里一圈只问核心一次。什么时候再问一圈（重问节奏归出口，ARCHITECTURE §3.9）：来了一条流 · 哪台换了一条流 ·
 * 哪台推 `changed {needs}`（那台的需手动只这一个铃；去抖合成一次）· 那一圈有台没答上之后退避再问（[RETRY_FIRST_MS] 起、翻倍、封顶；都答上了归零）；
 * 别处答完 / 登完也可以叫它重问（[reask]）。会话进出需手动由那台照它的活会话表推 `needs`，这里不看点。
 * 总览照它画、聊天屏右上那颗入口数它、聊天屏那一块需手动读它里面那一条 —— 别处都不再自己问。判 · 排 · 写字全在那台与核心；这里只记。
 */
@OptIn(FlowPreview::class)
class NeedsBoard(
    sources: Flow<Map<String, NeedsSource>>,
    private val scope: CoroutineScope,
    /** 交核心问一圈的那一口（生产上 `CoreResident.needsAll`）：[NeedsFace.input] 那一行 ⇒ 合好的那一行。 */
    private val ask: suspend (String) -> String,
    /** 本机单调钟（记收到那一份的时刻）。 */
    private val now: () -> Long = ::monoMs,
) {
    private val _state = MutableStateFlow(NeedsNow())

    val state: StateFlow<NeedsNow> = _state.asStateFlow()

    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val watched = java.util.concurrent.ConcurrentHashMap<String, Job>()

    @Volatile private var current: Map<String, NeedsSource> = emptyMap()

    init {
        scope.launch {
            sources.collect { srcs ->
                current = srcs
                _state.update { it.copy(on = srcs.keys) }
                // 收了的那几条：不再盯；合好的那一份里还有它的项 ⇒ 再问一圈。
                val gone = watched.keys - srcs.keys
                for (id in gone) watched.remove(id)?.cancel()
                if (gone.isNotEmpty()) wake.trySend(Unit)
                for ((id, src) in srcs) if (!watched.containsKey(id)) watched[id] = watch(id, src)
            }
        }
        scope.launch {
            var backoff = 0L
            var retry: Job? = null
            wake.receiveAsFlow().debounce(DEBOUNCE_MS).collect {
                retry?.cancel()
                if (round()) {
                    backoff = 0L
                } else {
                    backoff = if (backoff == 0L) RETRY_FIRST_MS else minOf(backoff * 2, RETRY_MAX_MS)
                    val wait = backoff
                    retry =
                        scope.launch {
                            delay(wait)
                            wake.trySend(Unit)
                        }
                }
            }
        }
    }

    /** 再问一圈（去抖照常）。 */
    fun reask() {
        wake.trySend(Unit)
    }

    /** 盯一台：通没通记进板；那台推 `needs` · 换了一条流 ⇒ 叫一圈。 */
    private fun watch(
        id: String,
        src: NeedsSource,
    ): Job =
        scope.launch {
            launch {
                src.link.state.collect { line ->
                    _state.update { s -> s.copy(down = if (line is LinkLine.Up) s.down - id else s.down + id) }
                }
            }
            launch {
                merge(
                    src.pushes.filter { it is Changed.Needs }.map { },
                    src.link.seen,
                ).collect { wake.trySend(Unit) }
            }
            wake.trySend(Unit)
        }

    /** 问核心一圈：换上合好的那一份；有台没答上 ⇒ `false`（退避再问）。 */
    private suspend fun round(): Boolean {
        val srcs = current
        val hosts = srcs.map { (id, s) -> NeedsFace.Host(s.handle(), id) }
        val merged = NeedsFace.of(ask(NeedsFace.input(hosts, READ_MS)))
        _state.update { it.copy(merged = merged, asked = srcs.keys, at = now()) }
        return merged.down.isEmpty()
    }

    companion object {
        /** 推送 · 会话帧连着来时合成一次重问。 */
        const val DEBOUNCE_MS = 300L

        /** 每台等多久（等它接上的那一段也算在里面）。 */
        private const val READ_MS = 15_000L

        /** 有台没答上之后头一回隔多久再问一圈；之后每回翻倍、封顶 [RETRY_MAX_MS]。 */
        const val RETRY_FIRST_MS = 1_000L
        private const val RETRY_MAX_MS = 30_000L
    }
}
