@file:Suppress("MatchingDeclarationName") // 「对话」一屏的 composable 与它的测试标签放一处

package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.clickable
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
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
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize()) {
        val whole = state.linkProblem ?: state.listProblem
        when {
            whole != null -> ProblemBar(whole, onRetry)
            state.connecting || (state.listLoading && state.sections.isEmpty()) ->
                Text(copyText("history.list.reading", "machines" to machine), style = monoSmall, modifier = Modifier.padding(16.dp))
            state.sections.isEmpty() && state.needs.isEmpty() ->
                Text(copyText("history.list.empty"), style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.EMPTY))
            else ->
                LazyColumn(Modifier.fillMaxSize().testTag(OverviewTags.LIST)) {
                    state.linkNotice?.let { p -> item { ProblemBar(p, onRetry) } }
                    state.needsProblem?.let { p -> item { ProblemBar(p, onRetry) } }
                    needsSection(state.needs, onOpen)
                    state.notice?.let { n -> item { Text(n, style = monoSmall, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) } }
                    for (s in state.sections) historySection(s, onOpen)
                    if (state.truncated) {
                        item { Text(copyText("history.list.truncated"), style = monoSmall, modifier = Modifier.padding(16.dp)) }
                    }
                }
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
) {
    s.title?.let { t -> item { SectionHeader(t) } }
    items(s.rows, key = { it.sessionId }) { r ->
        DotRow(r.dot, r.status?.text, onClick = { onOpen(r.sessionId) }) {
            Column(Modifier.weight(1f)) {
                Text(r.label, maxLines = 1, overflow = TextOverflow.Ellipsis, style = rowTitle())
                r.status?.let { StatusText(it) }
            }
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
@Composable
private fun DotRow(
    dot: SessionDot?,
    label: String?,
    onClick: () -> Unit,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowDouble)
            .clickable(onClick = onClick)
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
