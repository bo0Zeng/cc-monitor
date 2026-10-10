/**
 * 总览面的分区模型：需手动 / 在跑 / 下线原因不明 / 按项目。
 *
 * 分区、分组、排序、记号全收在 [sections] 这条纯函数里，屏只负责画，单测读它的返回值。
 * 屏上的先后顺序、记号是否真画出来，这里钉不住。
 */
@file:Suppress("MatchingDeclarationName") // 本文件按「分区」主题聚合：三分区 + 记号 + 排序，全是纯函数

package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.transport.WaitingCopy
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/** 分区 1 的段名。 */
const val SECTION_NEEDS_MANUAL = "需手动"

/** 分区 2 的段名。 */
const val SECTION_RUNNING = "在跑"

/** 分区 3 的段名。不带计数：不说全局「共 N 条对话」。 */
const val SECTION_BY_PROJECT = "按项目"

/** `cwd` 缺席的行归这一组。不许丢、也不许猜一个项目。 */
const val UNKNOWN_PROJECT_GROUP = "⟨还没认出项目的对话⟩"

/**
 * 一个已经翻到过的项目的摘要。组头的「已显示 / 共」只对翻到过的项目说，
 * 所以 [HistoryPaging.pagedProjects] 里只放 `ConversationPager.fetched` 里出现过的项目。
 *
 * @param projectPath 项目的真实路径（不是 `dirName`），要和 [ConversationRow.cwd] 对上。
 * @param sessionCount daemon 自报的对话总数，以它为准，不去数磁盘上的文件。
 */
data class ProjectSummary(
    val projectPath: String?,
    val sessionCount: Int,
    val lastActivityMs: Long,
)

/**
 * 一行的出处，点这一行打开什么全看它。
 *
 * 「daemon 说它此刻怎么样」和「磁盘上有这么一条」是两种事实，合成一个就得为没在跑的对话编一个 [SessionLight]。
 */
sealed interface RowSource {
    /** 流当前宣告的会话。有记号、有状态词。 */
    data class Live(
        val row: SessionRow,
    ) : RowSource

    /** 只在磁盘查询里出现过，没有活性信息 ⇒ 记号留白（[markFor]）。 */
    data class History(
        val row: ConversationRow,
    ) : RowSource
}

/**
 * 分区里的一行。
 *
 * @param updatedAtMs 排序用。活行没有时间字段 ⇒ `0`，被历史翻到后合并时带上真时间。
 * @param messageCount 消息数，只有历史查询给（`messageCountApprox`），活行为 `0`。
 *   独立成字段而不从 [historyRow] 取：合并时活行赢会把 [source] 换成 `Live`，这个数就丢了。
 */
data class OverviewRow(
    val sessionId: String,
    val title: String,
    val cwd: String?,
    val updatedAtMs: Long,
    val messageCount: Int,
    val source: RowSource,
) {
    /** 活行的原始数据；纯历史行为 `null`。渲染分区 1/2 时用它复用既有的行组件。 */
    val liveRow: SessionRow? get() = (source as? RowSource.Live)?.row

    /** 历史行的原始数据；活行为 `null`。 */
    val historyRow: ConversationRow? get() = (source as? RowSource.History)?.row
}

/**
 * 分区 3 里的一个项目组。
 *
 * @param total 这个项目一共有多少条对话。`null` = 还没翻到 ⇒ 组头不说数。别拿 [shown] 顶上。
 */
data class ProjectGroup(
    val name: String,
    val projectPath: String?,
    val total: Int?,
    val rows: List<OverviewRow>,
) {
    /** 这个组现在显示了几条，只是 [rows] 的长度。 */
    val shown: Int get() = rows.size
}

/**
 * 一个分区，恰好四个，见 [sections]。
 *
 * 类型上刻意没有「全局总数」的位置：不许说的话就让它无处可放。
 */
sealed interface OverviewSection {
    val title: String

    /** 本段的全部行，摊平。 */
    val rows: List<OverviewRow>

    /** 分区 1 / 分区 2：按状态组织。 */
    data class Status(
        override val title: String,
        override val rows: List<OverviewRow>,
    ) : OverviewSection

    /**
     * 下线了，但服务器说不出为什么。
     *
     * 放进分区模型而不是在屏上另画一段，这样它出不出、出几行也由 [sections] 决定，单测量得到。
     * 不是 [Status] 的实例：那两段讲「现在需手动做什么」，这段讲「它不在了而原因不明」，
     * 单独成类型才能让屏上必须带上 [UNCLEAR_REMOVAL_WHY] 那句解释。
     */
    data class UnclearRemoval(
        override val rows: List<OverviewRow>,
    ) : OverviewSection {
        override val title: String get() = UNCLEAR_REMOVAL_TITLE
    }

    /** 分区 3：按项目组织。 */
    data class ByProject(
        val groups: List<ProjectGroup>,
    ) : OverviewSection {
        override val title: String get() = SECTION_BY_PROJECT
        override val rows: List<OverviewRow> get() = groups.flatMap { it.rows }
    }
}

/**
 * 把一屏状态收成分区。
 *
 * 分区 1/2 与分区 3 不去重：前者按状态、后者按项目，一条对话在两条轴上各占一格不算重复。
 * 分区 3 内部按 `sessionId` 合并，见 [projectGroups]。
 *
 * 分区 1 按危险度排（误答 `sandbox request` / `permission prompt` 的后果远重于 `dialog open`），
 * 同档保持输入顺序（`sortedBy` 稳定）。
 *
 * @return 长度恒为 4：[SECTION_NEEDS_MANUAL] / [SECTION_RUNNING] / [UNCLEAR_REMOVAL_TITLE] / [SECTION_BY_PROJECT]。
 *   空分区照样返回，空段不渲染是渲染层的事。
 */
fun sections(state: SessionOverviewUiState): List<OverviewSection> =
    listOf(
        OverviewSection.Status(
            SECTION_NEEDS_MANUAL,
            state.needsYou.sortedBy { dangerRank(it.waitingFor) }.map { it.toOverviewRow() },
        ),
        OverviewSection.Status(SECTION_RUNNING, state.running.map { it.toOverviewRow() }),
        OverviewSection.UnclearRemoval(state.removedUnknown.map { it.toOverviewRow() }),
        OverviewSection.ByProject(projectGroups(state)),
    )

/**
 * 分区 3：把全部行（所有活动分区、归档、下线原因不明、历史）按项目分组。
 * 「其余」「已接替」不单列分区，是这里的行状态。
 *
 * 组序：⟨还没认出项目的对话⟩ 永远最后；其次按组里最急的那一行（[markRank]）；
 * 同档按 [ProjectSummary.lastActivityMs] 倒序，即 `ConversationPager` 的项目序；没翻到的项目按 0 处理。
 *
 * 组内先按记号档次、同档按时间倒序。流上宣告过但历史还没翻到的行记号档天然更高，自然排在前面。
 */
private fun projectGroups(state: SessionOverviewUiState): List<ProjectGroup> {
    val live =
        (state.needsYou + state.running + state.others + state.superseded + state.removedUnknown)
            .map { it.toOverviewRow() }
    val merged = LinkedHashMap<String, OverviewRow>()
    state.historyPaging.rows.forEach { h -> merged[h.sessionId] = h.toOverviewRow() }
    live.forEach { l ->
        val old = merged[l.sessionId]
        // 活行赢，但带上历史行独有的事实：`cwd` 缺席的活行靠它认出项目；流上没有时间与消息数。
        merged[l.sessionId] =
            l.copy(
                cwd = l.cwd ?: old?.cwd,
                updatedAtMs = maxOf(l.updatedAtMs, old?.updatedAtMs ?: 0L),
                messageCount = maxOf(l.messageCount, old?.messageCount ?: 0),
            )
    }
    val byPath = state.historyPaging.pagedProjects.associateBy { it.projectPath }
    return merged.values
        .groupBy { it.cwd }
        .map { (cwd, rows) ->
            ProjectGroup(
                name = projectDisplayName(cwd),
                projectPath = cwd,
                total = cwd?.let { byPath[it]?.sessionCount },
                rows = rows.sortedWith(compareBy<OverviewRow> { markRank(it) }.thenByDescending { it.updatedAtMs }),
            )
        }.sortedWith(
            compareBy<ProjectGroup> { it.projectPath == null }
                .thenBy { g -> g.rows.minOfOrNull { markRank(it) } ?: RANK_NO_MARK }
                .thenByDescending { byPath[it.projectPath]?.lastActivityMs ?: 0L },
        )
}

/**
 * 组头显示的项目名 = 路径的最后一段。`cwd` 缺席 ⇒ [UNKNOWN_PROJECT_GROUP]，不猜。
 * 最后一段为空（`/`）时退回整条路径。
 */
internal fun projectDisplayName(cwd: String?): String {
    if (cwd == null) return UNKNOWN_PROJECT_GROUP
    return cwd.trimEnd('/').substringAfterLast('/').ifEmpty { cwd }
}

/** 组头右边那句。还没翻到的项目不说数；只对单个已翻到的项目说，不加总。 */
internal fun projectGroupCounter(group: ProjectGroup): String? = group.total?.let { "已显示 ${group.shown} / 共 $it" }

/**
 * 行首记号。纯历史行返回空串：磁盘上的对话没有活性信息，不许画成「已停」。
 *
 * 记号是形状不是颜色，不靠颜色单独承载信息。
 * 归档行和下线原因不明的行不点状态灯，用说「它去哪了」的记号，不替对端说它怎么样。
 */
internal fun markFor(row: OverviewRow): String =
    when (val s = row.source) {
        is RowSource.History -> ""
        is RowSource.Live ->
            when {
                s.row.removalUnknown -> MARK_OFFLINE_UNCLEAR
                s.row.archived -> MARK_SUPERSEDED
                else -> lightMarkOf(s.row.light)
            }
    }

/**
 * 记号档次，小的排前面：`◆ 0 > ● 1 > ▸ 2 > ○ 3 > · 4 > × 5 > 无 6 > ⤳ 7 > ⇣ 8`。
 *
 * `·`、`×` 是流宣告过的状态，比「流上什么都没说」多一份事实，排在无记号之前。
 * `⇣` 在 `⤳` 之后：`⤳` 至少知道为什么走。排序只说先看哪个，不说它死了。
 */
internal fun markRank(row: OverviewRow): Int =
    when (val s = row.source) {
        is RowSource.History -> RANK_NO_MARK
        is RowSource.Live ->
            when {
                s.row.removalUnknown -> RANK_OFFLINE_UNCLEAR
                s.row.archived -> RANK_SUPERSEDED
                else -> lightRank(s.row.light)
            }
    }

/**
 * 行上那句状态词。纯历史行返回空串。
 *
 * 等待态的人话来自 [WaitingCopy.headlineFor]，那张表全仓只有一份（聊天屏与投递路径也用它）。
 * 下线原因不明说 [WORD_OFFLINE_UNCLEAR]，不说已停 / 结束 / 崩，原因由 [UNCLEAR_REMOVAL_WHY] 讲。
 */
internal fun statusWordFor(row: OverviewRow): String =
    when (val s = row.source) {
        is RowSource.History -> ""
        is RowSource.Live ->
            when {
                s.row.removalUnknown -> WORD_OFFLINE_UNCLEAR
                s.row.archived -> "已接替"
                else -> lightWord(s.row.light, s.row.waitingFor)
            }
    }

/**
 * 分区 1 内的排序键，小的排前面，即危险度。
 *
 * 顺序由 [WaitingCopy] 那张唯一的表定（键序就是危险度序），这里不另写值域表。
 * 未知的新值不翻译、也不当成最危险。
 */
internal fun dangerRank(waitingFor: String?): Int = WaitingCopy.dangerRank(waitingFor)

/**
 * 分区 3 里一行的副标题。不含 `cwd`，组头已经说了项目。
 * `updatedAtMs == 0` ⇒ 不出时间，不编一个「刚刚」。
 */
internal fun overviewRowSubtitle(
    row: OverviewRow,
    nowMs: Long,
): String =
    buildList {
        statusWordFor(row).takeIf { it.isNotEmpty() }?.let { add(it) }
        row.liveRow?.let { live ->
            // `waitingFor` 的原始机器码不上屏；等待的人话已由 [statusWordFor] 说过。
            if (live.livenessUncertain) add("状态是推测的")
            if (!live.attachable) add("这条在电脑上不接受远程接入")
        }
        // 消息数是用户判断「是不是这条」的主要线索。
        row.messageCount.takeIf { it > 0 }?.let { add("$it 条") }
        relativeWhen(row.updatedAtMs, nowMs)?.let { add(it) }
    }.joinToString(" · ")

/**
 * 时间戳 → 一句人话：今天 `HH:mm`、昨天「昨天」、更早 `M月d日`。
 *
 * 不说「几分钟前」：只有绝对时刻 `updatedAtMs`，照实说不会错。
 *
 * @return `null` = 没有时间可说（`ms <= 0`）。调用方不填占位符，空格子留白。
 */
internal fun relativeWhen(
    ms: Long,
    nowMs: Long,
): String? {
    if (ms <= 0L) return null
    val zone = ZoneId.systemDefault()
    val day = Instant.ofEpochMilli(ms).atZone(zone).toLocalDate()
    val today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate()
    return when (day) {
        // 手拼而不用 `String.format`：后者按默认 locale 渲染数字，这里要的是时钟读数。
        today ->
            Instant
                .ofEpochMilli(ms)
                .atZone(zone)
                .toLocalTime()
                .let { "${pad2(it.hour)}:${pad2(it.minute)}" }
        today.minusDays(1) -> "昨天"
        else -> monthDay(day)
    }
}

private fun pad2(n: Int): String = n.toString().padStart(2, '0')

private fun monthDay(day: LocalDate): String = "${day.monthValue}月${day.dayOfMonth}日"

private fun SessionRow.toOverviewRow(): OverviewRow =
    OverviewRow(
        sessionId = sessionId,
        title = title,
        cwd = cwd,
        updatedAtMs = 0L, // SessionRow 上没有时间字段，不编。
        messageCount = 0, // 流上也没有消息数。
        source = RowSource.Live(this),
    )

private fun ConversationRow.toOverviewRow(): OverviewRow =
    OverviewRow(
        sessionId = sessionId,
        title = title,
        cwd = cwd,
        updatedAtMs = updatedAtMs,
        messageCount = messageCount,
        source = RowSource.History(this),
    )

private fun lightMarkOf(light: SessionLight): String =
    when (light) {
        SessionLight.WaitingInput -> "◆"
        SessionLight.Working -> "●"
        SessionLight.Shell -> "▸"
        SessionLight.Idle -> "○"
        SessionLight.Unknown -> "·"
        SessionLight.Stopped -> "×"
    }

private fun lightRank(light: SessionLight): Int =
    when (light) {
        SessionLight.WaitingInput -> 0
        SessionLight.Working -> 1
        SessionLight.Shell -> 2
        SessionLight.Idle -> 3
        SessionLight.Unknown -> 4
        SessionLight.Stopped -> 5
    }

/** @param waitingFor 只有 [SessionLight.WaitingInput] 用得上；映射在 [WaitingCopy]，这里只取来。 */
private fun lightWord(
    light: SessionLight,
    waitingFor: String?,
): String =
    when (light) {
        SessionLight.WaitingInput -> WaitingCopy.headlineFor(waitingFor)
        SessionLight.Working -> "在跑"
        SessionLight.Shell -> "在跑别的命令"
        SessionLight.Idle -> "空闲"
        SessionLight.Unknown -> "说不好现在怎么样"
        SessionLight.Stopped -> "已停"
    }

/** `/branch`、`/clear` 接替，不是死了：不点状态灯，只给一个「换了个 sid」的记号。 */
private const val MARK_SUPERSEDED = "⤳"

/**
 * 下线原因不明的行首记号。不是灯，说的是「它去哪了」。
 * 不复用 `·`：那是「它还在，状态说不好」，这个是「它已经不在了」。
 */
private const val MARK_OFFLINE_UNCLEAR = "⇣"

/** 下线原因不明的状态词。不说已停 / 结束 / 崩 / 死。 */
internal const val WORD_OFFLINE_UNCLEAR = "下线了"

/** 「流上什么都没说」这一档，见 [markRank]。 */
private const val RANK_NO_MARK = 6

/** 已接替排倒数第二。 */
private const val RANK_SUPERSEDED = 7

/** 下线原因不明排最后：连为什么走都不知道。 */
private const val RANK_OFFLINE_UNCLEAR = 8
