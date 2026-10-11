package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** 常驻流上的一帧 `turn_end`（`IPC-COMMANDS.md`「turn_end」：判定在后端适配层，这里只读）。 */
data class TurnEnd(
    val sid: String,
    /** 这一轮最后那条记录的 uuid。 */
    val uuid: String,
    /** 通知的标题（后端写好的字，照抄；叫这条会话什么由后端定）。 */
    val title: String,
    /** 通知的正文（后端写好的字，照抄）。 */
    val body: String,
) {
    companion object {
        const val KIND: String = "turn_end"

        /** 不是 `turn_end` 或缺格 ⇒ `null`。 */
        fun of(frame: Map<String, Any?>): TurnEnd? =
            if (frame.str("kind") != KIND) {
                null
            } else {
                decoding { TurnEnd(frame.str("session_id").need(), frame.str("uuid").need(), frame.str("title").need(), frame.str("body").need()) }
            }
    }
}

/**
 * 「一轮完成」：后端每见一条结束记录发一帧、自己不去重（一条消息按内容块拆成几条记录，同一轮会来几帧），
 * 折成一轮一条归客户端（`wire.rs` `TurnEnd` 头注）。这里只做这一折，不判哪条算结束。
 */
object TurnEnds {
    /** 同一会话静默这么久没再来帧 ⇒ 这一轮报出去。 */
    const val SETTLE_MS: Long = 1_200L

    /**
     * 按会话各折各的（一条流管所有会话，整条流套 `debounce` 会让一条压掉另一条）：
     * 静默满 [settleMs] 报最后那一帧；它的 uuid 和这条会话上次报过的一样 ⇒ 不再报。
     */
    fun rounds(
        frames: Flow<TurnEnd>,
        settleMs: Long = SETTLE_MS,
    ): Flow<TurnEnd> =
        channelFlow {
            val lock = Mutex()
            val timers = HashMap<String, Job>()
            val latest = HashMap<String, TurnEnd>()
            val reported = HashMap<String, String>()
            frames.collect { te ->
                lock.withLock {
                    latest[te.sid] = te
                    timers.remove(te.sid)?.cancel()
                    timers[te.sid] = launch { settle(te.sid, settleMs, lock, timers, latest, reported) }
                }
            }
        }

    private suspend fun ProducerScope<TurnEnd>.settle(
        sid: String,
        settleMs: Long,
        lock: Mutex,
        timers: MutableMap<String, Job>,
        latest: MutableMap<String, TurnEnd>,
        reported: MutableMap<String, String>,
    ) {
        delay(settleMs)
        val round =
            lock.withLock {
                timers.remove(sid)
                val last = latest.remove(sid) ?: return
                if (reported[sid] == last.uuid) return
                reported[sid] = last.uuid
                last
            }
        send(round)
    }
}
