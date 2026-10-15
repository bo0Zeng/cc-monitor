package com.ccmonitor.mobile.ui.chat

import android.content.Intent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionNewFacts
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.TerminalsList
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ui.chat.needs.ChatNeeds
import com.ccmonitor.mobile.ui.chat.needs.rememberChatNeeds
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPane
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.chatHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import com.ccmonitor.mobile.ui.session.TerminalOps
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.util.UUID

/**
 * 生产的聊天屏：一条会话的完整接线。起会话、送字、正文全走那台核心的常驻流（[HostBackends]），见 [ChatSession]。
 * 对话身份是 [chatKey]，不是屏幕实例：对话活在应用级 `ChatController`，用稳定的 key 取，进屏才不会新建对话。
 *
 * 本文件不写 `onDispose`：它在每次离屏时都跑，包括进设置再回来；返回上一屏不算离开对话。
 * `release` 由导航层在「返回 = 离开这个对话」时调。
 */
@Composable
fun ChatRoute(
    hostId: String,
    /** 已有会话的 sid；新建会话是这张草稿的票（[newConversationId]）。 */
    sessionId: String,
    /** 抽屉里点下去的事，去哪由导航层定；不给默认值，免得某个调用方静默焊死整个抽屉。 */
    onDrawerAction: (DrawerAction) -> Unit,
    /** 新建会话（发第一条时请那台起）；`false` ＝ 从列表点进来的那一条。 */
    isNew: Boolean = false,
    /** 新建会话从哪进来的那个目录（在一条会话里点「新建会话」⇒ 那条的目录）；没有 ⇒ `null`（那台最近的第一个 · `~`）。 */
    draftCwd: String? = null,
    /** 顶栏「⋯」→ 轮换：这条会话（sid）的轮换屏。 */
    onOpenRotation: (String) -> Unit = {},
    /** 顶栏「⋯」→ 在终端里打开：终端屏开一个标签，附着这个 tmux 会话（名字是那台名单上的）。 */
    onOpenTerminal: (String) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    // 先把连接建起来；持有挂在对话上，由 `ChatController.sessionFor(hostId = …)` 负责。
    val link by rememberHostLink(hostId, chatHolder(chatKey(hostId, sessionId)), retainWhileComposed = false)
    KeepAliveWhileLinked(link)
    val host = rememberChatHost(hostId) ?: return
    val product = rememberProductName(hostId, host.agent)
    val open = if (isNew) ChatOpen(hostId, chatKey(hostId, sessionId), ChatTarget.Draft(ticket = sessionId, cwd = draftCwd, agent = host.agent), host.machine) else existingChat(hostId, sessionId, host.machine)
    val vm: ChatViewModel =
        org.koin.androidx.compose
            .koinViewModel(key = open.key) { parametersOf(open) }
    val state by vm.state.collectAsStateWithLifecycle()
    val backends = koinInject<HostBackends>()
    val sid = remember(vm) { vm.state.map { it.sid } }
    // 在屏上才进这台的 stream-watch 名单（离屏 ⇒ 退出，列表页上名单是空的）。
    DisposableEffect(vm) {
        vm.visible(true)
        onDispose { vm.visible(false) }
    }

    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    // 在一条会话里点「新建会话」⇒ 新的那张起在这条的目录。
    val onDrawer: (DrawerAction) -> Unit = { action ->
        val cwd = state.cwd
        onDrawerAction(if (action == DrawerAction.NewConversation && cwd != null) DrawerAction.NewDraft(hostId, cwd) else action)
    }
    val picker = vm.picker
    val needs = rememberChatNeeds(open.key, backends.of(hostId), sid, host.machine)
    val terminals = remember(hostId) { TerminalOps(backends.of(hostId), { host.machine }) }
    val needsUi by needs.desk.state.collectAsStateWithLifecycle()
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawer) {
        Scaffold(
            modifier = modifier,
            topBar = {
                ChatTopBar(
                    title = if (state.drafting) copyText("newSession.title.plain") else product,
                    onOpenDrawer = { scope.launch { drawerState.open() } },
                    sid = state.sid?.takeIf { !state.drafting },
                    findTerminal = { s -> terminals.list().getOrNull()?.let { TerminalsList.holding(it, s) }?.name },
                    onTerminal = onOpenTerminal,
                    onRotation = onOpenRotation,
                )
            },
        ) { inner ->
            // 键盘弹起：垫上输入法，消息区缩到输入行上方（不垫的话系统把整窗往上推，顶栏和消息被推出屏外）。
            // 先认领 Scaffold 已垫的那几块，手势条那一段才不算两遍。
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(inner)
                    .consumeWindowInsets(inner)
                    .imePadding(),
            ) {
                if (state.drafting && picker != null) {
                    NewSessionPane(hostId, host.machine, picker, onSwitchMachine = { id -> onDrawerAction(DrawerAction.NewDraft(id, null)) })
                }
                ChatScreen(
                    state = state,
                    actions = vm.actions,
                    onRetrySend = vm::retry,
                    onLoadOlder = vm::loadOlder,
                    awaiting = needsUi.awaiting,
                    needs = { ChatNeeds(needs) },
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

/** 开一条对话要的那几样（DI 按类型取这一件，不按位置取几个裸串）。 */
data class ChatOpen(
    val hostId: String,
    val key: String,
    val target: ChatTarget,
    val machine: String,
)

/** 读一条已有的会话（从列表点进来）。 */
fun existingChat(
    hostId: String,
    sid: String,
    machine: String,
): ChatOpen = ChatOpen(hostId, chatKey(hostId, sid), ChatTarget.Existing(sid), machine)

/**
 * 一条会话的 [ChatSession]（应用级，同一个 key ⇒ 同一条）。[linkOf] 给那台的核心（生产上是 `HostBackends.of(hostId)`，先 `ensure` 接上），
 * [watchOf] 给那台的 `stream-watch` 名单（同一条常驻流，`HostBackends.of`）。
 */
fun ChatController.sessionOf(
    open: ChatOpen,
    linkOf: (hostId: String) -> CoreLink,
    watchOf: (hostId: String) -> SessionWatch,
    /** 那台的本地缓存（生产上是 `LocalCache.chat(hostId)`）。 */
    cacheOf: (hostId: String) -> ChatCache = { ChatCache.None },
): ChatSession =
    sessionFor(key = open.key, hostId = open.hostId) { scope ->
        ChatSession(linkOf(open.hostId), watchOf(open.hostId), open.target, open.machine, scope, cache = cacheOf(open.hostId))
    }

/** 这台在手机上的名字 ＋ 存的那一家；还没读到 ⇒ `null`。 */
private data class ChatHost(
    val machine: String,
    /** 这台存的那一家（线上 kind；没选 ⇒ `null`）：新建会话起哪一家由那台按它认。 */
    val agent: String?,
)

@Composable
private fun rememberChatHost(hostId: String): ChatHost? {
    val hosts = koinInject<HostRepository>()
    val got by produceState<ChatHost?>(null, hostId) {
        val h = runCatching { hosts.get(hostId) }.getOrNull()
        value =
            ChatHost(
                machine = h?.label ?: hostId,
                agent = h?.agentKind,
            )
    }
    return got
}

/** 顶栏的名字：这台那一家写给人看的名字（那台 `session-new-facts` 给的）；还没问到 ⇒ 空。 */
@Composable
private fun rememberProductName(
    hostId: String,
    stored: String?,
): String {
    val backends = koinInject<HostBackends>()
    val got by produceState("", hostId, stored) {
        val r = backends.of(hostId).call(SessionNewFacts.COMMAND, emptyMap())
        value = (r as? Reply.Ok)?.let { SessionNewFacts.of(it.data) }?.let { SessionNewFacts.pick(it.agents, stored)?.text }.orEmpty()
    }
    return got
}

/**
 * 聊天屏的顶栏：左上 ☰ 拉出抽屉；右上 ⋯ 只放对这条会话本身做的事：在终端里打开（那台名单上有它所在的终端才有，第二行是那个 tmux 会话名；
 * 答不了的需手动从这里进终端）· 轮换。还没起好的新建会话（[sid] 为 `null`）不画 ⋯。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatTopBar(
    /** 这台那一家写给人看的名字（那台给的）。不写主机名，界面上不出现「连接」那套说法。 */
    title: String,
    onOpenDrawer: () -> Unit,
    sid: String?,
    /** 那条会话所在的 tmux 会话名（问那台的终端名单）；不在任何终端里 · 问不到 ⇒ `null`。 */
    findTerminal: suspend (String) -> String?,
    onTerminal: (String) -> Unit,
    onRotation: (String) -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    var terminal by remember(sid) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    TopAppBar(
        navigationIcon = {
            PhIconButton(
                Ph.List,
                DRAWER_OPEN_DESCRIPTION,
                onClick = onOpenDrawer,
                modifier = Modifier.testTag(ChatTestTags.DRAWER_OPEN),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        title = { Text(title, style = MaterialTheme.typography.titleMedium) },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = LocalAppTokens.current.bg2, scrolledContainerColor = LocalAppTokens.current.bg2),
        actions = {
            if (sid != null) {
                IconButton(
                    onClick = {
                        menu = true
                        scope.launch { terminal = findTerminal(sid) }
                    },
                    modifier = Modifier.semantics { contentDescription = copyText("history.row.more") }.testTag(ChatTestTags.MORE),
                ) { Text(MORE_GLYPH, style = MaterialTheme.typography.titleLarge) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    terminal?.let { name ->
                        DropdownMenuItem(
                            text = {
                                Column {
                                    Text(copyText("sessionHead.act.openTerm"))
                                    Text(name, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            },
                            modifier = Modifier.testTag(ChatTestTags.MORE_TERMINAL),
                            onClick = {
                                menu = false
                                onTerminal(name)
                            },
                        )
                    }
                    DropdownMenuItem(text = { Text(copyText("mobChat.more.rotation")) }, onClick = {
                        menu = false
                        onRotation(sid)
                    })
                }
            }
        },
    )
}

private const val MORE_GLYPH = "⋯"

/**
 * 连上主机就起前台保活服务。
 *
 * 服务按「有活动会话或有在飞轮次」存活，在飞轮次由 `ChatController.hasInFlightTurn` 持有；
 * 不在这里起，只走聊天屏的用户就收不到「离开 app 后回复完成」的推送。
 * 服务没会话也没在飞轮次时会自停，所以这里只管起、不管停。
 */
@Composable
private fun KeepAliveWhileLinked(link: HostLink) {
    val context = LocalContext.current
    LaunchedEffect(link is HostLink.Ready) {
        if (link is HostLink.Ready) {
            val app = context.applicationContext
            ContextCompat.startForegroundService(app, Intent(app, SshKeepAliveService::class.java))
        }
    }
}

/**
 * 对话的稳定标识。
 *
 * 一个 Claude 对话由「哪台主机 + 哪个对话编号」共同确定 —— 只用后者的话，
 * 两台主机上碰巧同号的对话会被当成同一个。
 */
fun chatKey(
    hostId: String,
    sessionId: String,
): String = "$hostId/$sessionId"

/**
 * 新建会话那张草稿的票：界面上的 [chatKey] 用它，起会话也带它（`session-new.ticket`：期限到了同一张再问，那台不起第二个）。
 * 会话起好之后的 sid 是那台报到时说的，不是它。
 */
fun newConversationId(): String = UUID.randomUUID().toString()
