package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.wire.gen.ManualItem
import com.ccmonitor.mobile.core.wire.gen.NeedsItem
import com.ccmonitor.mobile.core.wire.gen.NeedsRow
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chats.HostName

/** 需手动屏的一行（照那台给的先后）。 */
sealed interface NeedsLine {
    val host: HostName

    /** 一条在等的会话：点了打开它。 */
    data class Session(
        override val host: HostName,
        val row: NeedsRow,
    ) : NeedsLine

    /** 计划里一片要人看的那几格（同一片收成一行 · 带条数）：点了进那一片的大纲。 */
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

/**
 * 需手动屏此刻：那几行 · 各台读的结局分开（读中 · 整份没问到 · 断着 · 会外那一截读不出）· 入口那一个数。
 * 「无需手动事项」只在各台都读到、加起来 0 件时说（[empty]），下一行写读到了哪几台（[readFrom]）；没读到不当没有。
 */
data class Overview(
    val needs: List<NeedsLine>,
    val down: List<HostName>,
    val unreadable: List<Pair<HostName, String>>,
    /** 整份没问到的那几台（那一问的那一句）：一台一行 ＋［重试］。 */
    val failed: List<Pair<HostName, Problem>>,
    /** 还在读的那几台：骨架。 */
    val loading: List<HostName>,
    /** 读到了的那几台（通着、手上有一份）。 */
    val readFrom: List<HostName>,
    /** 入口的数（各台加起来，断着的不算）。 */
    val total: Int,
    /** 有哪台没读全（会外那一截读不出 · 整份没问到）⇒ 入口不带数字、虚线框（数说不准）。 */
    val uncertain: Boolean,
) {
    /** 列表里不止一台 ⇒ 会外那一行第二行写机器名。 */
    val manyHosts: Boolean get() = needs.map { it.host }.distinct().size > 1

    /** 各台都读到了、一件也没有。 */
    val empty: Boolean get() = needs.isEmpty() && loading.isEmpty() && failed.isEmpty() && readFrom.isNotEmpty()
}

/**
 * 板 ⇒ 需手动屏。机器的先后：当前那台在前，别的照机器列表的先后；每台里的行照那台给的先后，不重排。
 * 这里只按台排、按片收行、把条数相加；判 · 排 · 写字都在那台。
 */
fun overviewOf(
    board: Map<String, MachineNeeds>,
    hosts: List<HostName>,
    current: String,
): Overview {
    val order = hosts.sortedBy { it.id != current }.filter { board.containsKey(it.id) }
    val of = { h: HostName -> board.getValue(h.id) }
    val up = order.filter { !of(it).down }
    return Overview(
        needs = up.flatMap { h -> linesOf(h, of(h).list?.items.orEmpty()) },
        down = order.filter { of(it).down },
        unreadable = order.mapNotNull { h -> of(h).unreadable?.let { h to it } },
        failed = order.mapNotNull { h -> of(h).unread?.let { h to (it.problem(h.label, SessionsNeeds.COMMAND, null) ?: unreadableReply(h.label, SessionsNeeds.COMMAND, it.toString())) } },
        loading = order.filter { of(it).reading },
        readFrom = up.filter { of(it).list != null },
        total = board.values.sumOf { it.counted },
        uncertain = board.values.any { it.unreadable != null || it.unread != null },
    )
}

/** 一台的几项 ⇒ 行（照那台的先后；计划同一片收成一行）。 */
private fun linesOf(
    h: HostName,
    items: List<NeedsItem>,
): List<NeedsLine> {
    val out = mutableListOf<NeedsLine>()
    for (it in items) {
        val last = out.lastOrNull()
        when (it) {
            is NeedsItem.Session -> out += NeedsLine.Session(h, it.session)
            is NeedsItem.Manual -> out += NeedsLine.Manual(h, it.manual)
            is NeedsItem.Plan ->
                if (last is NeedsLine.Plan && last.workspace == it.workspace && last.slice == it.slice) {
                    out[out.lastIndex] = last.copy(n = last.n + 1)
                } else {
                    out += NeedsLine.Plan(h, it.workspace, it.slice, 1)
                }
        }
    }
    return out
}
