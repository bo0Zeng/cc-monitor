package com.ccmonitor.mobile.ui.chat

import android.content.Intent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
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
import com.ccmonitor.mobile.core.claude.bridge.ChatHistorySource
import com.ccmonitor.mobile.core.claude.bridge.PipeDiagnostics
import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.bridge.PipeUplinkSink
import com.ccmonitor.mobile.core.claude.bridge.TurnState
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ssh.PipeLauncher
import com.ccmonitor.mobile.ssh.TmuxBackend
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.chatHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_GLYPH
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.util.UUID

/**
 * 生产的聊天屏：一个对话的完整接线。
 *
 * 起管道 → tail `events.ndjson` → 喂 `ChatSession` → 上行追加 `in.ndjson`。
 * 对话身份是 [chatKey]，不是屏幕实例：对话活在应用级 `ChatController`，用稳定的 key 取，进屏才不会新建对话。
 *
 * 本文件不写 `onDispose`：它在每次离屏时都跑，包括进设置再回来；返回上一屏不算离开对话。
 * `release` 由导航层在「返回 = 离开这个对话」时调。
 */
@Composable
fun ChatRoute(
    hostId: String,
    sessionId: String,
    /** 抽屉里点下去的事，去哪由导航层定；不给默认值，免得某个调用方静默焊死整个抽屉。 */
    onDrawerAction: (DrawerAction) -> Unit,
    /** 起会话时的权限模式（见 `ClaudeInvocation.PERMISSION_MODES`）；null 表示用 Claude 自己的默认。 */
    permissionMode: String? = null,
    /**
     * 这个对话是刚开的，远端还不存在。
     *
     * 决定带不带 `--resume`：对远端没见过的编号 resume 会直接失败。缺省 `false`，从列表点进来的对话确实存在。
     */
    isNew: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val manager = koinInject<SshConnectionManager>()
    // 记「这条对话被确认过」用，见 [ConfirmConversationOnFirstSuccess]。
    val settings = koinInject<SettingsRepository>()
    val pipe = remember(manager, hostId, sessionId) { PipeSession(manager.commandChannel(hostId), sessionId) }
    // 「重新接上内容」重跑整条开屏序列：建连 → 账号门 → 幂等重建管道 → 重挂下行。
    // 管道已死时只重挂下行就是 tail 一个不再增长的文件，按钮点得动、对面却收不到。
    var attempt by remember(hostId, sessionId) { mutableIntStateOf(0) }
    // 上行每次写之前先确认对面在听，见 [rememberUplinkSink]。
    val launchRef = remember(hostId, sessionId) { mutableStateOf<ChatLaunchContext?>(null) }
    val sink = rememberUplinkSink(manager, hostId, sessionId, pipe, launchRef, permissionMode, isNew)
    val vm: ChatViewModel = rememberChatViewModel(hostId, sessionId, sink)

    // 先把连接建起来，`commandChannel` 无活连接会抛。
    // 持有不在这里 retain：持有挂在对话上，由 `ChatController.sessionFor(hostId = …)` 负责。
    val link by rememberHostLink(hostId, chatHolder(chatKey(hostId, sessionId)), retainWhileComposed = false, attempt = attempt)

    // 在哪个目录、用哪条命令跑；读出来之前不许起管道。
    val launch = rememberChatLaunchContext(hostId)

    // key 必须覆盖谓词里的每个变量：少一个，切换主机或对话后就会对着上一个对话发命令。
    // `link` 也在 key 里，连接没通就起管道等于对着必然抛异常的通道发命令。
    LaunchedEffect(hostId, sessionId, launch, permissionMode, link, attempt) {
        // 上行探活要拿到同一份启动参数。
        launchRef.value = launch
        if (launch == null) return@LaunchedEffect // 还在读那台主机的配置，等它
        if (link !is HostLink.Ready) {
            // 接不上必须说出来，不留一个永远空白的屏。
            (link as? HostLink.Failed)?.let { vm.reportStartFailure("接不上这台主机：${it.why}") }
            return@LaunchedEffect
        }
        // 没选账号不许起，且必须在起管道之前拦：跑起来再解释，用户只看到 `api_error`。
        if (!launch.accountChosen) {
            // 降级不是删除：翻历史不需要管道，已有对话在这里照样要能读历史。
            // 这里重复调一次而不把下面那句提前，是为了不改成功路径的调用顺序。
            attachHistory(vm, manager, hostId, launch, sessionId)
            vm.reportStartFailure(NO_ACCOUNT_CHOSEN)
            return@LaunchedEffect
        }
        val why = startPipeFor(manager, hostId, sessionId, launch, permissionMode, isNew)
        // 起不来要说出来，走会话状态行那条唯一通道，不另开 Toast。
        // 起成功但一直没内容也要说出来，这里计帧数给下面的探针用。
        var framesSeen = 0
        // 第一次是 `start`、之后是 `reattach`，由 [ChatSession.openOrReattach] 判定，调用方不另记。
        vm.openOrReattach(pipe.frames().onEach { framesSeen++ })
        if (why != null) vm.reportStartFailure(why)

        attachHistory(vm, manager, hostId, launch, sessionId)

        // 等一小会儿，一帧都没来就去问那条命令自己说了什么。
        // 判据是「一帧都没来」而非「界面还空着」：健康的管道起来就吐钩子事件，而新对话界面空着是正常的。
        // 日志里没线索时不报，那与「远端没配钩子、健康但安静」分不开。
        delay(SILENT_START_PROBE_MS)
        if (framesSeen == 0) {
            PipeDiagnostics(manager.commandChannel(hostId), sessionId)
                .explainFailure()
                ?.let { vm.reportStartFailure("$REMOTE_FAILED_PREFIX$it") }
        }
    }

    KeepAliveWhileLinked(link)

    val attach = rememberAttachmentPicker(hostId, sessionId, vm)

    val state by vm.state.collectAsStateWithLifecycle()

    ConfirmConversationOnFirstSuccess(hostId, sessionId, state.turn, settings)

    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = { ChatTopBar(onOpenDrawer = { scope.launch { drawerState.open() } }) },
        ) { inner ->
            ChatScreen(
                state = state,
                onSend = vm::send,
                // 等待态下的逃生动作，必须接：不接的话 `inputBlockedBy` 会连禁用一起关掉，只剩一条拦不住任何东西的提示。
                onSendAnyway = vm::sendAnyway,
                onRetrySend = vm::retry,
                onStop = vm::stop,
                onLoadOlder = vm::loadOlder,
                onAttach = attach,
                // 重跑整条开屏序列；门关着时会再报一次那句提示，而不是让按钮消失。
                onReattach = { attempt++ },
                modifier = Modifier.fillMaxSize().padding(inner),
            )
        }
    }
}

/**
 * 聊天屏的顶栏：左上 ☰ 拉出抽屉。
 *
 * 右上 ⋯ 不画：它只放对这条对话本身做的事，那几项还没有。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatTopBar(onOpenDrawer: () -> Unit) {
    TopAppBar(
        navigationIcon = {
            IconButton(
                onClick = onOpenDrawer,
                modifier = Modifier.semantics { contentDescription = DRAWER_OPEN_DESCRIPTION }.testTag(ChatTestTags.DRAWER_OPEN),
            ) {
                Text(DRAWER_OPEN_GLYPH, style = MaterialTheme.typography.titleLarge)
            }
        },
        title = { Text(CHAT_TITLE) },
    )
}

/** 落地屏的标题。不写主机名，界面上不出现「连接」那套说法。 */
internal const val CHAT_TITLE = "Claude"

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
 * 起这条对话的常驻管道。
 *
 * 调用方必须先过账号门；本函数不重复判定，两处各判一次就会漂移。
 *
 * @return null 表示起成功；非 null 是人可读的失败原因。
 */
private suspend fun startPipeFor(
    manager: SshConnectionManager,
    hostId: String,
    sessionId: String,
    launch: ChatLaunchContext,
    permissionMode: String?,
    isNew: Boolean,
): String? =
    PipeLauncher.start(
        channel = manager.commandChannel(hostId),
        backend = TmuxBackend,
        sessionId = sessionId,
        launchCommand = launch.launchCommand,
        workdir = launch.workdir,
        permissionMode = permissionMode,
        // 已有对话接着它跑，不起同名的空会话；新开的对话不能 resume，见 [resumeIdFor]。
        // 注意：管道不回放历史，历史另从会话记录读。
        resumeSessionId = resumeIdFor(sessionId, isNew),
        // 新对话把编号指定给 Claude，本机编号就是它的编号，下次找得回来。
        newSessionId = newSessionIdFor(sessionId, isNew),
        // 账号目录也要传给起的那侧，否则远端做账号隔离时起出来是未登录态。
        // 这里为 null 只表示用户选的是「听远端自己的」；选没选过由 `accountChosen` 表达。
        claudeDir = launch.claudeDir,
    )

/**
 * 聊天面 VM 的取法。
 *
 * 参数按类型取，其中两个都是对话编号、含义不同：
 * [ChatSessionKey] 是本进程里的 `<hostId>/<sid>`（`ChatController` 的键），
 * [ClaudeSessionId] 是 daemon 与 pidfile 都认的 sid，跨通路信号汇按它索引；混用则信号恒 null 且不报错。
 */
@Composable
private fun rememberChatViewModel(
    hostId: String,
    sessionId: String,
    sink: PipeUplinkSink,
): ChatViewModel =
    org.koin.androidx.compose.koinViewModel {
        parametersOf(
            ChatSessionKey(chatKey(hostId, sessionId)),
            sink,
            HostId(hostId),
            ClaudeSessionId(sessionId),
        )
    }

/**
 * 生产的上行出口：带探活的 [PipeUplinkSink]。
 *
 * 注意：`listening` 是尾随 lambda，往中间插参数会把既有实参绑错。
 * `remember` 的 key 里不放 [launchRef]、[permissionMode]、[isNew]：探活读的是当前值，
 * 而 sink 换实例会连带重建 `ChatViewModel` 和整条对话，所以变的东西走 `State`。
 */
@Composable
private fun rememberUplinkSink(
    manager: SshConnectionManager,
    hostId: String,
    sessionId: String,
    pipe: PipeSession,
    launchRef: State<ChatLaunchContext?>,
    permissionMode: String?,
    isNew: Boolean,
): PipeUplinkSink =
    remember(manager, hostId, pipe) {
        PipeUplinkSink(manager.commandChannel(hostId), pipe) {
            ensureListeningFor(manager, hostId, sessionId, launchRef.value, permissionMode, isNew)
        }
    }

/**
 * 上行写之前的探活（`PipeListening` 的生产实现）。
 *
 * 管道不在时 `printf … >> in.ndjson` 照样成功，不探活的话每条消息都显示已送达却无人接收。
 * 探活就是再调一次 [startPipeFor]：`PipeLauncher.start` 幂等，在跑什么都不做，不在就重建；参数与开屏同一份。
 *
 * 局限：只保证那条 tmux 会话在，不保证里面的命令还活着；刚重建时 `tail -n 0 -f` 未必已打开文件；
 * 每发一条多一次远端往返。
 *
 * @return null 表示在听或刚重建好；非 null 是人可读的原因，调用方必须当成没发出去。
 */
private suspend fun ensureListeningFor(
    manager: SshConnectionManager,
    hostId: String,
    sessionId: String,
    launch: ChatLaunchContext?,
    permissionMode: String?,
    isNew: Boolean,
): String? {
    // 配置还没读到：不能拿 null 去起，也不能放行，如实说一句让用户重试。
    val ctx = launch ?: return NOT_READY_TO_SEND
    // 账号门这里也要守，否则开屏被拦后直接打字发送会绕过它。
    if (!ctx.accountChosen) return NO_ACCOUNT_CHOSEN
    return startPipeFor(manager, hostId, sessionId, ctx, permissionMode, isNew)
}

/** 配置还没读出来时那句话。文案不出现内部词（`连接`、`SSH`、`tmux`、技术义的`会话` 等）。 */
internal const val NOT_READY_TO_SEND = "这台机器的设置还没读出来，稍等一下再发"

/**
 * 挂上历史翻页。`--resume` 不回放历史，只 resume 的话 Claude 记得上下文、屏幕却是空的。
 *
 * 只读这份记录，绝不 tail 它：它正被这条对话写着。
 * cwd 未知就读不了（记录落在编码后的 cwd 目录下），这时不挂，界面如实显示「没有更早的了」。
 */
private fun attachHistory(
    vm: ChatViewModel,
    manager: SshConnectionManager,
    hostId: String,
    launch: ChatLaunchContext,
    sessionId: String,
) {
    ClaudePaths
        .sessionRecordPath(launch.claudeDir ?: ClaudePaths.DEFAULT_CLAUDE_DIR, launch.workdir, sessionId)
        ?.let { path ->
            val history = ChatHistorySource(manager.commandChannel(hostId), path)
            vm.attachHistoryPaging { history.loadOlder() }
        }
}

/**
 * 等多久还没有任何内容，就去看那条命令自己说了什么。
 *
 * 健康的管道几百毫秒内就吐钩子事件；太短会在慢主机上误报，太长会让用户对着空白干等。
 */
private const val SILENT_START_PROBE_MS = 4000L

/**
 * 远端起不来时上屏那句话的前缀，后面跟它自己吐的原话。
 *
 * 文案不出现内部词，所以不能把 `bridge.log` 这类名字端给用户。
 */
internal const val REMOTE_FAILED_PREFIX = "远端那个 Claude 没起来："

/**
 * 这台机器还没选账号时上屏的那句话。
 *
 * 不给错误码：要说清发生了什么、该做什么。文案不出现内部词；
 * 「抽屉最底一行」「编辑」是界面上真有的两处，照着念就能走到。
 * 它与「接不上这台主机」共用 `ChatSession.reportStartFailure` 那一个反馈槽。
 */
internal const val NO_ACCOUNT_CHOSEN = "这台机器还没选账号；从抽屉最底一行进服务器列表，点这台的「编辑」，把 Claude 账号目录填上"

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
 * 给新开的对话发编号。
 *
 * 同一个串同时是远端目录名与常驻命令的标识、界面上的 [chatKey]、Claude 自己的对话编号（`--session-id`）。
 * 所以必须是裸 UUID：`--session-id` 只认合法 UUID，加前缀会让本机编号与 Claude 的分叉，下次找不回来。
 */
fun newConversationId(): String = UUID.randomUUID().toString()

/**
 * 这一轮要不要 `--resume` 已有的对话，要的话接哪一条。
 *
 * 从列表点进来的（`isNew = false`）给那个编号，否则会起一条同名的空对话；
 * 刚开的给 `null`，编号远端没见过，接它必然失败。
 */
fun resumeIdFor(
    sessionId: String,
    isNew: Boolean,
): String? = sessionId.takeUnless { isNew }

/**
 * 这一轮要不要用 `--session-id` 把编号指定给 Claude。
 *
 * 与 [resumeIdFor] 是同一个布尔的镜像，结构上不可能两个都给；两个都给 `ClaudeInvocation.pipeInvocation` 会抛。
 */
fun newSessionIdFor(
    sessionId: String,
    isNew: Boolean,
): String? = sessionId.takeIf { isNew }

/**
 * 这条对话在远端真跑起来过，下次才允许 `--resume` 它。
 *
 * 判据是有一轮成功结束：`TurnState.Done(ok = true)`。
 * 不能拿进屏当判据：连接失败也会记下编号，下次 resume 一个远端没有的编号，Claude 当场退出。
 * 也不能只看 `Done`，失败的一轮同样是 `Done`。
 * 局限：只证明这个编号在远端跑起来过，不证明记录现在还在。
 */
@Composable
private fun ConfirmConversationOnFirstSuccess(
    hostId: String,
    sessionId: String,
    turn: TurnState,
    settings: SettingsRepository,
) {
    val turnOk = (turn as? TurnState.Done)?.ok == true
    LaunchedEffect(hostId, sessionId, turnOk) {
        if (turnOk) settings.setConfirmedConversation(hostId, sessionId)
    }
}
