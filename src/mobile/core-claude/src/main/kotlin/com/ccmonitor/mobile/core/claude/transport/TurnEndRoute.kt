package com.ccmonitor.mobile.core.claude.transport

/**
 * α 的帧 → 「哪条会话、哪个文件完成了一轮」。
 *
 * [TurnEndDebouncer] 答「这些帧折成几个事件」，本类答「事件该投到哪个通知槽」：[JsonlFrame.TurnEnd]
 * 只有 `{sessionId, uuid}`、没有 path，而通知槽按 path 分，所以 `sid → path` 从 [JsonlFrame.SessionAdded.path] 攒。
 *
 * 时钟由调用方喂。本类自己记一份 `lastSeenMs`（[msUntilNextRipe]），只用来排下一次什么时候问，
 * 不参与判决（判决全在 [TurnEndDebouncer]）。这份镜像算错了最坏是早问一次或晚问一点，
 * 不会多出或少出事件。别把判决搬进来。
 *
 * path 可以是 null（会话刚起还没写首行时），这时 [Done.slotKey] 退回 `sid:<sid>`：宁可投到一个偏僻的槽，
 * 也不因为不知道 path 就不通知。
 *
 * 不做：起流（[DaemonTurnEndSource]）；判 α 该不该用（`TurnEndPathArbiter`）；会话被移除时抢着结算
 * 未结算的轮次（[JsonlFrame.SessionRemoved] 一到就 [TurnEndDebouncer.forget]，与 β 撤 tail 时的行为一致；
 * 要改得给去抖器开一个按 sid 的结算口，全局早结算会让别的会话同一轮结算两次）。
 */
class TurnEndRoute(
    private val settleMs: Long = TurnEndDebouncer.SETTLE_MS,
) {
    private val debouncer = TurnEndDebouncer(settleMs)

    /** 一条会话「住在哪、是哪一家」—— 从 [JsonlFrame.SessionAdded] 攒的。 */
    private data class Where(
        val path: String?,
        val agentKindWire: String?,
    )

    private val whereBySid = mutableMapOf<String, Where>()

    /** [msUntilNextRipe] 用的调度镜像（不参与判决）。 */
    private val lastSeenBySid = mutableMapOf<String, Long>()

    /**
     * 一轮结算了。
     *
     * @property path 这条会话的 jsonl 路径；null = α 还没说过。
     * @property agentKindWire wire 上的原始串（`"claude"` / `"codex"` / null = 缺）。原样传，不在这里折成
     *   [com.ccmonitor.mobile.core.claude.model.AgentKind]：调用方要分辨「没说」与「说了 claude」，前者退回主机配的那一家。
     */
    data class Done(
        val sessionId: String,
        val uuid: String,
        val path: String?,
        val agentKindWire: String?,
    ) {
        /**
         * 通知槽的 key。
         *
         * path 认得出就用 path：β 的槽也按 path 分，两条路用同一个 key，切换那一下两条路都发了
         * 也只是一次看不见的替换，而不是两条通知。
         */
        val slotKey: String get() = path ?: SID_SLOT_PREFIX + sessionId
    }

    /**
     * 收一帧。认得的三种：
     * - [JsonlFrame.TurnEnd] → 喂去抖器
     * - [JsonlFrame.SessionAdded] → 更新 `sid → path/kind`（并入，不整条重建：同一个 sid 会收到不止一条 added，
     *   第二条可能省了 path）
     * - [JsonlFrame.SessionRemoved] → 两份状态都清掉
     *
     * 其余帧（`line` / `session_status` / `overflow` / `hello` / 未知）原样跳过：本类只管轮次与住址。
     */
    fun onFrame(
        frame: JsonlFrame,
        nowMs: Long,
    ) {
        when (frame) {
            is JsonlFrame.TurnEnd -> {
                debouncer.onFrame(frame, nowMs)
                lastSeenBySid[frame.sessionId] = nowMs
            }

            is JsonlFrame.SessionAdded -> {
                val prev = whereBySid[frame.sessionId]
                whereBySid[frame.sessionId] =
                    Where(
                        path = frame.path ?: prev?.path,
                        agentKindWire = frame.agentKind ?: prev?.agentKindWire,
                    )
            }

            is JsonlFrame.SessionRemoved -> {
                debouncer.forget(frame.sessionId)
                whereBySid.remove(frame.sessionId)
                lastSeenBySid.remove(frame.sessionId)
            }

            else -> Unit
        }
    }

    /** 结算：把 [TurnEndDebouncer.Settled] 配上住址。顺序随去抖器（按 sid 排序）。 */
    fun settle(nowMs: Long): List<Done> {
        // 先按与去抖器同一条成熟判据修剪调度镜像，再问去抖器。[TurnEndDebouncer.settle] 会摘掉所有成熟的，
        // 哪怕因 uuid 重复而不返回；镜像只按返回值清的话，那个 sid 会永远留着 ⇒ [msUntilNextRipe] 恒 0
        // ⇒ 调用方空转烧 CPU，而通知一条不少，很难查。
        lastSeenBySid.entries.removeAll { nowMs - it.value >= settleMs }
        return debouncer.settle(nowMs).map { s ->
            val where = whereBySid[s.sessionId]
            Done(s.sessionId, s.uuid, where?.path, where?.agentKindWire)
        }
    }

    /**
     * α 这条流要死了，把还没结算的全倒出来。
     *
     * 不倒会漏通知：α 死掉 ⇒ 回落 β ⇒ β 第一轮吞历史（没有这个 path 的基线）⇒ 静默窗口里那一轮两条路都不发。
     * 倒出来也不会多发：调用方用它播种 β 的去重基线，β 读到同一个 uuid 就不发了。
     */
    fun flushAll(nowMs: Long): List<Done> = settle(nowMs + settleMs)

    /**
     * 距离**最早**一个待结算会话成熟还有多久；null = 没有待结算的。
     *
     * 调用方拿它当下一次该等多久。取各会话的最小值，不是「最后一帧之后再等一个窗口」：
     * 后者会让一条话多的会话把另一条安静会话的窗口无限续上。
     */
    fun msUntilNextRipe(nowMs: Long): Long? {
        val earliest = lastSeenBySid.values.minOrNull() ?: return null
        return (earliest + settleMs - nowMs).coerceAtLeast(0L)
    }

    /** 此刻记得几条会话的住址。 */
    fun knownSessionCount(): Int = whereBySid.size

    companion object {
        /** path 认不出时 [Done.slotKey] 的前缀。它不是路径。 */
        const val SID_SLOT_PREFIX: String = "sid:"
    }
}
