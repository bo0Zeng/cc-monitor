package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.conversationsHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 「对话」这一屏的入口：先建 SSH 连接（`HostLink`），通了再在上面接那台的常驻流、读核心的清单。
 * 顶栏同聊天屏：抽屉按钮 ＋「对话」＋ 这台的名字；抽屉与聊天屏是同一个。
 */
@Composable
fun ConversationsRoute(
    hostId: String,
    onOpen: (sessionId: String) -> Unit,
    onDrawerAction: (DrawerAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    val link by rememberHostLink(hostId, conversationsHolder(hostId))
    val machine = rememberMachineName(hostId)
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = { ConversationsTopBar(machine, onOpenDrawer = { scope.launch { drawerState.open() } }) },
        ) { inner ->
            when (val l = link) {
                is HostLink.Failed -> HostLinkFailed(l.why, Modifier.padding(inner))
                HostLink.Connecting -> Unit
                HostLink.Ready ->
                    if (machine != null) {
                        val vm = conversationsViewModel(hostId, machine)
                        val state by vm.state.collectAsStateWithLifecycle()
                        ConversationsScreen(state, machine, onOpen, vm::retry, Modifier.fillMaxSize().padding(inner))
                    }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ConversationsTopBar(
    machine: String?,
    onOpenDrawer: () -> Unit,
) {
    TopAppBar(
        navigationIcon = { PhIconButton(Ph.List, DRAWER_OPEN_DESCRIPTION, onClick = onOpenDrawer, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        title = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.sm)) {
                Text(copyText("mobDrawer.item.chats"), style = MaterialTheme.typography.titleMedium)
                machine?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = LocalAppTokens.current.bg2, scrolledContainerColor = LocalAppTokens.current.bg2),
    )
}

/** 这台在手机上的名字（机器表里的 `label`）；还没读到 ⇒ `null`。 */
@Composable
private fun rememberMachineName(hostId: String): String? {
    val hosts = koinInject<HostRepository>()
    val name by produceState<String?>(null, hostId) { value = runCatching { hosts.get(hostId)?.label }.getOrNull() ?: hostId }
    return name
}

/** 一台的清单 VM。「对话」这一屏与抽屉「最近」取同一个（同一个 key）。 */
@Composable
private fun conversationsViewModel(
    hostId: String,
    machine: String,
): ConversationsViewModel = koinViewModel(key = "conversations-$hostId") { parametersOf(hostId, machine) }

/**
 * 抽屉「最近」：与「对话」这一屏同一个来源。
 *
 * @param linkReady 这台的 SSH 连接通了没有。没通 ⇒ `null`。
 * @return `null` ＝ 还没拿到；拿到后是清单最前面的那些行（核心已按最后活动排好）。
 */
@Composable
fun rememberRecent(
    hostId: String,
    linkReady: Boolean,
): List<HistoryItem>? {
    val machine = rememberMachineName(hostId)
    if (!linkReady || machine == null) return null
    val state by conversationsViewModel(hostId, machine).state.collectAsStateWithLifecycle()
    return state.recent.takeIf { !state.listLoading }
}

/** SSH 都接不上时那一句（建连那一层给的原因）。 */
@Composable
private fun HostLinkFailed(
    why: String,
    modifier: Modifier = Modifier,
) {
    Text(why, modifier = modifier.fillMaxSize().padding(24.dp))
}
