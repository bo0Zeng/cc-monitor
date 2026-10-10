package com.ccmonitor.mobile.ui.nav

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.ccmonitor.mobile.agent.agentKindOrDefault
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarHostScaffold
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatRoute
import com.ccmonitor.mobile.ui.chat.chatKey
import com.ccmonitor.mobile.ui.chat.newConversationId
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import com.ccmonitor.mobile.ui.drawer.DrawerNav
import com.ccmonitor.mobile.ui.host.HostEditorScreen
import com.ccmonitor.mobile.ui.host.HostListScreen
import com.ccmonitor.mobile.ui.identity.IdentityScreen
import com.ccmonitor.mobile.ui.overview.ConversationsRoute
import com.ccmonitor.mobile.ui.session.SessionScreen
import com.ccmonitor.mobile.ui.session.SessionTabManager
import com.ccmonitor.mobile.ui.session.SessionTabsSheet
import com.ccmonitor.mobile.ui.session.SessionsOverviewScreen
import com.ccmonitor.mobile.ui.settings.ButtonSettingsScreen
import com.ccmonitor.mobile.ui.settings.SettingsScreen
import com.ccmonitor.mobile.ui.sftp.SftpScreen
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/**
 * 导航宿主。终端 tab 的列表、切换、新建由会话内工具条唤出 [SessionTabsSheet]，
 * 触发器在 imePadding 内，键盘弹起时也够得着。
 */
@Composable
fun AppNavHost() {
    val nav = rememberNavController()
    val tabManager = koinInject<SessionTabManager>() // 多 tab 总览状态 + per-key VM store
    // 新界面总开关，不是显式关就是开。显式关（`FLAG_OFF`）时点服务器开终端 tab、启动进服务器列表；屏上已无关掉的入口。
    val settings = koinInject<SettingsRepository>()
    val chatController = koinInject<ChatController>()
    // 初值是 null（还不知道），不是 false：写 false 的话第一帧就把「关」送进落地判定，
    // 导航去服务器列表并弹掉 Launch，真值到了也没人再看。
    val newUi by settings.newUiEnabled().collectAsStateWithLifecycle<Boolean?>(null)
    // 新对话的权限模式，白名单校验在 `ClaudeInvocation.permissionModeFlag`，这里只透传。
    // 注意：初值 null 是合法值（不带 `--permission-mode`）。若真值到之前管道就起了，第二趟会命中
    // `startOnceCommand` 的幂等支，第一趟的模式永久胜出而界面报成功。够不够得着 `startPipeFor`
    // 取决于 `ChatRoute` 的早退门，单元测试量不了。
    val permissionMode by settings.permissionMode().collectAsStateWithLifecycle(null)
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
    ) {
        // 全应用的操作反馈与撤销宿主。装在根部：撤销要跨屏活着。
        AppSnackbarHostScaffold {
            AppNavGraph(nav, tabManager, newUi, chatController, permissionMode)
        }
    }
}

@Composable
private fun AppNavGraph(
    nav: NavHostController,
    tabManager: SessionTabManager,
    /** 三值：`null` = 开关还没读到。只有 [launchRoute] 处理这个第三态。 */
    newUi: Boolean?,
    chatController: ChatController,
    permissionMode: String?,
) {
    // 开终端 tab 并给出路由，作为参数交给 [onConnectDestination]。
    val openTerminal: (String) -> String = { "session/${tabManager.open(it, SessionTabManager.TabTarget.Terminal())}" }
    // 点服务器不能「等一等」：不导航就是点了没反应。所以「还不知道」按仓库同一规则折成开
    // （`newUiEnabled` 判的是 `!= FLAG_OFF`）。app 里没有写 `FLAG_OFF` 的路径，真值恒为开，
    // 折下来也是开；若加回关掉的入口，这里要改成三值。
    val onConnect = rememberConnectAction(nav, newUi != false, openTerminal)
    // 起点是判定屏，见 [Screen.Launch]。
    NavHost(navController = nav, startDestination = Screen.Launch.route) {
        launchRoute(nav, newUi)
        hostsRoute(nav, tabManager, onConnect)
        chatRoutes(nav, tabManager, chatController, permissionMode)
        composable(Screen.SessionsOverview.route) {
            // 终端会话总览；「+新会话」进「新建会话 · 选主机」页。
            SessionsOverviewScreen(
                onOpen = { key -> nav.navigate("session/$key") },
                onNewSession = { nav.navigate(Screen.NewSessionHostPicker.route) },
                onBack = { nav.popBackStack() },
            )
        }
        composable(Screen.NewSessionHostPicker.route) {
            NewSessionHostPickerScreen(nav, tabManager)
        }
        composable(Screen.Settings.route) {
            // 设置面上的去处住 `settingsEntries()`，这里只把路由交给 nav。
            SettingsScreen(
                onBack = { nav.popBackStack() },
                onNavigate = { route -> nav.navigate(route) },
            )
        }
        composable(Screen.ButtonSettings.route) {
            ButtonSettingsScreen(hostId = null, onBack = { nav.popBackStack() })
        }
        composable(Screen.HostButtons.route) { entry ->
            val id = entry.arguments?.getString("id") ?: return@composable
            ButtonSettingsScreen(hostId = id, onBack = { nav.popBackStack() })
        }
        sessionRoute(nav, tabManager)
        composable(Screen.Identities.route) { IdentityScreen(onBack = { nav.popBackStack() }) }
        composable("host/new") { AddHostRoute(nav) }
        composable(Screen.HostEdit.route) { entry ->
            val id = entry.arguments?.getString("id") ?: return@composable
            HostEditorScreen(
                hostId = id,
                onDone = { nav.popBackStack() },
                onEditButtons = { nav.navigate("host/$id/buttons") },
            )
        }
    }
}

/**
 * Session 路由：tab 宿主按 kind 分派 SessionScreen / SftpScreen。
 *
 * 注意：切 tab 用 launchSingleTop + popUpTo(Hosts) 防返回栈堆积，且目标是当前 tab 时必须短路：
 * `popUpTo` 先于 `launchSingleTop` 去重执行，会先弹掉 session/$key 再比对，结果重建当前 entry、
 * 重置 rememberSaveable 并闪一下。
 */
private fun NavGraphBuilder.sessionRoute(
    nav: NavHostController,
    tabManager: SessionTabManager,
) {
    composable(Screen.Session.route) { entry ->
        val key = entry.arguments?.getString("key") ?: return@composable
        // SFTP tab「在此打开终端」开一个终端 tab。
        HostedSession(
            tabManager,
            key,
            onGone = { nav.popBackStack() },
            onOpenTerminalTab = { hid, path ->
                nav.navigate("session/${tabManager.open(hid, SessionTabManager.TabTarget.Terminal(cd = path))}")
            },
            // tmux 附着开新终端 tab，同名复用；复用可能命中当前 tab，照上面的规则短路。
            onOpenTmuxTab = { hid, name ->
                val k = tabManager.open(hid, SessionTabManager.TabTarget.Terminal(tmuxSession = name))
                if (k != key) {
                    nav.navigate("session/$k") {
                        launchSingleTop = true
                        popUpTo(Screen.Hosts.route)
                    }
                }
            },
            // resume 开新终端 tab，按 sid 去重；shell 里跑 `<cc/cct> --resume <sid>`，cwd 由远端按 sid 定。
            // launcherId 继承发起 tab；title 作 tab 名，command 是长按选定的命令，都不进去重键。
            // 同 sid 再 resume 会命中当前 tab，照上面的规则短路。
            onOpenResumeTab = { hid, cwd, sid, launcherId, title, command ->
                val k =
                    tabManager.open(
                        hid,
                        SessionTabManager.TabTarget.Terminal(cd = cwd, launcherId = launcherId, resumeSessionId = sid),
                        label = title,
                        resumeCommand = command,
                    )
                if (k != key) {
                    nav.navigate("session/$k") {
                        launchSingleTop = true
                        popUpTo(Screen.Hosts.route)
                    }
                }
            },
            // 面板点击切 tab。
            onSwitchTab = { k ->
                if (k != key) {
                    nav.navigate("session/$k") {
                        launchSingleTop = true
                        popUpTo(Screen.Hosts.route)
                    }
                }
            },
            // 面板「＋ 终端 tab」：当前主机强开新 tab。
            onNewTerminalTab = { hid ->
                nav.navigate("session/${tabManager.open(hid, SessionTabManager.TabTarget.Terminal(), forceNew = true)}")
            },
            // 面板「＋ 文件 tab」：每主机至多一个 SFTP tab，可能命中当前 tab，照上面的规则短路。
            onNewSftpTab = { hid ->
                val k = tabManager.open(hid, SessionTabManager.TabTarget.Sftp)
                if (k != key) {
                    nav.navigate("session/$k") {
                        launchSingleTop = true
                        popUpTo(Screen.Hosts.route)
                    }
                }
            },
        )
    }
}

/**
 * 「新建会话 · 选主机」页：复用 [HostListScreen]，但终端连接一律 `forceNew = true`；「文件」照常去重。
 * 选中后 `popUpTo` 弹出本页，返回键不回到这里，也不留 force-new 状态。
 */
@Composable
private fun NewSessionHostPickerScreen(
    nav: NavHostController,
    tabManager: SessionTabManager,
) {
    // 选中即进会话并弹出本页。SFTP 去重可能命中已在栈顶的 tab，所以加 launchSingleTop。
    fun openAndLeave(key: String) {
        nav.navigate("session/$key") {
            launchSingleTop = true
            popUpTo(Screen.NewSessionHostPicker.route) { inclusive = true }
        }
    }
    HostListScreen(
        // 本页只开终端 tab，与 agent 种类无关。
        onConnect = { hostId, _ -> openAndLeave(tabManager.open(hostId, SessionTabManager.TabTarget.Terminal(), forceNew = true)) },
        onAddHost = { nav.navigate("host/new") },
        onIdentities = { nav.navigate(Screen.Identities.route) },
        onFiles = { hostId -> openAndLeave(tabManager.open(hostId, SessionTabManager.TabTarget.Sftp)) },
        onEdit = { hostId -> nav.navigate("host/$hostId/edit") },
        onLaunch = { hostId, launcherId ->
            openAndLeave(tabManager.open(hostId, SessionTabManager.TabTarget.Terminal(launcherId = launcherId), forceNew = true))
        },
        onSessions = { nav.popBackStack() }, // 本页只从会话总览进，「会话」即返回
        newSessionMode = true,
        title = "新建会话 · 选择主机",
    )
}

/**
 * 宿主一个 tab。VM 挂在 [SessionTabManager] 的 per-key store 上（覆盖 `LocalViewModelStoreOwner`），
 * 跨「总览 ↔ tab」导航保活；只有 `close(key)` 才清掉 store、释放连接。
 * tab 已关则 [onGone] 退回。按 [SessionTabManager.TabTarget] 分派终端或 SFTP 屏，并挂 tab 面板 [SessionTabsSheet]。
 */
@Composable
private fun HostedSession(
    tabManager: SessionTabManager,
    key: String,
    onGone: () -> Unit,
    onOpenTerminalTab: (hostId: String, path: String) -> Unit,
    onOpenTmuxTab: (hostId: String, tmuxName: String) -> Unit,
    // resume 开新终端 tab。launcherId 定 cc/cct；title 查不到时为 null；cwd 可为 null，由远端按 sid 兜底；
    // command 是长按选定的命令，默认链为 null。
    onOpenResumeTab: (hostId: String, cwd: String?, sessionId: String, launcherId: String?, title: String?, command: String?) -> Unit,
    onSwitchTab: (key: String) -> Unit,
    onNewTerminalTab: (hostId: String) -> Unit,
    onNewSftpTab: (hostId: String) -> Unit,
) {
    // 响应式观察 sessions：面板「×」原地关掉当前 tab 时要触发 onGone，否则会困在已清 store 的死会话上。
    val sessions by tabManager.sessions.collectAsState()
    val open = sessions.firstOrNull { it.key == key }
    if (open == null) {
        LaunchedEffect(key) { onGone() }
        return
    }
    val store = tabManager.storeFor(key)
    val owner =
        remember(key) {
            object : ViewModelStoreOwner {
                override val viewModelStore: ViewModelStore = store
            }
        }
    // tab 面板显隐。rememberSaveable：旋转与进程恢复不丢。
    var showTabs by rememberSaveable { mutableStateOf(false) }
    CompositionLocalProvider(LocalViewModelStoreOwner provides owner) {
        when (val target = open.target) {
            is SessionTabManager.TabTarget.Terminal ->
                SessionScreen(
                    open.hostId,
                    target.cd,
                    target.launcherId,
                    target.tmuxSession, // attach tab 首连即附着该 tmux 会话
                    target.resumeSessionId, // resume tab 首连即起 tmux 跑 <cc> --resume
                    open.resumeCommand, // 长按选定的 resume 命令，是 tab 元数据，不在 target 里
                    onAttachInNewTab = { name -> onOpenTmuxTab(open.hostId, name) },
                    // 续接开新 resume tab，launcherId 继承本 tab。
                    onResumeInNewTab = { cwd, sid, title, command -> onOpenResumeTab(open.hostId, cwd, sid, target.launcherId, title, command) },
                    onShowTabs = { showTabs = true },
                )
            // 「在此打开终端」开新终端 tab，不替换当前 SFTP tab。
            SessionTabManager.TabTarget.Sftp ->
                SftpScreen(
                    open.hostId,
                    onOpenTerminal = { path -> onOpenTerminalTab(open.hostId, path) },
                    onShowTabs = { showTabs = true },
                )
        }
    }
    if (showTabs) {
        HostedTabsSheet(
            hostId = open.hostId,
            currentKey = key,
            onHide = { showTabs = false },
            onSwitchTab = onSwitchTab,
            onNewTerminalTab = onNewTerminalTab,
            onNewSftpTab = onNewSftpTab,
        )
    }
}

/** tab 面板接线：选择动作先收面板再转发；「＋」两项都用当前 tab 的主机。 */
@Composable
private fun HostedTabsSheet(
    hostId: String,
    currentKey: String,
    onHide: () -> Unit,
    onSwitchTab: (key: String) -> Unit,
    onNewTerminalTab: (hostId: String) -> Unit,
    onNewSftpTab: (hostId: String) -> Unit,
) {
    SessionTabsSheet(
        currentKey = currentKey,
        onSwitchTab = { k ->
            onHide()
            onSwitchTab(k)
        },
        onNewTerminalTab = {
            onHide()
            onNewTerminalTab(hostId)
        },
        onNewSftpTab = {
            onHide()
            onNewSftpTab(hostId)
        },
        onDismiss = onHide,
    )
}

private fun NavGraphBuilder.hostsRoute(
    nav: NavHostController,
    tabManager: SessionTabManager,
    onConnect: (String, AgentKind) -> Unit,
) {
    composable(Screen.Hosts.route) {
        HostListScreen(
            // 点主机去哪儿见 [onConnectDestination]。
            onConnect = onConnect,
            onAddHost = { nav.navigate("host/new") },
            onIdentities = { nav.navigate(Screen.Identities.route) },
            // 文件 = 开一个 SFTP tab，与终端 tab 一样保活、共享连接。
            onFiles = { hostId ->
                nav.navigate("session/${tabManager.open(hostId, SessionTabManager.TabTarget.Sftp)}")
            },
            onEdit = { hostId -> nav.navigate("host/$hostId/edit") },
            onLaunch = { hostId, launcherId ->
                nav.navigate("session/${tabManager.open(hostId, SessionTabManager.TabTarget.Terminal(launcherId = launcherId))}")
            },
            onSessions = { nav.navigate(Screen.SessionsOverview.route) },
        )
    }
}

/**
 * 启动落地判定（见 [Screen.Launch]）。判定本身是纯函数 [launchDestination]，这里读出异步事实后替换掉自己。
 *
 * 注意：上次那台主机要先查主机表核对还在，否则拿已删的 hostId 起聊天屏会一屏报错。
 */
private fun NavGraphBuilder.launchRoute(
    nav: NavHostController,
    newUi: Boolean?,
) {
    composable(Screen.Launch.route) {
        val settings = koinInject<SettingsRepository>()
        val hosts = koinInject<HostRepository>()
        LaunchedEffect(newUi) {
            // 核对时顺手留下那台服务器本身，种类从它读。
            val landing = settings.getLastHost()?.let { id -> hosts.get(id) }
            val lastHost = landing?.id
            val target =
                launchDestination(
                    newUi = newUi,
                    landingHostId = lastHost,
                    // null 主机时这个值用不上（直接回 Hosts），缺省取 Claude 档。
                    agentKind = landing?.agentKindOrDefault() ?: AgentProfile.DEFAULT.kind,
                    newConversationId = ::newConversationId,
                ) ?: return@LaunchedEffect // 「还不知道」⇒ 不导航，等真值那一趟
            nav.navigate(target) { popUpTo(Screen.Launch.route) { inclusive = true } }
        }
    }
}

/** 点一台主机之后去哪儿。开着新界面时要先挂起读「上次那个对话」；关着时不进协程，保持原时序。 */
@Composable
private fun rememberConnectAction(
    nav: NavHostController,
    newUi: Boolean,
    openTerminal: (String) -> String,
): (String, AgentKind) -> Unit {
    val settings = koinInject<SettingsRepository>()
    val scope = rememberCoroutineScope()
    // 种类由调用方同步交进来，不在这里查主机表：关着开关的那条路不能多一次挂起读。
    return { hostId, agentKind ->
        if (newUi) {
            scope.launch {
                val last = settings.getLastConversation(hostId)
                // 只有确认过的编号才允许 `--resume`，见 `chatLanding`。
                val confirmed = settings.getConfirmedConversation(hostId)
                nav.navigate(
                    onConnectDestination(hostId, true, agentKind, last, ::newConversationId, confirmed, openTerminal),
                )
            }
        } else {
            nav.navigate(onConnectDestination(hostId, false, agentKind, null, ::newConversationId, null, openTerminal))
        }
    }
}

/** 对话总览与单个对话两条路由。 */
private fun NavGraphBuilder.chatRoutes(
    nav: NavHostController,
    tabManager: SessionTabManager,
    chatController: ChatController,
    permissionMode: String?,
) {
    // Claude 对话总览，从抽屉「对话」进。注意：与 `sessions`（终端 tab 总览）是两件事。
    composable(Screen.Conversations.route) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        ConversationsRoute(
            hostId = hostId,
            onOpen = { sid -> nav.navigate(Screen.Chat.of(hostId, sid)) },
        )
    }
    composable(
        Screen.Chat.route,
        // `new` 是可选查询参数，声明默认值后 `chat/{hostId}/{sid}` 也能匹配。
        arguments =
            listOf(
                navArgument(Screen.Chat.ARG_NEW) {
                    type = NavType.BoolType
                    defaultValue = false
                },
            ),
    ) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val sid = entry.arguments?.getString("sid").orEmpty()
        val isNew = entry.arguments?.getBoolean(Screen.Chat.ARG_NEW) ?: false
        // 记下当前服务器（打开 app 时取上次那台）。对话编号也记一份：点服务器列表一行时 `chatLanding` 还读它。
        val settings = koinInject<SettingsRepository>()
        LaunchedEffect(hostId, sid) {
            settings.setLastHost(hostId)
            settings.setLastConversation(hostId, sid)
        }
        // 「离开这个对话」的唯一判据：这一条出了返回栈（返回、或被抽屉换掉）；进设置再回来、转屏都不算。
        // 挂在这一条上的 VM 只在那一刻被清掉。对话是应用级的，不 release 就只能等上限逐出。
        viewModel(viewModelStoreOwner = entry, key = "leave-chat") { OnLeftBackStack { chatController.release(chatKey(hostId, sid)) } }
        ChatRoute(
            hostId = hostId,
            sessionId = sid,
            permissionMode = permissionMode,
            isNew = isNew,
            onDrawerAction = { action -> nav.followDrawer(action, hostId, tabManager) },
        )
    }
}

/** 照 [drawerDestination] 走一步。换对话（新对话 / 最近里的一条）是换掉当前这条，不往栈上叠。 */
private fun NavHostController.followDrawer(
    action: DrawerAction,
    hostId: String,
    tabManager: SessionTabManager,
) {
    val to =
        drawerDestination(
            action = action,
            hostId = hostId,
            newConversationId = ::newConversationId,
            openFiles = { "session/${tabManager.open(it, SessionTabManager.TabTarget.Sftp)}" },
            openTerminal = { "session/${tabManager.open(it, SessionTabManager.TabTarget.Terminal())}" },
        )
    navigate(to.route) {
        if (to.replacesChat) popUpTo(Screen.Chat.route) { inclusive = true }
    }
}

/**
 * 抽屉里每一件事去哪。
 *
 * | 动作 | 去处 |
 * |---|---|
 * | ⊕ 新对话 | 这台上空的新对话，换掉当前这条 |
 * | 最近里的一条 | 那条对话（接着聊），换掉当前这条 |
 * | 对话 | 对话列表 |
 * | 文件 · 终端 | 当前服务器的文件屏 · 终端屏 |
 * | 底行名字 · ⚙ | 服务器列表 · 设置 |
 *
 * [openFiles] / [openTerminal] 做成参数：`SessionTabManager.open` 会真建 tab，测试不该为验一张表去建它。
 */
internal fun drawerDestination(
    action: DrawerAction,
    hostId: String,
    newConversationId: () -> String,
    openFiles: (String) -> String,
    openTerminal: (String) -> String,
): DrawerNav =
    when (action) {
        DrawerAction.NewConversation -> DrawerNav(Screen.Chat.of(hostId, newConversationId(), isNew = true), replacesChat = true)
        is DrawerAction.OpenRecent -> DrawerNav(Screen.Chat.of(hostId, action.sessionId), replacesChat = true)
        DrawerAction.Conversations -> DrawerNav(Screen.Conversations.of(hostId), replacesChat = false)
        DrawerAction.Files -> DrawerNav(openFiles(hostId), replacesChat = false)
        DrawerAction.Terminal -> DrawerNav(openTerminal(hostId), replacesChat = false)
        DrawerAction.Servers -> DrawerNav(Screen.Hosts.route, replacesChat = false)
        DrawerAction.Settings -> DrawerNav(Screen.Settings.route, replacesChat = false)
    }

/**
 * 加一台服务器：保存后直接进这台上空的新对话，它也成为当前服务器。
 * 同时弹掉「加一台」和来时的服务器列表：第一次装时返回键不该退回空列表。
 */
@Composable
private fun AddHostRoute(nav: NavHostController) {
    val settings = koinInject<SettingsRepository>()
    val scope = rememberCoroutineScope()
    HostEditorScreen(
        hostId = null,
        onDone = { nav.popBackStack() },
        onEditButtons = {},
        onAdded = { host ->
            scope.launch {
                settings.setLastHost(host.id)
                nav.navigate(freshLanding(host.id, host.agentKindOrDefault(), ::newConversationId)) {
                    popUpTo(Screen.Hosts.route) { inclusive = true }
                }
            }
        },
    )
}

/**
 * 点一台服务器之后去哪儿。
 *
 * | `newUi` | `agentKind` | 去处 |
 * |---|---|---|
 * | 关 | 任意 | 开一个终端 tab |
 * | 开 | ClaudeCode | 直接进聊天屏（见 [chatLanding]） |
 * | 开 | Codex | 对话总览（只读档），不进聊天屏 |
 *
 * Codex 不落聊天屏：上行（`ChatSession` / `UplinkSink` / `PipeLauncher`）只按 Claude CLI 的管道形状写，
 * 落聊天屏等于默认走一条必然失败的路；读那半（`CodexSessionCatalog`）是通的，只读档给得出东西。
 */
internal fun onConnectDestination(
    hostId: String,
    newUi: Boolean,
    /** 这台服务器的 agent 种类。注意：刻意无默认值，漏传要编译不过，而不是静默按 Claude 处理。 */
    agentKind: AgentKind,
    /**
     * 这台主机上次进的对话（`SettingsRepository.getLastConversation`），null = 没进过 ⇒ 开新的。
     * 无默认值：漏传会静默退化成每次都新开。
     */
    lastConversationId: String?,
    /** 新对话的编号工厂。做成参数是因为 [newConversationId] 每次调用都不同，测试没法钉住它。 */
    newConversationId: () -> String,
    /**
     * 被远端确认过的编号，见 [chatLanding]。
     *
     * 注意：它必须排在 [openTerminal] 之前。调用方用尾随 lambda，最后一个位置留给函数型参数。
     */
    confirmedConversationId: String? = null,
    /** 开一个终端 tab 并给出路由。做成参数：`SessionTabManager.open` 会真建 tab，测试不该为此去建。 */
    openTerminal: (String) -> String,
): String =
    when {
        !newUi -> openTerminal(hostId)
        // 输入行都不出现的档不拿聊天屏当落地点；判定读 `AgentProfile.landsOnChatScreen`，它派生自 `supportsUplink`。
        !AgentProfile.of(agentKind).landsOnChatScreen -> Screen.Conversations.of(hostId)
        else -> chatLanding(hostId, lastConversationId, newConversationId, confirmedConversationId)
    }

/**
 * 点进去就是聊天界面，历史对话退一步才看。
 *
 * 落地路由必须当场定，不能先问远端「最近的对话是哪个」，所以用本地记的 `lastConversationId`：
 *
 * | 上次进过 | 去处 |
 * |---|---|
 * | 有 | 那个对话（`isNew = false` ⇒ `--resume` 接着跑） |
 * | 没有 | 新开一个（`isNew = true` ⇒ 带 `--session-id`） |
 *
 * 记的编号可能在远端已经没了，那时打开的是空对话，与新开几乎一样，可接受；不为此做同步远端探测。
 * 对话列表从抽屉「对话」进（见 `drawerDestination`）。
 */
internal fun chatLanding(
    hostId: String,
    lastConversationId: String?,
    newConversationId: () -> String,
    /**
     * 被远端确认存在过的编号（`SettingsRepository.getConfirmedConversation`），只决定带不带 `--resume`：
     * 记住的编号与它相等才带，否则按新对话起。
     *
     * 注意：连接失败那次也会记下编号，resume 远端没有的编号会让 Claude 当场退出（`No conversation found`）。
     */
    confirmedConversationId: String? = null,
): String =
    lastConversationId
        ?.takeIf { it.isNotBlank() }
        ?.let {
            // 判据是逐字相等，不是「确认过的非空」：后者会把换了的对话也当成确认过。
            if (it == confirmedConversationId) {
                Screen.Chat.of(hostId, it)
            } else {
                Screen.Chat.of(hostId, it, isNew = true)
            }
        }
        ?: Screen.Chat.of(hostId, newConversationId(), isNew = true)

/**
 * 打开 app 那一刻去哪儿。
 *
 * | `newUi` | 当前服务器（上次那台，核对过还在） | 去处 |
 * |---|---|---|
 * | `null`（开关还没读到） | — | `null` = 不导航，留在 [Screen.Launch] 等真值 |
 * | 关 | — | [Screen.Hosts] |
 * | 开 | 没有 | [Screen.Hosts]（第一次装的第一屏） |
 * | 开 | 有 | 这台上空的新对话（见 [freshLanding]）；不回上次那条 |
 *
 * `null` 那一格防的是冷启动被假的「关」弹走。[landingHostId] 必须已核对过存在，核对在 [launchRoute] 里做。
 */
internal fun launchDestination(
    /** 三值：`null` = 还没读到（⇒ 不导航）· `true` = 开 · `false` = 显式关。 */
    newUi: Boolean?,
    landingHostId: String?,
    /** [landingHostId] 那台服务器的 agent 种类 —— 无默认值，理由同 [onConnectDestination]。 */
    agentKind: AgentKind,
    newConversationId: () -> String,
): String? =
    when {
        newUi == null -> null
        !newUi || landingHostId == null -> Screen.Hosts.route
        else -> freshLanding(landingHostId, agentKind, newConversationId)
    }

/** 进一台服务器、从空白开始：打开 app 与加完一台共用。Claude 档进空的新对话，不落聊天屏的档进对话列表。 */
internal fun freshLanding(
    hostId: String,
    agentKind: AgentKind,
    newConversationId: () -> String,
): String =
    if (AgentProfile.of(agentKind).landsOnChatScreen) {
        Screen.Chat.of(hostId, newConversationId(), isNew = true)
    } else {
        Screen.Conversations.of(hostId)
    }

/** 挂在一条返回栈记录上：那一条出栈时（且只在那时）[onCleared] 跑 [action]。 */
private class OnLeftBackStack(
    private val action: () -> Unit,
) : ViewModel() {
    override fun onCleared() = action()
}
