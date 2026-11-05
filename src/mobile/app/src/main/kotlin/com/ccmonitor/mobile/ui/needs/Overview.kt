package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.NeedsFace
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.wire.gen.ManualItem
import com.ccmonitor.mobile.core.wire.gen.NeedsCell
import com.ccmonitor.mobile.core.wire.gen.NeedsClock
import com.ccmonitor.mobile.core.wire.gen.NeedsItem
import com.ccmonitor.mobile.core.wire.gen.NeedsMerged
import com.ccmonitor.mobile.core.wire.gen.NeedsRow
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chats.HostName

/** 需手动屏的一行（照核心合好的先后）。 */
sealed interface NeedsLine {
    val host: HostName

    /** 一条在等的会话：点了打开它。[clock] ＝ 那台写好的会走的那一句（不知道等了多久 ⇒ `null`）；[receivedAt] ＝ 收到那一份的时刻（本机单调钟）。 */
    data class Session(
        override val host: HostName,
        val row: NeedsRow,
        val clock: NeedsClock? = null,
        val receivedAt: Long = 0L,
    ) : NeedsLine {
        /** 第二行此刻的字：那台写好的那一句、已等照收到以后走过的接着走；没有 ⇒ 那台写的种类那一句。 */
        fun state(now: Long): String = clock?.let { waitingNow(it.text, row.needs.waitedMs, receivedAt, now) } ?: row.needs.text
    }

    /** 计划里要人看的一条（一条一行）：落在 [cell] 那一格 ⇒ 点了进那一格；没有格可落 ⇒ 进那一片的大纲。 */
    data class Plan(
        override val host: HostName,
        val workspace: String,
        val slice: String,
        val cell: NeedsCell?,
    ) : NeedsLine {
        /** 第一行：那一格的「编号 标题」（那台写好）；没有格 ⇒ 片名。 */
        val title: String get() = cell?.label ?: slice
    }

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
    /** 断着的那几台（那条链上那台写好的离线那一句）。 */
    val down: List<Pair<HostName, String>>,
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
 * 板 ⇒ 需手动屏。机器的先后（当前那台在前，别的照机器列表的先后）只用来排断着 · 读中 · 没问到那几行，和交核心时的台序；
 * 需手动那几行的先后 · 总数 · 会外读不出的那几台都由核心合（[mergedOf]，本机后端 `sessions-needs-merge` 那一份合法）；
 * 判 · 排 · 写字（连计划那一格的「编号 标题」）都在那台。
 */
fun overviewOf(
    board: Map<String, MachineNeeds>,
    hosts: List<HostName>,
    current: String,
): Overview {
    val order = hosts.sortedBy { it.id != current }.filter { board.containsKey(it.id) }
    val of = { h: HostName -> board.getValue(h.id) }
    val up = order.filter { !of(it).down }
    val byId = order.associateBy { it.id }
    val merged = mergedOf(board, up.map { it.id })
    return Overview(
        needs = merged?.items.orEmpty().mapNotNull { x -> byId[x.origin]?.let { h -> lineOf(h, x.item, of(h).at) } },
        down = order.filter { of(it).down }.map { it to of(it).offline },
        unreadable = merged?.unreadable.orEmpty().mapNotNull { u -> byId[u.origin]?.let { it to u.said } },
        failed = order.mapNotNull { h -> of(h).unread?.let { h to (it.problem(h.label, SessionsNeeds.COMMAND, of(h).offline) ?: unreadableReply(h.label, SessionsNeeds.COMMAND, it.toString())) } },
        loading = order.filter { of(it).reading },
        readFrom = up.filter { of(it).list != null },
        total = merged?.total ?: 0,
        uncertain = uncertainOf(board, merged),
    )
}

/** 数说不准：核心合的那一份（[merged]）里有会外读不出的那台，或有哪台整份没问到（总览 · 入口同这一处）。 */
fun uncertainOf(
    board: Map<String, MachineNeeds>,
    merged: NeedsMerged?,
): Boolean = merged?.unreadable.orEmpty().isNotEmpty() || board.values.any { it.unread != null }

/**
 * 通着、手上有一份的那几台（照 [order] 的台序）交核心合一份（[NeedsFace]，`face_core::needs::merge_needs`）：
 * 跨台照各项的 `rank` 排、数总数、带上会外读不出的那几台。一台都没有 ⇒ `null`（零项）。
 */
fun mergedOf(
    board: Map<String, MachineNeeds>,
    order: List<String>,
): NeedsMerged? {
    val answered = order.mapNotNull { id -> board[id]?.takeIf { !it.down }?.raw?.let { id to it } }
    return if (answered.isEmpty()) null else NeedsFace.of(answered)
}

/** 合好的一项 ⇒ 一行（[at] ＝ 收到那台那一份的时刻）。 */
private fun lineOf(
    h: HostName,
    item: NeedsItem,
    at: Long,
): NeedsLine =
    when (item) {
        is NeedsItem.Session -> NeedsLine.Session(h, item.session, item.clock, at)
        is NeedsItem.Manual -> NeedsLine.Manual(h, item.manual)
        is NeedsItem.Plan -> NeedsLine.Plan(h, item.workspace, item.slice, item.cell)
    }

/**
 * 那台写好的会走的那一句（`{dur}` 空位）此刻的字：已等 ＝ [waitedMs]（那台答出那一刻）＋ 本机从 [receivedAt] 起走过的，
 * 当成起点在 `receivedAt − waitedMs` 的会走的钟交 [DurationFormat.clock] 填。不知道等了多久 ⇒ `null`。
 */
fun waitingNow(
    text: String,
    waitedMs: Long?,
    receivedAt: Long,
    now: Long,
): String? = waitedMs?.let { DurationFormat.clock(text, receivedAt - it, null, now) }
