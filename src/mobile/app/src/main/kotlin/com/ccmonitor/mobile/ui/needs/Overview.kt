package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.wire.gen.ManualItem
import com.ccmonitor.mobile.core.wire.gen.NeedsCell
import com.ccmonitor.mobile.core.wire.gen.NeedsClock
import com.ccmonitor.mobile.core.wire.gen.NeedsItem
import com.ccmonitor.mobile.core.wire.gen.NeedsRow
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

/** 没答上的一台（核心合好的那一行）：那一句（已叫出那台）· 复制详情 · [retry] ⇒ 那台通着却没答上（出［重试］；断着的不出）。 */
data class DownLine(
    val host: HostName,
    val said: String,
    val detail: String,
    val retry: Boolean,
)

/** 会外那一截读不出的一台：那一行（核心写好的）· 那台那一句（长按看）。 */
data class UnreadableLine(
    val host: HostName,
    val line: String,
    val said: String,
)

/**
 * 需手动屏此刻：那几行 · 头那一句 · 没答上的 · 会外读不出的（这几样都是核心合好的那一份，[NeedsNow.merged]）· 读中的那几台（骨架）。
 * 「无需手动事项」只在各台都读到、加起来 0 件时说（[empty]），下一行写读到了哪几台（[readFrom]）；没读到不当没有。
 */
data class Overview(
    val needs: List<NeedsLine>,
    /** 组头那一句（核心写好：数说得准 ⇒「需手动 n」，说不准 ⇒ 不带数）。 */
    val head: String,
    val down: List<DownLine>,
    val unreadable: List<UnreadableLine>,
    /** 还在读的那几台：骨架。 */
    val loading: List<HostName>,
    /** 答了的那几台（通着、这一问问到了）。 */
    val readFrom: List<HostName>,
    /** 各台加起来几项（没答上的不算）。 */
    val total: Int,
    /** 数说得准（核心判的：没有哪台读中 · 断着 · 答失败 · 会外读不出）。 */
    val sure: Boolean,
) {
    /** 列表里不止一台 ⇒ 会外那一行第二行写机器名。 */
    val manyHosts: Boolean get() = needs.map { it.host }.distinct().size > 1

    /** 核心说得准、一件也没有、读到过至少一台（各台读没读到只照核心的 [sure]，这里不另判）。 */
    val empty: Boolean get() = sure && total == 0 && readFrom.isNotEmpty()
}

/**
 * 板 ⇒ 需手动屏。需手动那几行的先后 · 总数 · 头那一句 · 说得准说不准只照核心合好的需手动那一列（[NeedsNow.merged] 的 `manual`；
 * 界面答得了的那几条在待答请求那一列，不进这一屏，从会话列表 · 钉条答）；没答上 · 会外读不出那几行同一份（本机后端 `sessions-needs-merge` 那一份合法）；
 * 判 · 排 · 写字（连计划那一格的「编号 标题」）都在那台。
 * 机器的先后（当前那台在前，别的照机器列表的先后）只排读中 · 读到了那几行。还没问完过一圈 ⇒ 没有行、数说不准。
 */
fun overviewOf(
    board: NeedsNow,
    hosts: List<HostName>,
    current: String,
): Overview {
    val order = hosts.sortedBy { it.id != current }.filter { it.id in board.on }
    val byId = order.associateBy { it.id }
    val merged = board.merged
    val manual = merged?.manual
    return Overview(
        needs = manual?.items.orEmpty().mapNotNull { x -> byId[x.origin]?.let { h -> lineOf(h, x.item, board.at) } },
        head = manual?.head.orEmpty(),
        down = merged?.down.orEmpty().mapNotNull { d -> byId[d.origin]?.let { DownLine(it, d.said, d.detail, retry = it.id !in board.down) } },
        unreadable = merged?.unreadable.orEmpty().mapNotNull { u -> byId[u.origin]?.let { UnreadableLine(it, u.line, u.said) } },
        loading = order.filter { board.reading(it.id) },
        readFrom = order.filter { board.answered(it.id) },
        total = manual?.total ?: 0,
        sure = manual?.sure == true,
    )
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
