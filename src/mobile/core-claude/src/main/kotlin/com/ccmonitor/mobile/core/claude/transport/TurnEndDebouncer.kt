package com.ccmonitor.mobile.core.claude.transport

/**
 * 把 α 的 [JsonlFrame.TurnEnd] 帧折成「一轮结束」事件，供前台服务发完成通知。
 * 配合它的是 [TurnEndRoute]（配住址）与 [DaemonTurnEndSource]（起流 + 降级）。
 *
 * 约束：
 * 1. 去抖必须保留，而且按会话各记一份。后端每见一条 turn-end 记录就发一帧、自己不去重；
 *    Claude Code 把一条消息按内容块拆成多条记录，共享同一 `stop_reason` 而 uuid 各不相同，
 *    所以同一轮会来多帧，光按 uuid 挡不住，去抖才吃得掉。α 是一条流管所有会话，直接套 `Flow.debounce`
 *    会让 A 会话的帧压掉 B 会话的。
 * 2. uuid 去重也要保留：重连 / 重读时同一轮会再来一遍，靠 [Settled.uuid] 与上次结算值比。
 * 3. 起流必须带 `--tail-only`：不带的话后端会把历史里每一条 turn-end 都发成帧，一个大会话当场灌一串通知。
 *
 * 不做：offset / 截断检测 / 僵尸 tail（β 读原文才需要）；`sid → path` 映射（[JsonlFrame.TurnEnd] 没有 path，
 * 调用方从 [JsonlFrame.SessionAdded.path] 建，那一格在会话还没写首行时为 null）；判前后台；计时
 * （[settle] 由调用方喂 `nowMs`）。
 */
class TurnEndDebouncer(
    private val settleMs: Long = SETTLE_MS,
) {
    /** 一个会话上「还没结算」的最后一帧。 */
    private data class Pending(
        val uuid: String,
        val lastSeenMs: Long,
    )

    /** 已结算过的轮次（uuid 去重用）。 */
    data class Settled(
        val sessionId: String,
        val uuid: String,
    )

    private val pending = mutableMapOf<String, Pending>()
    private val lastSettled = mutableMapOf<String, String>()

    /** 收到一帧。同一会话在 [settleMs] 内再来 ⇒ 只更新待结算值（吃掉「同一轮多帧」的那一步）。 */
    fun onFrame(
        frame: JsonlFrame.TurnEnd,
        nowMs: Long,
    ) {
        pending[frame.sessionId] = Pending(frame.uuid, nowMs)
    }

    /**
     * 结算：返回静默已满 [settleMs] 且 uuid 与上次结算值不同的会话，按 sid 排序（结果可复现）。
     */
    fun settle(nowMs: Long): List<Settled> {
        val ripe = pending.filterValues { nowMs - it.lastSeenMs >= settleMs }
        if (ripe.isEmpty()) return emptyList()
        val out = ArrayList<Settled>(ripe.size)
        ripe.keys.sorted().forEach { sid ->
            val uuid = ripe.getValue(sid).uuid
            pending.remove(sid)
            if (lastSettled[sid] != uuid) {
                lastSettled[sid] = uuid
                out += Settled(sid, uuid)
            }
        }
        return out
    }

    /**
     * 会话没了（[JsonlFrame.SessionRemoved]）⇒ 两份状态都清掉。
     *
     * 清掉 [lastSettled] 意味着同一个 sid 再出现时，它的上一轮会被再通知一次。这是故意的：
     * 中间可能换了文件（`/compact` 原地重写），拿旧 uuid 去重会把真的新一轮当成重复而漏掉。宁多勿少。
     */
    fun forget(sessionId: String) {
        pending.remove(sessionId)
        lastSettled.remove(sessionId)
    }

    /** 此刻有几个会话待结算。 */
    fun pendingCount(): Int = pending.size

    companion object {
        /**
         * 静默多久算一轮结束。α 与 β 两条路共用这一个常量，切换前后的延迟才一致。
         */
        const val SETTLE_MS: Long = 1_200L
    }
}
