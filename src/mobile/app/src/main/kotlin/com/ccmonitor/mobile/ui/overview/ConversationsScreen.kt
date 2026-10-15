@file:Suppress("MatchingDeclarationName") // 「对话」一屏的 composable 与它的测试标签放一处

package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.StatusDot
import kotlinx.coroutines.delay

/** 屏上几处的测试标签。 */
object OverviewTags {
    const val LIST = "overview-list"
    const val EMPTY = "overview-empty"
    const val PROBLEM = "overview-problem"
    const val HIDDEN = "overview-hidden"
    const val SHOW_HIDDEN = "overview-show-hidden"
}

/** 长按一行（手机的右键菜单）：「隐藏」/「取消隐藏」；隐藏的那一行标题底下是那台写好的那一句（为什么隐藏）。 */
@Composable
private fun RowMenu(
    item: HistoryItem,
    onHide: (HistoryItem) -> Unit,
    onDismiss: () -> Unit,
) {
    val hidden = item.hidden != null
    ActionSheet(
        title = item.label,
        note = item.hidden,
        actions =
            listOf(
                SheetAction(
                    if (hidden) Ph.Eye else Ph.EyeSlash,
                    if (hidden) copyText("history.menu.unhide") else copyText("history.menu.hide"),
                    onClick = { onHide(item) },
                ),
            ),
        onDismiss = onDismiss,
    )
}

/** 漏斗拉起的过滤（同桌面历史页「筛选」里那一格）：眼下只有「显示已隐藏」。点一下就换，不另设应用键。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConversationsFilterSheet(
    showHidden: Boolean,
    onShowHidden: (Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = tokens.card, scrimColor = MaterialTheme.colorScheme.scrim, tonalElevation = 0.dp) {
        Column(Modifier.fillMaxWidth().padding(bottom = tokens.spacing.lg)) {
            Text(copyText("history.page.filter"), style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm))
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Touch.rowSetting)
                    .toggleable(value = showHidden, role = Role.Switch, onValueChange = onShowHidden)
                    .padding(horizontal = tokens.spacing.lg)
                    .testTag(OverviewTags.SHOW_HIDDEN),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(copyText("history.filter.showHidden"), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                Switch(checked = showHidden, onCheckedChange = null)
            }
        }
    }
}

/**
 * 「对话」这一屏：需手动在最前，下面是核心排好的清单（段头 · 标题 · 时刻 · 状态字都照抄）。
 * 失败只说核心那一句（或桌面同一个概念的那句）＋［重试］［复制详情］。
 */
@Composable
fun ConversationsScreen(
    state: ConversationsUiState,
    machine: String,
    onOpen: (sessionId: String) -> Unit,
    onRetry: () -> Unit,
    onHide: (HistoryItem) -> Unit,
    modifier: Modifier = Modifier,
) {
    var menuFor by remember { mutableStateOf<HistoryItem?>(null) }
    menuFor?.let { RowMenu(it, onHide, onDismiss = { menuFor = null }) }
    Column(modifier.fillMaxSize()) {
        val whole = state.linkProblem ?: state.listProblem
        // 手上有上次那一份（本地缓存）⇒ 照画；离线 · 没问成的那一句排在最前。
        val cachedOnly = state.sections.isNotEmpty()
        when {
            whole != null && !cachedOnly -> ProblemBar(whole, onRetry)
            !cachedOnly && (state.connecting || state.listLoading) ->
                Text(copyText("history.list.reading", "machines" to machine), style = monoSmall, modifier = Modifier.padding(16.dp))
            state.sections.isEmpty() && state.needs.isEmpty() ->
                Text(copyText("history.list.empty"), style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.EMPTY))
            else -> ConversationsList(state, whole, onOpen, onRetry, onMenu = { menuFor = it })
        }
    }
}

/** 清单本身：最前是离线 / 没问成的那一句（有的话）· 后端旧了那一句 · 需手动，下面一段一段。 */
@Composable
private fun ConversationsList(
    state: ConversationsUiState,
    whole: Problem?,
    onOpen: (sessionId: String) -> Unit,
    onRetry: () -> Unit,
    onMenu: (HistoryItem) -> Unit,
) {
    LazyColumn(Modifier.fillMaxSize().testTag(OverviewTags.LIST)) {
        whole?.let { p -> item { ProblemBar(p, onRetry) } }
        state.linkNotice?.let { p -> item { ProblemBar(p, onRetry) } }
        state.needsProblem?.let { p -> item { ProblemBar(p, onRetry) } }
        needsSection(state.needs, onOpen)
        state.notice?.let { n -> item { Text(n, style = monoSmall, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) } }
        for (s in state.sections) historySection(s, onOpen, onMenu)
        if (state.truncated) {
            item { Text(copyText("history.list.truncated"), style = monoSmall, modifier = Modifier.padding(16.dp)) }
        }
    }
}

private fun LazyListScope.needsSection(
    needs: List<NeedsItem>,
    onOpen: (String) -> Unit,
) {
    if (needs.isEmpty()) return
    // 需手动一节置顶、段头琥珀（同桌面侧栏顶上那颗琥珀条）。
    item { SectionHeader(copyText("commandBar.group.needs"), color = LocalAppTokens.current.warn) }
    items(needs, key = { "needs-${it.sessionId}" }) { n ->
        DotRow(n.dot, n.text, onClick = { onOpen(n.sessionId) }) {
            Column(Modifier.weight(1f)) {
                n.label?.let { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis, style = rowTitle()) }
                Text(n.text, color = toneColor(Tone.NEED), style = MaterialTheme.typography.bodySmall)
                WaitedLine(n)
            }
        }
    }
}

private fun LazyListScope.historySection(
    s: HistorySection,
    onOpen: (String) -> Unit,
    onMenu: (HistoryItem) -> Unit,
) {
    s.title?.let { t -> item { SectionHeader(t) } }
    items(s.rows, key = { it.sessionId }) { r ->
        DotRow(r.dot, r.status?.text, onClick = { onOpen(r.sessionId) }, onLongClick = { onMenu(r) }) {
            Column(Modifier.weight(1f)) {
                Text(r.label, maxLines = 1, overflow = TextOverflow.Ellipsis, style = rowTitle())
                r.status?.let { StatusText(it) }
            }
            // 隐藏的行（开着「显示已隐藏」才在）：同桌面历史行的那颗小标。
            r.hidden?.let { Text(copyText("history.row.hidden"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.testTag(OverviewTags.HIDDEN)) }
            r.agentTag?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            r.atText?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@Composable
private fun SectionHeader(
    text: String,
    color: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) {
    Text(text, style = MaterialTheme.typography.labelMedium, color = color, modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp))
}

@Composable
private fun StatusText(s: Toned) {
    Text(s.text, color = toneColor(s.tone), style = MaterialTheme.typography.bodySmall)
}

/** 行标题：正文字号 600。 */
@Composable
private fun rowTitle() = MaterialTheme.typography.titleSmall

/**
 * 一行：行首一颗状态点（固定一列宽）＋ 内容。双行（标题 ＋ 状态字），行高照适配层 [Touch.rowDouble]。
 * [label] 是点的读屏名（那一行的状态字）；没有 ⇒ 点不念。
 */
@OptIn(ExperimentalFoundationApi::class) // combinedClickable：点行打开，长按 ＝ 菜单
@Composable
private fun DotRow(
    dot: SessionDot?,
    label: String?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowDouble)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        StatusDot(dot, label)
        content()
    }
}

/**
 * 语气 ⇒ 字色：闭集，按核心给的语气选，不按字。状态字回 `--text-2`，只有需手动那句琥珀、出错那句红字；
 * 在跑不再用强调色当字色（那是状态点的事，状态点九种等核心给齐两轴再画）。
 */
@Composable
private fun toneColor(t: Tone?): Color {
    val tokens = LocalAppTokens.current
    return when (t) {
        Tone.NEED, Tone.WARN -> tokens.warn
        Tone.FAIL -> tokens.errorText
        Tone.NOW, Tone.BUSY, Tone.PLAIN, null -> MaterialTheme.colorScheme.onSurfaceVariant
    }
}

@Composable
private fun ProblemBar(
    p: Problem,
    onRetry: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().padding(16.dp).testTag(OverviewTags.PROBLEM)) {
        ErrorLine(p.text)
        Row {
            SecondaryButton(onClick = onRetry) { Text(copyText("front.act.retry")) }
            CopyDetailButton(p.text, p.detail)
        }
    }
}

/** 「已等 …」那一行：屏上每秒重画（[NeedsItem.waitedAt]，同桌面的走表），不重问那台。 */
@Composable
private fun WaitedLine(n: NeedsItem) {
    val now by produceState(monoNow(), n.receivedAt) {
        while (true) {
            value = monoNow()
            delay(TICK_MS)
        }
    }
    Text(n.waitedAt(now)?.let { copyText("needs.bar.sub", "waited" to it) } ?: copyText("needs.bar.subBare"), style = monoSmall)
}

private const val TICK_MS = 1_000L
