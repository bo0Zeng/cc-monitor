package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.core.claude.catalog.LiveSessionScan

/**
 * turn-end watcher 每台主机并发 tail 的上限。每条 tail 整个生命周期独占一条 exec channel。
 * 取 4：4 条 tail + 交互 shell + 阅读面 tail + 发现用的 exec ≈ 7，低于 OpenSSH 默认 `MaxSessions=10`，
 * 给新开 tab 和 SFTP 留余量；顶满时 channel-open 失败会落到下一个开通道的人头上。
 *
 * 只对 β 起作用：α 供货时 β 一条 tail 都不留，[WatchPlan.droppedLive] 恒为 0。
 */
const val MAX_WATCH_TAILS = 4

/**
 * 一轮发现后的 tail 计划（纯逻辑）。
 * [targets] 是本轮该 tail 的 JSONL 路径；[cancelStale] 表示能否撤掉不在 [targets] 里的现有 tail
 * （只有活动集可信时为真）；[droppedLive] 是被 [MAX_WATCH_TAILS] 截掉的活会话数，>0 时调用方必须记日志，不许静默丢。
 */
data class WatchPlan(
    val targets: List<String>,
    val cancelStale: Boolean,
    val droppedLive: Int,
)

/**
 * 从活动集扫描结果 [scan] 算 tail 计划，保持 refs 的顺序（最近 mtime 在前）：
 * - α 供货（[alphaCoversTurnEnd]）→ 不 tail，撤掉全部 β tail。
 * - 探测失败（`probeOk=false`，`live` 不可信）→ 不加也不撤。撤了的话一次探测抖动就会误杀活 tail，
 *   重建时从 offset 0 全量重放几 MB。
 * - 可信活动集非空 → tail 全部活会话（截到 [maxTails]），允许撤已死会话的 tail。判活靠 pidfile，
 *   含 `/proc/<pid>/stat` starttime 防 PID 复用；同主机多个活会话是常态。
 * - 可信零活会话（Claude 不写 pidfile 或没在跑）→ 只 tail 最新 mtime 一条，允许撤旧 tail。
 * - 判活不权威（[LiveSessionScan.authoritativeLiveness]=false，如 Codex 只有 mtime）→ tail 最近 N 条；
 *   几个会话交替写时都在前 N 里，不会来回撤建导致全量重放。允许撤跌出前 N 的。
 */
fun watchTargets(
    scan: LiveSessionScan,
    maxTails: Int = MAX_WATCH_TAILS,
    alphaCoversTurnEnd: Boolean = false,
): WatchPlan {
    // α 供货时 cancelStale 必须为 true，靠它撤掉交接前的 β tail；否则两条路一直并行搬字节。
    // 这一支排在 probeOk 短路之前：α 供货时 β 的探测结果不相干，而 probeOk=false 那支会让 β 的 tail 赖着不走。
    // 该不该用 α 由 `TurnEndPathArbiter` 判，调用方问过它才传 true。
    return when {
        alphaCoversTurnEnd -> WatchPlan(targets = emptyList(), cancelStale = true, droppedLive = 0)
        !scan.probeOk -> WatchPlan(targets = emptyList(), cancelStale = false, droppedLive = 0)
        // Codex：盯最近 N 条；cancelStale 只撤跌出前 N 的。
        !scan.authoritativeLiveness ->
            WatchPlan(
                targets = scan.refs.take(maxTails).map { it.path },
                cancelStale = true,
                droppedLive = (scan.refs.size - maxTails).coerceAtLeast(0),
            )
        scan.refs.any { it.live } -> {
            val live = scan.refs.filter { it.live }.map { it.path }
            WatchPlan(targets = live.take(maxTails), cancelStale = true, droppedLive = (live.size - maxTails).coerceAtLeast(0))
        }
        // 可信零活会话：refs 是按 mtime 排的兜底列表，取最新一条。
        else -> WatchPlan(targets = listOfNotNull(scan.refs.firstOrNull()?.path), cancelStale = true, droppedLive = 0)
    }
}
