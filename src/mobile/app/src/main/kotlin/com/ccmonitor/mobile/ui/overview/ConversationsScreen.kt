@file:Suppress("MatchingDeclarationName") // 「对话」一屏的 composable 与它的测试标签放一处

package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.ui.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.copyPlainText
import kotlinx.coroutines.launch

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
    item { SectionHeader(copyText("commandBar.group.needs")) }
    items(needs, key = { "needs-${it.sessionId}" }) { n ->
        Column(Modifier.fillMaxWidth().clickable { onOpen(n.sessionId) }.padding(horizontal = 16.dp, vertical = 8.dp)) {
            n.label?.let { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            Text(n.text, color = toneColor(Tone.NEED), style = MaterialTheme.typography.bodyMedium)
            Text(
                n.waitedText?.let { copyText("needs.bar.sub", "waited" to it) } ?: copyText("needs.bar.subBare"),
                style = monoSmall,
            )
        }
    }
}

private fun LazyListScope.historySection(
    s: HistorySection,
    onOpen: (String) -> Unit,
) {
    s.title?.let { t -> item { SectionHeader(t) } }
    items(s.rows, key = { it.sessionId }) { r ->
        Row(
            Modifier.fillMaxWidth().clickable { onOpen(r.sessionId) }.padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(r.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                r.status?.let { StatusText(it) }
            }
            r.agentTag?.let { Text(it, style = monoSmall) }
            r.atText?.let { Text(it, style = monoSmall) }
        }
    }
}

@Composable
private fun SectionHeader(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp))
}

@Composable
private fun StatusText(s: Toned) {
    Text(s.text, color = toneColor(s.tone), style = monoSmall)
}

/** 语气 ⇒ 颜色：闭集，按核心给的语气选，不按字。 */
@Composable
private fun toneColor(t: Tone?): Color {
    val tokens = LocalAppTokens.current
    return when (t) {
        Tone.NEED, Tone.WARN -> tokens.warn
        Tone.FAIL -> MaterialTheme.colorScheme.error
        Tone.NOW -> tokens.success
        Tone.BUSY -> MaterialTheme.colorScheme.primary
        Tone.PLAIN, null -> tokens.textFaint
    }
}

@Composable
private fun ProblemBar(
    p: Problem,
    onRetry: () -> Unit,
) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    Column(Modifier.fillMaxWidth().padding(16.dp).testTag(OverviewTags.PROBLEM)) {
        Text(p.text, color = MaterialTheme.colorScheme.error)
        Row {
            TextButton(onClick = onRetry) { Text(copyText("front.act.retry")) }
            if (p.detail.isNotBlank()) {
                TextButton(onClick = { scope.launch { clipboard.copyPlainText(p.text + "\n" + p.detail) } }) { Text(copyText("detail.act.copy")) }
            }
        }
    }
}
