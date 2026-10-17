package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.wire.ManualItem
import com.ccmonitor.mobile.core.claude.link.wire.NeedsItem
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.ui.chats.HostName

/** 总览「需手动」组的一行（照那台给的先后）。 */
sealed interface NeedsLine {
    val host: HostName

    /** 一条在等的会话：点了打开它。 */
    data class Session(
        override val host: HostName,
        val row: NeedsRow,
    ) : NeedsLine

    /** 计划里一片要人看的那几格（同一片收成一行 · 带条数）；手机没有计划页 ⇒ 只画、点了不跳。 */
    data class Plan(
        override val host: HostName,
        val workspace: String,
        val slice: String,
        val n: Int,
    ) : NeedsLine

    /** 会话外的一项：有解法 ⇒［登录…］、整行可点；没有 ⇒ 第二行写为什么、不可点。 */
    data class Manual(
        override val host: HostName,
        val item: ManualItem,
    ) : NeedsLine
}

/** 「在跑」组的一行：那台会话表里点是运行中 · 后台的那几条。 */
data class RunningLine(
    val host: HostName,
    val sid: String,
    val title: String,
    val cwd: String?,
)

/** 总览此刻：需手动组 · 组底那几句（断着的 · 会外读不出的）· 在跑组 · 入口那一个数。 */
data class Overview(
    val needs: List<NeedsLine>,
    val down: List<HostName>,
    val unreadable: List<Pair<HostName, String>>,
    val running: List<RunningLine>,
    /** 入口的数（各台加起来，断着的不算）。 */
    val total: Int,
    /** 有哪台会外那一截读不出 ⇒ 入口不带数字、虚线框（数说不准）。 */
    val uncertain: Boolean,
) {
    /** 列表里不止一台 ⇒ 会外那一行第二行写机器名。 */
    val manyHosts: Boolean get() = (needs.map { it.host } + running.map { it.host }).distinct().size > 1
}

/**
 * 板 ＋ 各台会话表 ⇒ 总览。机器的先后：当前那台在前，别的照机器列表的先后；每台里的行照那台给的先后，不重排。
 * 这里只按台排、按片收行、把条数相加；判 · 排 · 写字都在那台。
 */
fun overviewOf(
    board: Map<String, MachineNeeds>,
    tables: Map<String, SessionTable>,
    hosts: List<HostName>,
    current: String,
): Overview {
    val order = hosts.sortedBy { it.id != current }
    val up = order.filter { board[it.id]?.down != true }
    return Overview(
        needs = up.flatMap { h -> linesOf(h, board[h.id]?.list?.items.orEmpty()) },
        down = order.filter { board[it.id]?.down == true },
        unreadable = order.mapNotNull { h -> board[h.id]?.unreadable?.let { h to it } },
        running = up.flatMap { h -> tables[h.id]?.let { runningOf(h, it) }.orEmpty() },
        total = board.values.sumOf { it.counted },
        uncertain = board.values.any { it.unreadable != null },
    )
}

/** 一台的几项 ⇒ 行（照那台的先后；计划同一片收成一行）。 */
private fun linesOf(
    h: HostName,
    items: List<NeedsItem>,
): List<NeedsLine> {
    val out = mutableListOf<NeedsLine>()
    for (it in items) {
        val session = it.session
        val manual = it.manual
        val slice = it.slice
        val last = out.lastOrNull()
        when {
            session != null -> out += NeedsLine.Session(h, session)
            manual != null -> out += NeedsLine.Manual(h, manual)
            slice == null -> Unit
            last is NeedsLine.Plan && last.workspace == it.workspace && last.slice == slice -> out[out.lastIndex] = last.copy(n = last.n + 1)
            else -> out += NeedsLine.Plan(h, it.workspace.orEmpty(), slice, 1)
        }
    }
    return out
}

/** 一台会话表里点是运行中 · 后台的那几条。 */
private fun runningOf(
    h: HostName,
    t: SessionTable,
): List<RunningLine> =
    t.faces
        .filterValues { it.dot == SessionDot.RUNNING || it.dot == SessionDot.BACKGROUND }
        .keys
        .map { sid ->
            val s = t.sessions[sid]
            RunningLine(h, sid, s?.name ?: s?.cwd?.substringAfterLast('/') ?: sid, s?.cwd)
        }
