package com.ccmonitor.mobile.core.claude.transport

/**
 * 总览通路 → [SessionSignals] 的桥。总览通路上的等待态投进总线之后，聊天屏也能看见，
 * 而谁都不用直连对方的通路。
 *
 * 两条到达路径都要接：后端连上时先同步全扫一遍，已经在等的会话只走 `session_added`
 * （带初始 status/waitingFor），不会再来 `session_status`。只接后者，「连上之前就在等」的会话
 * 永远看不见，而这是手机上最常见的形状（合上手机、过一小时回来、重连）。
 *
 * `Overflow`：丢掉的 `session_status` 永远补不回来。本桥的 [onFrame] 收到它就把手上全部等待态
 * 一起作废（还没按 `lost` 里的 sid 精确作废，那一套目前只在 `DaemonSessionSource` 的快照路上），
 * 宁可少拦一次，也不拿一条可能过期的 `waiting` 去挡上行。见 [OVERFLOW_LOST_UNPARSED]。
 */
object DaemonSignalBridge {
    /**
     * 一帧后端下行 → 往 [bus] 投的那一下。
     *
     * @param nowMs 本地观察到这一帧的时刻（不是它开始等的时刻，流上没有那个）。
     */
    fun onFrame(
        bus: SessionSignals,
        frame: JsonlFrame,
        nowMs: Long,
    ) {
        when (frame) {
            // 路径①：宣告时就带着的初始 status/activity/waitingFor；漏掉它，「连上之前就在等」永远看不见。
            is JsonlFrame.SessionAdded ->
                bus.publishWaiting(
                    sessionId = frame.sessionId,
                    status = frame.status,
                    waitingFor = frame.waitingFor,
                    source = SignalSource.DAEMON_SESSION_ADDED,
                    nowMs = nowMs,
                    // 这一帧上没有 status、只有 activity，不投这一格这条路径就恒哑。
                    activity = frame.activity,
                )

            // 路径②：状态变更帧。
            is JsonlFrame.SessionStatus ->
                bus.publishWaiting(
                    sessionId = frame.sessionId,
                    status = frame.status,
                    waitingFor = frame.waitingFor,
                    source = SignalSource.DAEMON_SESSION_STATUS,
                    nowMs = nowMs,
                    activity = frame.activity,
                )

            // 会话没了，连信号一起忘掉，别在总线上留一条永远不会更新的等待态。
            is JsonlFrame.SessionRemoved -> bus.forget(frame.sessionId)

            // 丢帧：全部作废（见类头注）。
            is JsonlFrame.Overflow ->
                bus.signals.value.keys
                    .forEach { bus.invalidateWaiting(it, SessionSignals.LOST_STATUS_FRAME) }

            else -> Unit
        }
    }

    /**
     * 一份合并后的快照 → 总线，给以 `State` 为食的消费方（总览面 VM）。
     *
     * 快照分不出这一格是从哪种帧来的 ⇒ 一律记 [SignalSource.DAEMON_SNAPSHOT]；要精确来源走 [onFrame]。
     * 快照是全量的：不在快照里的 sid 会被清掉等待态。
     */
    fun onSnapshot(
        bus: SessionSignals,
        state: DaemonSessionSource.State,
        nowMs: Long,
    ) {
        state.sessions.values.forEach { s ->
            bus.publishWaiting(
                sessionId = s.sessionId,
                status = s.status,
                waitingFor = s.waitingFor,
                source = SignalSource.DAEMON_SNAPSHOT,
                nowMs = nowMs,
                activity = s.activity,
            )
        }
        // 快照里没有的 sid：等待态作废（会话已经不在表里了）
        val live = state.sessions.keys
        bus.signals.value.keys
            .filterNot { it in live }
            .forEach { bus.invalidateWaiting(it, GONE_FROM_SNAPSHOT) }
    }

    /** [onSnapshot] 里「这个 sid 已经不在会话表里」的作废理由。 */
    const val GONE_FROM_SNAPSHOT = "这条对话已经不在 daemon 的会话表里"

    /**
     * 已知缺口：`Overflow.lost` 已经解析，但 [onFrame] 还没按 sid 作废，仍一刀切作废全部等待态。
     * 要接就把 `Overflow` 那一支改成读 `frame.lost` / `frame.lostTruncated`，
     * 形照 `DaemonSessionSource.invalidateLostStatus`（点了名的作废那几条 · 点不出名的全部作废）。
     */
    const val OVERFLOW_LOST_UNPARSED = "Overflow.lost 已解析，但本桥的 onFrame 还没按 sid 作废"
}
