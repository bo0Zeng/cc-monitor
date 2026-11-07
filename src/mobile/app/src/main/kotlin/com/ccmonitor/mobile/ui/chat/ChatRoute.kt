package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.clickable
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
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberDrawerState
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.TermOpen
import com.ccmonitor.mobile.core.wire.gen.TermWay
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.agents.agentsEntry
import com.ccmonitor.mobile.ui.chat.attach.AttachStore
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import com.ccmonitor.mobile.ui.chat.needs.ChatNeeds
import com.ccmonitor.mobile.ui.chat.needs.NeedsViewModel
import com.ccmonitor.mobile.ui.chat.needs.rememberChatNeeds
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPane
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPicker
import com.ccmonitor.mobile.ui.chats.RenameDialog
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.MachineBar
import com.ccmonitor.mobile.ui.common.chatHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import com.ccmonitor.mobile.ui.needs.NeedsEntry
import com.ccmonitor.mobile.ui.session.SessionTabManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.io.File
import java.io.IOException
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
    /** 从搜内容的一处命中打开：命中那一行的字节起点（缺 ⇒ 读最后一屏）· 落在哪次工具调用的卡里（见 [ChatSession.jump]）。 */
    at: Long? = null,
    tool: String? = null,
    /** 顶栏「⋯」→ 轮换：这条会话（sid）的轮换屏。 */
    onOpenRotation: (String) -> Unit = {},
    /** 顶栏「⋯」→ agent：这条会话（sid）的 agent 列表。 */
    onOpenAgents: (String) -> Unit = {},
    /** 顶栏「⋯」→ 打开工作目录：文件屏开一个标签、落在这条会话的项目目录（那台上的全路径）。 */
    onOpenDir: (String) -> Unit = {},
    /** 顶栏「终端」：终端屏开一个标签，附着它住的那个终端（句柄是那台写在脸上的），终端已经不在 ⇒ 先让那台恢复再附着。 */
    onOpenTerminal: (SessionTabManager.TabTarget.Terminal) -> Unit = {},
    /** 顶栏右上「需手动 n」：需手动屏。 */
    onOpenOverview: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    // 先把连接建起来；持有挂在对话上，由 `ChatController.sessionFor(hostId = …)` 负责。
    val link by rememberHostLink(hostId, chatHolder(chatKey(hostId, sessionId)), retainWhileComposed = false)
    val scope = rememberCoroutineScope()
    val host = rememberChatHost(hostId) ?: return
    val open = if (isNew) ChatOpen(hostId, chatKey(hostId, sessionId), ChatTarget.Draft(ticket = sessionId, cwd = draftCwd), host.machine) else existingChat(hostId, sessionId, host.machine, at, tool)
    val vm: ChatViewModel = koinViewModel(key = open.key) { parametersOf(open) }
    val state by vm.state.collectAsStateWithLifecycle()
    val backends = koinInject<HostBackends>()
    // 在屏上才进这台的 stream-watch 名单（离屏 ⇒ 退出，列表页上名单是空的）。
    DisposableEffect(vm) {
        vm.visible(true)
        onDispose { vm.visible(false) }
    }

    val drawerState = rememberDrawerState(DrawerValue.Closed)
    // 在一条会话里点「新建会话」⇒ 新的那张起在这条的目录。
    val onDrawer: (DrawerAction) -> Unit = { action ->
        val cwd = state.cwd
        onDrawerAction(if (action == DrawerAction.NewConversation && cwd != null) DrawerAction.NewDraft(hostId, cwd) else action)
    }
    val needs = rememberChatNeeds(open.key, hostId, backends.of(hostId), remember(vm) { vm.state.map { it.sid } }, host.machine)
    // ⋯ 里的会话里找 · 大纲 · 结束会话 · 无人答时自动答（判定都在那台，见 [ChatMore]）。
    val more = remember(open.key) { ChatMore(backends.of(hostId), host.machine, scope, { vm.state.value.sid }, vm::jump) }
    // 点标题出的「会话信息」（只读，字都是那台写的，见 [ChatInfo]）。
    val info = remember(open.key) { ChatInfo(backends.of(hostId), host.machine, scope, { vm.state.value.sid }) { vm.state.value.facts } }
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawer) {
        Scaffold(
            modifier = modifier,
            topBar = {
                ChatTopBar(
                    title = if (state.drafting) copyText("newSession.title.plain") else state.title.orEmpty(),
                    head = headLine(state).takeIf { !state.drafting },
                    onTitle = info::open,
                    onOpenDrawer = { scope.launch { drawerState.open() } },
                    sid = state.sid?.takeIf { !state.drafting },
                    terminal = state.terminal,
                    onTerminal = onOpenTerminal,
                    onRotation = onOpenRotation,
                    openDir = state.cwd?.let { dir -> { onOpenDir(dir) } },
                    rename = Renaming(state.label, vm::rename),
                    agents = rememberAgentsEntry(hostId, state.sid)?.let { MoreAgents(it, onOpenAgents) },
                    needs = { NeedsEntry(onOpenOverview) },
                    more = more,
                    endTitle = state.label.orEmpty(),
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
                ChatHead(hostId, host.machine, vm.picker?.takeIf { state.drafting }) { id -> onDrawerAction(DrawerAction.NewDraft(id, null)) }
                ChatMorePanes(more)
                ChatInfoPane(info, host.machine) { sid -> onDrawerAction(DrawerAction.OpenRecent(sid)) }
                ChatBody(vm, state, needs, Modifier.weight(1f))
            }
        }
    }
}

/**
 * 顶栏下那一截：机器条（那台的连接态，空新会话与读一条会话同一条；通着无事不画）＋ 还没起好的新建会话那一排 chip（[picker] 不为 `null` 时）。
 */
@Composable
private fun ChatHead(
    hostId: String,
    machine: String,
    picker: NewSessionPicker?,
    onSwitchMachine: (String) -> Unit,
) {
    MachineBar(hostId)
    if (picker != null) NewSessionPane(hostId, machine, picker, onSwitchMachine = onSwitchMachine)
}

/** 正文 ＋ 需手动 ＋ 输入那一圈（需手动里在等的那一步也递给正文，那张工具卡上标它）。 */
@Composable
private fun ChatBody(
    vm: ChatViewModel,
    state: ChatUiState,
    needs: NeedsViewModel,
    modifier: Modifier,
) {
    val needsUi by needs.desk.state.collectAsStateWithLifecycle()
    val attach = vm.attach?.collectAsStateWithLifecycle()?.value
    ChatScreen(
        state = state,
        actions = vm.actions,
        onLoadOlder = vm::loadOlder,
        process = ProcessActions(toggle = vm::toggle, jumpFail = vm::jumpFail),
        onBranch = vm::toggleBranch,
        awaiting = needsUi.awaiting,
        needs = { ChatNeeds(needs) },
        modifier = modifier,
        attach = attach,
        onFollow = vm::follow,
        loadImage = vm::image,
        loadHl = vm::segHl,
        loadDoc = vm::mdDoc,
    )
}

/** 过程行那两样动作：开 ⇄ 合 · 跳到第一处失败（都按那一段的 `after`）。 */
data class ProcessActions(
    val toggle: (String) -> Unit = {},
    val jumpFail: (String) -> Unit = {},
)

/** 开一条对话要的那几样（DI 按类型取这一件，不按位置取几个裸串）。 */
data class ChatOpen(
    val hostId: String,
    val key: String,
    val target: ChatTarget,
    val machine: String,
    /** 从一处命中打开（[ChatSession.jump]）；缺 ⇒ 读最后一屏。 */
    val at: Long? = null,
    val tool: String? = null,
)

/** 读一条已有的会话（从列表点进来；从搜内容的一处命中点进来 ⇒ 带 [at] · [tool]）。 */
fun existingChat(
    hostId: String,
    sid: String,
    machine: String,
    at: Long? = null,
    tool: String? = null,
): ChatOpen = ChatOpen(hostId, chatKey(hostId, sid), ChatTarget.Existing(sid), machine, at, tool)

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
    /** 那台的附件在本机住哪（生产上是 `files/attach/<主机>`）；不给 ⇒ 没有附件那一排。 */
    attachRoot: ((hostId: String) -> File)? = null,
    /** 现开本机一份的描述符（交核心）。 */
    fdOf: (File) -> Int = { throw IOException(it.path) },
    /** 那台推来的 `changed` 帧（生产上是那台常驻流的 `changed`；会话的输入队名单从这里来）。 */
    changedOf: (hostId: String) -> Flow<Changed> = { emptyFlow() },
): ChatSession =
    sessionFor(key = open.key, hostId = open.hostId) { scope ->
        val link = linkOf(open.hostId)
        val store = attachRoot?.let { AttachStore(it(open.hostId)) }
        val attach: ((CoroutineScope, () -> String?) -> AttachTray)? = store?.let { st -> { s, sid -> AttachTray(link, open.machine, st, s, sid, fdOf) } }
        ChatSession(
            link,
            watchOf(open.hostId),
            open.target,
            open.machine,
            scope,
            cache = cacheOf(open.hostId),
            attachOf = attach,
            queueMoved = changedOf(open.hostId),
        )
    }.also { s -> open.at?.let { s.jump(it, open.tool) } }

/** 这台在手机上的名字；还没读到 ⇒ `null`。 */
private data class ChatHost(
    val machine: String,
)

@Composable
private fun rememberChatHost(hostId: String): ChatHost? {
    val hosts = koinInject<HostRepository>()
    val got by produceState<ChatHost?>(null, hostId) {
        val h = runCatching { hosts.get(hostId) }.getOrNull()
        value = ChatHost(machine = h?.label ?: hostId)
    }
    return got
}

/**
 * 顶栏常驻的「终端」那一颗：照脸上 `can.terminal.far` 画（手机从不在会话住的那台上看，挑 `far`）——点得了 ⇒ 开终端屏；
 * 做不了（或还没收到脸）⇒ 灰着。长按出那台写好的那一句（悬停那一句 · 做不了的原因）。不异步长出来、不问那台在哪。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TerminalButton(
    sid: String,
    way: TermWay?,
    onTerminal: (SessionTabManager.TabTarget.Terminal) -> Unit,
) {
    val target = terminalTarget(way, sid)
    val label = copyText("mobDrawer.item.terminal")
    TooltipBox(
        positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
        tooltip = { terminalSaid(way)?.let { PlainTooltip { Text(it) } } },
        state = rememberTooltipState(),
    ) {
        PhIconButton(
            Ph.TerminalWindow,
            label,
            onClick = { target?.let(onTerminal) },
            modifier = Modifier.testTag(ChatTestTags.TERMINAL),
            enabled = target != null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** 点「终端」开哪个（纯函数，判据读）：接进它住的那个终端（那台写在脸上的句柄）· 先恢复再附着；做不了 · 句柄判不出 · 还没收到脸 ⇒ `null`（灰着）。 */
internal fun terminalTarget(
    way: TermWay?,
    sid: String,
): SessionTabManager.TabTarget.Terminal? =
    when (way) {
        is TermWay.Here ->
            when (val open = way.open) {
                is TermOpen.Attach -> open.terminal?.let { SessionTabManager.TabTarget.Terminal(tmuxSession = it) }
                TermOpen.Resume -> SessionTabManager.TabTarget.Terminal(resume = sid)
            }
        is TermWay.Front, is TermWay.No, null -> null
    }

/** 长按出的那一句（那台写好的）：做得了 ⇒ 去做什么；做不了 ⇒ 为什么。 */
internal fun terminalSaid(way: TermWay?): String? =
    when (way) {
        is TermWay.Front -> way.said
        is TermWay.Here -> way.said
        is TermWay.No -> way.why
        null -> null
    }

/** 聊天 ⋯ 里的一项（有哪几项、什么顺序照 [chatMoreItems]）。只放对这条会话本身做的事；计划是全局去处，在抽屉里。 */
enum class ChatMoreItem {
    Agents,
    Rename,
    Rotation,
    OpenDir,

    /** 「查找 · 大纲」一项（同桌面那一个框的两种看法）：先开查找，框里一颗切到大纲。 */
    FindOutline,
    AutoAnswer,
    End,
}

/** ⋯「改标题」：此刻的名字（框里的初值；还没有 ⇒ 空框）· 交那台（名字等会话簿那一格换）。 */
private class Renaming(
    val label: String?,
    val commit: (String) -> Unit,
)

/**
 * 聊天屏的顶栏：左上 ☰ 拉出抽屉，标题是这条会话叫什么（那台定的）；右上常驻「终端」那一颗（[TerminalButton]；答不了的需手动从这里进终端），
 * ⋯ 里那几项照 [chatMoreItems]（只放对这条会话本身做的事；计划是全局去处，在抽屉里）。
 * 还没起好的新建会话（[sid] 为 `null`）不画终端与 ⋯。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatTopBar(
    /** 这条会话那台写好的整行标题（会话簿的 `title`）；还没起好的新建会话写「新建会话」。 */
    title: String,
    /** 第二行：点 ＋ 状态一句（＋ 上下文要注意那一句）；簿里还没这一条 · 新建会话 ⇒ `null`、不画。 */
    head: HeadLine?,
    /** 点标题：出「会话信息」。 */
    onTitle: () -> Unit,
    onOpenDrawer: () -> Unit,
    sid: String?,
    /** 脸上「终端」那一格（`can.terminal.far`）；还没收到脸 ⇒ `null`（那一颗照画、灰着）。 */
    terminal: TermWay?,
    onTerminal: (SessionTabManager.TabTarget.Terminal) -> Unit,
    onRotation: (String) -> Unit,
    /** ⋯「打开工作目录」；这条会话的目录还不知道 ⇒ `null`、不出这一项。 */
    openDir: (() -> Unit)?,
    rename: Renaming,
    /** ⋯「agent」那一项；这条会话没有运行表 ⇒ `null`、不出这一项。 */
    agents: MoreAgents?,
    /** ⋯ 左边那颗「需手动 n」（0 不画；空的新建会话里同样有）。 */
    needs: @Composable () -> Unit,
    /** ⋯ 里查找与大纲 · 无人答时自动答 · 结束会话那几项做事的那一头。 */
    more: ChatMore,
    /** 这条会话的名字（「结束会话…」确认框里列它）。 */
    endTitle: String,
) {
    var menu by remember { mutableStateOf(false) }
    var renaming by remember(sid) { mutableStateOf(false) }
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
        title = { TopTitle(title, head, onTitle.takeIf { sid != null }) },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = LocalAppTokens.current.bg2, scrolledContainerColor = LocalAppTokens.current.bg2),
        actions = {
            needs()
            if (sid != null) {
                TerminalButton(sid, terminal, onTerminal)
                IconButton(
                    onClick = { menu = true },
                    modifier = Modifier.semantics { contentDescription = copyText("history.row.more") }.testTag(ChatTestTags.MORE),
                ) { Text(MORE_GLYPH, style = MaterialTheme.typography.titleLarge) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    val close = { menu = false }
                    for (item in chatMoreItems(agents = agents != null, openDir = openDir != null, more = more)) {
                        when (item) {
                            ChatMoreItem.Agents ->
                                agents?.let { a ->
                                    DropdownMenuItem(text = { Text(a.text) }, onClick = {
                                        close()
                                        a.open(sid)
                                    })
                                }
                            ChatMoreItem.Rename ->
                                DropdownMenuItem(text = { Text(copyText("history.menu.rename")) }, onClick = {
                                    close()
                                    renaming = true
                                })
                            ChatMoreItem.Rotation ->
                                DropdownMenuItem(text = { Text(copyText("machinePage.tab.rot")) }, onClick = {
                                    close()
                                    onRotation(sid)
                                })
                            ChatMoreItem.OpenDir -> openDir?.let { OpenDirItem(it, close) }
                            ChatMoreItem.FindOutline -> FindOutlineItem(more, close)
                            ChatMoreItem.AutoAnswer -> AutoAnswerItem(more, close)
                            ChatMoreItem.End -> EndItem(more, endTitle, close)
                        }
                    }
                }
                if (renaming) RenameDialog(rename.label.orEmpty(), onDismiss = { renaming = false }, onRename = rename.commit)
            }
        },
    )
}

private const val MORE_GLYPH = "⋯"

/** 顶栏标题那两行：那台写好的整行标题 ＋ [head]；[onClick] 给了（会话起好了）⇒ 点了出「会话信息」。 */
@Composable
private fun TopTitle(
    title: String,
    head: HeadLine?,
    onClick: (() -> Unit)?,
) {
    Column(onClick?.let { Modifier.clickable(onClick = it) } ?: Modifier) {
        Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        head?.let { HeadLineRow(it) }
    }
}

/** ⋯「打开工作目录」那一项（字同桌面）：先收起菜单再去。 */
@Composable
private fun OpenDirItem(
    go: () -> Unit,
    close: () -> Unit,
) = DropdownMenuItem(text = { Text(copyText("tabBarView.tab.cwdHint")) }, onClick = {
    close()
    go()
})

/** ⋯「agent」那一项：字是那台写好的那一句（[agentsEntry]），点了进这条会话的 agent 列表。 */
private data class MoreAgents(
    val text: String,
    val open: (String) -> Unit,
)

/** 会话簿里这条会话（[sid]）的运行表那一句；还没起好 · 没有运行表 ⇒ `null`。 */
@Composable
private fun rememberAgentsEntry(
    hostId: String,
    sid: String?,
): String? {
    val backends = koinInject<HostBackends>()
    val book by remember(hostId) { backends.of(hostId).table }.collectAsStateWithLifecycle()
    return sid?.let { agentsEntry(book, it) }
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
 * 新建会话那张草稿的票：界面上的 [chatKey] 用它，起会话也带它（`session-open` 的 `ticket`：期限到了同一张再问，那台不起第二个）。
 * 会话起好之后的 sid 是那台报到时说的，不是它。
 */
fun newConversationId(): String = UUID.randomUUID().toString()
