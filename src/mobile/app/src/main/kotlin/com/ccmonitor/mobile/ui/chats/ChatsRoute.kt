package com.ccmonitor.mobile.ui.chats

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.FilesDeleteSession
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
 * 顶栏：抽屉 · 会话 · 🔍（换成搜索框，返回先收框）· 漏斗（有筛选生效时强调色）。手指在滚时列表冻住，松手换最新的。
 */
@Composable
fun ChatsRoute(
    hostId: String,
    onOpen: (hostId: String, sessionId: String) -> Unit,
    onNewInDir: (hostId: String, cwd: String) -> Unit,
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
    val listState = rememberLazyListState()
    LaunchedEffect(listState.isScrollInProgress) { if (listState.isScrollInProgress) board.hold() else board.release() }
    var filtering by remember { mutableStateOf(false) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var typed by rememberSaveable { mutableStateOf("") }
    val closeSearch = {
        searching = false
        typed = ""
        board.query(null)
    }
    BackHandler(enabled = searching, onBack = closeSearch)
    var menuFor by remember { mutableStateOf<ChatRow?>(null) }
    if (filtering) FilterSheet(ui, board::filters, onDismiss = { filtering = false })
    menuFor?.let { r -> RowSheets(board, r, scope, onNewInDir, onDismiss = { menuFor = null }) }
    val act =
        ChatsActions(
            open = onOpen,
            menu = { menuFor = it },
            retry = { b -> retry(board, links, attempts, b) },
            deploy = { id -> board.chats(id)?.deploy?.invoke() },
            filters = board::filters,
            newInDir = onNewInDir,
            searchContent = board::searchContent,
        )
    ChatDrawer(hostId, links[hostId] is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = {
                if (searching) {
                    SearchBar(typed, onType = {
                        typed = it
                        board.query(it)
                    }, onClose = closeSearch)
                } else {
                    ChatsTopBar(ui.filters.active, onSearch = { searching = true }, onFilter = { filtering = true }, onOpenDrawer = { scope.launch { drawerState.open() } })
                }
            },
        ) { inner -> ChatsScreen(ui, act, listState, Modifier.fillMaxSize().padding(inner)) }
    }
}

/**
 * 长按一行：抽屉（[RowMenu]）· 改标题框 · 删除确认框。交那台的那一问在屏的协程 [scope] 里跑，结果出在应用根部的反馈里。
 * 抽屉点了要弹框的那一项先关抽屉、留住那一行，框关了才算收完（[onDismiss]）。
 */
@Composable
private fun RowSheets(
    board: ChatsBoard,
    r: ChatRow,
    /** 屏的协程（抽屉关了照跑完）。 */
    scope: CoroutineScope,
    onNewInDir: (hostId: String, cwd: String) -> Unit,
    onDismiss: () -> Unit,
) {
    val snackbar = LocalAppSnackbar.current
    var dialog by remember(r) { mutableStateOf<RowDialog?>(null) }
    val on = { job: suspend (HostChats) -> Unit -> scope.launch { board.chats(r.hostId)?.let { job(it) } } }
    when (dialog) {
        null -> {
            val ops =
                MenuOps(
                    star = { on { annotate(it, snackbar, HistoryAnnotate.star(r.sessionId, !r.starred), copyText("history.star.failed")) } },
                    rename = { dialog = RowDialog.RENAME },
                    hide = { on { toggleHidden(it, snackbar, r) } },
                    newInDir = { onNewInDir(r.hostId, r.projectPath) },
                    delete = { dialog = RowDialog.DELETE },
                )
            // 点了要弹框的那一项：抽屉照关（ActionSheet 点完就关），可那一行留着给框用。
            RowMenu(r, ops, onDismiss = { if (dialog == null) onDismiss() })
        }
        RowDialog.RENAME ->
            RenameDialog(r, onDismiss = onDismiss) { title ->
                on { annotate(it, snackbar, HistoryAnnotate.rename(r.sessionId, title), copyText("history.rename.failed"), done = copyText("history.rename.done")) }
            }
        RowDialog.DELETE ->
            DeleteDialog(r, board.chats(r.hostId)?.machine ?: r.hostId, onDismiss = onDismiss) {
                on { deleteSession(it, snackbar, r) }
            }
    }
}

/** 长按之后弹的那个框。 */
private enum class RowDialog { RENAME, DELETE }

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
        is Fetched.Failed -> snackbar.show(Notice(copyText("history.hide.failed"), detailOf(r)))
        Fetched.Loading -> Unit
    }
}

/** 标星 · 改标题：交那台改那一格；成了重问清单（[done] 有就出一句），没成照那台那一句报。 */
private suspend fun annotate(
    chats: HostChats,
    snackbar: AppSnackbarController,
    args: Map<String, Any?>,
    failed: String,
    done: String? = null,
) {
    when (val r = chats.call(HistoryAnnotate.COMMAND, args, HistoryAnnotate::of)) {
        is Fetched.Got -> {
            chats.fetch()
            done?.let(snackbar::show)
        }
        is Fetched.Failed -> snackbar.show(Notice(failed, detailOf(r)))
        Fetched.Loading -> Unit
    }
}

/** 删一条历史会话（那台删记录文件连同附属）；成了重问清单。 */
private suspend fun deleteSession(
    chats: HostChats,
    snackbar: AppSnackbarController,
    row: ChatRow,
) {
    when (val r = chats.call(FilesDeleteSession.COMMAND, FilesDeleteSession.args(row.sessionId)) { }) {
        is Fetched.Got -> {
            chats.fetch()
            snackbar.show(copyText("history.delete.done", "label" to row.label))
        }
        is Fetched.Failed -> snackbar.show(Notice(copyText("history.delete.failed", "label" to row.label), detailOf(r)))
        Fetched.Loading -> Unit
    }
}

private fun detailOf(f: Fetched.Failed): String = listOf(f.problem.text, f.problem.detail).filter(String::isNotBlank).joinToString("\n")

private suspend fun setHide(
    chats: HostChats,
    sid: String,
    pin: Hide?,
) = chats.call(HistoryAnnotate.COMMAND, HistoryAnnotate.hide(sid, pin), HistoryAnnotate::of).also { if (it is Fetched.Got) chats.fetch() }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatsTopBar(
    filtered: Boolean,
    onSearch: () -> Unit,
    onFilter: () -> Unit,
    onOpenDrawer: () -> Unit,
) {
    TopAppBar(
        actions = {
            PhIconButton(Ph.MagnifyingGlass, copyText("mobChats.search.aria"), onClick = onSearch, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            PhIconButton(Ph.Funnel, copyText("history.page.filter"), onClick = onFilter, tint = if (filtered) LocalAppTokens.current.accent else MaterialTheme.colorScheme.onSurfaceVariant)
        },
        navigationIcon = { PhIconButton(Ph.List, DRAWER_OPEN_DESCRIPTION, onClick = onOpenDrawer, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        title = { Text(copyText("mobDrawer.item.chats"), style = MaterialTheme.typography.titleMedium) },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = LocalAppTokens.current.bg2, scrolledContainerColor = LocalAppTokens.current.bg2),
    )
}

/** 搜索框顶栏：← 收起（同返回）· 框（边打边搜标题）。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SearchBar(
    text: String,
    onType: (String) -> Unit,
    onClose: () -> Unit,
) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    TopAppBar(
        navigationIcon = { PhIconButton(Ph.ArrowLeft, copyText("mobKit.act.back"), onClick = onClose, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        title = {
            TextField(
                value = text,
                onValueChange = onType,
                placeholder = { Text(copyText("mobChats.search.placeholder")) },
                singleLine = true,
                colors =
                    TextFieldDefaults.colors(
                        focusedContainerColor = Color.Transparent,
                        unfocusedContainerColor = Color.Transparent,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
                modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag(ChatsTags.SEARCH),
            )
        },
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
