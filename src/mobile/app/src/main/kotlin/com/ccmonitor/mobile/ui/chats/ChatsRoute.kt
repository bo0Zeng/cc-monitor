package com.ccmonitor.mobile.ui.chats

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.HistoryAnnotate
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarController
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.conversationsHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 会话屏的入口：勾上的每台各建一条 SSH 持有（离屏放手），通了那台的常驻流就接上、问清单；几台并成一列交 [ChatsScreen]。
 * 顶栏：抽屉 · 会话 · 漏斗（有筛选生效时强调色）。手指在滚时列表冻住，松手换最新的。
 */
@Composable
fun ChatsRoute(
    hostId: String,
    onOpen: (hostId: String, sessionId: String) -> Unit,
    onDrawerAction: (DrawerAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    val board: ChatsBoard = koinViewModel(key = "chats-$hostId") { parametersOf(hostId) }
    val ui by board.ui.collectAsStateWithLifecycle()
    val checked by board.checked.collectAsStateWithLifecycle()
    val attempts = remember { mutableStateMapOf<String, Int>() }
    val links = remember { mutableStateMapOf<String, HostLink>() }
    for (c in checked) {
        key(c.hostId) {
            val link by rememberHostLink(c.hostId, conversationsHolder(c.hostId), attempt = attempts[c.hostId] ?: 0)
            LaunchedEffect(link) {
                links[c.hostId] = link
                board.ssh(c.hostId, link)
            }
        }
    }
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val snackbar = LocalAppSnackbar.current
    val listState = rememberLazyListState()
    LaunchedEffect(listState.isScrollInProgress) { if (listState.isScrollInProgress) board.hold() else board.release() }
    var filtering by remember { mutableStateOf(false) }
    var menuFor by remember { mutableStateOf<ChatRow?>(null) }
    if (filtering) FilterSheet(ui, board::filters, onDismiss = { filtering = false })
    menuFor?.let { r -> RowMenu(r, onHide = { scope.launch { board.chats(r.hostId)?.let { toggleHidden(it, snackbar, r) } } }, onDismiss = { menuFor = null }) }
    val act =
        ChatsActions(
            open = { onOpen(it.hostId, it.sessionId) },
            menu = { menuFor = it },
            retry = { b -> retry(board, links, attempts, b) },
            deploy = { id -> board.chats(id)?.deploy?.invoke() },
            filters = board::filters,
        )
    ChatDrawer(hostId, links[hostId] is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = { ChatsTopBar(ui.filters.active, onFilter = { filtering = true }, onOpenDrawer = { scope.launch { drawerState.open() } }) },
        ) { inner -> ChatsScreen(ui, act, listState, Modifier.fillMaxSize().padding(inner)) }
    }
}

/** 一台的［重试］/［重新连接］：SSH 那层接不上 ⇒ 整条重建；否则交那台（断着重接、通着重问）。 */
private fun retry(
    board: ChatsBoard,
    links: Map<String, HostLink>,
    attempts: MutableMap<String, Int>,
    b: HostBar,
) {
    if (links[b.hostId] is HostLink.Failed) attempts[b.hostId] = (attempts[b.hostId] ?: 0) + 1 else board.chats(b.hostId)?.retry()
}

/**
 * 长按菜单「隐藏 / 取消隐藏」：交单个那一格（`on` / `off`，与桌面历史同一个口）。隐藏成了给撤销，撤销把改之前那一格原样交回去；
 * 取消隐藏是明说要显示，不给撤销。撤销在应用根部的反馈里跑（这一屏关了也照做）。
 */
private suspend fun toggleHidden(
    chats: HostChats,
    snackbar: AppSnackbarController,
    row: ChatRow,
) {
    val hiding = row.hidden == null
    when (val r = setHide(chats, row.sessionId, if (hiding) Hide.ON else Hide.OFF)) {
        is Fetched.Got ->
            if (hiding) {
                snackbar.showUndo(copyText("history.hide.done", "label" to row.label)) {
                    (setHide(chats, row.sessionId, r.value.prev) as? Fetched.Failed)?.let { snackbar.show(Notice(it.problem.text, it.problem.detail)) }
                }
            }
        is Fetched.Failed -> snackbar.show(Notice(copyText("history.hide.failed"), listOf(r.problem.text, r.problem.detail).filter(String::isNotBlank).joinToString("\n")))
        Fetched.Loading -> Unit
    }
}

private suspend fun setHide(
    chats: HostChats,
    sid: String,
    pin: Hide?,
) = chats.call(HistoryAnnotate.COMMAND, HistoryAnnotate.hide(sid, pin), HistoryAnnotate::of).also { if (it is Fetched.Got) chats.fetch() }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatsTopBar(
    filtered: Boolean,
    onFilter: () -> Unit,
    onOpenDrawer: () -> Unit,
) {
    TopAppBar(
        actions = {
            PhIconButton(Ph.Funnel, copyText("history.page.filter"), onClick = onFilter, tint = if (filtered) LocalAppTokens.current.accent else MaterialTheme.colorScheme.onSurfaceVariant)
        },
        navigationIcon = { PhIconButton(Ph.List, DRAWER_OPEN_DESCRIPTION, onClick = onOpenDrawer, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        title = { Text(copyText("mobDrawer.item.chats"), style = MaterialTheme.typography.titleMedium) },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = LocalAppTokens.current.bg2, scrolledContainerColor = LocalAppTokens.current.bg2),
    )
}

/** 抽屉「最近」：这台一份清单（没筛选），隐藏的不列。 */
class RecentChats(
    make: (CoroutineScope) -> HostChats,
) : ViewModel() {
    private val chats = make(viewModelScope)

    /** `null` ＝ 还没拿到；拿到后是清单最前面那些行（核心已按最后活动排好）。 */
    val recent: StateFlow<List<ChatRow>?> =
        chats.state
            .map { s -> hostPart(s).takeIf { !it.loading }?.rows?.filter { it.hidden == null } }
            .stateIn(viewModelScope, SharingStarted.Eagerly, null)
}

/**
 * 抽屉「最近」。
 *
 * @param linkReady 这台的 SSH 连接通了没有。没通 ⇒ `null`。
 */
@Composable
fun rememberRecent(
    hostId: String,
    linkReady: Boolean,
): List<ChatRow>? {
    val hosts = koinInject<HostRepository>()
    val machine by produceState<String?>(null, hostId) { value = runCatching { hosts.get(hostId)?.label }.getOrNull()?.ifBlank { null } ?: hostId }
    val name = machine
    if (!linkReady || name == null) return null
    val vm: RecentChats = koinViewModel(key = "recent-$hostId") { parametersOf(hostId, name) }
    return vm.recent.collectAsStateWithLifecycle().value
}
