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
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.DeployStepLine
import com.ccmonitor.mobile.ui.common.deployLabel
import kotlinx.coroutines.delay

/** 屏上几处的测试标签。 */
object OverviewTags {
    const val LIST = "overview-list"
    const val EMPTY = "overview-empty"
    const val PROBLEM = "overview-problem"
    const val DEPLOY = "overview-deploy"
    const val DEPLOYED = "overview-deployed"
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
    onDeploy: () -> Unit = {},
) {
    Column(modifier.fillMaxSize()) {
        val whole = state.linkProblem ?: state.listProblem
        val fixing = Fixing(state.fix, state.deploying, onDeploy)
        when {
            whole != null -> ProblemBar(whole, onRetry, fixing.takeIf { state.linkProblem != null })
            // 装完换常驻、重接的那一会儿：照旧说装到哪一步了，不换回「读取中」。
            state.connecting && state.deploying is Deploying.Step ->
                Column(Modifier.padding(16.dp).testTag(OverviewTags.PROBLEM)) { DeployStepLine(state.deploying) }
            state.connecting || (state.listLoading && state.sections.isEmpty()) ->
                Text(copyText("history.list.reading", "machines" to machine), style = monoSmall, modifier = Modifier.padding(16.dp))
            else -> ConversationsList(state, fixing, onOpen, onRetry)
        }
    }
}

@Composable
private fun ConversationsList(
    state: ConversationsUiState,
    fixing: Fixing,
    onOpen: (sessionId: String) -> Unit,
    onRetry: () -> Unit,
) {
    LazyColumn(Modifier.fillMaxSize().testTag(OverviewTags.LIST)) {
        (state.deploying as? Deploying.Done)?.let { d ->
            item { Text(d.text, style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.DEPLOYED)) }
        }
        state.linkNotice?.let { p -> item { ProblemBar(p, onRetry, fixing) } }
        state.needsProblem?.let { p -> item { ProblemBar(p, onRetry) } }
        // 一条都没有：照旧一句「无会话」，上面那两条（装好那一句 · 要更新）照样在。
        if (state.sections.isEmpty() && state.needs.isEmpty()) {
            item { Text(copyText("history.list.empty"), style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.EMPTY)) }
        }
        needsSection(state.needs, onOpen)
        state.notice?.let { n -> item { Text(n, style = monoSmall, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) } }
        for (s in state.sections) historySection(s, onOpen)
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
    item { SectionHeader(copyText("commandBar.group.needs")) }
    items(needs, key = { "needs-${it.sessionId}" }) { n ->
        Column(Modifier.fillMaxWidth().clickable { onOpen(n.sessionId) }.padding(horizontal = 16.dp, vertical = 8.dp)) {
            n.label?.let { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            Text(n.text, color = toneColor(Tone.NEED), style = MaterialTheme.typography.bodyMedium)
            WaitedLine(n)
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

/** 问题条上的修法：核心点名的那一种（[fix]）· 装到哪一步了（[deploying]）· 按下去做什么。 */
private class Fixing(
    val fix: Fix?,
    val deploying: Deploying?,
    val onDeploy: () -> Unit,
)

/**
 * 问题条：核心那一句 ＋［重试］［复制详情］；核心点名装一次会好 ⇒ 最前一颗［安装］/［更新］。
 * 装的时候同一处换成那一步的话（＋ 进度条），按钮变灰不挪位；没装上 ⇒ 换成失败那一句，按钮可再按。
 */
@Composable
private fun ProblemBar(
    p: Problem,
    onRetry: () -> Unit,
    fixing: Fixing? = null,
) {
    val step = fixing?.deploying as? Deploying.Step
    val failed = (fixing?.deploying as? Deploying.Failed)?.said
    Column(Modifier.fillMaxWidth().padding(16.dp).testTag(OverviewTags.PROBLEM)) {
        when {
            step != null -> DeployStepLine(step)
            failed != null -> Text(failed.text, color = MaterialTheme.colorScheme.error)
            else -> Text(p.text, color = MaterialTheme.colorScheme.error)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            fixing?.fix?.let { fix ->
                Button(onClick = fixing.onDeploy, enabled = step == null, modifier = Modifier.testTag(OverviewTags.DEPLOY)) { Text(deployLabel(fix)) }
            }
            if (step == null) TextButton(onClick = onRetry) { Text(copyText("front.act.retry")) }
            CopyDetailButton(failed?.text ?: p.text, failed?.detail ?: p.detail)
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
