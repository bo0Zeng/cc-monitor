@file:Suppress("MatchingDeclarationName") // 文件按「总览面」主题聚合 Screen + 分区 + 行

package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 会话总览面。「需手动」排最前：deferred 批准 / 限流 / 认证失效都如实呈现。
 *
 * 纯拉模型：不闪、不弹、不震动、不抢焦点，用户主动进来看。
 * 用 `LazyColumn` 而不是 `Column + verticalScroll`，会话数没有上界。
 */
@Composable
fun SessionOverviewScreen(
    state: SessionOverviewUiState,
    onOpen: (SessionRow) -> Unit,
    modifier: Modifier = Modifier,
    /** 重连：既是丢帧后的补齐，也是 `fatal` 之后唯一的重试入口。 */
    onReconnect: () -> Unit = {},
    /** 打开一条历史对话（不一定活着）。活会话的行走 [onOpen]。 */
    onOpenConversation: (ConversationRow) -> Unit = {},
    /**
     * 再往回翻一页，也是历史那半出错后的重试入口。由屏上的显式按钮触发，没有滚动监听。
     *
     * 函数型参数放最后：调用方用尾随 lambda，往后插参数会绑错。
     */
    onLoadOlder: () -> Unit = {},
) {
    // 一次进屏内「现在」不变，否则跨午夜时同一行的时间字会无故跳。
    val nowMs = remember { System.currentTimeMillis() }
    Column(modifier.fillMaxSize()) {
        // 不完整、失败都必须看得见，否则空列表会被当成真的没有对话。文案已在 VM 里分档并去术语化。
        state.fatal?.let { BannerWithRetry(it, OverviewTags.FATAL, onReconnect) }
        state.degraded?.let { BannerWithRetry("⚠ $it", OverviewTags.DEGRADED, onReconnect) }

        when {
            state.loading && state.isEmpty && state.fatal == null ->
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.padding(end = 12.dp))
                    Text("正在看服务器上有哪些对话…", style = monoSmall)
                }
            // 只有确实拿到了列表、且连历史也为空，才说「还没有对话」。
            state.isCompletelyEmpty &&
                state.fatal == null &&
                !state.loading &&
                !state.historyPaging.loading ->
                Text(
                    "这台服务器上还没有对话",
                    style = monoSmall,
                    modifier = Modifier.padding(16.dp).testTag(OverviewTags.EMPTY),
                )
            else ->
                LazyColumn(Modifier.fillMaxSize().testTag(OverviewTags.LIST)) {
                    allSections(state, nowMs, onOpen, onOpenConversation)
                    historyPagingFooter(state.historyPaging, onLoadOlder)
                }
        }
    }
}

/**
 * 分区全部由 [sections] 算出来，这里不写 `filter`，只按类型分派渲染。
 * 单测读的是 `sections()` 的列表顺序，钉不住屏上从上到下的顺序。
 */
private fun androidx.compose.foundation.lazy.LazyListScope.allSections(
    state: SessionOverviewUiState,
    nowMs: Long,
    onOpen: (SessionRow) -> Unit,
    onOpenConversation: (ConversationRow) -> Unit,
) {
    sections(state).forEach { s ->
        when (s) {
            is OverviewSection.Status -> statusSection(s, nowMs, onOpen)
            is OverviewSection.UnclearRemoval -> unclearRemovalSection(s, nowMs, onOpen)
            is OverviewSection.ByProject -> byProjectSection(s, nowMs, onOpen, onOpenConversation)
        }
    }
}

/**
 * 分区 3「按项目」：每个项目一个组头，组内一行一条对话。
 *
 * 组头的「已显示 {n} / 共 {N}」只对已经翻到过的项目说；没翻到的项目总数未知，拿 `shown` 顶上就是编数。
 */
private fun androidx.compose.foundation.lazy.LazyListScope.byProjectSection(
    section: OverviewSection.ByProject,
    nowMs: Long,
    onOpenLive: (SessionRow) -> Unit,
    onOpenHistory: (ConversationRow) -> Unit,
) {
    if (section.rows.isEmpty()) return // 空分区不渲染成一个空壳标题
    item(key = "header#byproject") { SectionHeader(section.title) }
    section.groups.forEach { group ->
        item(key = "group#${group.projectPath ?: "?"}") { ProjectGroupHeader(group) }
        items(group.rows, key = { "row#${group.projectPath}#${it.sessionId}" }) {
            // 组头已经说了项目，行里不再重复路径。
            OverviewRowView(it, nowMs, showCwd = false, onOpenLive = onOpenLive, onOpenHistory = onOpenHistory)
        }
    }
}

/**
 * 历史翻页的页脚：加载中 / 已是最早 / 载更早，同时最多显示一个。
 *
 * 互斥由 [HistoryPaging] 的派生属性给，这里不手写第二份。
 * 触发用一个必然可达的显式按钮，不靠滚动监听，把「翻页逻辑对不对」与「滚动触发灵不灵」分开。
 * 它说的是「还能不能再往回翻」，与哪个项目无关，所以挂在整段末尾。
 */
private fun androidx.compose.foundation.lazy.LazyListScope.historyPagingFooter(
    paging: HistoryPaging,
    onLoadOlder: () -> Unit,
) {
    // 失败要留在屏上，且不清空已翻到的行。
    paging.error?.let { why ->
        item(key = "history#error") {
            Row(Modifier.padding(16.dp, 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "⚠ $why",
                    style = monoSmall,
                    modifier = Modifier.weight(1f).testTag(OverviewTags.HISTORY_ERROR),
                )
                TextButton(onClick = onLoadOlder) { Text("重试") }
            }
        }
    }
    when {
        paging.loading ->
            item(key = "history#loading") {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.padding(end = 12.dp))
                    Text("正在翻更早的对话…", style = monoSmall)
                }
            }
        // 只有接上了翻页才允许说这句，见 [HistoryPaging.attached]。
        paging.showsOldestReached ->
            item(key = "history#oldest") {
                Text(
                    "已是最早",
                    style = monoSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp).testTag(OverviewTags.HISTORY_OLDEST),
                )
            }
        paging.canLoadMore ->
            item(key = "history#more") {
                TextButton(
                    onClick = onLoadOlder,
                    modifier = Modifier.padding(start = 8.dp).testTag(OverviewTags.HISTORY_MORE),
                ) { Text("载入更早的对话") }
            }
    }
}

/** 分区 1 / 分区 2。空段整段不渲染，不出「需手动（0）」的空壳。 */
private fun androidx.compose.foundation.lazy.LazyListScope.statusSection(
    section: OverviewSection.Status,
    nowMs: Long,
    onOpen: (SessionRow) -> Unit,
) {
    if (section.rows.isEmpty()) return
    item(key = "header#${section.title}") { SectionHeader("${section.title}（${section.rows.size}）") }
    items(section.rows, key = { "row#${section.title}#${it.sessionId}" }) {
        // 分区 1/2 没有组头，路径留在行里。
        OverviewRowView(it, nowMs, showCwd = true, onOpenLive = onOpen, onOpenHistory = {})
    }
}

@Composable
private fun SectionHeader(text: String) =
    Text(
        text,
        style = monoSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, top = 12.dp, bottom = 4.dp),
    )

/**
 * 项目组头：左边项目名，右边 `已显示 {n} / 共 {N}`。
 *
 * 还没翻到的项目右边那半整个不出现，见 [projectGroupCounter]。
 */
@Composable
private fun ProjectGroupHeader(group: ProjectGroup) =
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(group.name, style = monoSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        projectGroupCounter(group)?.let {
            Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }

/**
 * 「下线了，说不出为什么」一区。既不是「被接替」（那是已知的事），也不能并进「其余」（那是「还在，没在跑」）。
 *
 * 标题下多一行 [UNCLEAR_REMOVAL_WHY]：说清发生了什么、为什么说不出、不是操作出了错，
 * 否则用户会默认是 app 的 bug。不写对端版本串，用户看不懂也没有动作可做。
 * 行用共用的 [OverviewRowView]，可否点开、记号、状态词都由它和 [markFor] / [statusWordFor] 给。
 */
private fun androidx.compose.foundation.lazy.LazyListScope.unclearRemovalSection(
    section: OverviewSection.UnclearRemoval,
    nowMs: Long,
    onOpen: (SessionRow) -> Unit,
) {
    if (section.rows.isEmpty()) return
    item(key = "header#unclear-removal") {
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp)) {
            Text(
                "${section.title}（${section.rows.size}）",
                style = monoSmall,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                UNCLEAR_REMOVAL_WHY,
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    items(section.rows, key = { "row#unclear#${it.sessionId}" }) {
        // 这一区没有组头，路径留在行里。
        OverviewRowView(it, nowMs, showCwd = true, onOpenLive = onOpen, onOpenHistory = {})
    }
}

/** 下线原因不明那一区的标题，即 [OverviewSection.UnclearRemoval.title]。只说发生了什么，不说已停 / 结束 / 崩。 */
internal const val UNCLEAR_REMOVAL_TITLE = "下线了，说不出为什么"

/** 那一区的解释。句中的「结束」是在列出排除不掉的两种可能，不是给这一区下定义。 */
internal const val UNCLEAR_REMOVAL_WHY =
    "这台服务器上的助手版本太旧，它不会说这几条是被 /clear、/branch 接替了，还是真的结束了。不是操作出了错。"

/**
 * 四个分区共用的行组件。只留一套渲染，同一条对话在两个分区里显示同样的记号和词。
 *
 * 单测量的是 [markFor] / [overviewRowSubtitle] 返回的字，钉不住下面的 `Text` 是否真画出来。
 */
@Composable
private fun OverviewRowView(
    row: OverviewRow,
    nowMs: Long,
    showCwd: Boolean,
    onOpenLive: (SessionRow) -> Unit,
    onOpenHistory: (ConversationRow) -> Unit,
) {
    val live = row.liveRow
    // `attachable == false` 不给点开，但这一行仍然显示。
    // 归档、下线原因不明的行已不在流上，点进去只会是空对话，也不给点。
    val openable = live == null || (live.attachable && !live.archived && !live.removalUnknown)
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(enabled = openable) {
                live?.let(onOpenLive) ?: row.historyRow?.let(onOpenHistory)
            }
            // 树缩进。`depth == 0` 是常态，不预留恒空的槽位。
            .padding(start = 16.dp + TREE_INDENT * (live?.depth ?: 0), end = 16.dp)
            .padding(vertical = 10.dp),
    ) {
        val mark = markFor(row)
        Text(
            if (mark.isEmpty()) row.title else "$mark ${row.title}",
            color = if (openable) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        val sub =
            listOfNotNull(
                row.cwd.takeIf { showCwd },
                overviewRowSubtitle(row, nowMs).takeIf { it.isNotEmpty() },
            ).joinToString(" · ")
        if (sub.isNotEmpty()) {
            Text(sub, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** 横幅必须带一个出口：只说「读不到」而不给动作，等于把人晾在那儿。 */
@Composable
private fun BannerWithRetry(
    text: String,
    tag: String,
    onReconnect: () -> Unit,
) = Row(Modifier.padding(16.dp, 8.dp), verticalAlignment = Alignment.CenterVertically) {
    Text(text, style = monoSmall, modifier = Modifier.weight(1f).testTag(tag))
    TextButton(onClick = onReconnect, modifier = Modifier.testTag(OverviewTags.RETRY)) { Text("重试") }
}

/** 每一级血统的缩进量。 */
private val TREE_INDENT = 14.dp

/** 供 UI 测试定位，与文案解耦。 */
object OverviewTags {
    const val LIST = "overview-list"
    const val EMPTY = "overview-empty"
    const val DEGRADED = "overview-degraded"
    const val RETRY = "overview-retry"
    const val FATAL = "overview-fatal"

    /** 历史翻页的三个终态各一个 tag。 */
    const val HISTORY_MORE = "overview-history-more"
    const val HISTORY_OLDEST = "overview-history-oldest"
    const val HISTORY_ERROR = "overview-history-error"
}
