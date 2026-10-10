package com.ccmonitor.mobile.core.claude.transport

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * 跨通路信号汇：两条互不相通的数据通路之间的桥。
 *
 * | 通路 | 走什么 | 谁在消费 |
 * |---|---|---|
 * | 聊天通路 | tmux + `in.ndjson` / `events.ndjson` + 两个 `tail`，不经后台程序 | 聊天屏 |
 * | 总览通路 | cc-monitor 后端（[DaemonSessionSource]）| 总览面 |
 *
 * 想同时出现在两个屏上的信号（等待态、配额……）谁产就往这里投，谁要就从这里取，两个屏都不直连对方的通路。
 *
 * 等待态的产方只有总览通路：聊天屏那条无界面模式的 claude（`entrypoint: "sdk-cli"`）不往 pidfile 写
 * `status` / `waitingFor`，流上也没有 `control_request` / `can_use_tool` 下行，`system/status` 只说
 * `"requesting"`（在问 API，不是在等人）。聊天屏是纯消费方。
 *
 * 加下一个跨屏信号：
 * 1. 在 [SessionSignal] 上加一个新字段（带默认值）；
 * 2. 产方调一个 `publishXxx(sessionId, …, nowMs)`；
 * 3. 消方从 [signals] 按 sid 取，或用 [waitingGate] 那样的纯判定函数取结论。
 *
 * 注意：必须是新字段。本类没有来源仲裁，[publishWaiting] 最后写的人赢；让第二条通路也往 `waiting`
 * 这一格投，会把第一条通路的真读数清掉而不报错。要两个产方喂同一格，先做来源仲裁。
 *
 * 本类不持有时钟：每个入口都收 `nowMs`，测试推时钟而不是 sleep。
 *
 * 时刻是我们看到它的时刻，不是它开始等的时刻（流上没有后者）。只能说「看到它在等，{t} 前」，
 * 不能说「等了 {t}」：app 没开着的那段会被算成 0。所以字段叫 [WaitingSignal.observedAtMs]。
 *
 * 线程：投递方都在主线程，[MutableStateFlow] 足够，不另加锁。
 */
class SessionSignals(
    /**
     * 等待态的新鲜度阈值：超过这么久没有新读数，拦就降级成「提示但不拦」。
     * 数值是占位，不是量出来的；有了真实等待时长的分布再改。
     */
    private val freshnessMs: Long = DEFAULT_FRESHNESS_MS,
) {
    private val _signals = MutableStateFlow<Map<String, SessionSignal>>(emptyMap())

    /** 全部对话的跨通路信号快照。两个屏都从这里取。 */
    val signals: StateFlow<Map<String, SessionSignal>> = _signals.asStateFlow()

    fun signalFor(sessionId: String): SessionSignal? = _signals.value[sessionId]

    /**
     * 投一条等待态读数。[status] == [STATUS_WAITING] 或 [activity] == [ACTIVITY_NEEDS_YOU] 任一成立就算在等。
     *
     * 两格都不说在等时清空这条 sid 的等待态，而不是什么都不做：「不再等了」和「没人说过在等」
     * 在屏幕上必须是同一个样子。新后端不发 [status]、只发 [activity]，只看 [status] 的话等待态每帧都被清掉。
     *
     * 注意：本方法只许有一个产方（总览通路），原因见类头注。
     *
     * @param status 上游 pidfile 的原始机器码（`waiting`/`busy`/`idle`/`shell`/…），原样收。新后端上恒 `null`。
     * @param waitingFor 等什么。旧版 CC 不写它，所以判定不看它。
     * @param activity 后端算好的活动档（wire 原串），带默认值排在最后，按位置传参的调用点不受影响。
     */
    fun publishWaiting(
        sessionId: String,
        status: String?,
        waitingFor: String?,
        source: SignalSource,
        nowMs: Long,
        activity: String? = null,
    ) {
        if (status != STATUS_WAITING && activity != ACTIVITY_NEEDS_YOU) {
            mutate(sessionId) { it.copy(waiting = null) }
            return
        }
        mutate(sessionId) {
            it.copy(
                waiting =
                    WaitingSignal(
                        waitingFor = waitingFor,
                        observedAtMs = nowMs,
                        source = source,
                    ),
            )
        }
    }

    /**
     * 投一条配额读数（[com.ccmonitor.mobile.core.claude.bridge.BridgeFrame.RateLimit] 的 `raw`）。
     * 解不出来就不投（不覆盖上一条真读数）。
     */
    fun publishRateLimit(
        sessionId: String,
        raw: Map<String, Any?>?,
        nowMs: Long,
    ) {
        val reading = RateLimitReading.parse(raw, nowMs) ?: return
        mutate(sessionId) { it.copy(rateLimit = reading) }
    }

    /**
     * 投一条「它想做的事被挡住了」（下行 `system/permission_denied`）。
     *
     * 这不是等待态：等待态是停在那儿等人答，这条是已经被拒了、并且已经继续往下走了。
     * 混成一格的话，屏幕会在一件早已结束的事上拦住上行。
     *
     * 注意：这条帧在真实会话落盘里还没见到过，形状（`tool_name` / `message`）没有落盘样本支撑。
     * 它是加法的：帧不来就什么都不显示。
     */
    fun publishBlocked(
        sessionId: String,
        toolName: String?,
        humanText: String?,
        nowMs: Long,
    ) {
        mutate(sessionId) {
            it.copy(blocked = BlockedSignal(toolName = toolName, humanText = humanText, observedAtMs = nowMs))
        }
    }

    /**
     * 让这条 sid 的等待态立刻作废。用在两处：流上丢过 `session_status`（丢了补不回来，
     * 手上那个 `waiting` 可能是过期的最后一帧）；会话没了。
     *
     * @param why 作废的理由，出现在 [WaitingGate.degradedWhy] 里：拦与不拦的理由要读得出来，不只是一个布尔。
     */
    fun invalidateWaiting(
        sessionId: String,
        why: String,
    ) {
        mutate(sessionId) { it.copy(waiting = null, waitingInvalidatedWhy = why) }
    }

    /** 对话收掉了，连同它的全部信号一起忘掉（本类是进程级的，不忘就是泄漏）。 */
    fun forget(sessionId: String) {
        _signals.value = _signals.value - sessionId
    }

    /**
     * 拦不拦上行，以及不拦时的理由。纯判定，不改状态。
     *
     * 三条出口：
     * - 没有等待态 ⇒ [WaitingGate.signal] 为 null、不拦；
     * - 有等待态且够新 ⇒ 拦；
     * - 有等待态但太旧，或被 [invalidateWaiting] 作废过 ⇒ 不拦，[WaitingGate.degradedWhy] 说出为什么。
     */
    fun waitingGate(
        sessionId: String,
        nowMs: Long,
    ): WaitingGate {
        val entry = _signals.value[sessionId]
        val waiting = entry?.waiting
        if (waiting == null) {
            return WaitingGate(signal = null, blocks = false, degradedWhy = entry?.waitingInvalidatedWhy)
        }
        val ageMs = nowMs - waiting.observedAtMs
        if (ageMs > freshnessMs) {
            return WaitingGate(signal = waiting, blocks = false, degradedWhy = staleWhy(ageMs))
        }
        return WaitingGate(signal = waiting, blocks = true, degradedWhy = null)
    }

    private fun staleWhy(ageMs: Long): String = "$STALE_PREFIX${ageMs}ms > ${freshnessMs}ms"

    private fun mutate(
        sessionId: String,
        f: (SessionSignal) -> SessionSignal,
    ) {
        val cur = _signals.value
        val next = f(cur[sessionId] ?: SessionSignal(sessionId))
        _signals.value = cur + (sessionId to next)
    }

    companion object {
        /** pidfile / 后端帧里那个「在等人」的机器码。判定认它，不认 `waitingFor`。 */
        const val STATUS_WAITING = "waiting"

        /**
         * 后端活动档里「在等人」的值（`session_added.activity` / `session_status.activity`）。
         * 与后端逐字一致；漂了不报错，只会让等待态静默消失。
         */
        const val ACTIVITY_NEEDS_YOU = "needs_you"

        /**
         * 默认新鲜度阈值，没有量过。取 5 分钟的量级：比在电脑上答一个框慢得多，比合上手机过一小时快得多。
         */
        const val DEFAULT_FRESHNESS_MS = 5L * 60L * 1000L

        /** 降级理由的前缀。按它认，不按整句话（整句带毫秒数）。 */
        const val STALE_PREFIX = "等待态读数过期："

        /** [invalidateWaiting] 在「丢过 `session_status`」时的理由。 */
        const val LOST_STATUS_FRAME = "流上丢过 session_status，这条 sid 的等待态可能是过期的最后一帧"
    }
}

/** 一条信号是从哪条通路的哪种帧来的。 */
enum class SignalSource {
    /** 总览通路 · `session_added` 帧上宣告的初始 status（「连上之前就在等」走这条）。 */
    DAEMON_SESSION_ADDED,

    /** 总览通路 · `session_status` 变更帧。 */
    DAEMON_SESSION_STATUS,

    /**
     * 总览通路 · 合并后的快照（`DaemonSessionSource.State`）。分不出是哪种帧带来的，要精确来源走 [DaemonSignalBridge.onFrame]。
     */
    DAEMON_SNAPSHOT,

    /** 聊天通路 · 我们自己那条管道的下行。 */
    CHAT_DOWNLINK,
}

/** 一条对话上的全部跨通路信号。加字段一律带默认值。 */
data class SessionSignal(
    val sessionId: String,
    val waiting: WaitingSignal? = null,
    val rateLimit: RateLimitReading? = null,
    val blocked: BlockedSignal? = null,
    /** 最近一次 [SessionSignals.invalidateWaiting] 的理由。null = 没作废过。 */
    val waitingInvalidatedWhy: String? = null,
)

/** 「它在等人」这条读数。[observedAtMs] 是我们看到它的时刻，不是它开始等的时刻。 */
data class WaitingSignal(
    val waitingFor: String?,
    val observedAtMs: Long,
    val source: SignalSource,
)

/** 「它想做的一件事被挡住了」，不是等待态，见 [SessionSignals.publishBlocked]。 */
data class BlockedSignal(
    val toolName: String?,
    /** 远端产出的那句话，原样带回不翻译（读不懂每一种拦截）。 */
    val humanText: String?,
    val observedAtMs: Long,
)

/** [SessionSignals.waitingGate] 的结论。 */
data class WaitingGate(
    /** 手上那条等待态读数。null = 根本没有。 */
    val signal: WaitingSignal?,
    /** 拦不拦上行。 */
    val blocks: Boolean,
    /** 有读数却不拦时的理由（过期 / 被作废）。 */
    val degradedWhy: String?,
)
