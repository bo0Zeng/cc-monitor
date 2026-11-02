package com.ccmonitor.mobile.ui.agents

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.RunInfo
import com.ccmonitor.mobile.core.wire.gen.RunState
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionRuns
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.common.MachineBar
import com.ccmonitor.mobile.ui.common.rememberHostLink
import org.koin.compose.koinInject

/** ⋯ 里「agent」那一项的字：那台写好的那一句（`session_runs.summary`）；这条会话没有运行表 ⇒ `null`、不出这一项。 */
fun agentsEntry(
    book: SessionBook,
    sid: String,
): String? = book.runs[sid]?.summary

/** 列表的三组（照那台 `groups` 的先后，不按 `state` 自己分）。 */
enum class RunGroup { RUNNING, RECENT, OLDER }

/** 列表里的一项：组头（字）· 一行。 */
sealed interface AgentsItem {
    data class Head(
        val group: RunGroup,
        val text: String,
    ) : AgentsItem

    data class Row(
        val info: RunInfo,
    ) : AgentsItem
}

/**
 * 列表照 `groups` 排：三组各带组头，空的那一组不出；`groups` 里点到运行表里没有的不出行。
 * 更早那一组的组头字照那台写好的 `older_text`；收着（[olderOpen] 假）只出组头，开合是界面记的。
 */
fun agentsList(
    t: SessionRuns,
    olderOpen: Boolean,
): List<AgentsItem> {
    val byRun = t.runs.associateBy { it.run }
    val groups =
        listOf(
            Triple(RunGroup.RUNNING, t.groups.running, copyText("runs.state.running")),
            Triple(RunGroup.RECENT, t.groups.recent, copyText("agentsPanel.group.recent")),
            Triple(RunGroup.OLDER, t.groups.older, t.groups.olderText.orEmpty()),
        )
    return groups.flatMap { (g, ids, text) ->
        val rows = ids.mapNotNull { byRun[it] }.map { AgentsItem.Row(it) }
        when {
            rows.isEmpty() -> emptyList()
            g == RunGroup.OLDER && !olderOpen -> listOf(AgentsItem.Head(g, text))
            else -> listOf(AgentsItem.Head(g, text)) + rows
        }
    }
}

/**
 * agent 列表屏（聊天屏 ⋯ → agent）：这条会话（[sid]）派出的子 agent，数据是那台核心折进会话簿的运行表（`ResidentFeed.table`），不另问。
 * 顶上机器条（那台离线 ⇒ 虚线条 ＋［重新连接］，照画手上那份）；一行：状态点 · 名字 · 类别小框 · 状态 · 最近一步（字都是那台写好的），
 * 在等人 ⇒ 左琥珀条；点一行进 agent 屏。更早那一组收着，点组头开合。
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
    var olderOpen by rememberSaveable(sid) { mutableStateOf(false) }
    val items = table.runs[sid]?.let { agentsList(it, olderOpen) }.orEmpty()
    Scaffold(topBar = { AgentsTopBar(copyText("beProfile.slot.agent"), table.runs[sid]?.summary.orEmpty(), onBack) }) { inner ->
        Column(Modifier.fillMaxSize().padding(inner)) {
            MachineBar(hostId)
            if (items.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text(copyText("agentsPanel.page.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    items(items, key = { it.key() }) { item ->
                        when (item) {
                            is AgentsItem.Head -> GroupHead(item, olderOpen) { olderOpen = !olderOpen }
                            is AgentsItem.Row -> RunRowView(item.info) { onOpen(item.info) }
                        }
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

/** 组头：字照 [AgentsItem.Head.text]；更早那一组的组头点了开合（尖朝右 ⇒ 收着）。 */
@Composable
private fun GroupHead(
    head: AgentsItem.Head,
    olderOpen: Boolean,
    toggle: () -> Unit,
) {
    val older = head.group == RunGroup.OLDER
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (older) Modifier.heightIn(min = Touch.target).clickable(onClick = toggle) else Modifier)
            .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (older) {
            PhIcon(if (olderOpen) Ph.CaretDown else Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(head.text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** 一行：状态点 · 名字；第二行类别小框 · 状态 · 最近一步。在等人（`needs`）⇒ 左琥珀条、第二行琥珀。 */
@Composable
internal fun RunRowView(
    info: RunInfo,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val needs = info.needs != null
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .then(if (needs) Modifier.drawBehind { drawRect(tokens.warn, size = Size(3.dp.toPx(), size.height)) } else Modifier)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(8.dp).background(stateColor(info.state), CircleShape))
        Column(Modifier.weight(1f)) {
            Text(info.labelText, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                info.kind?.takeIf { it.isNotEmpty() }?.let { KindBox(it) }
                val second = listOfNotNull(info.statusText, info.lastText).filter { it.isNotEmpty() }
                if (second.isNotEmpty()) {
                    Text(
                        second.joinToString(copyText("kit.text.sep")),
                        style = MaterialTheme.typography.bodySmall,
                        color = if (needs) tokens.warn else MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

/** 类别小框（`kind` 原样，那台交的）。 */
@Composable
internal fun KindBox(kind: String) {
    val tokens = LocalAppTokens.current
    Text(
        kind,
        style = monoSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        modifier = Modifier.border(1.dp, tokens.borderFaint, RoundedCornerShape(tokens.radius.s)).padding(horizontal = 4.dp),
    )
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
