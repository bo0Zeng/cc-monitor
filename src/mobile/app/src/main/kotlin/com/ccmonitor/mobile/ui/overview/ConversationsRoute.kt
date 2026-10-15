package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarController
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
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
    val vm = machine?.let { conversationsViewModel(hostId, it) }
    val state = vm?.state?.collectAsStateWithLifecycle()?.value
    val snackbar = LocalAppSnackbar.current
    var filtering by remember { mutableStateOf(false) }
    if (filtering && vm != null && state != null) ConversationsFilterSheet(state.showHidden, vm::showHidden, onDismiss = { filtering = false })
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = {
                ConversationsTopBar(
                    machine,
                    filtered = state?.showHidden == true,
                    onFilter = { filtering = true }.takeIf { vm != null },
                    onOpenDrawer = { scope.launch { drawerState.open() } },
                )
            },
        ) { inner ->
            // 连没连上都先画清单：本地缓存里有上次那一份就照画（离线打开看得到），连上以后换成那台的。
            if (machine != null && vm != null && state != null) {
                val onHide: (HistoryItem) -> Unit = { item -> scope.launch { toggleHidden(vm, snackbar, item) } }
                when (val l = link) {
                    is HostLink.Failed ->
                        Column(Modifier.fillMaxSize().padding(inner)) {
                            HostLinkFailed(l.why)
                            if (state.sections.isNotEmpty()) ConversationsScreen(state, machine, onOpen, vm::retry, onHide)
                        }
                    HostLink.Connecting -> if (state.sections.isNotEmpty()) ConversationsScreen(state, machine, onOpen, vm::retry, onHide, Modifier.fillMaxSize().padding(inner))
                    HostLink.Ready -> ConversationsScreen(state, machine, onOpen, vm::retry, onHide, Modifier.fillMaxSize().padding(inner))
                }
            }
        }
    }
}

/**
 * 长按菜单「隐藏 / 取消隐藏」：交单个那一格（`on` / `off`，与桌面历史同一个口）。隐藏成了给撤销，撤销把改之前那一格原样交回去；
 * 取消隐藏是明说要显示，不给撤销。撤销在应用根部的反馈里跑（这一屏关了也照做）。
 */
private suspend fun toggleHidden(
    vm: ConversationsViewModel,
    snackbar: AppSnackbarController,
    item: HistoryItem,
) {
    val hiding = item.hidden == null
    when (val r = vm.setHide(item.sessionId, if (hiding) Hide.ON else Hide.OFF)) {
        is Fetched.Got ->
            if (hiding) {
                snackbar.showUndo(copyText("history.hide.done", "label" to item.label)) {
                    (vm.setHide(item.sessionId, r.value.prev) as? Fetched.Failed)?.let { snackbar.show(Notice(it.problem.text, it.problem.detail)) }
                }
            }
        is Fetched.Failed -> snackbar.show(Notice(copyText("history.hide.failed"), listOf(r.problem.text, r.problem.detail).filter(String::isNotBlank).joinToString("\n")))
        Fetched.Loading -> Unit
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ConversationsTopBar(
    machine: String?,
    filtered: Boolean,
    onFilter: (() -> Unit)?,
    onOpenDrawer: () -> Unit,
) {
    TopAppBar(
        actions = {
            // 过滤（同桌面历史页的「筛选」）：开着哪一样 ⇒ 漏斗着强调色。
            onFilter?.let {
                PhIconButton(Ph.Funnel, copyText("history.page.filter"), onClick = it, tint = if (filtered) LocalAppTokens.current.accent else MaterialTheme.colorScheme.onSurfaceVariant)
            }
        },
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
    Text(why, modifier = modifier.padding(24.dp))
}
