package com.ccmonitor.mobile.ui.needs

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.wire.ManualItem
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chats.HostName
import com.ccmonitor.mobile.ui.common.StatusDot
import com.ccmonitor.mobile.ui.common.dashedFrame
import com.ccmonitor.mobile.ui.login.LoginFlow
import com.ccmonitor.mobile.ui.login.LoginSheet
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import org.koin.compose.koinInject

/** 聊天屏右上那颗入口的标记（判据按它找）。 */
const val NEEDS_ENTRY_TAG = "needs-entry"

/**
 * 聊天屏顶栏 ⋯ 左边那颗「需手动 n」：n ＝ 板上各台加起来（断着的不算）；0 ⇒ 不画；有哪台会外那一截读不出 ⇒ 不带数字、虚线框。点了进总览。
 */
@Composable
fun NeedsEntry(onOpen: () -> Unit) {
    val board = koinInject<NeedsBoard>()
    val all by board.state.collectAsState()
    NeedsEntryView(board.total(all), all.values.any { it.unreadable != null }, onOpen)
}

/** 入口那颗写什么：0 且数说得准 ⇒ 不画（`null`）；数说不准 ⇒ 只写「需手动」；否则「需手动 n」。 */
fun entryText(
    n: Int,
    uncertain: Boolean,
): String? =
    when {
        uncertain -> copyText("tabBar.needs.label")
        n == 0 -> null
        else -> copyText("tabBar.needs.aria", "n" to n)
    }

/** [NeedsEntry] 的样子。 */
@Composable
fun NeedsEntryView(
    n: Int,
    uncertain: Boolean,
    onOpen: () -> Unit,
) {
    val text = entryText(n, uncertain) ?: return
    val warn = LocalAppTokens.current.warn
    val shape = RoundedCornerShape(6.dp)
    val base =
        Modifier
            .testTag(NEEDS_ENTRY_TAG)
            .semantics { contentDescription = copyText("tabBar.needs.aria", "n" to n) }
            .padding(horizontal = 4.dp)
            .clip(shape)
            .clickable(onClick = onOpen)
    Box(
        (if (uncertain) base.dashedFrame(warn) else base.border(BorderStroke(1.dp, warn), shape)).padding(horizontal = 10.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = warn, style = MaterialTheme.typography.labelLarge)
    }
}

/**
 * 总览（聊天屏右上「需手动 n」进）：顶栏 ← · 总览；「需手动 n」组（会话 · 计划 · 会话外，照板上那几台、当前那台在前）＋ 组底那几句；「在跑 n」组。
 * 点会话行 ⇒ 打开那条会话；点会话外那一行或［登录…］⇒ 登录抽屉（目标 ＝ 那一项的 `fix.args` 原样）；计划那一行只画（手机没有计划页）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
@Composable
fun OverviewScreen(
    current: String,
    onOpen: (hostId: String, sid: String) -> Unit,
    onBack: () -> Unit,
) {
    val board = koinInject<NeedsBoard>()
    val backends = koinInject<HostBackends>()
    val hostRepo = koinInject<HostRepository>()
    val hosts by hostRepo.observeAll().map { hs -> hs.map { HostName(it.id, it.label.ifBlank { it.id }) } }.collectAsState(initial = emptyList())
    val all by board.state.collectAsState()
    val tables by remember(backends) {
        backends.feeds.flatMapLatest { m ->
            if (m.isEmpty()) flowOf(emptyMap()) else combine(m.map { (id, f) -> f.table.map { id to it } }) { it.toMap() }
        }
    }.collectAsState(initial = emptyMap<String, SessionTable>())
    val ov = overviewOf(all, tables, hosts, current)
    val scope = rememberCoroutineScope()
    var login by remember { mutableStateOf<Pair<HostName, LoginFlow>?>(null) }
    Column(Modifier.fillMaxSize().navigationBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BackButton(onBack)
            Text(copyText("mobOverview.screen.title"), style = MaterialTheme.typography.titleMedium)
        }
        LazyColumn(Modifier.fillMaxSize()) {
            item { Head(if (ov.uncertain) copyText("tabBar.needs.label") else copyText("tabBar.needs.aria", "n" to ov.total)) }
            items(ov.needs) { line ->
                when (line) {
                    is NeedsLine.Session -> SessionLine(line) { onOpen(line.host.id, line.row.sid) }
                    is NeedsLine.Plan -> PlanLine(line)
                    is NeedsLine.Manual ->
                        ManualLine(line, ov.manyHosts) { item ->
                            item.fix?.let { fix -> login = line.host to LoginFlow(backends.of(line.host.id), line.host.label, fix.args, scope).also { it.start() } }
                        }
                }
            }
            if (ov.needs.isEmpty()) item { Note(copyText("needs.panel.empty")) }
            items(ov.down) { h -> Note(copyText("needs.panel.offline", "machine" to h.label), dashed = true) }
            items(ov.unreadable) { (h, said) -> Note(copyText("needs.panel.unreadable", "machine" to h.label), dashed = true, more = said) }
            if (ov.running.isNotEmpty()) {
                item { Head(copyText("tabBar.group.sumRunning", "n" to ov.running.size)) }
                items(ov.running) { r -> RunningRow(r) { onOpen(r.host.id, r.sid) } }
            }
        }
    }
    login?.let { (h, f) ->
        LoginSheet(f, null, null, h.label, onClosed = {
            login = null
            board.reask(h.id)
        })
    }
}

@Composable
private fun Head(text: String) {
    Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

@Composable
private fun SessionLine(
    line: NeedsLine.Session,
    onClick: () -> Unit,
) {
    val n = line.row.needs
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        StatusDot(SessionDot.NEEDS_YOU, n.text)
        Column(Modifier.weight(1f)) {
            Text(line.row.label, style = MaterialTheme.typography.bodyMedium)
            Text(n.text, style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.warn)
        }
        DurationFormat.waited(n.waitedMs, 0L, 0L)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
private fun PlanLine(line: NeedsLine.Plan) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        PhIcon(Ph.CheckSquare, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Column(Modifier.weight(1f)) {
            Text(line.slice, style = MaterialTheme.typography.bodyMedium)
            Text(line.workspace, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (line.n > 1) Text(line.n.toString(), style = MaterialTheme.typography.labelLarge, color = LocalAppTokens.current.warn)
    }
}

@Composable
private fun ManualLine(
    line: NeedsLine.Manual,
    manyHosts: Boolean,
    onLogin: (ManualItem) -> Unit,
) {
    val item = line.item
    val fix = item.fix
    val second = listOfNotNull(line.host.label.takeIf { manyHosts }, item.why.takeIf { fix == null }).joinToString(copyText("kit.text.sep"))
    val base = Modifier.fillMaxWidth()
    Row(
        (if (fix != null) base.clickable { onLogin(item) } else base).padding(horizontal = 16.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIcon(if (item.kind == "mcpLogin") Ph.Key else Ph.UserCircle, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Column(Modifier.weight(1f)) {
            Text(item.said, style = MaterialTheme.typography.bodyMedium)
            if (second.isNotEmpty()) Text(second, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (fix != null) OutlinedButton(onClick = { onLogin(item) }) { Text(copyText("newSession.account.login")) }
    }
}

@Composable
private fun RunningRow(
    r: RunningLine,
    onClick: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        StatusDot(SessionDot.RUNNING, r.title)
        Column(Modifier.weight(1f)) {
            Text(r.title, style = MaterialTheme.typography.bodyMedium)
            r.cwd?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

/** 组底那一句（断着 · 读不出 ⇒ 虚线框；读不出那一句长按看那台原话）。 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Note(
    text: String,
    dashed: Boolean = false,
    more: String? = null,
) {
    var open by remember { mutableStateOf(false) }
    val m = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)
    Column(
        (if (dashed) m.dashedFrame(MaterialTheme.colorScheme.outline) else m)
            .let { if (more != null) it.combinedClickable(onClick = {}, onLongClick = { open = !open }) else it }
            .padding(10.dp),
    ) {
        Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (open && more != null) Text(more, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
