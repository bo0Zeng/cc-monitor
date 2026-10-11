package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicBoolean

/** 一条会话「此刻有人在看」：看着的那一处拿一个 [hold]，不看了关掉；刚进名单那一格的 `from` 经 [from] 来。 */
interface SessionWatch {
    val from: SharedFlow<WatchFrom>

    fun hold(sid: String): AutoCloseable
}

/**
 * 一台的 `stream-watch` 名单（`IPC-PROTOCOL.md`「流只发要看的会话」）：此刻在屏上的那几条会话的并集。
 * 每次接上（[CoreLink.epoch] 变了 ＝ 换了一条新连接，新连接 ＝ 没报过）先照当前名单报一次 —— 列表页上就是 `[]`；
 * 名单一变再报。应答的 `from[]` 原样从 [from] 发出去，由看着那一条的去补 `[已有, seq)`。
 * 报没成（这条断了）不重试：下一次接上照样整份重报。
 */
class WatchList(
    private val link: CoreLink,
    scope: CoroutineScope,
) : SessionWatch {
    private val held = MutableStateFlow<Map<String, Int>>(emptyMap())
    private val fromFlow = MutableSharedFlow<WatchFrom>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    override val from: SharedFlow<WatchFrom> = fromFlow.asSharedFlow()

    init {
        scope.launch {
            link.epoch.collectLatest { e ->
                if (e > 0) held.map { it.keys }.distinctUntilChanged().collect { report(it) }
            }
        }
    }

    override fun hold(sid: String): AutoCloseable {
        held.update { it + (sid to (it[sid] ?: 0) + 1) }
        val closed = AtomicBoolean(false)
        return AutoCloseable {
            if (closed.compareAndSet(false, true)) {
                held.update { m -> (m[sid] ?: 0).let { n -> if (n <= 1) m - sid else m + (sid to n - 1) } }
            }
        }
    }

    private suspend fun report(sids: Set<String>) {
        val r = link.call(StreamWatch.COMMAND, StreamWatch.args(sids))
        if (r is Reply.Ok) StreamWatch.of(r.data)?.forEach { fromFlow.emit(it) }
    }

    private companion object {
        const val BUFFER = 64
    }
}
