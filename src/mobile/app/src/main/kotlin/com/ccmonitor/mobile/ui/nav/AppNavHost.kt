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
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarHostScaffold
import com.ccmonitor.mobile.ui.agents.AgentScreen
import com.ccmonitor.mobile.ui.agents.AgentStack
import com.ccmonitor.mobile.ui.agents.AgentsScreen
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatRoute
import com.ccmonitor.mobile.ui.chat.chatKey
import com.ccmonitor.mobile.ui.chat.newConversationId
import com.ccmonitor.mobile.ui.chats.ChatsRoute
import com.ccmonitor.mobile.ui.common.UnmigratedHostKeys
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import com.ccmonitor.mobile.ui.drawer.DrawerNav
import com.ccmonitor.mobile.ui.files.FilesScreen
import com.ccmonitor.mobile.ui.host.HostEditorScreen
import com.ccmonitor.mobile.ui.host.HostListScreen
import com.ccmonitor.mobile.ui.identity.IdentityScreen
import com.ccmonitor.mobile.ui.needs.OverviewScreen
import com.ccmonitor.mobile.ui.plan.PlanCellScreen
import com.ccmonitor.mobile.ui.plan.PlanOutlineScreen
import com.ccmonitor.mobile.ui.plan.PlanSlicesScreen
import com.ccmonitor.mobile.ui.rotation.RotFocus
import com.ccmonitor.mobile.ui.rotation.RotationScreen
import com.ccmonitor.mobile.ui.session.SessionScreen
import com.ccmonitor.mobile.ui.session.SessionTabManager
import com.ccmonitor.mobile.ui.session.SessionTabsSheet
import com.ccmonitor.mobile.ui.session.keys.KeysEditorScreen
import com.ccmonitor.mobile.ui.settings.AccountsScreen
import com.ccmonitor.mobile.ui.settings.Here
import com.ccmonitor.mobile.ui.settings.SettingsScreen
import com.ccmonitor.mobile.ui.usage.UsageScreen
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
    val chatController = koinInject<ChatController>()
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
    ) {
        // 全应用的操作反馈与撤销宿主。装在根部：撤销要跨屏活着。
        AppSnackbarHostScaffold {
            AppNavGraph(nav, tabManager, chatController)
            UnmigratedHostKeys()
        }
    }
}

@Composable
private fun AppNavGraph(
    nav: NavHostController,
    tabManager: SessionTabManager,
    chatController: ChatController,
) {
    // 点一台服务器 ⇒ 这台上空的新对话（见 [freshLanding]）。
    val onConnect: (String) -> Unit = { hostId -> nav.navigate(freshLanding(hostId, ::newConversationId)) }
    // 起点是判定屏，见 [Screen.Launch]。
    NavHost(navController = nav, startDestination = Screen.Launch.route) {
        launchRoute(nav)
        hostsRoute(nav, tabManager, onConnect)
        chatRoutes(nav, tabManager, chatController)
        composable(Screen.Settings.route) {
            // 设置面上的去处住 `settingsEntries()`，这里只把路由交给 nav。
            HereOrNone { here -> SettingsScreen(here, onBack = { nav.popBackStack() }, onNavigate = { route -> nav.navigate(route) }, onUsage = { nav.navigate(Screen.Usage.route) }) }
        }
        composable(Screen.Accounts.route) {
            HereOrNone { here ->
                if (here == null) {
                    LaunchedEffect(Unit) { nav.navigate(Screen.Hosts.route) { popUpTo(Screen.Accounts.route) { inclusive = true } } }
                } else {
                    AccountsScreen(here, onBack = { nav.popBackStack() }, onUsage = { nav.navigate(Screen.Usage.route) })
                }
            }
        }
        composable(Screen.Usage.route) {
            HereOrNone { here ->
                if (here == null) {
                    LaunchedEffect(Unit) { nav.navigate(Screen.Hosts.route) { popUpTo(Screen.Usage.route) { inclusive = true } } }
                } else {
                    UsageScreen(here, onBack = { nav.popBackStack() })
                }
            }
        }
        rotationRoutes(nav, tabManager)
        agentRoutes(nav)
        planRoutes(nav)
        sessionRoute(nav, tabManager)
        composable(Screen.Identities.route) { IdentityScreen(onBack = { nav.popBackStack() }) }
        composable(Screen.Keys.route) { KeysEditorScreen(onBack = { nav.popBackStack() }) }
        composable("host/new") { AddHostRoute(nav) }
        composable(Screen.HostEdit.route) { entry ->
            val id = entry.arguments?.getString("id") ?: return@composable
            HostEditorScreen(
                hostId = id,
                onDone = { nav.popBackStack() },
            )
        }
    }
}

/**
 * Session 路由：tab 宿主按 kind 分派 SessionScreen / FilesScreen。
 *
 * 注意：切 tab 用 launchSingleTop + popUpTo(Hosts) 防返回栈堆积，且目标是当前 tab 时必须短路：
 * `popUpTo` 先于 `launchSingleTop` 去重执行，会先弹掉 session/$key 再比对，结果重建当前 entry、
 * 重置 rememberSaveable 并闪一下。
 */
private fun NavGraphBuilder.sessionRoute(
    nav: NavHostController,
    tabManager: SessionTabManager,
) {
    composable(
        Screen.Session.route,
        arguments =
            listOf(
                navArgument(Screen.Session.ARG_OPEN) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
    ) { entry ->
        val key = entry.arguments?.getString("key") ?: return@composable
        // 文件 tab「在此打开终端」开一个终端 tab。
        HostedSession(
            tabManager,
            key,
            openPath = entry.arguments?.getString(Screen.Session.ARG_OPEN),
            onGone = { nav.popBackStack() },
            onEditKeys = { nav.navigate(Screen.Keys.route) },
            onEditHost = { hid -> nav.navigate("host/$hid/edit") },
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
            // 面板「＋ 文件 tab」：每主机至多一个 文件 tab，可能命中当前 tab，照上面的规则短路。
            onNewFilesTab = { hid ->
                val k = tabManager.open(hid, SessionTabManager.TabTarget.Files)
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
 * 宿主一个 tab。VM 挂在 [SessionTabManager] 的 per-key store 上（覆盖 `LocalViewModelStoreOwner`），
 * 跨 tab 导航保活；只有 `close(key)` 才清掉 store、释放连接。
 * tab 已关则 [onGone] 退回。按 [SessionTabManager.TabTarget] 分派终端屏或文件屏，并挂 tab 面板 [SessionTabsSheet]。
 */
@Composable
private fun HostedSession(
    tabManager: SessionTabManager,
    key: String,
    /** 文件 tab 进来时要打开的那个路径（可缺）。 */
    openPath: String?,
    onGone: () -> Unit,
    onEditKeys: () -> Unit,
    onEditHost: (hostId: String) -> Unit,
    onOpenTerminalTab: (hostId: String, path: String) -> Unit,
    onOpenTmuxTab: (hostId: String, tmuxName: String) -> Unit,
    onSwitchTab: (key: String) -> Unit,
    onNewTerminalTab: (hostId: String) -> Unit,
    onNewFilesTab: (hostId: String) -> Unit,
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
                    target.tmuxSession, // 进终端 tab 首连即敲那台渲出的那一行，接回该会话
                    target.resume,
                    onAttachInNewTab = { name -> onOpenTmuxTab(open.hostId, name) },
                    onShowTabs = { showTabs = true },
                    onEditKeys = onEditKeys,
                    onEditHost = onEditHost,
                )
            // 「在此打开终端」开新终端 tab，不替换当前 文件 tab。
            SessionTabManager.TabTarget.Files ->
                FilesScreen(
                    open.hostId,
                    onOpenTerminal = { path -> onOpenTerminalTab(open.hostId, path) },
                    onShowTabs = { showTabs = true },
                    onEditHost = onEditHost,
                    openPath = openPath,
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
            onNewFilesTab = onNewFilesTab,
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
    onNewFilesTab: (hostId: String) -> Unit,
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
        onNewFilesTab = {
            onHide()
            onNewFilesTab(hostId)
        },
        onDismiss = onHide,
    )
}

private fun NavGraphBuilder.hostsRoute(
    nav: NavHostController,
    tabManager: SessionTabManager,
    onConnect: (String) -> Unit,
) {
    composable(Screen.Hosts.route) {
        HostListScreen(
            // 点主机 ⇒ 这台上空的新对话（[freshLanding]）。
            onConnect = onConnect,
            onAddHost = { nav.navigate("host/new") },
            onIdentities = { nav.navigate(Screen.Identities.route) },
            // 文件 = 开一个 文件 tab，与终端 tab 一样保活、共享连接。
            onFiles = { hostId ->
                nav.navigate("session/${tabManager.open(hostId, SessionTabManager.TabTarget.Files)}")
            },
            onEdit = { hostId -> nav.navigate("host/$hostId/edit") },
        )
    }
}

/**
 * 启动落地判定（见 [Screen.Launch]）。判定本身是纯函数 [launchDestination]，这里读出异步事实后替换掉自己。
 *
 * 注意：上次那台主机要先查主机表核对还在，否则拿已删的 hostId 起聊天屏会一屏报错。
 */
private fun NavGraphBuilder.launchRoute(nav: NavHostController) {
    composable(Screen.Launch.route) {
        val settings = koinInject<SettingsRepository>()
        val hosts = koinInject<HostRepository>()
        LaunchedEffect(Unit) {
            val lastHost = settings.getLastHost()?.let { id -> hosts.get(id) }?.id
            val target = launchDestination(landingHostId = lastHost, newConversationId = ::newConversationId)
            nav.navigate(target) { popUpTo(Screen.Launch.route) { inclusive = true } }
        }
    }
}

/**
 * 轮换屏两个进口：聊天屏「⋯」→ 轮换（那条会话）· 设置 → 机器与账号 → 轮换（当前服务器 ＝ 上次那台，新会话用的那份）。
 * 提示里的［看代码］· 旧格式那几份开那台的文件屏。
 */
private fun NavGraphBuilder.rotationRoutes(
    nav: NavHostController,
    tabManager: SessionTabManager,
) {
    composable(
        Screen.Rotation.route,
        arguments =
            listOf(
                navArgument(Screen.Rotation.ARG_SID) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                navArgument(Screen.Rotation.ARG_STRATEGY) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                navArgument(Screen.Rotation.ARG_ACCOUNT) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
    ) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val args = entry.arguments
        val focus = args?.getString(Screen.Rotation.ARG_STRATEGY)?.let { RotFocus(it, args.getString(Screen.Rotation.ARG_ACCOUNT)) }
        RotationRoute(nav, tabManager, hostId, args?.getString(Screen.Rotation.ARG_SID), focus)
    }
    composable(Screen.RotationHere.route) {
        HereOrNone { here ->
            if (here == null) {
                LaunchedEffect(Unit) { nav.navigate(Screen.Hosts.route) { popUpTo(Screen.RotationHere.route) { inclusive = true } } }
            } else {
                RotationRoute(nav, tabManager, here.hostId, null)
            }
        }
    }
}

/** agent 列表 → agent 屏 → 它派出的（推屏；栈里已有同一个运行 ⇒ 退回到它，见 [AgentStack]）。 */
private fun NavGraphBuilder.agentRoutes(nav: NavHostController) {
    composable(Screen.Agents.route) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val sid = entry.arguments?.getString("sid").orEmpty()
        AgentsScreen(hostId, sid, onBack = { nav.popBackStack() }, onOpen = { nav.openAgent(hostId, sid, it.run) })
    }
    composable(Screen.Agent.route) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val sid = entry.arguments?.getString("sid").orEmpty()
        val run = entry.arguments?.getString(Screen.Agent.ARG_RUN).orEmpty()
        AgentScreen(hostId, sid, run, onBack = { nav.popBackStack() }, onOpen = { nav.openAgent(hostId, sid, it.run) })
    }
}

/** 计划三屏：片 → 大纲 → 一格详情（一格里点子格再压一层；返回一层退一层）。 */
private fun NavGraphBuilder.planRoutes(nav: NavHostController) {
    composable(Screen.Plan.route, arguments = listOf(optionalString("sid"))) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val sid = entry.arguments?.getString("sid")
        PlanSlicesScreen(hostId, onBack = { nav.popBackStack() }, onOpen = { ws, slice -> nav.navigate(Screen.PlanSlice.of(hostId, ws, slice, sid)) })
    }
    composable(Screen.PlanSlice.route, arguments = listOf(optionalString("ws"), optionalString("slice"), optionalString("sid"))) { entry ->
        val a = entry.arguments
        val hostId = a?.getString("hostId").orEmpty()
        val ws = a?.getString("ws").orEmpty()
        val slice = a?.getString("slice").orEmpty()
        PlanOutlineScreen(hostId, ws, slice, a?.getString("sid"), onBack = { nav.popBackStack() }, onOpenCell = { nav.navigate(Screen.PlanCell.of(hostId, ws, slice, it)) })
    }
    composable(Screen.PlanCell.route, arguments = listOf(optionalString("ws"), optionalString("slice"), optionalString("id"))) { entry ->
        val a = entry.arguments
        val hostId = a?.getString("hostId").orEmpty()
        val ws = a?.getString("ws").orEmpty()
        val slice = a?.getString("slice").orEmpty()
        val id = a?.getString("id").orEmpty()
        PlanCellScreen(hostId, ws, slice, id, onBack = { nav.popBackStack() }, onOpenCell = { nav.navigate(Screen.PlanCell.of(hostId, ws, slice, it)) })
    }
}

/** 进一个 agent 屏：就在顶上 ⇒ 不动；返回栈里已有那一屏（路由连参数对上）⇒ 退回到它；没有 ⇒ 压一层。 */
private fun NavHostController.openAgent(
    hostId: String,
    sid: String,
    run: String,
) {
    val top = currentBackStackEntry?.takeIf { it.destination.route == Screen.Agent.route }?.arguments
    val here = top?.takeIf { it.getString("hostId") == hostId && it.getString("sid") == sid }?.getString(Screen.Agent.ARG_RUN)
    AgentStack.open(here, run, popTo = { popBackStack(Screen.Agent.of(hostId, sid, it), inclusive = false) }, push = { navigate(Screen.Agent.of(hostId, sid, it)) })
}

/**
 * 设置那一组屏的当前那台：上次那台（核对过还在主机表里）；没有 ⇒ `null`。还在查 ⇒ 不画。
 * 设置 · 账号 · 轮换（设置页那一形）三处同一份。
 */
@Composable
private fun HereOrNone(content: @Composable (Here?) -> Unit) {
    val settings = koinInject<SettingsRepository>()
    val hosts = koinInject<HostRepository>()
    val here by produceState<Result<Here?>?>(null) {
        value = Result.success(settings.getLastHost()?.let { id -> hosts.get(id) }?.let { Here(it.id, it.label.ifEmpty { it.id }) })
    }
    here?.let { content(it.getOrNull()) }
}

@Composable
private fun RotationRoute(
    nav: NavHostController,
    tabManager: SessionTabManager,
    hostId: String,
    sid: String?,
    focus: RotFocus? = null,
) {
    val hosts = koinInject<HostRepository>()
    val machine by produceState<String?>(null, hostId) { value = runCatching { hosts.get(hostId) }.getOrNull()?.label ?: hostId }
    val name = machine ?: return
    RotationScreen(
        hostId = hostId,
        sid = sid,
        machine = name,
        onBack = { nav.popBackStack() },
        onOpenFile = { path -> nav.navigate(Screen.Session.of(tabManager.open(hostId, SessionTabManager.TabTarget.Files), path)) },
        onEditStrategy = { strategy, account -> nav.navigate(Screen.Rotation.edit(hostId, strategy, account)) },
        focus = focus,
    )
}

/** 可缺的一个字串查询参数（缺 ⇒ `null`）。 */
private fun optionalString(name: String) =
    navArgument(name) {
        type = NavType.StringType
        nullable = true
        defaultValue = null
    }

/** 对话总览与单个对话两条路由。 */
private fun NavGraphBuilder.chatRoutes(
    nav: NavHostController,
    tabManager: SessionTabManager,
    chatController: ChatController,
) {
    // Claude 对话总览，从抽屉「会话」进。注意：与 `sessions`（终端 tab 总览）是两件事。
    composable(Screen.Conversations.route) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        ChatsRoute(
            hostId = hostId,
            onOpen = chatsOpen(nav::navigate),
            onOpenAt = chatsOpenAt(nav::navigate),
            onNewInDir = { host, cwd -> nav.navigate(Screen.Chat.of(host, newConversationId(), isNew = true, cwd = cwd)) },
            onDrawerAction = { action -> nav.followDrawer(action, hostId, tabManager) },
            onEditHost = { hid -> nav.navigate("host/$hid/edit") },
            onOpenOverview = { nav.navigate(Screen.Overview.of(hostId)) },
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
            ) +
                // 从搜内容的一处命中打开：那一行的字节起点（Long 型的参数不许缺，所以按字串交）· 落在哪张工具卡里。
                listOf(Screen.Chat.ARG_CWD, Screen.Chat.ARG_AT, Screen.Chat.ARG_TOOL).map(::optionalString),
    ) { entry ->
        val hostId = entry.arguments?.getString("hostId").orEmpty()
        val sid = entry.arguments?.getString("sid").orEmpty()
        val isNew = entry.arguments?.getBoolean(Screen.Chat.ARG_NEW) ?: false
        // 记下当前服务器（打开 app 时取上次那台）；不记上次那条会话（打开 app 落在空的新建会话）。
        val settings = koinInject<SettingsRepository>()
        LaunchedEffect(hostId) { settings.setLastHost(hostId) }
        // 「离开这个对话」的唯一判据：这一条出了返回栈（返回、或被抽屉换掉）；进设置再回来、转屏都不算。
        // 挂在这一条上的 VM 只在那一刻被清掉。对话是应用级的，不 release 就只能等上限逐出。
        viewModel(viewModelStoreOwner = entry, key = "leave-chat") { OnLeftBackStack { chatController.release(chatKey(hostId, sid)) } }
        ChatRoute(
            hostId = hostId,
            sessionId = sid,
            isNew = isNew,
            draftCwd = entry.arguments?.getString(Screen.Chat.ARG_CWD),
            at = entry.arguments?.getString(Screen.Chat.ARG_AT)?.toLong(),
            tool = entry.arguments?.getString(Screen.Chat.ARG_TOOL),
            onDrawerAction = { action -> nav.followDrawer(action, hostId, tabManager) },
            onOpenRotation = { s -> nav.navigate(Screen.Rotation.of(hostId, s)) },
            onOpenAgents = { s -> nav.navigate(Screen.Agents.of(hostId, s)) },
            onOpenPlan = { s -> nav.navigate(Screen.Plan.of(hostId, s)) },
            onOpenTerminal = { to -> nav.navigate("session/${tabManager.open(hostId, to)}") },
            onOpenOverview = { nav.navigate(Screen.Overview.of(hostId)) },
        )
    }
    composable(Screen.Overview.route) { entry ->
        OverviewScreen(
            current = entry.arguments?.getString("hostId").orEmpty(),
            onOpen = { host, sid -> nav.navigate(Screen.Chat.of(host, sid)) },
            onPlan = { host, ws, slice -> nav.navigate(Screen.PlanSlice.of(host, ws, slice, null)) },
            onBack = { nav.popBackStack() },
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
            openFiles = { "session/${tabManager.open(it, SessionTabManager.TabTarget.Files)}" },
            openTerminal = { "session/${tabManager.open(it, SessionTabManager.TabTarget.Terminal())}" },
        )
    navigate(to.route) {
        // 对话列表里再点「会话」不叠第二份。
        launchSingleTop = !to.replacesChat
        if (to.replacesChat) popUpTo(Screen.Chat.route) { inclusive = true }
    }
}

/**
 * 抽屉里每一件事去哪。
 *
 * | 动作 | 去处 |
 * |---|---|
 * | ⊕ 新对话 | 这台上空的新对话，换掉当前这条 |
 * | 新的一张（换机器 · 在一条会话里点新建） | 那台上空的新建会话（带那条的目录），换掉当前这条 |
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
        is DrawerAction.NewDraft -> DrawerNav(Screen.Chat.of(action.hostId, newConversationId(), isNew = true, cwd = action.cwd), replacesChat = true)
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
        onAdded = { host ->
            scope.launch {
                settings.setLastHost(host.id)
                nav.navigate(freshLanding(host.id, ::newConversationId)) {
                    popUpTo(Screen.Hosts.route) { inclusive = true }
                }
            }
        },
    )
}

/**
 * 打开 app 那一刻去哪儿。
 *
 * | 当前服务器（上次那台，核对过还在） | 去处 |
 * |---|---|
 * | 没有 | [Screen.Hosts]（第一次装的第一屏） |
 * | 有 | 这台上空的新对话（见 [freshLanding]）；不回上次那条 |
 *
 * [landingHostId] 必须已核对过存在，核对在 [launchRoute] 里做。
 */
internal fun launchDestination(
    landingHostId: String?,
    newConversationId: () -> String,
): String = landingHostId?.let { freshLanding(it, newConversationId) } ?: Screen.Hosts.route

/** 进一台服务器、从空白开始：打开 app 与加完一台共用。哪一家都进空的新对话（送字投进那条会话的输入队 `session-queue-put`，不分哪家）。 */
internal fun freshLanding(
    hostId: String,
    newConversationId: () -> String,
): String = Screen.Chat.of(hostId, newConversationId(), isNew = true)

/** 会话屏点一条会话 · 搜内容的块头：落在最新处。 */
internal fun chatsOpen(navigate: (String) -> Unit): (String, String) -> Unit = { host, sid -> navigate(Screen.Chat.of(host, sid)) }

/** 会话屏点搜内容的一处命中：落到那一处（`at` · `tool` 照交进来的）。 */
internal fun chatsOpenAt(navigate: (String) -> Unit): (String, String, Long, String?) -> Unit =
    { host, sid, at, tool -> navigate(Screen.Chat.of(host, sid, at = at, tool = tool)) }

/** 挂在一条返回栈记录上：那一条出栈时（且只在那时）[onCleared] 跑 [action]。 */
private class OnLeftBackStack(
    private val action: () -> Unit,
) : ViewModel() {
    override fun onCleared() = action()
}
