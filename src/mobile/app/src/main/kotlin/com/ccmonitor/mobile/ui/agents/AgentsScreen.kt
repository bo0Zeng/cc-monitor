package com.ccmonitor.mobile.ui.agents

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.RunInfo
import com.ccmonitor.mobile.core.wire.gen.RunState
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionRuns
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.common.rememberHostLink
import org.koin.compose.koinInject

/** ⋯ 里「agent」那一项的字：那台写好的那一句（`session_runs.summary`）；这条会话没有运行表 ⇒ `null`、不出这一项。 */
fun agentsEntry(
    book: SessionBook,
    sid: String,
): String? = book.runs[sid]?.summary

/** 列表的三组（照那台 `groups` 的先后，不按 `state` 自己分）。 */
enum class RunGroup { RUNNING, RECENT, OLDER }

/** 列表里的一项：组头 · 一行。 */
sealed interface AgentsItem {
    data class Head(
        val group: RunGroup,
    ) : AgentsItem

    data class Row(
        val info: RunInfo,
    ) : AgentsItem
}

/** 列表照 `groups` 排：三组各带组头，空的那一组不出；`groups` 里点到运行表里没有的不出行。 */
fun agentsList(t: SessionRuns): List<AgentsItem> {
    val byRun = t.runs.associateBy { it.run }
    val groups = listOf(RunGroup.RUNNING to t.groups.running, RunGroup.RECENT to t.groups.recent, RunGroup.OLDER to t.groups.older)
    return groups.flatMap { (g, ids) ->
        val rows = ids.mapNotNull { byRun[it] }.map { AgentsItem.Row(it) }
        if (rows.isEmpty()) emptyList() else listOf(AgentsItem.Head(g)) + rows
    }
}

/**
 * agent 列表屏（聊天屏 ⋯ → agent）：这条会话（[sid]）派出的子 agent，数据是那台核心折进会话簿的运行表（`ResidentFeed.table`），不另问。
 * 一行：状态点 · 名字 · 状态 · 最近一步（字都是那台写好的）；点一行进 agent 屏。
 */
@Composable
fun AgentsScreen(
    hostId: String,
    sid: String,
    onBack: () -> Unit,
    onOpen: (RunInfo) -> Unit,
) {
    rememberHostLink(hostId, "agents-$hostId")
    val backends = koinInject<HostBackends>()
    val feed = remember(hostId) { backends.of(hostId) }
    val table by feed.table.collectAsState()
    val items = table.runs[sid]?.let(::agentsList).orEmpty()
    Scaffold(topBar = { AgentsTopBar(copyText("agentsPanel.page.title"), table.runs[sid]?.summary.orEmpty(), onBack) }) { inner ->
        if (items.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(inner), contentAlignment = Alignment.Center) {
                Text(copyText("agentsPanel.page.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize().padding(inner)) {
                items(items, key = { it.key() }) { item ->
                    when (item) {
                        is AgentsItem.Head -> GroupHead(item.group)
                        is AgentsItem.Row -> RunRowView(item.info) { onOpen(item.info) }
                    }
                }
            }
        }
    }
}

private fun AgentsItem.key(): String =
    when (this) {
        is AgentsItem.Head -> "head#${group.name}"
        is AgentsItem.Row -> "run#${info.run}"
    }

/** 组头：在跑 · 最近结束照表里那两条；更早那一组只画分隔线（带个数的组头要那台写好，见 A2-4-4-1-1）。 */
@Composable
private fun GroupHead(g: RunGroup) {
    val text =
        when (g) {
            RunGroup.RUNNING -> copyText("agentsPanel.group.running")
            RunGroup.RECENT -> copyText("agentsPanel.group.recent")
            RunGroup.OLDER -> null
        }
    if (text == null) {
        HorizontalDivider(Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
    } else {
        Text(
            text,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp),
        )
    }
}

/** 一行：状态点 · 名字 · 状态 · 最近一步。 */
@Composable
internal fun RunRowView(
    info: RunInfo,
    onClick: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(8.dp).background(stateColor(info.state), CircleShape))
        Column(Modifier.weight(1f)) {
            Text(info.labelText, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val second = listOfNotNull(info.statusText, info.lastText).filter { it.isNotEmpty() }
            if (second.isNotEmpty()) {
                Text(
                    second.joinToString(copyText("kit.text.sep")),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** 状态点的颜色（只挑色，字是那台写好的 `status_text`）。 */
@Composable
internal fun stateColor(s: RunState): Color {
    val t = LocalAppTokens.current
    return when (s) {
        RunState.RUNNING -> t.stateActive
        RunState.DONE -> t.success
        RunState.FAILED -> t.errorStrong
        RunState.STOPPED -> t.textFaint
        else -> t.warn
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AgentsTopBar(
    title: String,
    second: String,
    onBack: () -> Unit,
) {
    TopAppBar(
        navigationIcon = {
            IconButton(onClick = onBack, modifier = Modifier.semantics { contentDescription = copyText("mobKit.act.back") }) {
                Text(BACK, style = MaterialTheme.typography.titleLarge)
            }
        },
        title = {
            Column {
                Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (second.isNotEmpty()) {
                    Text(second, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        },
    )
}

private const val BACK = "←"
