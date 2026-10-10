package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.core.claude.transport.AlphaUnavailable
import com.ccmonitor.mobile.core.claude.transport.TurnEndDebouncer

/**
 * 「这一刻由谁发完成通知」的唯一判决处（纯逻辑，无 Android 依赖）。
 *
 * 两条路：α 是守护进程帧流（`turn_end` 帧），β 是裸 tail 读 jsonl、自己判轮次。β 是 α 用不了时的落点，一直保留。
 * 抽成纯类是因为服务循环在 JVM 单测里碰不到；服务有没有问它，由结构判据另外钉住。
 *
 * 判准：
 * - α 可用 = 探测拿到 `hello`，`capabilities` 含 `tail-only`，`emits` 含 `turn_end`，且这台主机配的是 Claude。
 *   失败各档见 [AlphaUnavailable]。
 * - 回落后照样重试，否则一次 SSH 抖动会把这台机器永久钉在 β 上。退避从 [RETRY_BASE_MS] 起每次翻倍，封顶
 *   [RETRY_CAP_MS]。只有 [AlphaUnavailable.NotClaudeHost] 永不再试。
 * - 不复用 20 秒的发现周期：一次 α 尝试是两次 exec 加 daemon 冷启，对没装 daemon 的机器每 20 秒来一遍是纯浪费。
 * - 封顶 10 分钟而不放弃：电脑上装好 daemon 后最多 10 分钟自动切过去，不用重开 app。
 *
 * 任何时刻至少一条路在供货，靠的是交接顺序：α engaged 之前 β 一直跑；engaged 后再留 [HANDOVER_GRACE_MS] 才撤 β；
 * α 一死 [onUnavailable] 立刻把 [alphaCoversTurnEnd] 打回 false，下一轮发现就重建 β tail。
 * 重叠期最坏是同一轮两条路都发，而两条路投同一个通知槽（`doneNotifIdFor(slotKey)`），后一条替换前一条。
 *
 * 不起流、不收帧（`DaemonTurnEndSource` 管），不决定发不发（[TurnEndNotifyDecision] 管），不自己取时间（都收 `nowMs`）。
 */
class TurnEndPathArbiter(
    private val retryBaseMs: Long = RETRY_BASE_MS,
    private val retryCapMs: Long = RETRY_CAP_MS,
    private val handoverGraceMs: Long = HANDOVER_GRACE_MS,
    /** 稳住这么久才算「这次接上是成功的」⇒ 下次失败从第一级重新退避。见 [onUnavailable]。 */
    private val engagedStableMs: Long = RETRY_BASE_MS,
) {
    /** 这一刻该由谁发完成通知。 */
    enum class Path {
        /** 守护进程帧流（`turn_end` 帧）。 */
        Alpha,

        /** 裸 tail 读 jsonl 原文、自己判轮次。 */
        Beta,
    }

    private var engagedAtMs: Long? = null
    private var failures = 0
    private var nextTryAtMs = 0L
    private var lastWhy: AlphaUnavailable? = null

    /** α 现在是不是接上了。注意：接上 ≠ [alphaCoversTurnEnd]（那还要过交接宽限）。 */
    val alphaEngaged: Boolean get() = engagedAtMs != null

    /** 最近一次判 α 不可用的理由；null = 本 watcher 里还没判过。诊断用。 */
    val lastUnavailable: AlphaUnavailable? get() = lastWhy

    /** 这一刻名义上走哪条。 */
    fun path(): Path = if (alphaEngaged) Path.Alpha else Path.Beta

    /** 该不该（再）去探一次 α。 */
    fun shouldTryAlpha(nowMs: Long): Boolean = !alphaEngaged && nowMs >= nextTryAtMs

    /**
     * β 可以撤了吗。不是 [alphaEngaged] 的别名，还要熬过 [handoverGraceMs]。
     *
     * 没有这段宽限，交接处会漏通知：一轮正好在交接那一刻结束时，β 的 tail 被撤（撤 tail 不推进基线，不发），
     * 而 α 在 `session_added` 时已把游标推到 EOF，那一行写在之前，α 也看不见，两条路都不发。
     * 发现循环 20 秒一周期本身带一点宽限；写死这段是为了不依赖循环相位。
     */
    fun alphaCoversTurnEnd(nowMs: Long): Boolean = engagedAtMs?.let { nowMs - it >= handoverGraceMs } == true

    /** α 接上了。退避阶梯不在这里复位，见 [onUnavailable]。 */
    fun onEngaged(nowMs: Long) {
        engagedAtMs = nowMs
    }

    /**
     * α 这一刻用不了。
     *
     * - [AlphaUnavailable.NotClaudeHost] 永不再试：主机换 agent 种类会重连，重连会重建整个 watcher。
     * - 其余各档退避 `base << (失败次数-1)`，封顶 [retryCapMs]。
     * - 接上过但没稳住也算一次失败：否则 α 每次接上两秒就死时，会每 [retryBaseMs] 烧两次 exec，永远。
     *   engaged 满 [engagedStableMs] 之后才死的，才算成功、阶梯归零。
     */
    fun onUnavailable(
        why: AlphaUnavailable,
        nowMs: Long,
    ) {
        val wasStable = engagedAtMs?.let { nowMs - it >= engagedStableMs } == true
        engagedAtMs = null
        lastWhy = why
        if (why == AlphaUnavailable.NotClaudeHost) {
            nextTryAtMs = Long.MAX_VALUE
            return
        }
        failures = if (wasStable) 1 else failures + 1
        nextTryAtMs = nowMs + backoffMs()
    }

    /** 下一次重试要等多久（判据与日志用）。 */
    fun backoffMs(): Long {
        val shift = (failures - 1).coerceIn(0, SHIFT_CAP)
        return (retryBaseMs shl shift).coerceAtMost(retryCapMs)
    }

    /** 下一次允许探测的时刻；[Long.MAX_VALUE] = 永不。日志用。 */
    fun nextTryAtMs(): Long = nextTryAtMs

    companion object {
        /** 第一级退避。 */
        const val RETRY_BASE_MS: Long = 30_000L

        /** 退避上限：10 分钟。装好 daemon 之后最多等这么久就会自动切过去。 */
        const val RETRY_CAP_MS: Long = 600_000L

        /**
         * α 接上之后再留 β 多久。取静默窗（[TurnEndDebouncer.SETTLE_MS]）的几倍，
         * 盖住交接那一刻正在结算的那一轮。
         */
        const val HANDOVER_GRACE_MS: Long = 4 * TurnEndDebouncer.SETTLE_MS

        /** `shl` 的安全上限（`1 shl 63` 会翻成负数）。到这一级早就被 [RETRY_CAP_MS] 封住了。 */
        private const val SHIFT_CAP = 20
    }
}

/**
 * 一轮结算了：发不发通知、新的去重基线是什么（纯函数）。两条路共用，否则切换那一下会多发或少发一条。
 *
 * 两条路唯一的差别是 [replaysHistory]：β 用 `tail -c +N` 挂上去，首次 N=0 会先收到整份历史，首轮只记基线不通知；
 * α 用 `--tail-only` 挂上去，游标推到 EOF、不回放，首轮照发。
 *
 * 共用一个跨 α 生命期的基线账本（`SshKeepAliveService.baselineByPath`），同时挡住三件事：
 * α 死前把未结算轮次倒出并写基线，β 接手时比 uuid 不漏也不重；α 每次重连都是新的去抖器，同一轮重新结算时不重发。
 *
 * 注意：α 从未结算过轮次的会话在账本里没有条目，α 死后它的第一轮仍会被 β 吞掉。给每条 path 预写 null 基线
 * 会让每次回落都给每条会话发一条历史轮次的假通知，更坏，所以接受这个漏。
 *
 * 本函数不看前台。[Outcome.notify] 只说这一轮值得通知，前台门在调用方；前台时基线照样推进，
 * 不然切回后台会收到刚在屏上看完的那一轮。
 */
object TurnEndNotifyDecision {
    /**
     * @property notify 这一轮值得通知吗（还要再过前台门）。
     * @property writeBaseline 要不要把 [newBaseline] 写回账本。
     * @property newBaseline 写回去的值（可以是 null —— 「见过这个槽，但那时还不知道 uuid」）。
     */
    data class Outcome(
        val notify: Boolean,
        val writeBaseline: Boolean,
        val newBaseline: String?,
    )

    /**
     * @param previousBaseline 账本里这个槽的值（可能是 null，而 null 有两义，见 [baselineKnown]）。
     * @param baselineKnown 账本里有没有这个槽的条目。必须与 [previousBaseline] 分开传：账本里 null 是合法值
     *   （见过、但当时没读到 uuid），拿 `previousBaseline == null` 当「没见过」会让这个槽每轮都重吞历史、永不通知。
     * @param settledUuid 刚结算出来的 uuid；null = 这一轮没读出 uuid（β 的历史回放里会有）。
     * @param replaysHistory 这条路挂上去会不会先收到一串历史轮次。
     */
    fun decide(
        previousBaseline: String?,
        baselineKnown: Boolean,
        settledUuid: String?,
        replaysHistory: Boolean,
    ): Outcome {
        // β 首轮：只记基线、吞掉全部历史。
        if (replaysHistory && !baselineKnown) {
            return Outcome(notify = false, writeBaseline = true, newBaseline = settledUuid)
        }
        // 读不出 uuid、或与基线一样 ⇒ 什么都不做（账本也不动，写回去是同一个值）。
        if (settledUuid == null || settledUuid == previousBaseline) {
            return Outcome(notify = false, writeBaseline = false, newBaseline = previousBaseline)
        }
        return Outcome(notify = true, writeBaseline = true, newBaseline = settledUuid)
    }
}
