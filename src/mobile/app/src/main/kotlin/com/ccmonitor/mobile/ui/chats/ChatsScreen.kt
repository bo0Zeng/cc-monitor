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
import androidx.compose.material3.Button
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
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.DeployStepLine
import com.ccmonitor.mobile.ui.common.StatusDot
import com.ccmonitor.mobile.ui.common.deployLabel
import kotlinx.coroutines.delay

/** 屏上几处的测试标签。 */
object OverviewTags {
    const val LIST = "overview-list"
    const val EMPTY = "overview-empty"
    const val PROBLEM = "overview-problem"
    const val DEPLOY = "overview-deploy"
    const val DEPLOYED = "overview-deployed"
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
    onDeploy: () -> Unit = {},
) {
    var menuFor by remember { mutableStateOf<HistoryItem?>(null) }
    menuFor?.let { RowMenu(it, onHide, onDismiss = { menuFor = null }) }
    Column(modifier.fillMaxSize()) {
        val whole = state.linkProblem ?: state.listProblem
        val fixing = Fixing(state.fix, state.deploying, onDeploy)
        // 手上有上次那一份（本地缓存）⇒ 照画；离线 · 没问成的那一句排在最前。
        val cachedOnly = state.sections.isNotEmpty()
        when {
            whole != null && !cachedOnly -> ProblemBar(whole, onRetry, fixing.takeIf { state.linkProblem != null })
            // 装完换常驻、重接的那一会儿：照旧说装到哪一步了，不换回「读取中」。
            !cachedOnly && state.connecting && state.deploying is Deploying.Step ->
                Column(Modifier.padding(16.dp).testTag(OverviewTags.PROBLEM)) { DeployStepLine(state.deploying) }
            !cachedOnly && (state.connecting || state.listLoading) ->
                Text(copyText("history.list.reading", "machines" to machine), style = monoSmall, modifier = Modifier.padding(16.dp))
            else -> ConversationsList(state, whole, fixing, onOpen, onRetry, onMenu = { menuFor = it })
        }
    }
}

/** 清单本身：最前是离线 / 没问成的那一句（有的话）· 装好那一句 · 后端旧了那一句 · 需手动，下面一段一段。 */
@Composable
private fun ConversationsList(
    state: ConversationsUiState,
    whole: Problem?,
    fixing: Fixing,
    onOpen: (sessionId: String) -> Unit,
    onRetry: () -> Unit,
    onMenu: (HistoryItem) -> Unit,
) {
    LazyColumn(Modifier.fillMaxSize().testTag(OverviewTags.LIST)) {
        whole?.let { p -> item { ProblemBar(p, onRetry, fixing.takeIf { state.linkProblem != null }) } }
        (state.deploying as? Deploying.Done)?.let { d ->
            item { Text(d.text, style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.DEPLOYED)) }
        }
        state.linkNotice?.let { p -> item { ProblemBar(p, onRetry, fixing) } }
        state.needsProblem?.let { p -> item { ProblemBar(p, onRetry) } }
        // 一条都没有：照旧一句「无会话」，上面那几条（离线那一句 · 装好那一句 · 要更新）照样在。
        if (state.sections.isEmpty() && state.needs.isEmpty()) {
            item { Text(copyText("history.list.empty"), style = monoSmall, modifier = Modifier.padding(16.dp).testTag(OverviewTags.EMPTY)) }
        }
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
                // 第一层：★ · 标题 · 时刻；第二层：状态字 · 目录短名 · 家牌 ·「已隐藏」· 分叉那一句（有的才写，用 · 连）。
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (r.starred) PhIcon(Ph.Star, contentDescription = null, size = Touch.iconInline, tint = LocalAppTokens.current.warn)
                    Text(r.label, maxLines = 1, overflow = TextOverflow.Ellipsis, style = rowTitle(), modifier = Modifier.weight(1f))
                    r.atText?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                SecondLine(r)
            }
        }
    }
}

/** 第二层：状态字（带语气）在前，其余几格淡字，用 ` · ` 连。 */
@Composable
private fun SecondLine(r: HistoryItem) {
    val rest = listOfNotNull(r.project, r.agentTag, r.hidden?.let { copyText("history.row.hidden") }, r.fork)
    if (r.status == null && rest.isEmpty()) return
    Row(horizontalArrangement = Arrangement.spacedBy(0.dp)) {
        r.status?.let { StatusText(it) }
        val tail = rest.joinToString(copyText("kit.text.sep"))
        if (tail.isNotEmpty()) {
            Text(
                (if (r.status != null) copyText("kit.text.sep") else "") + tail,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = if (r.hidden != null) Modifier.testTag(OverviewTags.HIDDEN) else Modifier,
            )
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
            failed != null -> ErrorLine(failed.text)
            else -> ErrorLine(p.text)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            fixing?.fix?.let { fix ->
                Button(onClick = fixing.onDeploy, enabled = step == null, modifier = Modifier.testTag(OverviewTags.DEPLOY)) { Text(deployLabel(fix)) }
            }
            if (step == null) SecondaryButton(onClick = onRetry) { Text(copyText("front.act.retry")) }
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
